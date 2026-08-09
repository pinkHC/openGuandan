use serde_json::{Value, json};

use crate::{
    domain::{
        cards::sort_cards,
        round::{RoundState, TributeState},
        types::{Card, Seat},
    },
    rooms::types::RoomState,
};

/// Builds the complete public room state, adding only the requesting player's
/// own private hand. Reconnect credentials never enter this representation.
pub fn create_room_view(room: &RoomState, viewer_id: Option<&str>) -> Value {
    let viewer = viewer_id.and_then(|id| room.participants.get(id));
    let current_round = room
        .match_state
        .as_ref()
        .and_then(|match_state| match_state.current_round.as_ref());

    let self_view = viewer.map(|participant| {
        let hand = match (participant.seat, current_round) {
            (Some(seat), Some(round)) => sort_cards(&round.hands[seat.index()], round.level_rank),
            _ => Vec::new(),
        };
        json!({
            "participantId": participant.id,
            "role": participant.role,
            "seat": participant.seat,
            "ready": participant.ready,
            "hand": hand,
        })
    });

    let participants: Vec<Value> = room
        .participants
        .values()
        .map(|participant| {
            json!({
                "id": participant.id,
                "displayName": participant.display_name,
                "role": participant.role,
                "seat": participant.seat,
                "ready": participant.ready,
                "connected": participant.connected(),
            })
        })
        .collect();

    let match_view = room.match_state.as_ref().map(|match_state| {
        json!({
            "phase": match_state.phase,
            "teamLevels": match_state.team_levels,
            "nextRoundNumber": match_state.next_round_number,
            "previousRoundResult": match_state.previous_round_result,
            "currentRound": match_state.current_round.as_ref().map(public_round_view),
        })
    });

    json!({
        "roomCode": room.code,
        "phase": room.phase,
        "version": room.version,
        "hostId": room.host_id,
        "seats": room.seats,
        "participants": participants,
        "match": match_view,
        "self": self_view,
    })
}

fn public_round_view(round: &RoundState) -> Value {
    let active_seats: Vec<Seat> = round.active_seats.iter().copied().collect();
    json!({
        "number": round.number,
        "phase": round.phase,
        "levelRank": round.level_rank,
        "levelOwnerTeam": round.level_owner_team,
        "turnSeat": round.turn_seat,
        "currentPlay": round.current_play,
        "consecutivePasses": round.consecutive_passes,
        "finishOrder": round.finish_order,
        "activeSeats": active_seats,
        "handCounts": {
            "0": round.hands[0].len(),
            "1": round.hands[1].len(),
            "2": round.hands[2].len(),
            "3": round.hands[3].len(),
        },
        "tribute": round.tribute.as_ref().map(tribute_view),
    })
}

fn tribute_view(tribute: &TributeState) -> Value {
    let contributions_complete = tribute.contributions.len() == tribute.givers.len();
    let returns_complete = tribute.stage == crate::domain::round::TributeStage::Returning
        && tribute.returns.len() == tribute.receiver_for_giver.len();

    json!({
        "kind": tribute.kind,
        "stage": tribute.stage,
        "previousFirst": tribute.previous_first,
        "previousSecond": tribute.previous_second,
        "givers": tribute.givers,
        "receiverForGiver": tribute.receiver_for_giver,
        "contributedSeats": tribute.contributions.keys().copied().collect::<Vec<_>>(),
        "returnedSeats": tribute.returns.keys().copied().collect::<Vec<_>>(),
        "contributions": visible_submitted_cards(
            &tribute.contributions,
            tribute.kind == crate::domain::round::TributeKind::Single || contributions_complete,
        ),
        "returns": visible_submitted_cards(&tribute.returns, returns_complete),
    })
}

fn visible_submitted_cards(values: &indexmap::IndexMap<Seat, Card>, reveal: bool) -> Vec<Value> {
    if !reveal {
        return Vec::new();
    }
    values
        .iter()
        .map(|(seat, card)| json!({ "seat": seat, "card": card }))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::rooms::room_service::RoomService;

    use super::*;

    #[test]
    fn public_view_never_contains_credentials() {
        let rooms = RoomService::default();
        let credentials = rooms.create_room("甲").unwrap();
        let room = rooms.require_room(&credentials.room_code).unwrap();
        let serialized = create_room_view(&room, None).to_string();

        assert!(!serialized.contains(&credentials.reconnect_token));
        assert!(create_room_view(&room, None)["self"].is_null());
    }

    #[test]
    fn only_a_seated_viewer_receives_their_own_hand() {
        let rooms = RoomService::default();
        let host = rooms.create_room("甲").unwrap();
        let mut players = vec![host.clone()];
        for name in ["乙", "丙", "丁"] {
            players.push(rooms.join_room(&host.room_code, name).unwrap());
        }
        let spectator = rooms.join_room(&host.room_code, "观众").unwrap();

        for (index, player) in players.iter().enumerate() {
            rooms.connect(player, &format!("view-{index}")).unwrap();
        }
        for (index, player) in players.iter().enumerate() {
            let version = rooms.require_room(&host.room_code).unwrap().version;
            rooms
                .set_ready(
                    player.command(&format!("view-ready-{index}"), version),
                    true,
                )
                .unwrap();
        }
        let version = rooms.require_room(&host.room_code).unwrap().version;
        rooms
            .start_match(host.command("view-start", version))
            .unwrap();

        let room = rooms.require_room(&host.room_code).unwrap();
        let player_view = create_room_view(&room, Some(&host.participant_id));
        let spectator_view = create_room_view(&room, Some(&spectator.participant_id));
        let hand = player_view["self"]["hand"].as_array().unwrap();
        assert_eq!(hand.len(), 27);
        assert_eq!(spectator_view["self"]["hand"], Value::Array(Vec::new()));

        let public_json = spectator_view.to_string();
        for card in hand {
            let id = card["id"].as_str().unwrap();
            assert!(!public_json.contains(id));
        }
    }
}
