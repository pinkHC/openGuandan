import type { Card, CombinationKind } from "./types";

export const CARD_TYPE_NAMES: Record<CombinationKind, { zh: string; en: string }> = {
  single: { zh: "单张", en: "single" },
  pair: { zh: "对子", en: "pair" },
  triple: { zh: "三同张", en: "triple" },
  "full-house": { zh: "三带二", en: "full house" },
  straight: { zh: "顺子", en: "straight" },
  "consecutive-pairs": { zh: "三连对", en: "consecutive pairs" },
  "consecutive-triples": { zh: "钢板", en: "consecutive triples" },
  bomb: { zh: "炸弹", en: "bomb" },
  "straight-flush": { zh: "同花顺", en: "straight flush" },
  "joker-bomb": { zh: "天王炸", en: "joker bomb" },
};

export const SUIT_SYMBOLS = {
  heart: "♥",
  diamond: "♦",
  club: "♣",
  spade: "♠",
  joker: "★",
} as const;

export function cardRankLabel(card: Card): string {
  if (card.rank === "small-joker") return "小王";
  if (card.rank === "big-joker") return "大王";
  return card.rank;
}

export function cardAriaLabel(card: Card): string {
  if (card.rank === "small-joker") return "小王";
  if (card.rank === "big-joker") return "大王";
  const suits = { heart: "红桃", diamond: "方片", club: "梅花", spade: "黑桃" } as const;
  return `${suits[card.suit as keyof typeof suits]} ${card.rank}`;
}
