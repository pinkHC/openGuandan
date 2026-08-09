use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use indexmap::IndexMap;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::domain::{
    errors::RuleError,
    match_state::{
        MatchState, create_match, give_match_tribute, pass_match_turn, play_match_cards,
        return_match_tribute, start_next_round,
    },
    types::{CombinationDeclaration, Seat},
};

use super::types::{
    CommandContext, CommandResult, Participant, ParticipantCredentials, ParticipantRole, PlayerId,
    PublicationBarrier, PublicationMessage, PublicationReceipt, RoomEvent, RoomPhase, RoomState,
};

const ROOM_CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const MAX_PARTICIPANTS: usize = 104;
const MAX_PROCESSED_COMMANDS: usize = 500;
const MAX_SOCKETS_PER_PARTICIPANT: usize = 8;
const PUBLICATION_QUEUE_CAPACITY: usize = 128;

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

struct RoomServiceInner {
    rooms: Mutex<IndexMap<String, RoomState>>,
    publication_tx: mpsc::Sender<PublicationMessage>,
    publication_rx: Mutex<Option<mpsc::Receiver<PublicationMessage>>>,
    clock: Clock,
    reconnect_grace_ms: u64,
    room_idle_ttl_ms: u64,
}

/// Thread-safe, process-local owner of every active room.
#[derive(Clone)]
pub struct RoomService {
    inner: Arc<RoomServiceInner>,
}

impl RoomService {
    pub fn new(reconnect_grace_ms: u64, room_idle_ttl_ms: u64) -> Self {
        Self::with_clock(reconnect_grace_ms, room_idle_ttl_ms, system_time_ms)
    }

    pub fn with_clock(
        reconnect_grace_ms: u64,
        room_idle_ttl_ms: u64,
        clock: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        let (publication_tx, publication_rx) = mpsc::channel(PUBLICATION_QUEUE_CAPACITY);
        Self {
            inner: Arc::new(RoomServiceInner {
                rooms: Mutex::new(IndexMap::new()),
                publication_tx,
                publication_rx: Mutex::new(Some(publication_rx)),
                clock: Arc::new(clock),
                reconnect_grace_ms,
                room_idle_ttl_ms,
            }),
        }
    }

    fn now(&self) -> u64 {
        (self.inner.clock)()
    }

    /// Returns the single commit-ordered publication stream. It can only be taken once.
    pub fn take_publications(&self) -> Option<mpsc::Receiver<PublicationMessage>> {
        self.inner.publication_rx.lock().take()
    }

