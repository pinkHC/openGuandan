import type { FastifyInstance } from "fastify";
import { ZodError } from "zod";
import { RuleError } from "../domain/errors.js";
import type { RoomService } from "../rooms/room-service.js";
import { createRoomView } from "../views/room-view.js";
import { createRoomSchema, joinRoomSchema, roomCodeSchema } from "./schemas.js";

function statusForError(error: RuleError): number {
  if (error.code === "ROOM_NOT_FOUND") return 404;
  if (error.code === "INVALID_CREDENTIALS") return 401;
  if (
    error.code === "DISPLAY_NAME_TAKEN" ||
    error.code === "ROOM_FULL" ||
    error.code === "STALE_STATE"
  ) {
    return 409;
  }
  return 400;
}

export function registerHttpRoutes(app: FastifyInstance, rooms: RoomService): void {
  app.addHook("onSend", async (_request, reply) => {
    reply.header("cache-control", "no-store");
  });

  app.get("/health", async () => ({ ok: true }));

  app.post("/api/rooms", async (request, reply) => {
    const body = createRoomSchema.parse(request.body);
    const credentials = rooms.createRoom(body.displayName);
    return reply.code(201).send(credentials);
  });

  app.post("/api/rooms/:roomCode/join", async (request) => {
    const params = roomCodeSchema.parse((request.params as { roomCode?: unknown }).roomCode);
    const body = joinRoomSchema.parse(request.body);
    return rooms.joinRoom(params, body.displayName);
  });

  app.get("/api/rooms/:roomCode", async (request) => {
    const code = roomCodeSchema.parse((request.params as { roomCode?: unknown }).roomCode);
    return createRoomView(rooms.requireRoom(code));
  });

  app.setErrorHandler((error, _request, reply) => {
    if (error instanceof RuleError) {
      return reply.code(statusForError(error)).send({
        error: { code: error.code, message: error.message, details: error.details ?? null },
      });
    }

    if (error instanceof ZodError) {
      return reply.code(400).send({
        error: { code: "INVALID_REQUEST", message: "请求格式无效" },
      });
    }

    app.log.error(error);
    return reply.code(500).send({
      error: { code: "INTERNAL_ERROR", message: "服务器内部错误" },
    });
  });
}
