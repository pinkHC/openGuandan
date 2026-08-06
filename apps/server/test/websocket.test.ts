import assert from "node:assert/strict";
import test from "node:test";
import { io as createClient, type Socket } from "socket.io-client";
import { buildApplication } from "../src/app.js";

test("room.sync 返回当前参与者的个性化快照", async (context) => {
  const { app } = await buildApplication(
    {
      host: "127.0.0.1",
      port: 3004,
      corsOrigins: ["http://localhost:5174"],
      reconnectGraceMs: 90_000,
      roomIdleTtlMs: 600_000,
    },
    { logger: false },
  );
  await app.listen({ host: "127.0.0.1", port: 0 });

  const created = await app.inject({
    method: "POST",
    url: "/api/rooms",
    payload: { displayName: "同步者" },
  });
  const credentials = created.json<{
    roomCode: string;
    participantId: string;
    reconnectToken: string;
  }>();
  const address = app.server.address();
  if (address === null || typeof address === "string") throw new Error("测试服务器未监听 TCP 端口");

  const socket: Socket = createClient(`http://127.0.0.1:${address.port}`, {
    auth: {
      roomCode: credentials.roomCode,
      participantId: credentials.participantId,
      reconnectToken: credentials.reconnectToken,
    },
    transports: ["websocket"],
  });
  context.after(async () => {
    socket.disconnect();
    await app.close();
  });
  await new Promise<void>((resolve, reject) => {
    socket.once("connect", resolve);
    socket.once("connect_error", reject);
  });

  const response = await new Promise<{
    ok: boolean;
    version: number;
    snapshot: { roomCode: string; self: { participantId: string } };
  }>((resolve) => socket.emit("room.sync", resolve));
  assert.equal(response.ok, true);
  assert.equal(response.snapshot.roomCode, credentials.roomCode);
  assert.equal(response.snapshot.self.participantId, credentials.participantId);
  assert.equal(response.version, 1);
});
