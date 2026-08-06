// @vitest-environment node
import { describe, expect, it } from "vitest";
import { initialRoomUiState, roomUiReducer } from "../app/lib/room-state";

describe("房间界面状态", () => {
  it("切换手牌选择并可以统一清除", () => {
    const selected = roomUiReducer(initialRoomUiState, { type: "toggle-card", cardId: "card-1" });
    expect(selected.selectedCards).toEqual(["card-1"]);
    expect(roomUiReducer(selected, { type: "toggle-card", cardId: "card-1" }).selectedCards).toEqual([]);
    expect(roomUiReducer(selected, { type: "clear-selection" }).selectedCards).toEqual([]);
  });
});
