import { randomInt } from "node:crypto";
import { cardRankStrength, createDeck, dealCards, isWildcard, ordinaryRankValue, shuffleCards } from "./cards.js";
import { canBeat, resolveCombination } from "./combinations.js";
import { RuleError } from "./errors.js";
import {
  nextSeat,
  partnerSeat,
  teamForSeat,
  type Card,
  type Combination,
  type CombinationDeclaration,
  type OrdinaryRank,
  type Seat,
  type Team,
} from "./types.js";

export interface PlayedCombination {
  seat: Seat;
  cards: Card[];
  combination: Combination;
}

export interface RoundResult {
  winnerTeam: Team;
  finishOrder: Seat[];
  doubleLastSeats: Seat[];
  partnerPlacement: 2 | 3 | 4;
}

export interface TributeState {
  kind: "single" | "double";
  stage: "giving" | "returning";
  previousFirst: Seat;
  previousSecond: Seat | null;
  givers: Seat[];
  contributions: Map<Seat, Card>;
  receiverForGiver: Map<Seat, Seat>;
  returns: Map<Seat, Card>;
  leaderSeat: Seat | null;
}

export interface RoundState {
  number: number;
  levelRank: OrdinaryRank;
  levelOwnerTeam: Team | null;
  phase: "tribute" | "playing";
  hands: Map<Seat, Card[]>;
  activeSeats: Set<Seat>;
  turnSeat: Seat;
  currentPlay: PlayedCombination | null;
  consecutivePasses: number;
  finishOrder: Seat[];
  tribute: TributeState | null;
}

export interface CreateRoundOptions {
  number: number;
  levelRank: OrdinaryRank;
  levelOwnerTeam: Team | null;
  previousResult: RoundResult | null;
  randomIndex?: (upperExclusive: number) => number;
}

function countBigJokers(hand: readonly Card[]): number {
  return hand.filter((card) => card.rank === "big-joker").length;
}

function nextActiveSeat(from: Seat, activeSeats: ReadonlySet<Seat>): Seat {
  let candidate = from;
  for (let offset = 0; offset < 4; offset += 1) {
    candidate = nextSeat(candidate);
    if (activeSeats.has(candidate)) return candidate;
  }
  throw new RuleError("NO_ACTIVE_PLAYER", "没有可继续行动的玩家");
}

function cardFromHand(round: RoundState, seat: Seat, cardId: string): Card {
  const card = round.hands.get(seat)?.find((candidate) => candidate.id === cardId);
  if (card === undefined) throw new RuleError("CARD_NOT_OWNED", "所选牌张不在玩家手中");
  return card;
}

function removeCards(hand: Card[], cardIds: ReadonlySet<string>): Card[] {
  return hand.filter((card) => !cardIds.has(card.id));
}

function startPlaying(round: RoundState, leaderSeat: Seat): void {
  round.phase = "playing";
  round.turnSeat = leaderSeat;
  round.tribute = null;
}

function createTributeState(
  hands: ReadonlyMap<Seat, Card[]>,
  levelRank: OrdinaryRank,
  previousResult: RoundResult,
): TributeState | null {
  const previousFirst = previousResult.finishOrder[0];
  if (previousFirst === undefined) throw new Error("Previous round has no first finisher");

  if (previousResult.doubleLastSeats.length === 2) {
    const givers = [...previousResult.doubleLastSeats];
    const bigJokers = givers.reduce<number>(
      (total, seat) => total + countBigJokers(hands.get(seat) ?? []),
      0,
    );
    if (bigJokers === 2) return null;

    return {
      kind: "double",
      stage: "giving",
      previousFirst,
      previousSecond: partnerSeat(previousFirst),
      givers,
      contributions: new Map(),
      receiverForGiver: new Map(),
      returns: new Map(),
      leaderSeat: null,
    };
  }

  const previousLast = previousResult.finishOrder[3];
  if (previousLast === undefined) throw new Error("Previous round has no last finisher");
  if (countBigJokers(hands.get(previousLast) ?? []) === 2) return null;

  return {
    kind: "single",
    stage: "giving",
    previousFirst,
    previousSecond: null,
    givers: [previousLast],
    contributions: new Map(),
    receiverForGiver: new Map([[previousLast, previousFirst]]),
    returns: new Map(),
    leaderSeat: previousLast,
  };
}

export function createRound(options: CreateRoundOptions): RoundState {
  const shuffled = shuffleCards(createDeck(), options.randomIndex);
  const hands = dealCards(shuffled);
  const randomLeader = (options.randomIndex?.(4) ?? randomInt(4)) as Seat;
  const tribute =
    options.previousResult === null
      ? null
      : createTributeState(hands, options.levelRank, options.previousResult);

  const round: RoundState = {
    number: options.number,
    levelRank: options.levelRank,
    levelOwnerTeam: options.levelOwnerTeam,
    phase: tribute === null ? "playing" : "tribute",
    hands,
    activeSeats: new Set([0, 1, 2, 3]),
    turnSeat:
      options.previousResult === null
        ? randomLeader
        : (options.previousResult.finishOrder[0] as Seat),
    currentPlay: null,
    consecutivePasses: 0,
    finishOrder: [],
    tribute,
  };

  // 抗贡时由上一轮上游领出。
  if (options.previousResult !== null && tribute === null) {
    const previousFirst = options.previousResult.finishOrder[0];
    if (previousFirst !== undefined) round.turnSeat = previousFirst;
  }

  return round;
}