    fn try_reserve_publication(&self) -> Result<mpsc::OwnedPermit<PublicationMessage>, RuleError> {
        self.inner
            .publication_tx
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    RuleError::new("SERVER_BUSY", "服务器繁忙，请稍后重试")
                }
                mpsc::error::TrySendError::Closed(_) => {
                    RuleError::internal("Room publication queue is closed")
                }
            })
    }

    async fn reserve_publication(
        &self,
    ) -> Result<mpsc::OwnedPermit<PublicationMessage>, RuleError> {
        self.inner
            .publication_tx
            .clone()
            .reserve_owned()
            .await
            .map_err(|_| RuleError::internal("Room publication queue is closed"))
    }

    pub async fn publication_barrier(&self, code: &str) -> Result<PublicationBarrier, RuleError> {
        let permit = self.reserve_publication().await?;
        let code = normalize_room_code(code);
        let rooms = self.inner.rooms.lock();
        let room = Arc::new(rooms.get(&code).ok_or_else(room_not_found)?.clone());
        let (ready_tx, ready) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let message = PublicationMessage::Fence {
            ready: ready_tx,
            release: Some(release_rx),
        };
        permit.send(message);
        Ok(PublicationBarrier {
            room,
            ready,
            release,
        })
    }

    fn enqueue_update(
        &self,
        room: &RoomState,
        events: Vec<RoomEvent>,
        permit: mpsc::OwnedPermit<PublicationMessage>,
    ) -> PublicationReceipt {
        let (completed, publication) = oneshot::channel();
        let message = PublicationMessage::Update {
            room: Arc::new(room.clone()),
            events,
            completed,
        };
        permit.send(message);
        publication
    }

    fn enqueue_flush(&self, permit: mpsc::OwnedPermit<PublicationMessage>) -> PublicationReceipt {
        let (completed, publication) = oneshot::channel();
        permit.send(PublicationMessage::Fence {
            ready: completed,
            release: None,
        });
        publication
    }

    pub fn create_room(&self, display_name: &str) -> Result<ParticipantCredentials, RuleError> {
        let display_name = validate_display_name(display_name)?;
        let publication_permit = self.try_reserve_publication()?;
        let now = self.now();
        let mut rooms = self.inner.rooms.lock();
        let code = generate_room_code(&rooms)?;
        let participant =
            create_participant(display_name, ParticipantRole::Player, Some(Seat::ZERO), now);
        let participant_id = participant.id.clone();

        let room = RoomState {
            instance_id: Uuid::new_v4(),
            code: code.clone(),
            phase: RoomPhase::Lobby,
            host_id: participant_id.clone(),
            participants: IndexMap::from([(participant_id.clone(), participant.clone())]),
            seats: [Some(participant_id), None, None, None],
            match_state: None,
            version: 1,
            last_activity_at: now,
            processed_commands: IndexMap::new(),
        };
        let credentials = credentials(&room, &participant);
        rooms.insert(code.clone(), room);
        drop(self.enqueue_update(
            rooms.get(&code).expect("newly inserted room exists"),
            Vec::new(),
            publication_permit,
        ));
        Ok(credentials)
    }

    pub fn join_room(
        &self,
        code: &str,
        display_name: &str,
    ) -> Result<ParticipantCredentials, RuleError> {
        let display_name = validate_display_name(display_name)?;
        let publication_permit = self.try_reserve_publication()?;
        let code = normalize_room_code(code);
        let now = self.now();
        let mut rooms = self.inner.rooms.lock();
        let room = rooms.get_mut(&code).ok_or_else(room_not_found)?;

        if room.participants.len() >= MAX_PARTICIPANTS {
            return Err(RuleError::new("ROOM_FULL", "房间人数已达上限"));
        }

        let normalized_name = display_name.to_lowercase();
        if room
            .participants
            .values()
            .any(|participant| participant.display_name.to_lowercase() == normalized_name)
        {
            return Err(RuleError::new(
                "DISPLAY_NAME_TAKEN",
                "该临时用户名已在房间中使用",
            ));
        }

        let open_seat = if room.phase == RoomPhase::Lobby {
            room.seats
                .iter()
                .position(Option::is_none)
                .map(|index| Seat::new(index as u8).expect("seat index is in range"))
        } else {
            None
        };
        let role = if open_seat.is_some() {
            ParticipantRole::Player
        } else {
            ParticipantRole::Spectator
        };
        let room_was_empty = room.participants.is_empty();
        let participant = create_participant(display_name, role, open_seat, now);
        let participant_id = participant.id.clone();

        room.participants
            .insert(participant_id.clone(), participant.clone());
        if let Some(seat) = open_seat {
            room.seats[seat.index()] = Some(participant_id.clone());
        }
        if room_was_empty {
            room.host_id = participant_id;
        }
        room.version += 1;
        room.last_activity_at = now;
        let credentials = credentials(room, &participant);
        drop(self.enqueue_update(room, Vec::new(), publication_permit));
        Ok(credentials)
    }

    pub fn require_room(&self, code: &str) -> Result<RoomState, RuleError> {
        self.inner
            .rooms
            .lock()
            .get(&normalize_room_code(code))
            .cloned()
            .ok_or_else(room_not_found)
    }

    pub fn authenticate(
        &self,
        code: &str,
        participant_id: &str,
        reconnect_token: &str,
    ) -> Result<Uuid, RuleError> {
        let code = normalize_room_code(code);
        let rooms = self.inner.rooms.lock();
        let room = rooms.get(&code).ok_or_else(room_not_found)?;
        match room.participants.get(participant_id) {
            Some(participant) if participant.reconnect_token == reconnect_token => {
                Ok(room.instance_id)
            }
            _ => Err(RuleError::new("INVALID_CREDENTIALS", "房间身份凭证无效")),
        }
    }

    #[cfg(test)]
    pub(crate) fn connect(
        &self,
        participant: &ParticipantCredentials,
        socket_id: &str,
    ) -> Result<RoomState, RuleError> {
        self.connect_socket(
            &participant.room_code,
            &participant.participant_id,
            &participant.reconnect_token,
            socket_id,
        )
    }

    pub fn connect_socket(
        &self,
        code: &str,
        participant_id: &str,
        reconnect_token: &str,
        socket_id: &str,
    ) -> Result<RoomState, RuleError> {
        let publication_permit = self.try_reserve_publication()?;
        let code = normalize_room_code(code);
        let now = self.now();
        let mut rooms = self.inner.rooms.lock();
        let room = rooms.get_mut(&code).ok_or_else(room_not_found)?;
        let participant = room
            .participants
            .get_mut(participant_id)
            .filter(|participant| participant.reconnect_token == reconnect_token)
            .ok_or_else(|| RuleError::new("INVALID_CREDENTIALS", "房间身份凭证无效"))?;
        if !participant.socket_ids.contains(socket_id)
            && participant.socket_ids.len() >= MAX_SOCKETS_PER_PARTICIPANT
        {
            return Err(RuleError::new(
                "TOO_MANY_CONNECTIONS",
                "同一房间身份的连接数量过多",
            ));
        }
        let was_connected = participant.connected();
        participant.socket_ids.insert(socket_id.to_owned());
        participant.disconnected_at = None;
        let became_connected = !was_connected && participant.connected();
        room.last_activity_at = now;
        let events = if became_connected {
            vec![RoomEvent::new(
                "participant.connection",
                json!({ "participantId": participant_id, "connected": true }),
            )]
        } else {
            Vec::new()
        };
        drop(self.enqueue_update(room, events, publication_permit));
        Ok(room.clone())
    }

    pub async fn disconnect_socket(
        &self,
        code: &str,
        participant_id: &str,
        socket_id: &str,
    ) -> Option<RoomState> {
        let code = normalize_room_code(code);
        let registered = self
            .inner
            .rooms
            .lock()
            .get(&code)
            .and_then(|room| room.participants.get(participant_id))
            .is_some_and(|participant| participant.socket_ids.contains(socket_id));
        if !registered {
            return None;
        }

        let publication_permit = match self.reserve_publication().await {
            Ok(permit) => permit,
            Err(error) => {
                tracing::error!(code = %error.code, "cannot publish socket disconnection");
                return None;
            }
        };
        let now = self.now();
        let mut rooms = self.inner.rooms.lock();
        let room = rooms.get_mut(&code)?;
        let Some(participant) = room.participants.get_mut(participant_id) else {
            return Some(room.clone());
        };
        let was_connected = participant.connected();
        if !participant.socket_ids.remove(socket_id) {
            return Some(room.clone());
        }
        if !participant.connected() {
            participant.disconnected_at = Some(now);
        }
        let became_disconnected = was_connected && !participant.connected();
        room.last_activity_at = now;
        let events = if became_disconnected {
            vec![RoomEvent::new(
                "participant.connection",
                json!({ "participantId": participant_id, "connected": false }),
            )]
        } else {
            Vec::new()
        };
        drop(self.enqueue_update(room, events, publication_permit));
        Some(room.clone())
    }

    pub fn set_ready(
        &self,
        command: CommandContext<'_>,
        ready: bool,
    ) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            if room.phase != RoomPhase::Lobby {
                return Err(RuleError::new("MATCH_ALREADY_STARTED", "一局牌已经开始"));
            }
            let player = seated_player_mut(room, participant_id)?;
            player.ready = ready;
            Ok(vec![RoomEvent::new(
                "room.ready",
                json!({ "participantId": participant_id, "ready": ready }),
            )])
        })
    }

    pub fn change_seat(
        &self,
        command: CommandContext<'_>,
        target_seat: Seat,
    ) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            if room.phase != RoomPhase::Lobby {
                return Err(RuleError::new("MATCH_ALREADY_STARTED", "一局牌已经开始"));
            }
            let previous_seat = participant(room, participant_id)?.seat;
            if previous_seat == Some(target_seat) {
                return Err(RuleError::new("SEAT_UNCHANGED", "玩家已在该座位"));
            }
            if room.seats[target_seat.index()].is_some() {
                return Err(RuleError::new("SEAT_OCCUPIED", "该座位已被其他玩家占用"));
            }
            if let Some(previous_seat) = previous_seat {
                room.seats[previous_seat.index()] = None;
            }
            room.seats[target_seat.index()] = Some(participant_id.to_owned());

            let participant = room
                .participants
                .get_mut(participant_id)
                .expect("validated participant remains in the room");
            participant.role = ParticipantRole::Player;
            participant.seat = Some(target_seat);
            participant.ready = false;

            Ok(vec![RoomEvent::new(
                "room.seat_changed",
                json!({
                    "participantId": participant_id,
                    "fromSeat": previous_seat,
                    "toSeat": target_seat,
                }),
            )])
        })
    }

    pub fn start_match(&self, command: CommandContext<'_>) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            require_host(room, participant_id)?;
            if room.phase != RoomPhase::Lobby {
                return Err(RuleError::new("MATCH_ALREADY_STARTED", "一局牌已经开始"));
            }
            if room.seats.iter().any(Option::is_none) {
                return Err(RuleError::new("SEATS_NOT_FULL", "必须坐满四名玩家"));
            }
            require_all_players_connected(room)?;
            if room.seats.iter().any(|id| {
                id.as_ref()
                    .and_then(|id| room.participants.get(id))
                    .is_none_or(|participant| !participant.ready)
            }) {
                return Err(RuleError::new("PLAYERS_NOT_READY", "四名玩家必须全部准备"));
            }

            room.match_state = Some(create_match());
            room.phase = RoomPhase::Playing;
            Ok(vec![RoomEvent::new(
                "match.started",
                json!({ "roundNumber": 1, "levelRank": "2" }),
            )])
        })
    }

    pub fn play_cards(
        &self,
        command: CommandContext<'_>,
        card_ids: &[String],
        declaration: Option<&CombinationDeclaration>,
    ) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            let (match_state, seat) = active_match_for_player(room, participant_id)?;
            let outcome = play_match_cards(match_state, seat, card_ids, declaration)?;
            let Some(round_result) = outcome.round_result else {
                return Ok(vec![]);
            };

            let finished = RoomEvent::new(
                "round.finished",
                json!({
                    "winnerTeam": round_result.winner_team,
                    "finishOrder": round_result.finish_order,
                    "doubleLastSeats": round_result.double_last_seats,
                    "partnerPlacement": round_result.partner_placement,
                    "teamLevels": match_state.team_levels,
                }),
            );
            let Some(winner_team) = outcome.match_winner else {
                return Ok(vec![finished]);
            };

            reset_match(room);
            Ok(vec![
                finished,
                RoomEvent::new("match.finished", json!({ "winnerTeam": winner_team })),
            ])
        })
    }

    pub fn pass(&self, command: CommandContext<'_>) -> Result<CommandResult, RuleError> {
        self.execute_silent_player_action(command, pass_match_turn)
    }

    pub fn give_tribute(
        &self,
        command: CommandContext<'_>,
        card_id: &str,
    ) -> Result<CommandResult, RuleError> {
        self.execute_silent_player_action(command, |match_state, seat| {
            give_match_tribute(match_state, seat, card_id)
        })
    }

    pub fn return_tribute(
        &self,
        command: CommandContext<'_>,
        card_id: &str,
    ) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            let (match_state, seat) = active_match_for_player(room, participant_id)?;
            let Some(tribute) = return_match_tribute(match_state, seat, card_id)? else {
                return Ok(vec![]);
            };

            let contributions: Vec<Value> = tribute
                .contributions
                .iter()
                .map(|(seat, card)| json!({ "seat": seat, "card": card }))
                .collect();
            let returns: Vec<Value> = tribute
                .returns
                .iter()
                .map(|(seat, card)| json!({ "seat": seat, "card": card }))
                .collect();
            Ok(vec![RoomEvent::new(
                "tribute.completed",
                json!({
                    "kind": tribute.kind,
                    "contributions": contributions,
                    "returns": returns,
                    "leaderSeat": tribute.leader_seat,
                }),
            )])
        })
    }

    pub fn start_next_round(
        &self,
        command: CommandContext<'_>,
    ) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            require_host(room, participant_id)?;
            require_all_players_connected(room)?;
            let round = start_next_round(active_match(room)?)?;
            Ok(vec![RoomEvent::new(
                "round.started",
                json!({
                    "roundNumber": round.number,
                    "levelRank": round.level_rank,
                    "phase": round.phase,
                }),
            )])
        })
    }

    pub fn abort_match(&self, command: CommandContext<'_>) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            require_host(room, participant_id)?;
            if room.match_state.is_none() {
                return Err(RuleError::new("NO_ACTIVE_MATCH", "当前没有进行中的一局牌"));
            }
            reset_match(room);
            Ok(vec![RoomEvent::new(
                "match.aborted",
                json!({ "by": participant_id }),
            )])
        })
    }

    fn execute_silent_player_action(
        &self,
        command: CommandContext<'_>,
        operation: impl FnOnce(&mut MatchState, Seat) -> Result<(), RuleError>,
    ) -> Result<CommandResult, RuleError> {
        self.execute(command, |room, participant_id| {
            let (match_state, seat) = active_match_for_player(room, participant_id)?;
            operation(match_state, seat)?;
            Ok(Vec::new())
        })
    }

    fn execute(
        &self,
        command: CommandContext<'_>,
        operation: impl FnOnce(&mut RoomState, &str) -> Result<Vec<RoomEvent>, RuleError>,
    ) -> Result<CommandResult, RuleError> {
        let CommandContext {
            room_code,
            participant_id,
            action_id,
            expected_version,
        } = command;
        let code = normalize_room_code(room_code);
        let publication_permit = self.try_reserve_publication()?;
        let now = self.now();
        let mut rooms = self.inner.rooms.lock();
        let room = rooms.get_mut(&code).ok_or_else(room_not_found)?;
        let key = format!("{participant_id}:{action_id}");

        if let Some(&version) = room.processed_commands.get(&key) {
            return Ok(CommandResult {
                version,
                duplicate: true,
                publication: self.enqueue_flush(publication_permit),
            });
        }
        if room.version != expected_version {
            return Err(
                RuleError::new("STALE_STATE", "客户端状态已经过期，请先同步最新房间状态")
                    .with_details(json!({ "expectedVersion": room.version })),
            );
        }

        let events = operation(room, participant_id)?;
        room.version += 1;
        room.last_activity_at = now;
        room.processed_commands.insert(key, room.version);
        while room.processed_commands.len() > MAX_PROCESSED_COMMANDS {
            room.processed_commands.shift_remove_index(0);
        }
        let publication = self.enqueue_update(room, events, publication_permit);
        Ok(CommandResult {
            version: room.version,
            duplicate: false,
            publication,
        })
    }

    pub async fn remove_expired(&self) -> Vec<String> {
        let now = self.now();
        let room_codes: Vec<String> = self.inner.rooms.lock().keys().cloned().collect();
        let mut deleted = Vec::new();

        for code in room_codes {
            let publication_permit = match self.reserve_publication().await {
                Ok(permit) => permit,
                Err(error) => {
                    tracing::error!(code = %error.code, "cannot publish room cleanup");
                    break;
                }
            };
            let mut rooms = self.inner.rooms.lock();
            let Some(room) = rooms.get_mut(&code) else {
                continue;
            };
            let version_before_cleanup = room.version;
            if room.phase == RoomPhase::Lobby {
                let expired: Vec<PlayerId> = room
                    .participants
                    .values()
                    .filter(|participant| {
                        !participant.connected()
                            && participant.disconnected_at.is_some_and(|disconnected_at| {
                                now.saturating_sub(disconnected_at) >= self.inner.reconnect_grace_ms
                            })
                    })
                    .map(|participant| participant.id.clone())
                    .collect();
                for participant_id in expired {
                    remove_participant(room, &participant_id);
                }
            }

            let any_connected = room.participants.values().any(Participant::connected);
            let should_delete = !any_connected
                && now.saturating_sub(room.last_activity_at) >= self.inner.room_idle_ttl_ms;
            if !should_delete && room.version != version_before_cleanup {
                drop(self.enqueue_update(room, Vec::new(), publication_permit));
            } else {
                drop(publication_permit);
            }

            if should_delete {
                rooms.shift_remove(&code);
                deleted.push(code);
            }
        }
        deleted
    }
}

