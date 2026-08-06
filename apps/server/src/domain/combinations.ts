import { cardRankStrength, isWildcard, ordinaryRankValue } from "./cards.js";
import { RuleError } from "./errors.js";
import {
  ORDINARY_RANKS,
  type Card,
  type CardRank,
  type Combination,
  type CombinationDeclaration,
  type CombinationKind,
  type OrdinaryRank,
  type OrdinarySuit,
  type WildcardAssignment,
} from "./types.js";

interface VirtualCard {
  sourceId: string;
  rank: CardRank;
  suit: OrdinarySuit | "joker";
}

const ORDINARY_SUITS: OrdinarySuit[] = ["heart", "diamond", "club", "spade"];

function sequencePatterns(length: number): OrdinaryRank[][] {
  const patterns: OrdinaryRank[][] = [];
  const lowAce = ["A", ...ORDINARY_RANKS.slice(0, length - 1)] as OrdinaryRank[];
  patterns.push(lowAce);

  for (let start = 0; start <= ORDINARY_RANKS.length - length; start += 1) {
    patterns.push(ORDINARY_RANKS.slice(start, start + length) as OrdinaryRank[]);
  }

  return patterns;
}

function countRanks(cards: readonly VirtualCard[]): Map<CardRank, number> {
  const counts = new Map<CardRank, number>();
  for (const card of cards) {
    counts.set(card.rank, (counts.get(card.rank) ?? 0) + 1);
  }
  return counts;
}

function findSequenceTop(
  counts: ReadonlyMap<CardRank, number>,
  sequenceLength: number,
  copiesPerRank: number,
): OrdinaryRank | null {
  for (const pattern of sequencePatterns(sequenceLength)) {
    if (
      counts.size === sequenceLength &&
      pattern.every((rank) => counts.get(rank) === copiesPerRank)
    ) {
      return pattern.at(-1) ?? null;
    }
  }
  return null;
}

function semanticKey(combination: Combination): string {
  return [
    combination.kind,
    combination.size,
    combination.primaryRank ?? "",
    combination.sequenceTop ?? "",
    combination.suit ?? "",
  ].join("|");
}

function classifyResolved(
  cards: readonly VirtualCard[],
  wildcardAssignments: Record<string, WildcardAssignment>,
): Combination[] {
  const combinations: Combination[] = [];
  const counts = countRanks(cards);
  const size = cards.length;
  const ordinaryOnly = cards.every((card) => card.suit !== "joker");

  const add = (combination: Omit<Combination, "wildcardAssignments">): void => {
    combinations.push({ ...combination, wildcardAssignments });
  };

  if (size === 1) {
    const card = cards[0];
    if (card !== undefined) {
      add({ kind: "single", size, primaryRank: card.rank });
    }
  }

  if (size === 2 && counts.size === 1) {
    const primaryRank = cards[0]?.rank;
    if (primaryRank !== undefined) add({ kind: "pair", size, primaryRank });
  }

  if (size === 3 && ordinaryOnly && counts.size === 1) {
    const primaryRank = cards[0]?.rank;
    if (primaryRank !== undefined) add({ kind: "triple", size, primaryRank });
  }

  if (size === 5) {
    for (const [rank, count] of counts) {
      if (count === 3 && rank !== "small-joker" && rank !== "big-joker") {
        const hasPair = [...counts.entries()].some(
          ([pairRank, pairCount]) => pairRank !== rank && pairCount === 2,
        );
        if (hasPair) add({ kind: "full-house", size, primaryRank: rank });
      }
    }

    if (ordinaryOnly) {
      const sequenceTop = findSequenceTop(counts, 5, 1);
      if (sequenceTop !== null) {
        add({ kind: "straight", size, sequenceTop });
        const suit = cards[0]?.suit;
        if (suit !== undefined && suit !== "joker" && cards.every((card) => card.suit === suit)) {
          add({ kind: "straight-flush", size, sequenceTop, suit });
        }
      }
    }
  }

  if (size === 6 && ordinaryOnly) {
    const pairTop = findSequenceTop(counts, 3, 2);
    if (pairTop !== null) add({ kind: "consecutive-pairs", size, sequenceTop: pairTop });

    const tripleTop = findSequenceTop(counts, 2, 3);
    if (tripleTop !== null) {
      add({ kind: "consecutive-triples", size, sequenceTop: tripleTop });
    }
  }

  if (size >= 4 && ordinaryOnly && counts.size === 1) {
    const primaryRank = cards[0]?.rank;
    if (primaryRank !== undefined) add({ kind: "bomb", size, primaryRank });
  }

  return combinations;
}

function enumerateResolvedCards(
  cards: readonly Card[],
  levelRank: OrdinaryRank,
): Array<{ cards: VirtualCard[]; assignments: Record<string, WildcardAssignment> }> {
  const wildcardIndexes = cards
    .map((card, index) => (isWildcard(card, levelRank) ? index : -1))
    .filter((index) => index >= 0);

  if (wildcardIndexes.length === 0) {
    return [
      {
        cards: cards.map((card) => ({ sourceId: card.id, rank: card.rank, suit: card.suit })),
        assignments: {},
      },
    ];
  }

  const choices: WildcardAssignment[] = [];
  for (const rank of ORDINARY_RANKS) {
    for (const suit of ORDINARY_SUITS) choices.push({ rank, suit });
  }

  const resolved: Array<{
    cards: VirtualCard[];
    assignments: Record<string, WildcardAssignment>;
  }> = [];

  const visit = (
    wildcardPosition: number,
    assignments: Record<string, WildcardAssignment>,
  ): void => {
    if (wildcardPosition === wildcardIndexes.length) {
      const virtualCards = cards.map<VirtualCard>((card) => {
        const assignment = assignments[card.id];
        if (assignment !== undefined) {
          return { sourceId: card.id, rank: assignment.rank, suit: assignment.suit };
        }
        return { sourceId: card.id, rank: card.rank, suit: card.suit };
      });
      resolved.push({ cards: virtualCards, assignments: { ...assignments } });
      return;
    }

    const cardIndex = wildcardIndexes[wildcardPosition];
    const card = cardIndex === undefined ? undefined : cards[cardIndex];
    if (card === undefined) return;

    for (const choice of choices) {
      assignments[card.id] = choice;
      visit(wildcardPosition + 1, assignments);
    }
    delete assignments[card.id];
  };

  visit(0, {});
  return resolved;
}