function validateTurn(round: RoundState, seat: Seat, expectedPhase: RoundState["phase"]): void {
  if (round.phase !== expectedPhase) {
    throw new RuleError("INVALID_PHASE", `当前轮牌不处于 ${expectedPhase} 阶段`);
  }
  if (expectedPhase === "playing" && round.turnSeat !== seat) {
    throw new RuleError("NOT_YOUR_TURN", "尚未轮到该玩家行动");
  }
}

function isMaximumTributeCard(
  hand: readonly Card[],
  selected: Card,
  levelRank: OrdinaryRank,
): boolean {
  const eligible = hand.filter((card) => !isWildcard(card, levelRank));
  const maximum = Math.max(...eligible.map((card) => cardRankStrength(card.rank, levelRank)));
  return cardRankStrength(selected.rank, levelRank) === maximum;
}

function clockwiseDistance(from: Seat, to: Seat): number {
  return (from - to + 4) % 4;
}

function finalizeContributions(round: RoundState, tribute: TributeState): void {
  if (tribute.kind === "double") {
    const [firstGiver, secondGiver] = tribute.givers;
    const firstCard = firstGiver === undefined ? undefined : tribute.contributions.get(firstGiver);
    const secondCard = secondGiver === undefined ? undefined : tribute.contributions.get(secondGiver);
    if (
      firstGiver === undefined ||
      secondGiver === undefined ||
      firstCard === undefined ||
      secondCard === undefined ||
      tribute.previousSecond === null
    ) {
      throw new Error("Incomplete double tribute");
    }

    const firstStrength = cardRankStrength(firstCard.rank, round.levelRank);
    const secondStrength = cardRankStrength(secondCard.rank, round.levelRank);
    let higherGiver: Seat;
    if (firstStrength > secondStrength) higherGiver = firstGiver;
    else if (secondStrength > firstStrength) higherGiver = secondGiver;
    else {
      higherGiver =
        clockwiseDistance(tribute.previousFirst, firstGiver) <=
        clockwiseDistance(tribute.previousFirst, secondGiver)
          ? firstGiver
          : secondGiver;
    }

    const lowerGiver = higherGiver === firstGiver ? secondGiver : firstGiver;
    tribute.receiverForGiver.set(higherGiver, tribute.previousFirst);
    tribute.receiverForGiver.set(lowerGiver, tribute.previousSecond);
    tribute.leaderSeat = higherGiver;
  }

  for (const [giver, card] of tribute.contributions) {
    const receiver = tribute.receiverForGiver.get(giver);
    if (receiver === undefined) throw new Error("Tribute receiver was not assigned");
    const giverHand = round.hands.get(giver) ?? [];
    round.hands.set(giver, removeCards(giverHand, new Set([card.id])));
    round.hands.get(receiver)?.push(card);
  }
  tribute.stage = "returning";
}

export function submitTribute(round: RoundState, seat: Seat, cardId: string): void {
  validateTurn(round, seat, "tribute");
  const tribute = round.tribute;
  if (tribute === null || tribute.stage !== "giving") {
    throw new RuleError("INVALID_TRIBUTE_STAGE", "当前不接受贡牌");
  }
  if (!tribute.givers.includes(seat)) throw new RuleError("NOT_TRIBUTE_GIVER", "该玩家无需贡牌");
  if (tribute.contributions.has(seat)) throw new RuleError("TRIBUTE_ALREADY_GIVEN", "该玩家已经贡牌");

  const card = cardFromHand(round, seat, cardId);
  const hand = round.hands.get(seat) ?? [];
  if (isWildcard(card, round.levelRank)) {
    throw new RuleError("WILDCARD_CANNOT_BE_TRIBUTE", "红桃级牌不能用于进贡");
  }
  if (!isMaximumTributeCard(hand, card, round.levelRank)) {
    throw new RuleError("TRIBUTE_NOT_MAXIMUM", "必须进贡手中最大的合资格牌");
  }

  tribute.contributions.set(seat, card);
  if (tribute.contributions.size === tribute.givers.length) finalizeContributions(round, tribute);
}

function canReturnCard(hand: readonly Card[], selected: Card, levelRank: OrdinaryRank): boolean {
  const lowCards = hand.filter(
    (card) => card.suit !== "joker" && ordinaryRankValue(card.rank as OrdinaryRank) <= 10,
  );
  if (lowCards.length > 0) return lowCards.some((card) => card.id === selected.id);

  const minimum = Math.min(...hand.map((card) => cardRankStrength(card.rank, levelRank)));
  return cardRankStrength(selected.rank, levelRank) === minimum;
}