impl Default for RoomService {
    fn default() -> Self {
        Self::new(90_000, 600_000)
    }
}

fn system_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is before the Unix epoch")
        .as_millis() as u64
}

fn normalize_room_code(code: &str) -> String {
    code.trim().to_uppercase()
}

fn room_not_found() -> RuleError {
    RuleError::new("ROOM_NOT_FOUND", "房间不存在或已过期")
}

fn validate_display_name(display_name: &str) -> Result<String, RuleError> {
    let trimmed = display_name.trim();
    let length = trimmed.encode_utf16().count();
    if !(1..=20).contains(&length) {
        return Err(RuleError::new(
            "INVALID_DISPLAY_NAME",
            "临时用户名长度必须为 1 至 20 个字符",
        ));
    }
    Ok(trimmed.to_owned())
}

fn generate_room_code(rooms: &IndexMap<String, RoomState>) -> Result<String, RuleError> {
    for _ in 0..100 {
        let code: String = (0..6)
            .map(|_| {
                let index = rand::random_range(0..ROOM_CODE_ALPHABET.len());
                ROOM_CODE_ALPHABET[index] as char
            })
            .collect();
        if !rooms.contains_key(&code) {
            return Ok(code);
        }
    }
    Err(RuleError::internal(
        "Unable to generate a unique room code after 100 attempts",
    ))
}

