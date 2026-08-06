import type { ParticipantCredentials } from "./types";

const STORAGE_KEY = "open-guandan:credentials:v1";

function readAll(): Record<string, ParticipantCredentials> {
  if (typeof window === "undefined") return {};
  try {
    return JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}") as Record<string, ParticipantCredentials>;
  } catch {
    return {};
  }
}

export function readCredentials(roomCode: string): ParticipantCredentials | null {
  return readAll()[roomCode.toUpperCase()] ?? null;
}

export function saveCredentials(credentials: ParticipantCredentials): void {
  if (typeof window === "undefined") return;
  const current = readAll();
  current[credentials.roomCode] = credentials;
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(current));
}

export function removeCredentials(roomCode: string): void {
  if (typeof window === "undefined") return;
  const current = readAll();
  delete current[roomCode.toUpperCase()];
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(current));
}