export function submitReturn(round: RoundState, seat: Seat, cardId: string): void {
  validateTurn(round, seat, "tribute");
  const tribute = round.tribute;
  if (tribute === null || tribute.stage !== "returning") {
    throw new RuleError("INVALID_TRIBUTE_STAGE", "当前不接受还牌");
  }

  const giverEntry = [...tribute.receiverForGiver.entries()].find(([, receiver]) => receiver === seat);
  if (giverEntry === undefined) throw new RuleError("NOT_TRIBUTE_RECEIVER", "该玩家无需还牌");
  if (tribute.returns.has(seat)) throw new RuleError("RETURN_ALREADY_GIVEN", "该玩家已经还牌");

  const card = cardFromHand(round, seat, cardId);
  const hand = round.hands.get(seat) ?? [];
  if (!canReturnCard(hand, card, round.levelRank)) {
    throw new RuleError("INVALID_RETURN_CARD", "还牌必须不大于 10；若无此类牌则必须还最小牌");
  }

  tribute.returns.set(seat, card);
  if (tribute.returns.size !== tribute.receiverForGiver.size) return;

  for (const [giver, receiver] of tribute.receiverForGiver) {
    const returned = tribute.returns.get(receiver);
    if (returned === undefined) throw new Error("Missing return card");
    round.hands.set(receiver, removeCards(round.hands.get(receiver) ?? [], new Set([returned.id])));
    round.hands.get(giver)?.push(returned);
  }

  if (tribute.leaderSeat === null) throw new Error("Tribute leader was not assigned");
  startPlaying(round, tribute.leaderSeat);
}

function createRoundResult(round: RoundState): RoundResult | null {
  const first = round.finishOrder[0];
  const second = round.finishOrder[1];
  if (first === undefined) return null;

  if (second !== undefined && teamForSeat(first) === teamForSeat(second)) {
    return {
      winnerTeam: teamForSeat(first),
      finishOrder: [first, second],
      doubleLastSeats: [...round.activeSeats],
      partnerPlacement: 2,
    };
  }

  if (round.finishOrder.length === 3) {
    const last = [...round.activeSeats][0];
    if (last === undefined) throw new Error("Missing last active player");
    const finishOrder = [...round.finishOrder, last];
    const partner = partnerSeat(first);
    const partnerIndex = finishOrder.indexOf(partner);
    if (partnerIndex !== 2 && partnerIndex !== 3) throw new Error("Invalid partner placement");
    return {
      winnerTeam: teamForSeat(first),
      finishOrder,
      doubleLastSeats: [],
      partnerPlacement: (partnerIndex + 1) as 3 | 4,
    };
  }

  return null;
}

export function playCards(
  round: RoundState,
  seat: Seat,
  cardIds: readonly string[],
  declaration?: CombinationDeclaration,
): RoundResult | null {
  validateTurn(round, seat, "playing");
  if (!round.activeSeats.has(seat)) throw new RuleError("PLAYER_FINISHED", "该玩家已经出完手牌");
  if (cardIds.length === 0 || new Set(cardIds).size !== cardIds.length) {
    throw new RuleError("INVALID_CARD_SELECTION", "必须选择至少一张且不得重复选择牌张");
  }

  const cards = cardIds.map((cardId) => cardFromHand(round, seat, cardId));
  const combination = resolveCombination(cards, round.levelRank, declaration);
  if (
    round.currentPlay !== null &&
    !canBeat(combination, round.currentPlay.combination, round.levelRank)
  ) {
    throw new RuleError("COMBINATION_TOO_SMALL", "所出牌型不能压制当前牌型");
  }

  const hand = round.hands.get(seat) ?? [];
  round.hands.set(seat, removeCards(hand, new Set(cardIds)));
  round.currentPlay = { seat, cards, combination };
  round.consecutivePasses = 0;

  if ((round.hands.get(seat)?.length ?? 0) === 0) {
    round.finishOrder.push(seat);
    round.activeSeats.delete(seat);
    const result = createRoundResult(round);
    if (result !== null) return result;
  }

  round.turnSeat = nextActiveSeat(seat, round.activeSeats);
  return null;
}

export function passTurn(round: RoundState, seat: Seat): void {
  validateTurn(round, seat, "playing");
  if (round.currentPlay === null) {
    throw new RuleError("CANNOT_PASS_WHEN_LEADING", "领出玩家不能过牌");
  }

  round.consecutivePasses += 1;
  const lastPlayerStillActive = round.activeSeats.has(round.currentPlay.seat);
  const passesNeeded = round.activeSeats.size - (lastPlayerStillActive ? 1 : 0);

  if (round.consecutivePasses >= passesNeeded) {
    const lastPlayedSeat = round.currentPlay.seat;
    let leader: Seat;
    if (round.activeSeats.has(lastPlayedSeat)) leader = lastPlayedSeat;
    else if (round.activeSeats.has(partnerSeat(lastPlayedSeat))) leader = partnerSeat(lastPlayedSeat);
    else leader = nextActiveSeat(lastPlayedSeat, round.activeSeats);

    round.currentPlay = null;
    round.consecutivePasses = 0;
    round.turnSeat = leader;
    return;
  }

  round.turnSeat = nextActiveSeat(seat, round.activeSeats);
}
