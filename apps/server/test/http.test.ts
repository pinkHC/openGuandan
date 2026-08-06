import assert from "node:assert/strict";
import test from "node:test";
import { buildApplication } from "../src/app.js";

test("HTTP 可以创建和加入临时房间", async (context) => {
  const { app } = await buildApplication(
    {
      host: "127.0.0.1",
      port: 3000,
      corsOrigins: ["http://localhost:5173"],
      reconnectGraceMs: 90_000,
      roomIdleTtlMs: 600_000,
    },
    { logger: false },
  );
  context.after(async () => app.close());

  const created = await app.inject({
    method: "POST",
    url: "/api/rooms",
    payload: { displayName: "创建者" },
  });
  assert.equal(created.statusCode, 201);
  const credentials = created.json<{
    roomCode: string;
    participantId: string;
    reconnectToken: string;
    role: string;
  }>();
  assert.equal(credentials.role, "player");
  assert.ok(credentials.reconnectToken.length >= 32);

  const joined = await app.inject({
    method: "POST",
    url: `/api/rooms/${credentials.roomCode}/join`,
    payload: { displayName: "加入者" },
  });
  assert.equal(joined.statusCode, 200);
  assert.equal(joined.json<{ role: string }>().role, "player");

  const duplicateName = await app.inject({
    method: "POST",
    url: `/api/rooms/${credentials.roomCode}/join`,
    payload: { displayName: "加入者" },
  });
  assert.equal(duplicateName.statusCode, 409);
});
