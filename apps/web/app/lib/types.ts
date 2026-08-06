export type Seat = 0 | 1 | 2 | 3;
export type Team = 0 | 1;
export type OrdinaryRank =
  | "2"
  | "3"
  | "4"
  | "5"
  | "6"
  | "7"
  | "8"
  | "9"
  | "10"
  | "J"
  | "Q"
  | "K"
  | "A";
export type CardRank = OrdinaryRank | "small-joker" | "big-joker";
export type Suit = "heart" | "diamond" | "club" | "spade" | "joker";

export type CombinationKind =
  | "single"
  | "pair"
  | "triple"
  | "full-house"
  | "straight"
  | "consecutive-pairs"
  | "consecutive-triples"
  | "bomb"
  | "straight-flush"
  | "joker-bomb";

export interface Card {
  id: string;
  deckIndex: 0 | 1;
  suit: Suit;
  rank: CardRank;
}

export interface CombinationDeclaration {
  kind: CombinationKind;
  primaryRank?: CardRank;
  sequenceTop?: OrdinaryRank;
}

export interface Combination extends CombinationDeclaration {
  size: number;
  suit?: Exclude<Suit, "joker">;
  wildcardAssignments: Record<string, { rank: OrdinaryRank; suit: Exclude<Suit, "joker"> }>;
}

export interface ParticipantCredentials {
  roomCode: string;
  participantId: string;
  reconnectToken: string;
  role: "player" | "spectator";
  seat: Seat | null;
}

export interface ParticipantView {
  id: string;
  displayName: string;
  role: "player" | "spectator";
  seat: Seat | null;
  ready: boolean;
  connected: boolean;
}

export interface RoundResult {
  winnerTeam: Team;
  finishOrder: Seat[];
  doubleLastSeats: Seat[];
  partnerPlacement: 2 | 3 | 4;
}

export interface TributeView {
  kind: "single" | "double";
  stage: "giving" | "returning";
  previousFirst: Seat;
  previousSecond: Seat | null;
  givers: Seat[];
  receiverForGiver: Record<string, Seat>;
  contributedSeats: Seat[];
  returnedSeats: Seat[];
  contributions: Array<{ seat: Seat; card: Card }>;
  returns: Array<{ seat: Seat; card: Card }>;
}

export interface RoundView {
  number: number;
  phase: "tribute" | "playing";
  levelRank: OrdinaryRank;
  levelOwnerTeam: Team | null;
  turnSeat: Seat;
  currentPlay: { seat: Seat; cards: Card[]; combination: Combination } | null;
  consecutivePasses: number;
  finishOrder: Seat[];
  activeSeats: Seat[];
  handCounts: Record<string, number>;
  tribute: TributeView | null;
}

export interface RoomSnapshot {
  roomCode: string;
  phase: "lobby" | "playing";
  version: number;
  hostId: string;
  seats: [string | null, string | null, string | null, string | null];
  participants: ParticipantView[];
  match: null | {
    phase: "playing" | "between-rounds";
    teamLevels: [OrdinaryRank, OrdinaryRank];
    nextRoundNumber: number;
    previousRoundResult: RoundResult | null;
    currentRound: RoundView | null;
  };
  self: null | {
    participantId: string;
    role: "player" | "spectator";
    seat: Seat | null;
    ready: boolean;
    hand: Card[];
  };
}

export interface ServerError {
  code: string;
  message: string;
  details?: { options?: CombinationDeclaration[]; expectedVersion?: number } | null;
}

export type CommandAck =
  | { ok: true; version: number; duplicate?: boolean; snapshot?: RoomSnapshot }
  | { ok: false; error: ServerError };
