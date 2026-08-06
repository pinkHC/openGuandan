import assert from "node:assert/strict";
import test from "node:test";
import { RoomService } from "../src/rooms/room-service.js";
import { InMemoryRoomStore } from "../src/rooms/room-store.js";
import { createRoomView } from "../src/views/room-view.js";

test("前四人入座，第五人及开局后加入者只能旁观", () => {
  const rooms = new RoomService(new InMemoryRoomStore());
  const players = [rooms.createRoom("玩家一")];
  players.push(rooms.joinRoom(players[0]!.roomCode, "玩家二"));
  players.push(rooms.joinRoom(players[0]!.roomCode, "玩家三"));
  players.push(rooms.joinRoom(players[0]!.roomCode, "玩家四"));
  const spectator = rooms.joinRoom(players[0]!.roomCode, "旁观者");
  assert.equal(spectator.role, "spectator");
  assert.equal(spectator.seat, null);

  for (const [index, player] of players.entries()) {
    rooms.connectSocket(player.roomCode, player.participantId, player.reconnectToken, `socket-${index}`);
  }

  let room = rooms.requireRoom(players[0]!.roomCode);
  for (const [index, player] of players.entries()) {
    rooms.setReady(room.code, player.participantId, `ready-${index}`, room.version, true);
  }
  rooms.startMatch(room.code, players[0]!.participantId, "start-match", room.version);
  room = rooms.requireRoom(room.code);
  assert.equal(room.phase, "playing");

  const lateSpectator = rooms.joinRoom(room.code, "迟到者");
  assert.equal(lateSpectator.role, "spectator");
});

test("个性化视图只向座位玩家发送自己的手牌", () => {
  const rooms = new RoomService(new InMemoryRoomStore());
  const host = rooms.createRoom("甲");
  const others = ["乙", "丙", "丁"].map((name) => rooms.joinRoom(host.roomCode, name));
  const spectator = rooms.joinRoom(host.roomCode, "观众");
  const players = [host, ...others];

  players.forEach((player, index) => {
    rooms.connectSocket(player.roomCode, player.participantId, player.reconnectToken, `view-${index}`);
  });
  const room = rooms.requireRoom(host.roomCode);
  players.forEach((player, index) => {
    rooms.setReady(room.code, player.participantId, `view-ready-${index}`, room.version, true);
  });
  rooms.startMatch(room.code, host.participantId, "view-start", room.version);

  const playerView = createRoomView(room, host.participantId) as {
    self: { hand: Card[] };
  };
  const spectatorView = createRoomView(room, spectator.participantId) as {
    self: { hand: Card[] };
  };
  assert.equal(playerView.self.hand.length, 27);
  assert.equal(spectatorView.self.hand.length, 0);

  const publicJson = JSON.stringify(spectatorView);
  for (const hiddenCard of playerView.self.hand) {
    assert.equal(publicJson.includes(hiddenCard.id), false);
  }
});

interface Card {
  id: string;
}
