import assert from "node:assert/strict";
import test from "node:test";
import { createMatch, passMatchTurn, playMatchCards } from "../src/domain/match.js";
import {
  passTurn,
  playCards,
  submitReturn,
  submitTribute,
  type RoundState,
  type TributeState,
} from "../src/domain/round.js";
import type { Card, CardRank, OrdinaryRank, Seat } from "../src/domain/types.js";

let nextId = 0;
function card(rank: CardRank): Card {
  return {
    id: `round-card-${nextId++}`,
    deckIndex: 0,
    rank,
    suit: rank.includes("joker") ? "joker" : "spade",
  };
}

function simpleRound(levelRank: OrdinaryRank = "7"): RoundState {
  return {
    number: 1,
    levelRank,
    levelOwnerTeam: null,
    phase: "playing",
    hands: new Map<Seat, Card[]>([
      [0, [card("3")]],
      [1, [card("4"), card("8")]],
      [2, [card("5")]],
      [3, [card("6"), card("9")]],
    ]),
    activeSeats: new Set([0, 1, 2, 3]),
    turnSeat: 0,
    currentPlay: null,
    consecutivePasses: 0,
    finishOrder: [],
    tribute: null,
  };
}

test("最后一手未被压制时由仍有牌的搭档借风", () => {
  const round = simpleRound();
  const lastCard = round.hands.get(0)?.[0];
  assert.ok(lastCard);
  assert.equal(playCards(round, 0, [lastCard.id]), null);
  passTurn(round, 1);
  passTurn(round, 2);
  passTurn(round, 3);
  assert.equal(round.turnSeat, 2);
  assert.equal(round.currentPlay, null);
});

test("同队取得上游和二游后立即双下结算", () => {
  const round = simpleRound();
  const firstCard = round.hands.get(0)?.[0];
  assert.ok(firstCard);
  playCards(round, 0, [firstCard.id]);
  passTurn(round, 1);
  passTurn(round, 2);
  passTurn(round, 3);

  const partnerCard = round.hands.get(2)?.[0];
  assert.ok(partnerCard);
  const result = playCards(round, 2, [partnerCard.id]);
  assert.ok(result);
  assert.deepEqual(result.finishOrder, [0, 2]);
  assert.deepEqual(new Set(result.doubleLastSeats), new Set([1, 3]));
  assert.equal(result.partnerPlacement, 2);
});

test("打 A 时必须搭档不是下游才能结束一局牌", () => {
  const match = createMatch(() => 0);
  match.teamLevels[0] = "A";
  const round = simpleRound("A");
  round.levelOwnerTeam = 0;
  match.currentRound = round;

  const firstCard = round.hands.get(0)?.[0];
  assert.ok(firstCard);
  playMatchCards(match, 0, [firstCard.id]);
  passMatchTurn(match, 1);
  passMatchTurn(match, 2);
  passMatchTurn(match, 3);
  const partnerCard = round.hands.get(2)?.[0];
  assert.ok(partnerCard);
  const outcome = playMatchCards(match, 2, [partnerCard.id]);
  assert.equal(outcome.matchWinner, 0);
});

test("单贡交换完成后由进贡者领出", () => {
  const round = simpleRound("7");
  const tributeCard = card("big-joker");
  const returnCard = card("2");
  round.hands = new Map<Seat, Card[]>([
    [0, [returnCard, card("J")]],
    [1, [card("3")]],
    [2, [card("4")]],
    [3, [tributeCard, card("10")]],
  ]);
  const tribute: TributeState = {
    kind: "single",
    stage: "giving",
    previousFirst: 0,
    previousSecond: null,
    givers: [3],
    contributions: new Map(),
    receiverForGiver: new Map([[3, 0]]),
    returns: new Map(),
    leaderSeat: 3,
  };
  round.phase = "tribute";
  round.tribute = tribute;

  submitTribute(round, 3, tributeCard.id);
  assert.equal(tribute.stage, "returning");
  assert.equal(round.hands.get(0)?.some((held) => held.id === tributeCard.id), true);

  submitReturn(round, 0, returnCard.id);
  assert.equal(round.phase, "playing");
  assert.equal(round.turnSeat, 3);
  assert.equal(round.tribute, null);
  assert.equal(round.hands.get(3)?.some((held) => held.id === returnCard.id), true);
});
