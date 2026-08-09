use std::{
    collections::HashMap,
    fmt,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use socketioxide::{
    ParserError, SocketIo,
    adapter::LocalAdapter,
    extract::{AckSender, SocketRef, TryData},
    handler::ConnectHandler,
    socket::Socket,
};
use socketioxide_core::Value as SocketValue;
use tokio::{sync::mpsc, task::JoinHandle};
use uuid::Uuid;

use crate::{
    domain::{
        errors::RuleError,
        types::{CardRank, CombinationDeclaration, CombinationKind, OrdinaryRank},
    },
    rooms::{
        room_service::RoomService,
        types::{CommandResult, PublicationMessage, RoomState},
    },
    transport::http::{
        ErrorPayload, internal_error_payload, invalid_message, rule_error_payload,
        validated_socket_room_code,
    },
    transport::{client_ip::client_ip, utf16_len},
    views::room_view::create_room_view,
};

const SOCKET_RATE_WINDOW: Duration = Duration::from_secs(10);
const SOCKET_RATE_MAX: u32 = 60;
const ROOM_RATE_MAX: u32 = 120;
const GLOBAL_RATE_MAX: u32 = 600;
const RATE_STATE_RETENTION: Duration = Duration::from_secs(10 * 60);
const HANDSHAKE_RATE_WINDOW: Duration = Duration::from_secs(60);
const HANDSHAKE_RATE_MAX: u32 = 240;

#[derive(Clone, Debug)]
struct SocketIdentity {
    room_code: String,
    room_instance_id: Uuid,
    participant_id: String,
    reconnect_token: String,
}

#[derive(Debug)]
struct RateWindow {
    started_at: Instant,
    last_seen: Instant,
    count: u32,
}

#[derive(Clone, Debug)]
struct SocketRateState {
    identity: Arc<Mutex<RateWindow>>,
    room: Arc<Mutex<RateWindow>>,
    global: Arc<Mutex<RateWindow>>,
}

struct SocketRateRegistryInner {
    identities: HashMap<String, Arc<Mutex<RateWindow>>>,
    rooms: HashMap<Uuid, Arc<Mutex<RateWindow>>>,
    global: Arc<Mutex<RateWindow>>,
}

#[derive(Clone)]
struct SocketRateRegistry(Arc<Mutex<SocketRateRegistryInner>>);

impl Default for SocketRateRegistry {
    fn default() -> Self {
        let now = Instant::now();
        Self(Arc::new(Mutex::new(SocketRateRegistryInner {
            identities: HashMap::new(),
            rooms: HashMap::new(),
            global: new_rate_window(now),
        })))
    }
}

impl SocketRateRegistry {
    fn state_for(&self, room_instance_id: Uuid, participant_id: &str) -> SocketRateState {
        let now = Instant::now();
        let mut states = self.0.lock();
        states.identities.retain(|_, state| {
            Arc::strong_count(state) > 1
                || now.duration_since(state.lock().last_seen) < RATE_STATE_RETENTION
        });
        states.rooms.retain(|_, state| {
            Arc::strong_count(state) > 1
                || now.duration_since(state.lock().last_seen) < RATE_STATE_RETENTION
        });
        let key = format!("{}:{participant_id}", room_instance_id.simple());
        let identity = Arc::clone(
            states
                .identities
                .entry(key)
                .or_insert_with(|| new_rate_window(now)),
        );
        let room = Arc::clone(
            states
                .rooms
                .entry(room_instance_id)
                .or_insert_with(|| new_rate_window(now)),
        );
        identity.lock().last_seen = now;
        room.lock().last_seen = now;
        SocketRateState {
            identity,
            room,
            global: Arc::clone(&states.global),
        }
    }
}

fn new_rate_window(now: Instant) -> Arc<Mutex<RateWindow>> {
    Arc::new(Mutex::new(RateWindow {
        started_at: now,
        last_seen: now,
        count: 0,
    }))
}

#[derive(Clone, Default)]
struct HandshakeRateRegistry(Arc<Mutex<HashMap<Option<IpAddr>, RateWindow>>>);

impl HandshakeRateRegistry {
    fn consume(&self, peer_ip: Option<IpAddr>) -> Result<(), ConnectRefusal> {
        let now = Instant::now();
        let mut states = self.0.lock();
        states.retain(|_, state| now.duration_since(state.last_seen) < RATE_STATE_RETENTION);
        let state = states.entry(peer_ip).or_insert(RateWindow {
            started_at: now,
            last_seen: now,
            count: 0,
        });
        if now.duration_since(state.started_at) >= HANDSHAKE_RATE_WINDOW {
            state.started_at = now;
            state.count = 0;
        }
        state.last_seen = now;
        state.count = state.count.saturating_add(1);
        if state.count > HANDSHAKE_RATE_MAX {
            return Err(ConnectRefusal(rule_error_payload(RuleError::new(
                "RATE_LIMITED",
                "连接尝试过于频繁，请稍后重试",
            ))));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SocketAuth {
    room_code: String,
    participant_id: String,
    reconnect_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadyPayload {
    action_id: String,
    version: u64,
    ready: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SimpleActionPayload {
    action_id: String,
    version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeclarationPayload {
    kind: CombinationKind,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    primary_rank: Option<CardRank>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    sequence_top: Option<OrdinaryRank>,
}

impl From<DeclarationPayload> for CombinationDeclaration {
    fn from(value: DeclarationPayload) -> Self {
        Self {
            kind: value.kind,
            primary_rank: value.primary_rank,
            sequence_top: value.sequence_top,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlayCardsPayload {
    action_id: String,
    version: u64,
    card_ids: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    declaration: Option<DeclarationPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CardActionPayload {
    action_id: String,
    version: u64,
    card_id: String,
}

trait ValidatePayload: Sized {
    fn validate(self) -> Result<Self, ErrorPayload>;
}

/// Serde normally treats an explicit JSON `null` like an omitted `Option`.
/// Zod's `.optional()` did not: a present field still had to contain the
/// declared value type. Keep that distinction at the wire boundary.
fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl ValidatePayload for ReadyPayload {
    fn validate(self) -> Result<Self, ErrorPayload> {
        validate_action_id(&self.action_id)?;
        Ok(self)
    }
}

impl ValidatePayload for SimpleActionPayload {
    fn validate(self) -> Result<Self, ErrorPayload> {
        validate_action_id(&self.action_id)?;
        Ok(self)
    }
}

impl ValidatePayload for PlayCardsPayload {
    fn validate(self) -> Result<Self, ErrorPayload> {
        validate_action_id(&self.action_id)?;
        if !(1..=10).contains(&self.card_ids.len()) {
            return Err(invalid_message("cardIds must contain 1 to 10 cards"));
        }
        for card_id in &self.card_ids {
            validate_card_id(card_id)?;
        }
        Ok(self)
    }
}

impl ValidatePayload for CardActionPayload {
    fn validate(self) -> Result<Self, ErrorPayload> {
        validate_action_id(&self.action_id)?;
        validate_card_id(&self.card_id)?;
        Ok(self)
    }
}

#[derive(Debug, Serialize)]
struct ErrorAck {
    ok: bool,
    error: ErrorPayload,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandAck {
    ok: bool,
    version: u64,
    duplicate: bool,
}

#[derive(Debug, Serialize)]
struct SyncAck {
    ok: bool,
    version: u64,
    snapshot: Value,
}

#[derive(Debug)]
struct ConnectRefusal(ErrorPayload);

impl fmt::Display for ConnectRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_json::to_string(&self.0) {
            Ok(serialized) => formatter.write_str(&serialized),
            Err(_) => {
                formatter.write_str(r#"{"code":"INTERNAL_ERROR","message":"服务器内部错误"}"#)
            }
        }
    }
}

impl std::error::Error for ConnectRefusal {}

/// Socketioxide's closure-based connect handlers are spawned after the CONNECT packet is sent.
/// This small handler keeps the lifecycle-critical setup synchronous, so a client's first event
/// or disconnect cannot overtake listener registration and room bookkeeping.
#[derive(Clone)]
struct SynchronousConnectHandler {
    rooms: RoomService,
}

impl ConnectHandler<LocalAdapter, ()> for SynchronousConnectHandler {
    fn call(&self, socket: Arc<Socket<LocalAdapter>>, _auth: Option<SocketValue>) {
        handle_connection(SocketRef::from(socket), self.rooms.clone());
    }
}

pub fn attach(io: &SocketIo, rooms: RoomService, trust_proxy: bool) -> JoinHandle<()> {
    let publications = rooms
        .take_publications()
        .expect("the room publication stream must only be attached once");
    let publication_io = io.clone();
    let publication_task =
        tokio::spawn(async move { dispatch_publications(publication_io, publications).await });

    let connection_handler = SynchronousConnectHandler {
        rooms: rooms.clone(),
    };

    let authentication_rooms = rooms;
    let rate_registry = SocketRateRegistry::default();
    let handshake_rates = HandshakeRateRegistry::default();
    let authentication = move |socket: SocketRef, TryData(auth): TryData<SocketAuth>| {
        let rooms = authentication_rooms.clone();
        let rate_registry = rate_registry.clone();
        let handshake_rates = handshake_rates.clone();
        async move {
            authenticate_connection(
                socket,
                rooms,
                rate_registry,
                handshake_rates,
                trust_proxy,
                auth,
            )
            .await
        }
    };

    io.ns("/", connection_handler.with(authentication));
    publication_task
}

async fn authenticate_connection(
    socket: SocketRef,
    rooms: RoomService,
    rate_registry: SocketRateRegistry,
    handshake_rates: HandshakeRateRegistry,
    trust_proxy: bool,
    auth: Result<SocketAuth, ParserError>,
) -> Result<(), ConnectRefusal> {
    let request = socket.req_parts();
    let peer_ip = client_ip(&request.headers, &request.extensions, trust_proxy);
    handshake_rates.consume(peer_ip)?;

    let auth = auth.map_err(|error| ConnectRefusal(invalid_message(error.to_string())))?;
    let room_code = validated_socket_room_code(auth.room_code).map_err(ConnectRefusal)?;
    if Uuid::parse_str(&auth.participant_id).is_err() {
        return Err(ConnectRefusal(invalid_message(
            "participantId must be a UUID",
        )));
    }
    if !(32..=128).contains(&utf16_len(&auth.reconnect_token)) {
        return Err(ConnectRefusal(invalid_message(
            "reconnectToken must contain 32 to 128 characters",
        )));
    }

    let room_instance_id = rooms
        .authenticate(&room_code, &auth.participant_id, &auth.reconnect_token)
        .map_err(|error| ConnectRefusal(rule_error_payload(error)))?;

    let identity = SocketIdentity {
        room_code,
        room_instance_id,
        participant_id: auth.participant_id,
        reconnect_token: auth.reconnect_token,
    };
    let rate_state = rate_registry.state_for(identity.room_instance_id, &identity.participant_id);
    consume_rate_state(&rate_state).map_err(ConnectRefusal)?;
    socket.extensions.insert(identity);
    socket.extensions.insert(rate_state);
    Ok(())
}

fn handle_connection(socket: SocketRef, rooms: RoomService) {
    let Some(identity) = socket.extensions.get::<SocketIdentity>() else {
        let _ = socket.clone().disconnect();
        return;
    };
    let identity = identity.clone();

    register_message_handlers(&socket, &rooms);

    let disconnect_rooms = rooms.clone();
    socket.on_disconnect(move |socket: SocketRef| {
        let rooms = disconnect_rooms.clone();
        async move { handle_disconnect(socket, rooms).await }
    });

    // Join before committing the connection so the commit-ordered initial
    // snapshot includes this socket in its target set.
    socket.join(identity_channel(&identity));

    let socket_id = socket.id.to_string();
    let room = match rooms.connect_socket(
        &identity.room_code,
        &identity.participant_id,
        &identity.reconnect_token,
        &socket_id,
    ) {
        Ok(room) => room,
        Err(error) => {
            tracing::warn!(code = %error.code, "socket disappeared between authentication and connection");
            let _ = socket.clone().disconnect();
            return;
        }
    };
    if room.instance_id != identity.room_instance_id {
        tracing::warn!(room_code = %room.code, "room instance changed during socket connection");
        let _ = socket.clone().disconnect();
    }
}

fn register_message_handlers(socket: &SocketRef, rooms: &RoomService) {
    let sync_rooms = rooms.clone();
    socket.on("room.sync", move |socket: SocketRef, ack: AckSender| {
        let rooms = sync_rooms.clone();
        async move { handle_sync(socket, ack, rooms).await }
    });

    register_command_handler::<ReadyPayload, _>(
        socket,
        rooms,
        "room.ready",
        |rooms, identity, payload| {
            rooms.set_ready(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
                payload.ready,
            )
        },
    );

    register_command_handler::<SimpleActionPayload, _>(
        socket,
        rooms,
        "match.start",
        |rooms, identity, payload| {
            rooms.start_match(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
            )
        },
    );

    register_command_handler::<PlayCardsPayload, _>(
        socket,
        rooms,
        "round.play",
        |rooms, identity, payload| {
            let declaration = payload.declaration.map(CombinationDeclaration::from);
            rooms.play_cards(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
                &payload.card_ids,
                declaration.as_ref(),
            )
        },
    );

    register_command_handler::<SimpleActionPayload, _>(
        socket,
        rooms,
        "round.pass",
        |rooms, identity, payload| {
            rooms.pass(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
            )
        },
    );
    register_command_handler::<SimpleActionPayload, _>(
        socket,
        rooms,
        "round.next",
        |rooms, identity, payload| {
            rooms.start_next_round(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
            )
        },
    );
    register_command_handler::<SimpleActionPayload, _>(
        socket,
        rooms,
        "match.abort",
        |rooms, identity, payload| {
            rooms.abort_match(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
            )
        },
    );

    register_command_handler::<CardActionPayload, _>(
        socket,
        rooms,
        "tribute.give",
        |rooms, identity, payload| {
            rooms.give_tribute(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
                &payload.card_id,
            )
        },
    );

    register_command_handler::<CardActionPayload, _>(
        socket,
        rooms,
        "tribute.return",
        |rooms, identity, payload| {
            rooms.return_tribute(
                &identity.room_code,
                &identity.participant_id,
                &payload.action_id,
                payload.version,
                &payload.card_id,
            )
        },
    );
}

fn register_command_handler<T, F>(
    socket: &SocketRef,
    rooms: &RoomService,
    event: &'static str,
    operation: F,
) where
    T: DeserializeOwned + ValidatePayload + Send + Sync + 'static,
    F: Fn(&RoomService, &SocketIdentity, T) -> Result<CommandResult, RuleError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let rooms = rooms.clone();
    socket.on(
        event,
        move |socket: SocketRef, TryData(payload): TryData<T>, ack: AckSender| {
            let rooms = rooms.clone();
            let operation = operation.clone();
            async move {
                handle_command(socket, ack, rooms, payload, operation).await;
            }
        },
    );
}

async fn handle_sync(socket: SocketRef, ack: AckSender, rooms: RoomService) {
    if let Err(error) = consume_rate_limit(&socket) {
        send_error_ack(ack, error);
        return;
    }
    let Some(identity) = socket.extensions.get::<SocketIdentity>() else {
        send_error_ack(ack, internal_error_payload());
        return;
    };
    let identity = identity.clone();
    let barrier = match rooms.publication_barrier(&identity.room_code).await {
        Ok(barrier) if barrier.room.instance_id == identity.room_instance_id => barrier,
        Ok(barrier) => {
            let _ = barrier.release.send(());
            send_error_ack(
                ack,
                rule_error_payload(RuleError::new("ROOM_NOT_FOUND", "房间不存在或已过期")),
            );
            return;
        }
        Err(error) => {
            send_error_ack(ack, rule_error_payload(error));
            return;
        }
    };
    let crate::rooms::types::PublicationBarrier {
        room,
        ready,
        release,
    } = barrier;
    if ready.await.is_err() {
        send_error_ack(ack, internal_error_payload());
        return;
    }
    let response = SyncAck {
        ok: true,
        version: room.version,
        snapshot: create_room_view(&room, Some(&identity.participant_id)),
    };
    if let Err(error) = ack.send(&response) {
        tracing::debug!(%error, "failed to acknowledge room.sync");
    }
    let _ = release.send(());
}

async fn handle_command<T, F>(
    socket: SocketRef,
    ack: AckSender,
    rooms: RoomService,
    payload: Result<T, ParserError>,
    operation: F,
) where
    T: ValidatePayload,
    F: FnOnce(&RoomService, &SocketIdentity, T) -> Result<CommandResult, RuleError>,
{
    if let Err(error) = consume_rate_limit(&socket) {
        send_error_ack(ack, error);
        return;
    }
    let payload = match payload {
        Ok(payload) => match payload.validate() {
            Ok(payload) => payload,
            Err(error) => {
                send_error_ack(ack, error);
                return;
            }
        },
        Err(error) => {
            send_error_ack(ack, invalid_message(error.to_string()));
            return;
        }
    };
    let Some(identity) = socket.extensions.get::<SocketIdentity>() else {
        send_error_ack(ack, internal_error_payload());
        return;
    };
    let identity = identity.clone();

    match operation(&rooms, &identity, payload) {
        Ok(result) => {
            if result.publication.await.is_err() {
                send_error_ack(ack, internal_error_payload());
                return;
            }
            let response = CommandAck {
                ok: true,
                version: result.version,
                duplicate: result.duplicate,
            };
            if let Err(error) = ack.send(&response) {
                tracing::debug!(%error, "failed to acknowledge socket command");
            }
        }
        Err(error) => send_error_ack(ack, rule_error_payload(error)),
    }
}

async fn handle_disconnect(socket: SocketRef, rooms: RoomService) {
    let Some(identity) = socket.extensions.get::<SocketIdentity>() else {
        return;
    };
    rooms
        .disconnect_socket(
            &identity.room_code,
            &identity.participant_id,
            &socket.id.to_string(),
        )
        .await;
}

async fn dispatch_publications(io: SocketIo, mut publications: mpsc::Receiver<PublicationMessage>) {
    while let Some(publication) = publications.recv().await {
        match publication {
            PublicationMessage::Update {
                room,
                events,
                completed,
            } => {
                let channel = room_channel(&room);
                for event in events {
                    if let Err(error) = io
                        .to(channel.clone())
                        .emit(event.event_type, &event.payload)
                        .await
                    {
                        tracing::debug!(
                            %error,
                            event = event.event_type,
                            "failed to broadcast room event"
                        );
                    }
                }
                broadcast_snapshots(&io, &room);
                if let Some(completed) = completed {
                    let _ = completed.send(());
                }
            }
            PublicationMessage::Barrier {
                room: _,
                ready,
                release,
            } => {
                if ready.send(()).is_ok() {
                    let _ = release.await;
                }
            }
            PublicationMessage::Flush { completed } => {
                let _ = completed.send(());
            }
        }
    }
}

fn broadcast_snapshots(io: &SocketIo, room: &RoomState) {
    let mut snapshots = HashMap::<String, Value>::new();
    for socket in io.to(room_channel(room)).sockets() {
        let Some(identity) = socket.extensions.get::<SocketIdentity>() else {
            continue;
        };
        if identity.room_code != room.code || identity.room_instance_id != room.instance_id {
            continue;
        }
        let snapshot = snapshots
            .entry(identity.participant_id.clone())
            .or_insert_with(|| create_room_view(room, Some(&identity.participant_id)));
        if let Err(error) = socket.emit("room.snapshot", snapshot) {
            tracing::debug!(%error, "failed to emit personalized room snapshot");
        }
    }
}

fn consume_rate_limit(socket: &SocketRef) -> Result<(), ErrorPayload> {
    let Some(rate_state) = socket.extensions.get::<SocketRateState>() else {
        return Err(internal_error_payload());
    };
    consume_rate_state(&rate_state)
}

fn consume_rate_state(rate_state: &SocketRateState) -> Result<(), ErrorPayload> {
    consume_rate_window(&rate_state.identity, SOCKET_RATE_MAX)?;
    consume_rate_window(&rate_state.room, ROOM_RATE_MAX)?;
    consume_rate_window(&rate_state.global, GLOBAL_RATE_MAX)
}

fn consume_rate_window(rate_window: &Mutex<RateWindow>, maximum: u32) -> Result<(), ErrorPayload> {
    let now = Instant::now();
    let mut window = rate_window.lock();
    if now.duration_since(window.started_at) >= SOCKET_RATE_WINDOW {
        window.started_at = now;
        window.count = 0;
    }
    window.last_seen = now;
    window.count = window.count.saturating_add(1);
    if window.count > maximum {
        return Err(rule_error_payload(RuleError::new(
            "RATE_LIMITED",
            "操作过于频繁，请稍后重试",
        )));
    }
    Ok(())
}

fn send_error_ack(ack: AckSender, error: ErrorPayload) {
    if let Err(send_error) = ack.send(&ErrorAck { ok: false, error }) {
        tracing::debug!(%send_error, "failed to acknowledge socket error");
    }
}

fn validate_action_id(value: &str) -> Result<(), ErrorPayload> {
    if !(8..=128).contains(&utf16_len(value)) {
        return Err(invalid_message("actionId must contain 8 to 128 characters"));
    }
    Ok(())
}

fn validate_card_id(value: &str) -> Result<(), ErrorPayload> {
    if !(1..=80).contains(&utf16_len(value)) {
        return Err(invalid_message("cardId must contain 1 to 80 characters"));
    }
    Ok(())
}

fn room_channel(room: &RoomState) -> String {
    format!("room:{}:{}", room.code, room.instance_id.simple())
}

fn identity_channel(identity: &SocketIdentity) -> String {
    format!(
        "room:{}:{}",
        identity.room_code,
        identity.room_instance_id.simple()
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn optional_command_fields_reject_explicit_null() {
        let base = json!({
            "actionId": "action-01",
            "version": 1,
            "cardIds": ["0:spade:3"]
        });
        assert!(serde_json::from_value::<PlayCardsPayload>(base).is_ok());
        assert!(
            serde_json::from_value::<PlayCardsPayload>(json!({
                "actionId": "action-01",
                "version": 1,
                "cardIds": ["0:spade:3"],
                "declaration": null
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<DeclarationPayload>(json!({
                "kind": "single",
                "primaryRank": null
            }))
            .is_err()
        );
    }

    #[test]
    fn idle_live_sockets_keep_sharing_aggregate_rate_windows() {
        let registry = SocketRateRegistry::default();
        let room_instance_id = Uuid::new_v4();
        let first_socket = registry.state_for(room_instance_id, "participant-1");
        let expired = Instant::now() - RATE_STATE_RETENTION - Duration::from_secs(1);
        first_socket.identity.lock().last_seen = expired;
        first_socket.room.lock().last_seen = expired;

        let reconnect = registry.state_for(room_instance_id, "participant-1");

        assert!(Arc::ptr_eq(&first_socket.identity, &reconnect.identity));
        assert!(Arc::ptr_eq(&first_socket.room, &reconnect.room));
    }
}
