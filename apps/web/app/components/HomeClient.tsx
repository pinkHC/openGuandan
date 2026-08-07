"use client";

import Link from "next/link";
import { FormEvent, useState } from "react";
import { ApiError, createRoom, joinRoom } from "../lib/api";
import { saveCredentials } from "../lib/storage";
import { Brand } from "./Brand";

function errorMessage(error: unknown): string {
  return error instanceof ApiError ? error.message : "发生了意外错误，请重试";
}

export function HomeClient() {
  const [createName, setCreateName] = useState("");
  const [joinName, setJoinName] = useState("");
  const [roomCode, setRoomCode] = useState("");
  const [busy, setBusy] = useState<"create" | "join" | null>(null);
  const [error, setError] = useState("");

  async function handleCreate(event: FormEvent) {
    event.preventDefault();
    setBusy("create");
    setError("");
    try {
      const credentials = await createRoom(createName);
      saveCredentials(credentials);
      window.location.assign(`/room/${credentials.roomCode}`);
    } catch (cause) {
      setError(errorMessage(cause));
      setBusy(null);
    }
  }

  async function handleJoin(event: FormEvent) {
    event.preventDefault();
    setBusy("join");
    setError("");
    const normalized = roomCode.trim().toUpperCase();
    if (normalized.length < 4 || joinName.trim().length === 0) {
      setError("请填写临时用户名和有效的房间码");
      setBusy(null);
      return;
    }
    try {
      const credentials = await joinRoom(normalized, joinName);
      saveCredentials(credentials);
      window.location.assign(`/room/${encodeURIComponent(normalized)}`);
    } catch (cause) {
      setError(errorMessage(cause));
      setBusy(null);
    }
  }

  return (
    <main className="home-shell">
      <header className="site-header">
        <Brand />
        <nav aria-label="主导航">
          <Link prefetch={false} href="/rules">玩法规则</Link>
          <a className="header-pill" href="#start">开始游戏</a>
        </nav>
      </header>

      <section className="hero" aria-labelledby="hero-title">
        <div className="hero__copy">
          <p className="eyebrow"><span /> 四人 · 两队 · 一条心</p>
          <h1 id="hero-title">和搭档一起，<br /><em>把这一手打漂亮。</em></h1>
          <p className="hero__lead">无需注册，不留牌局记录。创建一个房间，把房间码发给三位朋友，即刻开局。</p>
          <div className="hero__facts" aria-label="游戏特点">
            <span>2 × 54 张</span><i />
            <span>实时对战</span><i />
            <span>游客旁观</span>
          </div>
        </div>

        <div className="hero-table" aria-label="墨绿色虚拟牌桌装饰">
          <div className="hero-table__rim">
            <span className="mini-seat mini-seat--top">队友</span>
            <span className="mini-seat mini-seat--left">对手</span>
            <span className="mini-seat mini-seat--right">对手</span>
            <div className="hero-cards" aria-hidden="true">
              <span className="demo-card demo-card--black">A<small>♠</small></span>
              <span className="demo-card demo-card--red">A<small>♥</small></span>
              <span className="demo-card demo-card--black">A<small>♣</small></span>
              <span className="demo-card demo-card--red">A<small>♦</small></span>
            </div>
            <span className="mini-seat mini-seat--bottom">你</span>
          </div>
        </div>
      </section>

      <section className="start-panel" id="start" aria-labelledby="start-title">
        <div className="start-panel__intro">
          <p className="eyebrow"><span /> 开一桌</p>
          <h2 id="start-title">朋友到齐，就开牌。</h2>
          <p>用户名只在这个房间里使用。游戏开始后，新加入的朋友会自动成为旁观者。</p>
        </div>
        <div className="entry-grid">
          <form className="entry-card" onSubmit={handleCreate}>
            <span className="entry-card__index">01</span>
            <h3>Create Room</h3>
            <p>成为房主，生成一个新的六位房间码。</p>
            <label htmlFor="create-name">临时用户名</label>
            <input id="create-name" value={createName} onChange={(event) => setCreateName(event.target.value)} maxLength={20} placeholder="例如：青山" autoComplete="nickname" required />
            <button className="button button--gold" disabled={busy !== null} type="submit">{busy === "create" ? "正在创建…" : "创建房间"}<span aria-hidden="true">→</span></button>
          </form>

          <form className="entry-card entry-card--dark" onSubmit={handleJoin}>
            <span className="entry-card__index">02</span>
            <h3>Enter Room</h3>
            <p>输入朋友发来的房间码，加入牌桌。</p>
            <div className="two-fields">
              <div><label htmlFor="join-name">临时用户名</label><input id="join-name" value={joinName} onChange={(event) => setJoinName(event.target.value)} maxLength={20} placeholder="例如：听风" autoComplete="nickname" required /></div>
              <div><label htmlFor="room-code">房间码</label><input id="room-code" className="code-input" value={roomCode} onChange={(event) => setRoomCode(event.target.value.toUpperCase())} minLength={4} maxLength={12} placeholder="ABC123" autoCapitalize="characters" required /></div>
            </div>
            <button className="button button--ivory" disabled={busy !== null} type="submit">{busy === "join" ? "正在进入…" : "进入房间"}<span aria-hidden="true">→</span></button>
          </form>
        </div>
        {error && <p className="form-error" role="alert">{error}</p>}
      </section>

      <footer className="home-footer"><Brand compact /><p>开源、轻量、不保存历史牌局。</p><Link prefetch={false} href="/rules">查看完整规则 →</Link></footer>
    </main>
  );
}
