import type { MatchState } from "../domain/match.js";
import type { PlayerId, Seat, Team } from "../domain/types.js";

export type ParticipantRole = "player" | "spectator";

export interface Participant {
  id: PlayerId;
  displayName: string;
  reconnectToken: string;
  role: ParticipantRole;
  seat: Seat | null;
  ready: boolean;
  socketIds: Set<string>;
  disconnectedAt: number | null;
  joinedAt: number;
}

export interface StoredCommandResult {
  version: number;
  events: RoomEvent[];
}

export interface RoomState {
  code: string;
  phase: "lobby" | "playing";
  hostId: PlayerId;
  participants: Map<PlayerId, Participant>;
  seats: [PlayerId | null, PlayerId | null, PlayerId | null, PlayerId | null];
  match: MatchState | null;
  version: number;
  createdAt: number;
  lastActivityAt: number;
  processedCommands: Map<string, StoredCommandResult>;
}

export interface ParticipantCredentials {
  roomCode: string;
  participantId: PlayerId;
  reconnectToken: string;
  role: ParticipantRole;
  seat: Seat | null;
}

export interface RoomEvent {
  type:
    | "participant.joined"
    | "participant.connection"
    | "room.ready"
    | "match.started"
    | "match.aborted"
    | "round.finished"
    | "round.started"
    | "tribute.completed"
    | "match.finished";
  payload: unknown;
}

export interface CommandResult {
  version: number;
  duplicate: boolean;
  events: RoomEvent[];
}

export interface RoundFinishedPayload {
  winnerTeam: Team;
  finishOrder: Seat[];
  doubleLastSeats: Seat[];
  partnerPlacement: 2 | 3 | 4;
  teamLevels: readonly [string, string];
}
