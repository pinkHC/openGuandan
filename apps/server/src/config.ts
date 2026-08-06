import { z } from "zod";

const environmentSchema = z.object({
  HOST: z.string().default("0.0.0.0"),
  PORT: z.coerce.number().int().min(1).max(65535).default(3000),
  CORS_ORIGIN: z.string().default("http://localhost:5173"),
  ROOM_IDLE_TTL_MS: z.coerce.number().int().positive().default(600_000),
  RECONNECT_GRACE_MS: z.coerce.number().int().positive().default(90_000),
});

export interface ServerConfig {
  host: string;
  port: number;
  corsOrigins: string[];
  roomIdleTtlMs: number;
  reconnectGraceMs: number;
}

export function loadConfig(environment: NodeJS.ProcessEnv = process.env): ServerConfig {
  const parsed = environmentSchema.parse(environment);
  return {
    host: parsed.HOST,
    port: parsed.PORT,
    corsOrigins: parsed.CORS_ORIGIN.split(",")
      .map((origin) => origin.trim())
      .filter(Boolean),
    roomIdleTtlMs: parsed.ROOM_IDLE_TTL_MS,
    reconnectGraceMs: parsed.RECONNECT_GRACE_MS,
  };
}