fn create_participant(
    display_name: String,
    role: ParticipantRole,
    seat: Option<Seat>,
    now: u64,
) -> Participant {
    let mut token = [0_u8; 32];
    rand::fill(&mut token);
    Participant {
        id: Uuid::new_v4().to_string(),
        display_name,
        reconnect_token: URL_SAFE_NO_PAD.encode(token),
        role,
        seat,
        ready: false,
        socket_ids: Default::default(),
        disconnected_at: Some(now),
        joined_at: now,
    }
}

fn credentials(room: &RoomState, participant: &Participant) -> ParticipantCredentials {
    ParticipantCredentials {
        room_code: room.code.clone(),
        participant_id: participant.id.clone(),
        reconnect_token: participant.reconnect_token.clone(),
        role: participant.role,
        seat: participant.seat,
    }
}

fn participant<'a>(
    room: &'a RoomState,
    participant_id: &str,
) -> Result<&'a Participant, RuleError> {
    room.participants
        .get(participant_id)
        .ok_or_else(|| RuleError::new("PARTICIPANT_NOT_FOUND", "参与者不在房间中"))
}

fn seated_player<'a>(
    room: &'a RoomState,
    participant_id: &str,
) -> Result<&'a Participant, RuleError> {
    let participant = participant(room, participant_id)?;
    if participant.role != ParticipantRole::Player || participant.seat.is_none() {
        return Err(RuleError::new(
            "SPECTATOR_CANNOT_PLAY",
            "旁观者不能执行玩家操作",
        ));
    }
    Ok(participant)
}

