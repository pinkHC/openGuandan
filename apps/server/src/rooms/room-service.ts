import { randomBytes, randomUUID } from "node:crypto";
import {
  createMatch,
  giveMatchTribute,
  passMatchTurn,
  playMatchCards,
  returnMatchTribute,
  startNextRound,
} from "../domain/match.js";
import { RuleError } from "../domain/errors.js";
import type { CombinationDeclaration, PlayerId, Seat } from "../domain/types.js";
import type { RoomStore } from "./room-store.js";
import type {
  CommandResult,
  Participant,
  ParticipantCredentials,
  RoomEvent,
  RoomState,
  StoredCommandResult,
} from "./types.js";

const ROOM_CODE_ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const MAX_PARTICIPANTS = 104;
const MAX_PROCESSED_COMMANDS = 500;

export interface RoomServiceOptions {
  now?: () => number;
  reconnectGraceMs?: number;
  roomIdleTtlMs?: number;
}

export interface PlayCardsCommand {
  cardIds: string[];
  declaration?: CombinationDeclaration;
}

function connected(participant: Participant): boolean {
  return participant.socketIds.size > 0;
}

function commandKey(participantId: PlayerId, actionId: string): string {
  return `${participantId}:${actionId}`;
}

export class RoomService {
  private readonly now: () => number;
  readonly reconnectGraceMs: number;
  readonly roomIdleTtlMs: number;

  constructor(
    private readonly store: RoomStore,
    options: RoomServiceOptions = {},
  ) {
    this.now = options.now ?? Date.now;
    this.reconnectGraceMs = options.reconnectGraceMs ?? 90_000;
    this.roomIdleTtlMs = options.roomIdleTtlMs ?? 600_000;
  }

