export const ORDINARY_RANKS = [
  "2",
  "3",
  "4",
  "5",
  "6",
  "7",
  "8",
  "9",
  "10",
  "J",
  "Q",
  "K",
  "A",
] as const;

export type OrdinaryRank = (typeof ORDINARY_RANKS)[number];
export type JokerRank = "small-joker" | "big-joker";
export type CardRank = OrdinaryRank | JokerRank;
export type Suit = "heart" | "diamond" | "club" | "spade" | "joker";
export type OrdinarySuit = Exclude<Suit, "joker">;
export type Seat = 0 | 1 | 2 | 3;
export type Team = 0 | 1;
export type PlayerId = string;

export interface Card {
  id: string;
  deckIndex: 0 | 1;
  suit: Suit;
  rank: CardRank;
}

export const COMBINATION_KINDS = [
  "single",
  "pair",
  "triple",
  "full-house",
  "straight",
  "consecutive-pairs",
  "consecutive-triples",
  "bomb",
  "straight-flush",
  "all-joker",
] as const;

export type CombinationKind = (typeof COMBINATION_KINDS)[number];

export interface WildcardAssignment {
  rank: OrdinaryRank;
  suit: OrdinarySuit;
}

export interface Combination {
  kind: CombinationKind;
  size: number;
  primaryRank?: CardRank;
  sequenceTop?: OrdinaryRank;
  suit?: OrdinarySuit;
  wildcardAssignments: Record<string, WildcardAssignment>;
}

export interface CombinationDeclaration {
  kind: CombinationKind;
  primaryRank?: CardRank;
  sequenceTop?: OrdinaryRank;
}

export function teamForSeat(seat: Seat): Team {
  return (seat % 2) as Team;
}

export function partnerSeat(seat: Seat): Seat {
  return ((seat + 2) % 4) as Seat;
}

export function nextSeat(seat: Seat): Seat {
  return ((seat + 1) % 4) as Seat;
}
