"use client";

import Link from "next/link";
import { FormEvent, useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { io, type Socket } from "socket.io-client";
import { ApiError, joinRoom, SERVER_URL } from "../lib/api";
import { CARD_TYPE_NAMES } from "../lib/card-types";
import { readCredentials, removeCredentials, saveCredentials } from "../lib/storage";
import { initialRoomUiState, roomUiReducer } from "../lib/room-state";
import type { CommandAck, CombinationDeclaration, ParticipantCredentials, ParticipantView, RoomSnapshot, Seat, ServerError } from "../lib/types";
import { Brand } from "./Brand";
import { PlayingCard } from "./PlayingCard";

type ConnectionState = "connecting" | "waking" | "connected" | "reconnecting" | "error";

const SEATS: Seat[] = [0, 1, 2, 3];
const FINISH_NAMES = ["上游", "二游", "三游", "下游"];

function messageOf(error: unknown): string {
  return error instanceof ApiError ? error.message : "操作未能完成，请重试";
}

function parseSocketError(message: string): ServerError {
  try {
    return JSON.parse(message) as ServerError;
  } catch {
    return { code: "CONNECTION_ERROR", message: "房间连接失败，请检查房间是否仍然有效" };
  }
}

function connectionCopy(state: ConnectionState): string {
  if (state === "connected") return "已连接";
  if (state === "waking") return "服务器唤醒中";
  if (state === "reconnecting") return "正在重连";
  if (state === "error") return "连接异常";
  return "正在连接";
}

function participantAt(snapshot: RoomSnapshot, seat: Seat): ParticipantView | undefined {
  const id = snapshot.seats[seat];
  return snapshot.participants.find((participant) => participant.id === id);
}

function relativePosition(seat: Seat, selfSeat: Seat | null): "south" | "west" | "north" | "east" {
  const relative = ((seat - (selfSeat ?? 0) + 4) % 4) as Seat;
  return (["south", "west", "north", "east"] as const)[relative];
}

function SeatBadge({ snapshot, seat, selfSeat, showReady = false }: { snapshot: RoomSnapshot; seat: Seat; selfSeat: Seat | null; showReady?: boolean }) {
  const participant = participantAt(snapshot, seat);
  const round = snapshot.match?.currentRound;
  const position = relativePosition(seat, selfSeat);
  const isTurn = round?.turnSeat === seat && round.phase === "playing";
  const hasFinished = round?.finishOrder.includes(seat) ?? false;
  const secondary = participant
    ? showReady
      ? `座位 ${seat + 1} · ${seat % 2 === 0 ? "甲队" : "乙队"}`
      : hasFinished
        ? FINISH_NAMES[round?.finishOrder.indexOf(seat) ?? 0]
        : `${round?.handCounts[String(seat)] ?? 0} 张`
    : `座位 ${seat + 1}`;
  return (
    <div className={`seat-badge seat-badge--${position} ${isTurn ? "is-turn" : ""} ${participant?.connected === false ? "is-offline" : ""}`}>
      <span className="seat-badge__avatar">{participant?.displayName.slice(0, 1).toUpperCase() ?? "?"}</span>
      <span className="seat-badge__copy"><b>{participant?.displayName ?? "等待入座"}{snapshot.self?.seat === seat ? "（你）" : ""}</b><small>{secondary}</small></span>
      {showReady && participant && <span className={`ready-dot ${participant.ready ? "is-ready" : ""}`}>{participant.ready ? "已准备" : "未准备"}</span>}
    </div>
  );
}

function JoinRoom({ roomCode, onJoined }: { roomCode: string; onJoined: (credentials: ParticipantCredentials) => void }) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const credentials = await joinRoom(roomCode, name);
      saveCredentials(credentials);
      onJoined(credentials);
    } catch (cause) {
      setError(messageOf(cause));
      setBusy(false);
    }
  }
  return (
    <main className="join-shell">
      <header className="site-header"><Brand /><Link href="/rules">玩法规则</Link></header>
      <section className="join-card">
        <p className="eyebrow"><span /> 加入牌桌</p>
        <h1>房间 <strong>{roomCode}</strong></h1>
        <p>输入一个只在本房间使用的临时用户名。若牌局已开始，你将以旁观者身份加入。</p>
        <form onSubmit={submit}>
          <label htmlFor="join-room-name">临时用户名</label>
          <input id="join-room-name" value={name} onChange={(event) => setName(event.target.value)} maxLength={20} autoFocus autoComplete="nickname" placeholder="1–20 个字符" required />
          <button className="button button--gold" type="submit" disabled={busy}>{busy ? "正在加入…" : "进入房间"}<span>→</span></button>
        </form>
        {error && <p className="form-error" role="alert">{error}</p>}
        <Link className="text-link" href="/">← 返回首页</Link>
      </section>
    </main>
  );
}

