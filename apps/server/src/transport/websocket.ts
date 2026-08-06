import type { Server as HttpServer } from "node:http";
import { Server as SocketServer, type Socket } from "socket.io";
import { ZodError, type ZodType } from "zod";
import { RuleError } from "../domain/errors.js";
import type { RoomService } from "../rooms/room-service.js";
import type { CommandResult, RoomState } from "../rooms/types.js";
import { createRoomView } from "../views/room-view.js";
import {
  cardActionSchema,
  playCardsSchema,
  readySchema,
  simpleActionSchema,
  socketAuthSchema,
} from "./schemas.js";

interface SocketIdentity {
  roomCode: string;
  participantId: string;
}

interface SocketRateState {
  windowStartedAt: number;
  count: number;
}

type Ack = (response: unknown) => void;

function roomChannel(code: string): string {
  return `room:${code}`;
}

function errorPayload(error: unknown): unknown {
  if (error instanceof RuleError) {
    return { code: error.code, message: error.message, details: error.details ?? null };
  }
  if (error instanceof ZodError) {
    return { code: "INVALID_MESSAGE", message: "消息格式无效", details: error.issues };
  }
  return { code: "INTERNAL_ERROR", message: "服务器内部错误" };
}

function identity(socket: Socket): SocketIdentity {
  return socket.data.identity as SocketIdentity;
}

function consumeSocketRateLimit(socket: Socket): void {
  const now = Date.now();
  const existing = socket.data.rateState as SocketRateState | undefined;
  const state =
    existing === undefined || now - existing.windowStartedAt >= 10_000
      ? { windowStartedAt: now, count: 0 }
      : existing;
  state.count += 1;
  socket.data.rateState = state;
  if (state.count > 60) {
    throw new RuleError("RATE_LIMITED", "操作过于频繁，请稍后重试");
  }
}

export function attachWebSocket(
  httpServer: HttpServer,
  rooms: RoomService,
  corsOrigins: string[],
): SocketServer {
  const io = new SocketServer(httpServer, {
    cors: { origin: corsOrigins, credentials: false },
    maxHttpBufferSize: 64 * 1024,
  });

  const broadcastSnapshots = (room: RoomState): void => {
    for (const participant of room.participants.values()) {
      const view = createRoomView(room, participant.id);
      for (const socketId of participant.socketIds) io.to(socketId).emit("room.snapshot", view);
    }
  };

  const publishResult = (room: RoomState, result: CommandResult): void => {
    if (!result.duplicate) {
      for (const event of result.events) {
        io.to(roomChannel(room.code)).emit(event.type, event.payload);
      }
      broadcastSnapshots(room);
    }
  };

  io.use((socket, next) => {
    try {
      const auth = socketAuthSchema.parse(socket.handshake.auth);
      rooms.authenticate(auth.roomCode, auth.participantId, auth.reconnectToken);
      socket.data.identity = {
        roomCode: auth.roomCode,
        participantId: auth.participantId,
      } satisfies SocketIdentity;
      next();
    } catch (error) {
      next(new Error(JSON.stringify(errorPayload(error))));
    }
  });

  io.on("connection", (socket) => {
    const socketIdentity = identity(socket);
    const room = rooms.connectSocket(
      socketIdentity.roomCode,
      socketIdentity.participantId,
      socket.handshake.auth.reconnectToken as string,
      socket.id,
    );
    void socket.join(roomChannel(room.code));
    io.to(roomChannel(room.code)).emit("participant.connection", {
      participantId: socketIdentity.participantId,
      connected: true,
    });
    broadcastSnapshots(room);

    const handle = <T>(
      eventName: string,
      schema: ZodType<T>,
      operation: (payload: T) => CommandResult,
    ): void => {
      socket.on(eventName, (rawPayload: unknown, rawAck?: Ack) => {
        const ack: Ack = typeof rawAck === "function" ? rawAck : () => undefined;
        try {
          consumeSocketRateLimit(socket);
          const payload = schema.parse(rawPayload);
          const result = operation(payload);
          const currentRoom = rooms.requireRoom(socketIdentity.roomCode);
          publishResult(currentRoom, result);
          ack({ ok: true, version: result.version, duplicate: result.duplicate });
        } catch (error) {
          ack({ ok: false, error: errorPayload(error) });
        }
      });
    };

    handle("room.ready", readySchema, (payload) =>
      rooms.setReady(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
        payload.ready,
      ),
    );

    handle("match.start", simpleActionSchema, (payload) =>
      rooms.startMatch(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
      ),
    );

    handle("round.play", playCardsSchema, (payload) =>
      rooms.playCards(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
        {
          cardIds: payload.cardIds,
          ...(payload.declaration === undefined
            ? {}
            : {
                declaration: {
                  kind: payload.declaration.kind,
                  ...(payload.declaration.primaryRank === undefined
                    ? {}
                    : { primaryRank: payload.declaration.primaryRank }),
                  ...(payload.declaration.sequenceTop === undefined
                    ? {}
                    : { sequenceTop: payload.declaration.sequenceTop }),
                },
              }),
        },
      ),
    );

    handle("round.pass", simpleActionSchema, (payload) =>
      rooms.pass(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
      ),
    );

    handle("tribute.give", cardActionSchema, (payload) =>
      rooms.giveTribute(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
        payload.cardId,
      ),
    );

    handle("tribute.return", cardActionSchema, (payload) =>
      rooms.returnTribute(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
        payload.cardId,
      ),
    );

    handle("round.next", simpleActionSchema, (payload) =>
      rooms.startNextRound(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
      ),
    );

    handle("match.abort", simpleActionSchema, (payload) =>
      rooms.abortMatch(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        payload.actionId,
        payload.version,
      ),
    );

    socket.on("disconnect", () => {
      const currentRoom = rooms.disconnectSocket(
        socketIdentity.roomCode,
        socketIdentity.participantId,
        socket.id,
      );
      if (currentRoom !== undefined) {
        const stillConnected =
          (currentRoom.participants.get(socketIdentity.participantId)?.socketIds.size ?? 0) > 0;
        io.to(roomChannel(currentRoom.code)).emit("participant.connection", {
          participantId: socketIdentity.participantId,
          connected: stillConnected,
        });
        broadcastSnapshots(currentRoom);
      }
    });
  });

  return io;
}
