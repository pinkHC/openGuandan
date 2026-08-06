import cors from "@fastify/cors";
import rateLimit from "@fastify/rate-limit";
import Fastify, { type FastifyInstance } from "fastify";
import type { Server as SocketServer } from "socket.io";
import type { ServerConfig } from "./config.js";
import { RoomService } from "./rooms/room-service.js";
import { InMemoryRoomStore } from "./rooms/room-store.js";
import { registerHttpRoutes } from "./transport/http.js";
import { attachWebSocket } from "./transport/websocket.js";

export interface Application {
  app: FastifyInstance;
  io: SocketServer;
  rooms: RoomService;
}

export interface BuildApplicationOptions {
  logger?: boolean;
}

export async function buildApplication(
  config: ServerConfig,
  options: BuildApplicationOptions = {},
): Promise<Application> {
  const app = Fastify({ logger: options.logger ?? true, bodyLimit: 64 * 1024 });
  await app.register(cors, {
    origin: config.corsOrigins,
    credentials: false,
    methods: ["GET", "POST"],
  });
  await app.register(rateLimit, {
    max: 120,
    timeWindow: "1 minute",
  });

  const store = new InMemoryRoomStore();
  const rooms = new RoomService(store, {
    reconnectGraceMs: config.reconnectGraceMs,
    roomIdleTtlMs: config.roomIdleTtlMs,
  });
  registerHttpRoutes(app, rooms);
  const io = attachWebSocket(app.server, rooms, config.corsOrigins);

  const cleanupTimer = setInterval(() => rooms.removeExpired(), 60_000);
  cleanupTimer.unref();
  app.addHook("onClose", async () => {
    clearInterval(cleanupTimer);
    io.disconnectSockets(true);
    await io.of("/").adapter.close();
    io.engine.close();
  });

  return { app, io, rooms };
}