export function RoomClient({ roomCode }: { roomCode: string }) {
  const [credentials, setCredentials] = useState<ParticipantCredentials | null | undefined>(undefined);
  const [roomUi, dispatchRoomUi] = useReducer(roomUiReducer, initialRoomUiState);
  const snapshot = roomUi.snapshot;
  const [connection, setConnection] = useState<ConnectionState>("connecting");
  const selectedCards = roomUi.selectedCards;
  const [pending, setPending] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [ambiguity, setAmbiguity] = useState<{ cardIds: string[]; options: CombinationDeclaration[] } | null>(null);
  const socketRef = useRef<Socket | null>(null);
  const snapshotRef = useRef<RoomSnapshot | null>(null);

  useEffect(() => setCredentials(readCredentials(roomCode)), [roomCode]);
  useEffect(() => { snapshotRef.current = snapshot; }, [snapshot]);
  useEffect(() => dispatchRoomUi({ type: "clear-selection" }), [snapshot?.match?.currentRound?.turnSeat, snapshot?.match?.currentRound?.phase]);

  const syncRoom = useCallback((socket = socketRef.current) => {
    if (!socket?.connected) return;
    socket.emit("room.sync", (ack: CommandAck) => {
      if (ack.ok && ack.snapshot) dispatchRoomUi({ type: "snapshot", snapshot: ack.snapshot });
    });
  }, []);

  useEffect(() => {
    if (!credentials) return;
    setConnection("connecting");
    const wakingTimer = window.setTimeout(() => setConnection((current) => current === "connected" ? current : "waking"), 4000);
    const socket = io(SERVER_URL, {
      auth: { roomCode: credentials.roomCode, participantId: credentials.participantId, reconnectToken: credentials.reconnectToken },
      reconnection: true,
      reconnectionAttempts: Infinity,
      reconnectionDelay: 700,
      reconnectionDelayMax: 5000,
      timeout: 12000,
    });
    socketRef.current = socket;
    socket.on("connect", () => { window.clearTimeout(wakingTimer); setConnection("connected"); setError(""); syncRoom(socket); });
    socket.on("disconnect", () => setConnection("reconnecting"));
    socket.on("connect_error", (cause) => {
      const parsed = parseSocketError(cause.message);
      if (parsed.code === "INVALID_CREDENTIALS" || parsed.code === "ROOM_NOT_FOUND") {
        removeCredentials(roomCode);
        setCredentials(null);
        dispatchRoomUi({ type: "snapshot", snapshot: null });
        setError(parsed.message);
        socket.disconnect();
      } else {
        setConnection((current) => current === "waking" ? current : "reconnecting");
      }
    });
    socket.on("room.snapshot", (next: RoomSnapshot) => dispatchRoomUi({ type: "snapshot", snapshot: next }));
    socket.on("round.finished", () => setNotice("本轮牌已经结束，结算结果已更新"));
    socket.on("match.finished", (payload: { winnerTeam: 0 | 1 }) => setNotice(`${payload.winnerTeam === 0 ? "甲队" : "乙队"}成功过 A，赢得本局`));
    socket.on("match.aborted", () => setNotice("房主已结束当前牌局"));
    return () => { window.clearTimeout(wakingTimer); socket.disconnect(); socketRef.current = null; };
  }, [credentials, roomCode, syncRoom]);

  const emitCommand = useCallback(async (event: string, payload: Record<string, unknown>): Promise<CommandAck> => {
    const socket = socketRef.current;
    const latest = snapshotRef.current;
    if (!socket?.connected || !latest) return { ok: false, error: { code: "OFFLINE", message: "尚未连接到房间" } };
    setPending(true);
    const ack = await new Promise<CommandAck>((resolve) => {
      socket.timeout(12000).emit(event, { actionId: crypto.randomUUID(), version: latest.version, ...payload }, (timeoutError: Error | null, response?: CommandAck) => {
        if (timeoutError || !response) resolve({ ok: false, error: { code: "TIMEOUT", message: "服务器响应超时，请重试" } });
        else resolve(response);
      });
    });
    setPending(false);
    if (!ack.ok) {
      if (ack.error.code === "STALE_STATE") syncRoom();
      else setError(ack.error.message);
    }
    return ack;
  }, [syncRoom]);

  const participant = snapshot?.participants.find((item) => item.id === credentials?.participantId);
  const isHost = snapshot?.hostId === credentials?.participantId;
  const round = snapshot?.match?.currentRound ?? null;
  const paused = snapshot?.phase === "playing" && snapshot.seats.some((id) => id !== null && snapshot.participants.find((item) => item.id === id)?.connected === false);
  const canAct = connection === "connected" && !paused && !pending;
  const isMyTurn = round?.phase === "playing" && round.turnSeat === snapshot?.self?.seat;
  const selfSeat = snapshot?.self?.seat ?? null;
  const selectedSet = useMemo(() => new Set(selectedCards), [selectedCards]);

  function toggleCard(cardId: string) {
    dispatchRoomUi({ type: "toggle-card", cardId });
  }

  async function playSelected(declaration?: CombinationDeclaration, cardIds = selectedCards) {
    if (cardIds.length === 0) return;
    setError("");
    const ack = await emitCommand("round.play", { cardIds, ...(declaration ? { declaration } : {}) });
    if (ack.ok) { dispatchRoomUi({ type: "clear-selection" }); setAmbiguity(null); return; }
    const options = ack.error.details?.options;
    if (ack.error.code === "AMBIGUOUS_COMBINATION" && options?.length) {
      setError("");
      setAmbiguity({ cardIds, options });
    }
  }

  async function submitSingleCard(event: "tribute.give" | "tribute.return") {
    if (selectedCards.length !== 1) { setError("请选择一张牌"); return; }
    const ack = await emitCommand(event, { cardId: selectedCards[0] });
    if (ack.ok) dispatchRoomUi({ type: "clear-selection" });
  }

  function leaveRoom() {
    if (snapshot?.phase === "playing" && snapshot.self?.role === "player" && !window.confirm("离开会使牌局暂停，直到你重新连接。确定离开吗？")) return;
    removeCredentials(roomCode);
    socketRef.current?.disconnect();
    window.location.assign("/");
  }

  async function copyRoomCode() {
    try { await navigator.clipboard.writeText(roomCode); setNotice("房间码已复制"); }
    catch { setNotice(`房间码：${roomCode}`); }
  }

  if (credentials === undefined) return <main className="loading-screen"><span className="spinner" /><p>正在读取房间身份…</p></main>;
  if (credentials === null) return <JoinRoom roomCode={roomCode} onJoined={setCredentials} />;

  return (
    <main className="room-shell">
      <header className="room-header">
        <Brand compact />
        <div className="room-header__code"><small>房间码</small><button type="button" onClick={copyRoomCode}>{roomCode}<span aria-hidden="true">复制</span></button></div>
        <div className="room-header__actions"><span className={`connection-pill connection-pill--${connection}`}><i />{connectionCopy(connection)}</span><Link href="/rules" target="_blank">规则</Link><button className="plain-button" type="button" onClick={leaveRoom}>离开</button></div>
      </header>

      {connection !== "connected" && <div className="status-banner" role="status"><b>{connection === "waking" ? "免费服务器正在唤醒" : "正在恢复连接"}</b><span>{connection === "waking" ? "首次访问可能需要约一分钟，请保持页面打开。" : "连接恢复后会自动同步最新牌桌。"}</span></div>}
      {paused && <div className="status-banner status-banner--warn" role="status"><b>牌局已暂停</b><span>有玩家暂时断线，等待其重新连接。</span></div>}
      {error && <div className="toast toast--error" role="alert"><span>{error}</span><button onClick={() => setError("")} aria-label="关闭错误提示">×</button></div>}
      {notice && <div className="toast" role="status"><span>{notice}</span><button onClick={() => setNotice("")} aria-label="关闭提示">×</button></div>}

      {!snapshot ? (
        <section className="loading-screen loading-screen--room"><span className="spinner" /><h1>正在布置牌桌</h1><p>我们正在获取房间的最新状态。</p></section>
      ) : snapshot.phase === "lobby" ? (
        <section className="lobby-view">
          <div className="lobby-heading"><div><p className="eyebrow"><span /> 等候大厅</p><h1>坐满四人，准备开局。</h1><p>相对而坐的玩家互为搭档。房主在所有人准备后开始本局。</p></div><div className="team-legend"><span><i className="team-a" />甲队 · 座位 1、3</span><span><i className="team-b" />乙队 · 座位 2、4</span></div></div>
          <div className="lobby-layout">
            <div className="lobby-table">
              <div className="lobby-table__felt"><span className="felt-mark">惯<br />蛋</span>{SEATS.map((seat) => <SeatBadge key={seat} snapshot={snapshot} seat={seat} selfSeat={snapshot.self?.seat ?? 0} showReady />)}</div>
            </div>
            <aside className="lobby-sidebar">
              <div className="sidebar-card"><span className="sidebar-card__number">{snapshot.seats.filter(Boolean).length}<small>/4</small></span><div><b>玩家已入座</b><p>{snapshot.seats.every(Boolean) ? "人员到齐，等待全部准备。" : "把房间码分享给朋友。"}</p></div></div>
              {snapshot.self?.role === "player" ? <button className={`button ${snapshot.self.ready ? "button--outline" : "button--gold"}`} disabled={!canAct} onClick={() => void emitCommand("room.ready", { ready: !snapshot.self?.ready })}>{snapshot.self.ready ? "取消准备" : "准备好了"}</button> : <p className="spectator-note">你正在旁观此房间</p>}
              {isHost && <button className="button button--ivory" disabled={!canAct || snapshot.seats.some((id) => id === null) || snapshot.seats.some((id) => !snapshot.participants.find((p) => p.id === id)?.ready)} onClick={() => void emitCommand("match.start", {})}>开始本局 <span>→</span></button>}
              {!isHost && snapshot.self?.role === "player" && <p className="host-wait">等待房主开始本局</p>}
              <div className="spectators"><h2>旁观者 <span>{snapshot.participants.filter((p) => p.role === "spectator").length}</span></h2>{snapshot.participants.filter((p) => p.role === "spectator").map((p) => <p key={p.id}><i className={p.connected ? "online" : ""} />{p.displayName}</p>)}</div>
            </aside>
          </div>
        </section>
      ) : (
        <section className="game-view">
          <div className="score-strip"><div><small>甲队级数</small><strong>{snapshot.match?.teamLevels[0]}</strong></div><span>第 {round?.number ?? snapshot.match?.nextRoundNumber} 轮牌</span><div><small>乙队级数</small><strong>{snapshot.match?.teamLevels[1]}</strong></div></div>
          {snapshot.self?.role === "spectator" && <div className="spectator-banner">旁观模式 · 你看到的只有公开牌桌信息</div>}

          {snapshot.match?.phase === "between-rounds" ? (
            <div className="round-result">
              <p className="eyebrow"><span /> 本轮结算</p><h1>{snapshot.match.previousRoundResult?.winnerTeam === 0 ? "甲队" : "乙队"}赢得本轮牌</h1>
              <div className="finish-list">{snapshot.match.previousRoundResult?.finishOrder.map((seat, index) => <div key={seat}><span>{index + 1}</span><p><small>{FINISH_NAMES[index]}</small><b>{participantAt(snapshot, seat)?.displayName ?? `座位 ${seat + 1}`}</b></p></div>)}</div>
              <div className="level-result"><span>下一轮级数</span><b>甲队 {snapshot.match.teamLevels[0]}</b><i /> <b>乙队 {snapshot.match.teamLevels[1]}</b></div>
              {isHost ? <button className="button button--gold" disabled={!canAct} onClick={() => void emitCommand("round.next", {})}>开始下一轮牌 <span>→</span></button> : <p>等待房主开始下一轮牌</p>}
            </div>
          ) : round ? (
            <>
              <div className="game-table-wrap">
                <div className="game-table">
                  {SEATS.map((seat) => <SeatBadge key={seat} snapshot={snapshot} seat={seat} selfSeat={snapshot.self?.seat ?? null} />)}
                  <div className="table-center">
                    {round.phase === "tribute" ? <><span className="center-kicker">贡还牌阶段</span><h2>{round.tribute?.stage === "giving" ? "等待进贡" : "等待还牌"}</h2><p>{round.tribute?.kind === "double" ? "本轮为双贡" : "本轮为单贡"}</p></> : round.currentPlay ? <><span className="center-kicker">{participantAt(snapshot, round.currentPlay.seat)?.displayName} · {CARD_TYPE_NAMES[round.currentPlay.combination.kind].zh}</span><div className="played-cards">{round.currentPlay.cards.map((card) => <PlayingCard compact key={card.id} card={card} />)}</div></> : <><span className="center-kicker">新的一圈</span><h2>{participantAt(snapshot, round.turnSeat)?.displayName} 领出</h2></>}
                  </div>
                </div>
              </div>

              {round.phase === "tribute" && round.tribute && <div className="tribute-bar"><div><b>{round.tribute.stage === "giving" ? "请选择贡牌" : "请选择还牌"}</b><span>{round.tribute.stage === "giving" ? "必须提交手中点数最大的合资格牌" : "还牌通常不得超过 10"}</span></div>{selfSeat !== null && ((round.tribute.stage === "giving" && round.tribute.givers.includes(selfSeat)) || (round.tribute.stage === "returning" && Object.values(round.tribute.receiverForGiver).includes(selfSeat))) && <button className="button button--gold" disabled={!canAct || selectedCards.length !== 1} onClick={() => void submitSingleCard(round.tribute?.stage === "giving" ? "tribute.give" : "tribute.return")}>{round.tribute.stage === "giving" ? "确认进贡" : "确认还牌"}</button>}</div>}

              {snapshot.self?.role === "player" && <div className="hand-dock"><div className="hand-dock__meta"><span>你的手牌 <b>{snapshot.self.hand.length}</b></span><span>已选 {selectedCards.length} 张</span></div><div className="hand-scroll" role="group" aria-label="你的手牌">{snapshot.self.hand.map((card) => <PlayingCard key={card.id} card={card} selected={selectedSet.has(card.id)} disabled={!canAct} onToggle={() => toggleCard(card.id)} />)}</div><div className="action-bar">{selectedCards.length > 0 && <button className="plain-button" onClick={() => dispatchRoomUi({ type: "clear-selection" })}>清除选择</button>}{round.phase === "playing" && <><button className="button button--outline" disabled={!canAct || !isMyTurn || round.currentPlay === null} onClick={() => void emitCommand("round.pass", {})}>不出</button><button className="button button--gold" disabled={!canAct || !isMyTurn || selectedCards.length === 0} onClick={() => void playSelected()}>出牌 <span>→</span></button></>}</div></div>}
            </>
          ) : null}
          {isHost && <div className="host-controls"><button className="danger-link" disabled={pending} onClick={() => { if (window.confirm("确定结束当前牌局并返回大厅吗？")) void emitCommand("match.abort", {}); }}>结束当前牌局</button></div>}
        </section>
      )}

      {ambiguity && <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setAmbiguity(null); }}><div className="modal" role="dialog" aria-modal="true" aria-labelledby="ambiguity-title"><p className="eyebrow"><span /> 选择牌型</p><h2 id="ambiguity-title">这组牌有多种解释</h2><p>请选择你希望声明的牌型，服务端会再次验证。</p><div className="ambiguity-options">{ambiguity.options.map((option, index) => <button autoFocus={index === 0} key={`${option.kind}-${option.primaryRank ?? option.sequenceTop ?? index}`} onClick={() => void playSelected(option, ambiguity.cardIds)}><b>{CARD_TYPE_NAMES[option.kind].zh}</b><span>{CARD_TYPE_NAMES[option.kind].en}</span><small>{option.primaryRank ?? option.sequenceTop ?? ""}</small></button>)}</div><button className="plain-button" onClick={() => setAmbiguity(null)}>取消</button></div></div>}
    </main>
  );
}
