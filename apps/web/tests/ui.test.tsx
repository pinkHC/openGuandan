import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PlayingCard } from "../app/components/PlayingCard";
import { HomeClient } from "../app/components/HomeClient";

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