  private generateRoomCode(): string {
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const bytes = randomBytes(6);
      const code = [...bytes]
        .map((byte) => ROOM_CODE_ALPHABET[byte % ROOM_CODE_ALPHABET.length])
        .join("");
      if (!this.store.get(code)) return code;
    }
    throw new Error("Unable to generate a unique room code");
  }

  private createParticipant(displayName: string, role: Participant["role"], seat: Seat | null): Participant {
    const now = this.now();
    return {
      id: randomUUID(),
      displayName: displayName.trim(),
      reconnectToken: randomBytes(32).toString("base64url"),
      role,
      seat,
      ready: false,
      socketIds: new Set(),
      disconnectedAt: now,
      joinedAt: now,
    };
  }

  private credentials(room: RoomState, participant: Participant): ParticipantCredentials {
    return {
      roomCode: room.code,
      participantId: participant.id,
      reconnectToken: participant.reconnectToken,
      role: participant.role,
      seat: participant.seat,
    };
  }

  createRoom(displayName: string): ParticipantCredentials {
    this.validateDisplayName(displayName);
    const code = this.generateRoomCode();
    const participant = this.createParticipant(displayName, "player", 0);
    const now = this.now();
    const room: RoomState = {
      code,
      phase: "lobby",
      hostId: participant.id,
      participants: new Map([[participant.id, participant]]),
      seats: [participant.id, null, null, null],
      match: null,
      version: 1,
      createdAt: now,
      lastActivityAt: now,
      processedCommands: new Map(),
    };
    this.store.set(room);
    return this.credentials(room, participant);
  }

  joinRoom(code: string, displayName: string): ParticipantCredentials {
    this.validateDisplayName(displayName);
    const room = this.requireRoom(code);
    if (room.participants.size >= MAX_PARTICIPANTS) {
      throw new RuleError("ROOM_FULL", "房间人数已达上限");
    }
    const normalizedName = displayName.trim().toLocaleLowerCase();
    if (
      [...room.participants.values()].some(
        (participant) => participant.displayName.toLocaleLowerCase() === normalizedName,
      )
    ) {
      throw new RuleError("DISPLAY_NAME_TAKEN", "该临时用户名已在房间中使用");
    }

    const openSeatIndex = room.phase === "lobby" ? room.seats.findIndex((id) => id === null) : -1;
    const seat = openSeatIndex >= 0 ? (openSeatIndex as Seat) : null;
    const roomWasEmpty = room.participants.size === 0;
    const participant = this.createParticipant(
      displayName,
      seat === null ? "spectator" : "player",
      seat,
    );
    room.participants.set(participant.id, participant);
    if (seat !== null) room.seats[seat] = participant.id;
    if (roomWasEmpty) room.hostId = participant.id;
    room.version += 1;
    this.touch(room);
    return this.credentials(room, participant);
  }

  private validateDisplayName(displayName: string): void {
    const length = [...displayName.trim()].length;
    if (length < 1 || length > 20) {
      throw new RuleError("INVALID_DISPLAY_NAME", "临时用户名长度必须为 1 至 20 个字符");
    }
  }

  getRoom(code: string): RoomState | undefined {
    return this.store.get(code);
  }

  requireRoom(code: string): RoomState {
    const room = this.store.get(code);
    if (room === undefined) throw new RuleError("ROOM_NOT_FOUND", "房间不存在或已过期");
    return room;
  }

  authenticate(
    code: string,
    participantId: string,
    reconnectToken: string,
  ): { room: RoomState; participant: Participant } {
    const room = this.requireRoom(code);
    const participant = room.participants.get(participantId);
    if (participant === undefined || participant.reconnectToken !== reconnectToken) {
      throw new RuleError("INVALID_CREDENTIALS", "房间身份凭证无效");
    }
    return { room, participant };
  }

  connectSocket(code: string, participantId: string, token: string, socketId: string): RoomState {
    const { room, participant } = this.authenticate(code, participantId, token);
    participant.socketIds.add(socketId);
    participant.disconnectedAt = null;
    this.touch(room);
    return room;
  }

  disconnectSocket(code: string, participantId: string, socketId: string): RoomState | undefined {
    const room = this.store.get(code);
    const participant = room?.participants.get(participantId);
    if (room === undefined || participant === undefined) return room;
    participant.socketIds.delete(socketId);
    if (!connected(participant)) participant.disconnectedAt = this.now();
    this.touch(room);
    return room;
  }

  private touch(room: RoomState): void {
    room.lastActivityAt = this.now();
  }

  private participantForCommand(room: RoomState, participantId: PlayerId): Participant {
    const participant = room.participants.get(participantId);
    if (participant === undefined) throw new RuleError("PARTICIPANT_NOT_FOUND", "参与者不在房间中");
    return participant;
  }

  private seatedPlayer(room: RoomState, participantId: PlayerId): Participant & { seat: Seat } {
    const participant = this.participantForCommand(room, participantId);
    if (participant.role !== "player" || participant.seat === null) {
      throw new RuleError("SPECTATOR_CANNOT_PLAY", "旁观者不能执行玩家操作");
    }
    return participant as Participant & { seat: Seat };
  }

  private requireHost(room: RoomState, participantId: PlayerId): void {
    if (room.hostId !== participantId) throw new RuleError("HOST_ONLY", "只有房主可以执行此操作");
  }

  private requireAllPlayersConnected(room: RoomState): void {
    for (const playerId of room.seats) {
      const participant = playerId === null ? undefined : room.participants.get(playerId);
      if (participant === undefined || !connected(participant)) {
        throw new RuleError("GAME_PAUSED_FOR_RECONNECT", "有玩家断线，游戏暂时无法继续");
      }
    }
  }

  private execute(
    room: RoomState,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
    operation: () => RoomEvent[],
  ): CommandResult {
    const key = commandKey(participantId, actionId);
    const previous = room.processedCommands.get(key);
    if (previous !== undefined) {
      return { version: previous.version, duplicate: true, events: previous.events };
    }
    if (room.version !== expectedVersion) {
      throw new RuleError("STALE_STATE", "客户端状态已经过期，请先同步最新房间状态", {
        expectedVersion: room.version,
      });
    }

    const events = operation();
    room.version += 1;
    this.touch(room);
    const stored: StoredCommandResult = { version: room.version, events };
    room.processedCommands.set(key, stored);
    while (room.processedCommands.size > MAX_PROCESSED_COMMANDS) {
      const oldest = room.processedCommands.keys().next().value as string | undefined;
      if (oldest === undefined) break;
      room.processedCommands.delete(oldest);
    }
    return { version: room.version, duplicate: false, events };
  }

  setReady(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
    ready: boolean,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      if (room.phase !== "lobby") throw new RuleError("MATCH_ALREADY_STARTED", "一局牌已经开始");
      const player = this.seatedPlayer(room, participantId);
      player.ready = ready;
      return [{ type: "room.ready", payload: { participantId, ready } }];
    });
  }

  startMatch(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireHost(room, participantId);
      if (room.phase !== "lobby") throw new RuleError("MATCH_ALREADY_STARTED", "一局牌已经开始");
      if (room.seats.some((id) => id === null)) throw new RuleError("SEATS_NOT_FULL", "必须坐满四名玩家");
      this.requireAllPlayersConnected(room);
      if (
        room.seats.some((id) => {
          const participant = id === null ? undefined : room.participants.get(id);
          return participant?.ready !== true;
        })
      ) {
        throw new RuleError("PLAYERS_NOT_READY", "四名玩家必须全部准备");
      }

      room.match = createMatch();
      room.phase = "playing";
      return [{ type: "match.started", payload: { roundNumber: 1, levelRank: "2" } }];
    });
  }

  playCards(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
    command: PlayCardsCommand,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireAllPlayersConnected(room);
      const player = this.seatedPlayer(room, participantId);
      if (room.match === null) throw new RuleError("NO_ACTIVE_MATCH", "当前没有进行中的一局牌");
      const outcome = playMatchCards(
        room.match,
        player.seat,
        command.cardIds,
        command.declaration,
      );
      if (outcome.roundResult === null) return [];

      const finishedEvent: RoomEvent = {
        type: "round.finished",
        payload: {
          ...outcome.roundResult,
          teamLevels: [...room.match.teamLevels],
        },
      };

      if (outcome.matchWinner === null) return [finishedEvent];

      const winnerTeam = outcome.matchWinner;
      room.match = null;
      room.phase = "lobby";
      for (const participant of room.participants.values()) participant.ready = false;
      return [finishedEvent, { type: "match.finished", payload: { winnerTeam } }];
    });
  }

  pass(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireAllPlayersConnected(room);
      const player = this.seatedPlayer(room, participantId);
      if (room.match === null) throw new RuleError("NO_ACTIVE_MATCH", "当前没有进行中的一局牌");
      passMatchTurn(room.match, player.seat);
      return [];
    });
  }

  giveTribute(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
    cardId: string,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireAllPlayersConnected(room);
      const player = this.seatedPlayer(room, participantId);
      if (room.match === null) throw new RuleError("NO_ACTIVE_MATCH", "当前没有进行中的一局牌");
      giveMatchTribute(room.match, player.seat, cardId);
      return [];
    });
  }

  returnTribute(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
    cardId: string,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireAllPlayersConnected(room);
      const player = this.seatedPlayer(room, participantId);
      if (room.match === null) throw new RuleError("NO_ACTIVE_MATCH", "当前没有进行中的一局牌");
      const round = room.match.currentRound;
      const tribute = round?.tribute ?? null;
      returnMatchTribute(room.match, player.seat, cardId);
      if (round === null || tribute === null || round.tribute !== null) return [];
      return [
        {
          type: "tribute.completed",
          payload: {
            kind: tribute.kind,
            contributions: [...tribute.contributions.entries()].map(([seat, card]) => ({ seat, card })),
            returns: [...tribute.returns.entries()].map(([seat, card]) => ({ seat, card })),
            leaderSeat: tribute.leaderSeat,
          },
        },
      ];
    });
  }

  startNextRound(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireHost(room, participantId);
      this.requireAllPlayersConnected(room);
      if (room.match === null) throw new RuleError("NO_ACTIVE_MATCH", "当前没有进行中的一局牌");
      const round = startNextRound(room.match);
      return [
        {
          type: "round.started",
          payload: { roundNumber: round.number, levelRank: round.levelRank, phase: round.phase },
        },
      ];
    });
  }

  abortMatch(
    code: string,
    participantId: PlayerId,
    actionId: string,
    expectedVersion: number,
  ): CommandResult {
    const room = this.requireRoom(code);
    return this.execute(room, participantId, actionId, expectedVersion, () => {
      this.requireHost(room, participantId);
      if (room.match === null) throw new RuleError("NO_ACTIVE_MATCH", "当前没有进行中的一局牌");
      room.match = null;
      room.phase = "lobby";
      for (const participant of room.participants.values()) participant.ready = false;
      return [{ type: "match.aborted", payload: { by: participantId } }];
    });
  }

  removeExpired(): string[] {
    const now = this.now();
    const deletedRooms: string[] = [];

    for (const room of this.store.values()) {
      if (room.phase === "lobby") {
        for (const participant of [...room.participants.values()]) {
          if (
            !connected(participant) &&
            participant.disconnectedAt !== null &&
            now - participant.disconnectedAt >= this.reconnectGraceMs
          ) {
            this.removeParticipant(room, participant.id);
          }
        }
      }

      const hasConnectedParticipant = [...room.participants.values()].some(connected);
      if (!hasConnectedParticipant && now - room.lastActivityAt >= this.roomIdleTtlMs) {
        this.store.delete(room.code);
        deletedRooms.push(room.code);
      }
    }

    return deletedRooms;
  }

  private removeParticipant(room: RoomState, participantId: PlayerId): void {
    const participant = room.participants.get(participantId);
    if (participant === undefined) return;
    if (participant.seat !== null) room.seats[participant.seat] = null;
    room.participants.delete(participantId);
    if (room.hostId === participantId) {
      const replacement = [...room.participants.values()].sort((a, b) => a.joinedAt - b.joinedAt)[0];
      if (replacement !== undefined) room.hostId = replacement.id;
    }
    room.version += 1;
  }
}