fn seated_player_mut<'a>(
    room: &'a mut RoomState,
    participant_id: &str,
) -> Result<&'a mut Participant, RuleError> {
    let participant = room
        .participants
        .get_mut(participant_id)
        .ok_or_else(|| RuleError::new("PARTICIPANT_NOT_FOUND", "参与者不在房间中"))?;
    if participant.role != ParticipantRole::Player || participant.seat.is_none() {
        return Err(RuleError::new(
            "SPECTATOR_CANNOT_PLAY",
            "旁观者不能执行玩家操作",
        ));
    }
    Ok(participant)
}

fn active_match(room: &mut RoomState) -> Result<&mut MatchState, RuleError> {
    room.match_state
        .as_mut()
        .ok_or_else(|| RuleError::new("NO_ACTIVE_MATCH", "当前没有进行中的一局牌"))
}

fn active_match_for_player<'a>(
    room: &'a mut RoomState,
    participant_id: &str,
) -> Result<(&'a mut MatchState, Seat), RuleError> {
    require_all_players_connected(room)?;
    let seat = seated_player(room, participant_id)?
        .seat
        .expect("seated player has a seat");
    Ok((active_match(room)?, seat))
}

fn reset_match(room: &mut RoomState) {
    room.match_state = None;
    room.phase = RoomPhase::Lobby;
    room.participants
        .values_mut()
        .for_each(|participant| participant.ready = false);
}

