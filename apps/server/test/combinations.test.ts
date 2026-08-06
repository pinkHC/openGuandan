import assert from "node:assert/strict";
import test from "node:test";
import { canBeat, listCombinations, resolveCombination } from "../src/domain/combinations.js";
import type { Card, CardRank, OrdinarySuit } from "../src/domain/types.js";

let nextCardId = 0;
function card(rank: CardRank, suit: OrdinarySuit | "joker" = "spade"): Card {
  return {
    id: `test-card-${nextCardId++}`,
    deckIndex: 0,
    rank,
    suit: rank === "small-joker" || rank === "big-joker" ? "joker" : suit,
  };
}

test("识别十种规定牌型", () => {
  assert.equal(resolveCombination([card("3")], "7").kind, "single");
  assert.equal(resolveCombination([card("3"), card("3", "heart")], "7").kind, "pair");
  assert.equal(
    resolveCombination([card("4"), card("4", "heart"), card("4", "club")], "7").kind,
    "triple",
  );
  assert.equal(
    resolveCombination(
      [card("5"), card("5", "heart"), card("5", "club"), card("9"), card("9", "heart")],
      "7",
    ).kind,
    "full-house",
  );
  assert.equal(
    resolveCombination([card("3"), card("4"), card("5"), card("6"), card("7", "club")], "9")
      .kind,
    "straight",
  );
  assert.equal(
    resolveCombination(
      [card("3"), card("3"), card("4"), card("4"), card("5"), card("5")],
      "7",
    ).kind,
    "consecutive-pairs",
  );
  assert.equal(
    resolveCombination(
      [card("8"), card("8"), card("8"), card("9"), card("9"), card("9")],
      "7",
    ).kind,
    "consecutive-triples",
  );
  assert.equal(
    resolveCombination([card("Q"), card("Q"), card("Q"), card("Q")], "7").kind,
    "bomb",
  );

  const straightFlushCards = [
    card("6", "heart"),
    card("7", "heart"),
    card("8", "heart"),
    card("9", "heart"),
    card("10", "heart"),
  ];
  assert.equal(
    resolveCombination(straightFlushCards, "Q", {
      kind: "straight-flush",
      sequenceTop: "10",
    }).kind,
    "straight-flush",
  );

  assert.equal(
    resolveCombination(
      [card("small-joker"), card("small-joker"), card("big-joker"), card("big-joker")],
      "7",
    ).kind,
    "joker-bomb",
  );
});

test("A 可作连续牌型的低牌或高牌，但不能循环连接", () => {
  assert.equal(
    resolveCombination([card("A"), card("2"), card("3"), card("4"), card("5")], "7", {
      kind: "straight",
      sequenceTop: "5",
    }).sequenceTop,
    "5",
  );
  assert.equal(
    resolveCombination([card("10"), card("J"), card("Q"), card("K"), card("A")], "7", {
      kind: "straight",
      sequenceTop: "A",
    }).sequenceTop,
    "A",
  );
  assert.equal(
    listCombinations([card("J"), card("Q"), card("K"), card("A"), card("2")], "7").length,
    0,
  );
});

test("逢人配能组成炸弹且单独打出时仍为级牌", () => {
  const wildcard = card("7", "heart");
  const bomb = resolveCombination([card("8"), card("8"), card("8"), wildcard], "7", {
    kind: "bomb",
    primaryRank: "8",
  });
  assert.equal(bomb.kind, "bomb");
  assert.deepEqual(bomb.wildcardAssignments[wildcard.id]?.rank, "8");

  const single = resolveCombination([card("7", "heart")], "7");
  assert.equal(single.primaryRank, "7");
  assert.deepEqual(single.wildcardAssignments, {});
});

test("同花顺和炸弹遵守规定的层级", () => {
  const fourBomb = resolveCombination([card("A"), card("A"), card("A"), card("A")], "7");
  const fiveBomb = resolveCombination(
    [card("2"), card("2"), card("2"), card("2"), card("2")],
    "7",
  );
  const straightFlush = resolveCombination(
    [card("3", "club"), card("4", "club"), card("5", "club"), card("6", "club"), card("7", "club")],
    "9",
    { kind: "straight-flush", sequenceTop: "7" },
  );
  const sixBomb = resolveCombination(
    [card("3"), card("3"), card("3"), card("3"), card("3"), card("3")],
    "7",
  );
  const allJoker = resolveCombination(
    [card("small-joker"), card("small-joker"), card("big-joker"), card("big-joker")],
    "7",
  );

  assert.equal(canBeat(fiveBomb, fourBomb, "7"), true);
  assert.equal(canBeat(straightFlush, fiveBomb, "7"), true);
  assert.equal(canBeat(sixBomb, straightFlush, "7"), true);
  assert.equal(canBeat(allJoker, sixBomb, "7"), true);
});
