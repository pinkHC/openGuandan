import { randomInt } from "node:crypto";
import {
  ORDINARY_RANKS,
  type Card,
  type CardRank,
  type OrdinaryRank,
  type Seat,
  type Suit,
} from "./types.js";

const SUITS = ["heart", "diamond", "club", "spade"] as const;

export function createDeck(): Card[] {
  const cards: Card[] = [];

  for (const deckIndex of [0, 1] as const) {
    for (const suit of SUITS) {
      for (const rank of ORDINARY_RANKS) {
        cards.push({
          id: `${deckIndex}:${suit}:${rank}`,
          deckIndex,
          suit,
          rank,
        });
      }
    }

    cards.push({
      id: `${deckIndex}:joker:small-joker`,
      deckIndex,
      suit: "joker",
      rank: "small-joker",
    });
    cards.push({
      id: `${deckIndex}:joker:big-joker`,
      deckIndex,
      suit: "joker",
      rank: "big-joker",
    });
  }

  return cards;
}

export function shuffleCards(
  cards: readonly Card[],
  randomIndex: (upperExclusive: number) => number = randomInt,
): Card[] {
  const shuffled = [...cards];
  for (let index = shuffled.length - 1; index > 0; index -= 1) {
    const swapIndex = randomIndex(index + 1);
    const current = shuffled[index];
    const target = shuffled[swapIndex];
    if (current === undefined || target === undefined) {
      throw new Error("Invalid shuffle index");
    }
    shuffled[index] = target;
    shuffled[swapIndex] = current;
  }
  return shuffled;
}

export function dealCards(cards: readonly Card[]): Map<Seat, Card[]> {
  if (cards.length !== 108) {
    throw new Error(`Expected 108 cards, received ${cards.length}`);
  }

  const hands = new Map<Seat, Card[]>([
    [0, []],
    [1, []],
    [2, []],
    [3, []],
  ]);

  cards.forEach((card, index) => {
    const seat = (index % 4) as Seat;
    hands.get(seat)?.push(card);
  });

  return hands;
}

export function ordinaryRankValue(rank: OrdinaryRank): number {
  return ORDINARY_RANKS.indexOf(rank) + 2;
}

export function cardRankStrength(rank: CardRank, levelRank: OrdinaryRank): number {
  if (rank === "big-joker") return 17;
  if (rank === "small-joker") return 16;
  if (rank === levelRank) return 15;
  return ordinaryRankValue(rank);
}

export function isWildcard(card: Card, levelRank: OrdinaryRank): boolean {
  return card.suit === "heart" && card.rank === levelRank;
}

export function isOrdinaryCard(
  card: Card,
): card is Card & { rank: OrdinaryRank; suit: Exclude<Suit, "joker"> } {
  return card.suit !== "joker";
}

export function sortCards(cards: readonly Card[], levelRank: OrdinaryRank): Card[] {
  const suitOrder: Record<Suit, number> = {
    diamond: 0,
    club: 1,
    heart: 2,
    spade: 3,
    joker: 4,
  };

  return [...cards].sort((left, right) => {
    const strengthDifference =
      cardRankStrength(left.rank, levelRank) - cardRankStrength(right.rank, levelRank);
    if (strengthDifference !== 0) return strengthDifference;
    const suitDifference = suitOrder[left.suit] - suitOrder[right.suit];
    if (suitDifference !== 0) return suitDifference;
    return left.deckIndex - right.deckIndex;
  });
}