fn require_host(room: &RoomState, participant_id: &str) -> Result<(), RuleError> {
    if room.host_id != participant_id {
        return Err(RuleError::new("HOST_ONLY", "只有房主可以执行此操作"));
    }
    Ok(())
}

fn require_all_players_connected(room: &RoomState) -> Result<(), RuleError> {
    for participant_id in &room.seats {
        let connected = participant_id
            .as_ref()
            .and_then(|id| room.participants.get(id))
            .is_some_and(Participant::connected);
        if !connected {
            return Err(RuleError::new(
                "GAME_PAUSED_FOR_RECONNECT",
                "有玩家断线，游戏暂时无法继续",
            ));
        }
    }
    Ok(())
}

fn remove_participant(room: &mut RoomState, participant_id: &str) {
    let Some(participant) = room.participants.shift_remove(participant_id) else {
        return;
    };
    if let Some(seat) = participant.seat {
        room.seats[seat.index()] = None;
    }
    if room.host_id == participant_id
        && let Some(replacement) = room
            .participants
            .values()
            .min_by_key(|participant| participant.joined_at)
    {
        room.host_id = replacement.id.clone();
    }
    room.version += 1;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn first_four_participants_get_seats_and_fifth_is_spectator() {
        let rooms = RoomService::default();
        let first = rooms.create_room("玩家一").unwrap();
        let second = rooms.join_room(&first.room_code, "玩家二").unwrap();
        let third = rooms.join_room(&first.room_code, "玩家三").unwrap();
        let fourth = rooms.join_room(&first.room_code, "玩家四").unwrap();
        let spectator = rooms.join_room(&first.room_code, "旁观者").unwrap();

        assert_eq!(first.seat, Some(Seat::ZERO));
        assert_eq!(second.seat, Some(Seat::ONE));
        assert_eq!(third.seat, Some(Seat::TWO));
        assert_eq!(fourth.seat, Some(Seat::THREE));
        assert_eq!(spectator.role, ParticipantRole::Spectator);
        assert_eq!(spectator.seat, None);
    }

    #[test]
    fn display_name_limit_counts_utf16_code_units() {
        let rooms = RoomService::default();
        assert!(rooms.create_room(&"😀".repeat(10)).is_ok());
        let error = rooms.create_room(&"😀".repeat(11)).unwrap_err();
        assert_eq!(error.code, "INVALID_DISPLAY_NAME");
    }

    #[test]
    fn duplicate_command_wins_before_stale_version_check() {
        let rooms = RoomService::default();
        let host = rooms.create_room("甲").unwrap();
        let result = rooms.set_ready(host.command("action-01", 1), true).unwrap();
        assert_eq!(result.version, 2);

        let duplicate = rooms
            .set_ready(host.command("action-01", 0), false)
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.version, 2);
        assert!(
            rooms.require_room(&host.room_code).unwrap().participants[&host.participant_id].ready
        );
    }

    #[test]
    fn player_can_move_to_any_empty_seat_and_must_ready_again() {
        let rooms = RoomService::default();
        let host = rooms.create_room("甲").unwrap();
        let player = rooms.join_room(&host.room_code, "乙").unwrap();
        rooms
            .set_ready(player.command("ready-001", 2), true)
            .unwrap();

        let result = rooms
            .change_seat(player.command("change-01", 3), Seat::THREE)
            .unwrap();

        assert_eq!(result.version, 4);
        let room = rooms.require_room(&host.room_code).unwrap();
        let moved = &room.participants[&player.participant_id];
        assert_eq!(room.seats[1], None);
        assert_eq!(room.seats[3], Some(player.participant_id));
        assert_eq!(moved.role, ParticipantRole::Player);
        assert_eq!(moved.seat, Some(Seat::THREE));
        assert!(!moved.ready);
    }

    #[test]
    fn occupied_seat_cannot_be_selected() {
        let rooms = RoomService::default();
        let host = rooms.create_room("甲").unwrap();
        let player = rooms.join_room(&host.room_code, "乙").unwrap();

        let error = rooms
            .change_seat(host.command("change-01", 2), Seat::ONE)
            .unwrap_err();

        assert_eq!(error.code, "SEAT_OCCUPIED");
        let room = rooms.require_room(&host.room_code).unwrap();
        assert_eq!(room.version, 2);
        assert_eq!(room.seats[0], Some(host.participant_id));
        assert_eq!(room.seats[1], Some(player.participant_id));
    }

    #[tokio::test]
    async fn spectator_can_take_a_seat_released_during_the_lobby() {
        let now = Arc::new(AtomicU64::new(1_000));
        let clock = Arc::clone(&now);
        let rooms = RoomService::with_clock(100, 10_000, move || clock.load(Ordering::Relaxed));
        let host = rooms.create_room("甲").unwrap();
        let second = rooms.join_room(&host.room_code, "乙").unwrap();
        let third = rooms.join_room(&host.room_code, "丙").unwrap();
        let expired = rooms.join_room(&host.room_code, "丁").unwrap();
        let spectator = rooms.join_room(&host.room_code, "观众").unwrap();

        for (index, participant) in [&host, &second, &third, &spectator].into_iter().enumerate() {
            let socket_id = format!("socket-{index}");
            rooms.connect(participant, &socket_id).unwrap();
        }
        now.store(1_100, Ordering::Relaxed);
        assert!(rooms.remove_expired().await.is_empty());
        let room = rooms.require_room(&host.room_code).unwrap();
        assert!(!room.participants.contains_key(&expired.participant_id));
        assert_eq!(room.seats[3], None);

        rooms
            .change_seat(spectator.command("change-01", room.version), Seat::THREE)
            .unwrap();

        let room = rooms.require_room(&host.room_code).unwrap();
        let seated = &room.participants[&spectator.participant_id];
        assert_eq!(room.seats[3], Some(spectator.participant_id));
        assert_eq!(seated.role, ParticipantRole::Player);
        assert_eq!(seated.seat, Some(Seat::THREE));
    }

    #[test]
    fn one_identity_has_a_bounded_number_of_live_sockets() {
        let rooms = RoomService::default();
        let host = rooms.create_room("甲").unwrap();
        for index in 0..MAX_SOCKETS_PER_PARTICIPANT {
            rooms.connect(&host, &format!("socket-{index}")).unwrap();
        }

        let error = rooms.connect(&host, "one-too-many").unwrap_err();
        assert_eq!(error.code, "TOO_MANY_CONNECTIONS");
    }

    #[test]
    fn full_publication_queue_rejects_before_state_commit() {
        let rooms = RoomService::default();
        for index in 0..PUBLICATION_QUEUE_CAPACITY {
            rooms.create_room(&format!("玩家{index}")).unwrap();
        }
        let room_count = rooms.inner.rooms.lock().len();

        let error = rooms.create_room("队列已满").unwrap_err();
        assert_eq!(error.code, "SERVER_BUSY");
        assert_eq!(rooms.inner.rooms.lock().len(), room_count);
    }

    #[tokio::test]
    async fn cleanup_releases_lobby_seat_and_moves_host() {
        let now = Arc::new(AtomicU64::new(1_000));
        let clock = Arc::clone(&now);
        let rooms = RoomService::with_clock(100, 10_000, move || clock.load(Ordering::Relaxed));
        let host = rooms.create_room("甲").unwrap();
        let replacement = rooms.join_room(&host.room_code, "乙").unwrap();
        rooms.connect(&replacement, "replacement-socket").unwrap();

        now.store(1_100, Ordering::Relaxed);
        assert!(rooms.remove_expired().await.is_empty());
        let room = rooms.require_room(&host.room_code).unwrap();
        assert_eq!(room.host_id, replacement.participant_id);
        assert_eq!(room.seats[0], None);
        assert_eq!(room.seats[1], Some(replacement.participant_id));
    }
}
