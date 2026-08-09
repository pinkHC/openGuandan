import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PlayingCard } from "../app/components/PlayingCard";
import { HomeClient } from "../app/components/HomeClient";
import { SeatBadge } from "../app/components/RoomClient";
import type { RoomSnapshot, Seat } from "../app/lib/types";

describe("首页", () => {
  it("提供创建房间、进入房间和规则入口", () => {
    render(<HomeClient />);
    expect(screen.getByRole("heading", { name: /把这一手打漂亮/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /创建房间/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /进入房间/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "玩法规则" })).toHaveAttribute("href", "/rules");
  });
});

describe("手牌", () => {
  it("可通过按钮选择并暴露 aria-pressed 状态", () => {
    let selected = false;
    const card = { id: "0:heart:A", deckIndex: 0 as const, suit: "heart" as const, rank: "A" as const };
    const { rerender } = render(<PlayingCard card={card} selected={selected} onToggle={() => { selected = true; }} />);
    const button = screen.getByRole("button", { name: "选择红桃 A" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    rerender(<PlayingCard card={card} selected={selected} onToggle={() => undefined} />);
    expect(screen.getByRole("button", { name: "取消选择红桃 A" })).toHaveAttribute("aria-pressed", "true");
  });
});

describe("大厅座位", () => {
  it("空座位可被选择", () => {
    let selected: Seat | null = null;
    const snapshot: RoomSnapshot = {
      roomCode: "ABC123",
      phase: "lobby",
      version: 1,
      hostId: "host",
      seats: ["host", null, null, null],
      participants: [{ id: "host", displayName: "甲", role: "player", seat: 0, ready: false, connected: true }],
      match: null,
      self: { participantId: "host", role: "player", seat: 0, ready: false, hand: [] },
    };

    render(<SeatBadge snapshot={snapshot} seat={2} selfSeat={0} onSelect={(seat) => { selected = seat; }} />);
    fireEvent.click(screen.getByRole("button", { name: "选择座位 3" }));

    expect(selected).toBe(2);
  });
});
