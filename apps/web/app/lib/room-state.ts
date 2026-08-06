import type { RoomSnapshot } from "./types";

export interface RoomUiState {
  snapshot: RoomSnapshot | null;
  selectedCards: string[];
}

export type RoomUiAction =
  | { type: "snapshot"; snapshot: RoomSnapshot | null }
  | { type: "toggle-card"; cardId: string }
  | { type: "clear-selection" };

export const initialRoomUiState: RoomUiState = { snapshot: null, selectedCards: [] };

export function roomUiReducer(state: RoomUiState, action: RoomUiAction): RoomUiState {
  if (action.type === "snapshot") return { ...state, snapshot: action.snapshot };
  if (action.type === "clear-selection") return { ...state, selectedCards: [] };
  const selectedCards = state.selectedCards.includes(action.cardId)
    ? state.selectedCards.filter((id) => id !== action.cardId)
    : [...state.selectedCards, action.cardId];
  return { ...state, selectedCards };
}
