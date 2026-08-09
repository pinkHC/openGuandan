use std::{collections::BTreeSet, sync::Arc};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::domain::{match_state::MatchState, types::Seat};

pub type PlayerId = String;

/// Metadata shared by every state-changing room command.
#[derive(Clone, Copy, Debug)]
pub struct CommandContext<'a> {
    pub room_code: &'a str,
    pub participant_id: &'a str,
    pub action_id: &'a str,
    pub expected_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParticipantRole {
    Player,
    Spectator,
}

#[derive(Debug, Clone)]
pub struct Participant {
    pub id: PlayerId,
    pub display_name: String,
    pub reconnect_token: String,
    pub role: ParticipantRole,
    pub seat: Option<Seat>,
    pub ready: bool,
    pub socket_ids: BTreeSet<String>,
    pub disconnected_at: Option<u64>,
    pub joined_at: u64,
}

impl Participant {
    pub fn connected(&self) -> bool {
        !self.socket_ids.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomPhase {
    Lobby,
    Playing,
}

#[derive(Debug, Clone)]
pub struct RoomState {
    /// Distinguishes successive rooms that happen to reuse the same public code.
    pub instance_id: Uuid,
    pub code: String,
    pub phase: RoomPhase,
    pub host_id: PlayerId,
    pub participants: IndexMap<PlayerId, Participant>,
    pub seats: [Option<PlayerId>; 4],
    pub match_state: Option<MatchState>,
    pub version: u64,
    pub last_activity_at: u64,
    pub processed_commands: IndexMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantCredentials {
    pub room_code: String,
    pub participant_id: PlayerId,
    pub reconnect_token: String,
    pub role: ParticipantRole,
    pub seat: Option<Seat>,
}

#[cfg(test)]
impl ParticipantCredentials {
    pub(crate) fn command<'a>(
        &'a self,
        action_id: &'a str,
        expected_version: u64,
    ) -> CommandContext<'a> {
        CommandContext {
            room_code: &self.room_code,
            participant_id: &self.participant_id,
            action_id,
            expected_version,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoomEvent {
    pub event_type: &'static str,
    pub payload: Value,
}

impl RoomEvent {
    pub fn new(event_type: &'static str, payload: Value) -> Self {
        Self {
            event_type,
            payload,
        }
    }
}

pub type PublicationReceipt = oneshot::Receiver<()>;

/// An immutable, commit-ordered item for the transport publisher.
///
/// `Update` publishes domain events followed by the exact post-commit snapshot.
/// A `Fence` signals `ready` after preceding updates and, for a sync barrier,
/// waits for `release` so its acknowledgement cannot be overtaken.
#[derive(Debug)]
pub enum PublicationMessage {
    Update {
        room: Arc<RoomState>,
        events: Vec<RoomEvent>,
        completed: oneshot::Sender<()>,
    },
    Fence {
        ready: oneshot::Sender<()>,
        release: Option<oneshot::Receiver<()>>,
    },
}

#[derive(Debug)]
pub struct PublicationBarrier {
    pub room: Arc<RoomState>,
    pub ready: oneshot::Receiver<()>,
    pub release: oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct CommandResult {
    pub version: u64,
    pub duplicate: bool,
    pub publication: PublicationReceipt,
}
