"use client";

import { cardAriaLabel, cardRankLabel, SUIT_SYMBOLS } from "../lib/card-types";
import type { Card } from "../lib/types";

export function PlayingCard({ card, selected = false, disabled = false, compact = false, onToggle }: { card: Card; selected?: boolean; disabled?: boolean; compact?: boolean; onToggle?: () => void }) {
  const red = card.suit === "heart" || card.suit === "diamond" || card.rank === "big-joker";
  const content = <><span className="playing-card__rank">{cardRankLabel(card)}</span><span className="playing-card__suit">{SUIT_SYMBOLS[card.suit]}</span><small>{card.deckIndex + 1}</small></>;
  if (!onToggle) return <span className={`playing-card ${compact ? "playing-card--compact" : ""} ${red ? "playing-card--red" : ""}`} aria-label={cardAriaLabel(card)}>{content}</span>;
  return <button type="button" className={`playing-card playing-card--button ${compact ? "playing-card--compact" : ""} ${red ? "playing-card--red" : ""} ${selected ? "is-selected" : ""}`} aria-label={`${selected ? "取消选择" : "选择"}${cardAriaLabel(card)}`} aria-pressed={selected} disabled={disabled} onClick={onToggle}>{content}</button>;
}
