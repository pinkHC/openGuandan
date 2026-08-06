import type { ParticipantCredentials, RoomSnapshot, ServerError } from "./types";

export const SERVER_URL = (process.env.NEXT_PUBLIC_SERVER_URL ?? "http://localhost:3004").replace(/\/$/, "");

export class ApiError extends Error {
  constructor(public readonly serverError: ServerError) {
    super(serverError.message);
    this.name = "ApiError";
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${SERVER_URL}${path}`, {
      ...init,
      headers: { "content-type": "application/json", ...init?.headers },
    });
  } catch {
    throw new ApiError({ code: "NETWORK_ERROR", message: "暂时无法连接服务器，请稍后重试" });
  }
  const body = (await response.json()) as T | { error?: ServerError };
  if (!response.ok) {
    const serverError = "error" in (body as object) ? (body as { error?: ServerError }).error : undefined;
    throw new ApiError(serverError ?? { code: "REQUEST_FAILED", message: "请求未能完成" });
  }
  return body as T;
}

export function createRoom(displayName: string): Promise<ParticipantCredentials> {
  return request("/api/rooms", { method: "POST", body: JSON.stringify({ displayName }) });
}

export function joinRoom(roomCode: string, displayName: string): Promise<ParticipantCredentials> {
  return request(`/api/rooms/${encodeURIComponent(roomCode)}/join`, {
    method: "POST",
    body: JSON.stringify({ displayName }),
  });
}

export function getPublicRoom(roomCode: string): Promise<RoomSnapshot> {
  return request(`/api/rooms/${encodeURIComponent(roomCode)}`);
}
