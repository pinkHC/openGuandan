import type { RoomState } from "./types.js";

export interface RoomStore {
  get(code: string): RoomState | undefined;
  set(room: RoomState): void;
  delete(code: string): boolean;
  values(): IterableIterator<RoomState>;
}

export class InMemoryRoomStore implements RoomStore {
  private readonly rooms = new Map<string, RoomState>();

  get(code: string): RoomState | undefined {
    return this.rooms.get(code.toUpperCase());
  }

  set(room: RoomState): void {
    this.rooms.set(room.code, room);
  }

  delete(code: string): boolean {
    return this.rooms.delete(code.toUpperCase());
  }

  values(): IterableIterator<RoomState> {
    return this.rooms.values();
  }
}
