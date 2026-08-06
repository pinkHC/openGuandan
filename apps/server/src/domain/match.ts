import { RuleError } from "./errors.js";
import {
  createRound,
  passTurn,
  playCards,
  submitReturn,
  submitTribute,
  type RoundResult,
  type RoundState,
} from "./round.js";
import {
  ORDINARY_RANKS,
  type CombinationDeclaration,
  type OrdinaryRank,
  type Seat,
  type Team,
} from "./types.js";

export interface MatchState {
  phase: "playing" | "between-rounds";
  teamLevels: [OrdinaryRank, OrdinaryRank];
  previousRoundResult: RoundResult | null;
  currentRound: RoundState | null;
  nextRoundNumber: number;
  nextLevelRank: OrdinaryRank;
  nextLevelOwnerTeam: Team | null;
}

export interface MatchActionOutcome {
  roundResult: RoundResult | null;
  matchWinner: Team | null;
}

export function advanceLevel(rank: OrdinaryRank, steps: number): OrdinaryRank {
  const currentIndex = ORDINARY_RANKS.indexOf(rank);
  const nextIndex = Math.min(currentIndex + steps, ORDINARY_RANKS.length - 1);
  const nextRank = ORDINARY_RANKS[nextIndex];
  if (nextRank === undefined) throw new Error("Invalid level index");
  return nextRank;
}

export function createMatch(randomIndex?: (upperExclusive: number) => number): MatchState {
  const currentRound = createRound({
    number: 1,
    levelRank: "2",
    levelOwnerTeam: null,
    previousResult: null,
    ...(randomIndex === undefined ? {} : { randomIndex }),
  });

  return {
    phase: "playing",
    teamLevels: ["2", "2"],
    previousRoundResult: null,
    currentRound,
    nextRoundNumber: 2,
    nextLevelRank: "2",
    nextLevelOwnerTeam: null,
  };
}

function requireRound(match: MatchState): RoundState {
  if (match.phase !== "playing" || match.currentRound === null) {
    throw new RuleError("NO_ACTIVE_ROUND", "当前没有进行中的轮牌");
  }
  return match.currentRound;
}

function settleRound(match: MatchState, result: RoundResult): MatchActionOutcome {
  const round = requireRound(match);
  const passedAce =
    round.levelRank === "A" &&
    round.levelOwnerTeam === result.winnerTeam &&
    result.partnerPlacement !== 4;

  if (passedAce) {
    return { roundResult: result, matchWinner: result.winnerTeam };
  }

  const steps = result.partnerPlacement === 2 ? 3 : result.partnerPlacement === 3 ? 2 : 1;
  const nextLevel = advanceLevel(match.teamLevels[result.winnerTeam], steps);
  match.teamLevels[result.winnerTeam] = nextLevel;
  match.previousRoundResult = result;
  match.currentRound = null;
  match.phase = "between-rounds";
  match.nextLevelRank = nextLevel;
  match.nextLevelOwnerTeam = result.winnerTeam;

  return { roundResult: result, matchWinner: null };
}

export function playMatchCards(
  match: MatchState,
  seat: Seat,
  cardIds: readonly string[],
  declaration?: CombinationDeclaration,
): MatchActionOutcome {
  const round = requireRound(match);
  const result = playCards(round, seat, cardIds, declaration);
  return result === null
    ? { roundResult: null, matchWinner: null }
    : settleRound(match, result);
}

export function passMatchTurn(match: MatchState, seat: Seat): void {
  passTurn(requireRound(match), seat);
}

export function giveMatchTribute(match: MatchState, seat: Seat, cardId: string): void {
  submitTribute(requireRound(match), seat, cardId);
}

export function returnMatchTribute(match: MatchState, seat: Seat, cardId: string): void {
  submitReturn(requireRound(match), seat, cardId);
}

export function startNextRound(
  match: MatchState,
  randomIndex?: (upperExclusive: number) => number,
): RoundState {
  if (match.phase !== "between-rounds" || match.currentRound !== null) {
    throw new RuleError("ROUND_ALREADY_ACTIVE", "当前并非轮牌间隔阶段");
  }
  if (match.previousRoundResult === null) throw new Error("Missing previous round result");

  const round = createRound({
    number: match.nextRoundNumber,
    levelRank: match.nextLevelRank,
    levelOwnerTeam: match.nextLevelOwnerTeam,
    previousResult: match.previousRoundResult,
    ...(randomIndex === undefined ? {} : { randomIndex }),
  });
  match.currentRound = round;
  match.phase = "playing";
  match.nextRoundNumber += 1;
  return round;
}