function matchesDeclaration(
  combination: Combination,
  declaration: CombinationDeclaration,
): boolean {
  if (combination.kind !== declaration.kind) return false;
  if (
    declaration.primaryRank !== undefined &&
    combination.primaryRank !== declaration.primaryRank
  ) {
    return false;
  }
  if (
    declaration.sequenceTop !== undefined &&
    combination.sequenceTop !== declaration.sequenceTop
  ) {
    return false;
  }
  return true;
}

export function listCombinations(
  cards: readonly Card[],
  levelRank: OrdinaryRank,
): Combination[] {
  if (cards.length === 0 || cards.length > 10) return [];
  if (new Set(cards.map((card) => card.id)).size !== cards.length) return [];

  const allJoker =
    cards.length === 4 &&
    cards.filter((card) => card.rank === "small-joker").length === 2 &&
    cards.filter((card) => card.rank === "big-joker").length === 2;

  const unique = new Map<string, Combination>();
  if (allJoker) {
    const combination: Combination = {
      kind: "all-joker",
      size: 4,
      wildcardAssignments: {},
    };
    unique.set(semanticKey(combination), combination);
  }

  if (cards.length === 1 && isWildcard(cards[0] as Card, levelRank)) {
    const combination: Combination = {
      kind: "single",
      size: 1,
      primaryRank: levelRank,
      wildcardAssignments: {},
    };
    unique.set(semanticKey(combination), combination);
    return [...unique.values()];
  }

  for (const resolved of enumerateResolvedCards(cards, levelRank)) {
    for (const combination of classifyResolved(resolved.cards, resolved.assignments)) {
      const key = semanticKey(combination);
      if (!unique.has(key)) unique.set(key, combination);
    }
  }

  return [...unique.values()];
}

export function resolveCombination(
  cards: readonly Card[],
  levelRank: OrdinaryRank,
  declaration?: CombinationDeclaration,
): Combination {
  const candidates = listCombinations(cards, levelRank);
  const matches =
    declaration === undefined
      ? candidates
      : candidates.filter((candidate) => matchesDeclaration(candidate, declaration));

  if (matches.length === 0) {
    throw new RuleError("INVALID_COMBINATION", "所选牌张不能组成声明的合法牌型");
  }

  if (matches.length > 1) {
    throw new RuleError("AMBIGUOUS_COMBINATION", "所选牌张可以解释为多种牌型，请明确声明", {
      options: matches.map((candidate) => ({
        kind: candidate.kind,
        primaryRank: candidate.primaryRank,
        sequenceTop: candidate.sequenceTop,
      })),
    });
  }

  const match = matches[0];
  if (match === undefined) throw new RuleError("INVALID_COMBINATION", "无有效牌型");
  return match;
}

export function isBombCombination(kind: CombinationKind): boolean {
  return kind === "bomb" || kind === "straight-flush" || kind === "all-joker";
}

function sequenceStrength(rank: OrdinaryRank): number {
  return ordinaryRankValue(rank);
}

function bombStrength(
  combination: Combination,
  levelRank: OrdinaryRank,
): readonly [number, number, number] {
  if (combination.kind === "all-joker") return [4, 0, 0];
  if (combination.kind === "bomb" && combination.size >= 6) {
    return [3, combination.size, cardRankStrength(combination.primaryRank as CardRank, levelRank)];
  }
  if (combination.kind === "straight-flush") {
    return [2, 0, sequenceStrength(combination.sequenceTop as OrdinaryRank)];
  }
  if (combination.kind === "bomb") {
    return [combination.size === 5 ? 1 : 0, 0, cardRankStrength(combination.primaryRank as CardRank, levelRank)];
  }
  throw new RuleError("NOT_A_BOMB", "牌型不是炸弹类牌型");
}

function compareTuple(left: readonly number[], right: readonly number[]): number {
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

export function canBeat(
  challenger: Combination,
  incumbent: Combination,
  levelRank: OrdinaryRank,
): boolean {
  const challengerIsBomb = isBombCombination(challenger.kind);
  const incumbentIsBomb = isBombCombination(incumbent.kind);

  if (challengerIsBomb && !incumbentIsBomb) return true;
  if (!challengerIsBomb && incumbentIsBomb) return false;
  if (challengerIsBomb && incumbentIsBomb) {
    return compareTuple(bombStrength(challenger, levelRank), bombStrength(incumbent, levelRank)) > 0;
  }

  if (challenger.kind !== incumbent.kind || challenger.size !== incumbent.size) return false;

  if (challenger.sequenceTop !== undefined && incumbent.sequenceTop !== undefined) {
    return sequenceStrength(challenger.sequenceTop) > sequenceStrength(incumbent.sequenceTop);
  }

  if (challenger.primaryRank !== undefined && incumbent.primaryRank !== undefined) {
    return (
      cardRankStrength(challenger.primaryRank, levelRank) >
      cardRankStrength(incumbent.primaryRank, levelRank)
    );
  }

  return false;
}
