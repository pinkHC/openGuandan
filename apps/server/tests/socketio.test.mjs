import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createServer } from "node:net";
import path from "node:path";
import test from "node:test";
import { setTimeout as delay } from "node:timers/promises";

import { io } from "socket.io-client";

const SERVER_ROOT = fileURLToPath(new URL("..", import.meta.url));
const SERVER_BINARY = path.join(
  SERVER_ROOT,
  "target",
  "debug",
  process.platform === "win32" ? "open-guandan-server.exe" : "open-guandan-server",
);

function captureOutput(stream) {
  let output = "";
  stream.setEncoding("utf8");
  stream.on("data", (chunk) => {
    output = `${output}${chunk}`.slice(-20_000);
  });
  return () => output;
}

function spawnWithLogs(command, args, options) {
  const child = spawn(command, args, {
    ...options,
    shell: false,
    windowsHide: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const state = {
    child,
    getStdout: captureOutput(child.stdout),
    getStderr: captureOutput(child.stderr),
    spawnError: undefined,
  };
  child.on("error", (error) => {
    state.spawnError = error;
  });
  return state;
}

function processDiagnostics(processState) {
  const stdout = processState.getStdout().trim();
  const stderr = processState.getStderr().trim();
  return [
    stdout === "" ? "" : `stdout:\n${stdout}`,
    stderr === "" ? "" : `stderr:\n${stderr}`,
  ]
    .filter(Boolean)
    .join("\n");
}

async function waitForExit(processState) {
  const { child } = processState;
  if (child.exitCode !== null || child.signalCode !== null) {
    return { code: child.exitCode, signal: child.signalCode };
  }
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => resolve({ code, signal }));
  });
}

async function buildServer() {
  const build = spawnWithLogs("cargo", ["build", "--quiet", "--locked"], {
    cwd: SERVER_ROOT,
    env: process.env,
  });
  let result;
  try {
    result = await waitForExit(build);
  } catch (error) {
    throw new Error(`Could not run cargo build: ${error.message}`, { cause: error });
  }
  if (result.code !== 0) {
    throw new Error(
      `cargo build failed with ${result.signal ?? `exit code ${result.code}`}\n${processDiagnostics(build)}`,
    );
  }
}

async function getAvailablePort() {
  const server = createServer();
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen({ host: "127.0.0.1", port: 0, exclusive: true }, resolve);
    });
    const address = server.address();
    if (address === null || typeof address === "string") {
      throw new Error("Could not allocate a localhost TCP port");
    }
    return address.port;
  } finally {
    if (server.listening) {
      await new Promise((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
    }
  }
}

function startServer(port) {
  return spawnWithLogs(SERVER_BINARY, [], {
    cwd: SERVER_ROOT,
    env: {
      ...process.env,
      HOST: "127.0.0.1",
      PORT: String(port),
    },
  });
}

async function waitForHealth(baseUrl, server) {
  const deadline = Date.now() + 15_000;
  let lastError;

  while (Date.now() < deadline) {
    if (server.spawnError !== undefined) {
      throw new Error(`Could not start ${SERVER_BINARY}: ${server.spawnError.message}`, {
        cause: server.spawnError,
      });
    }
    if (server.child.exitCode !== null || server.child.signalCode !== null) {
      throw new Error(
        `Rust server exited before becoming healthy (${server.child.signalCode ?? `exit code ${server.child.exitCode}`})\n${processDiagnostics(server)}`,
      );
    }

    try {
      const response = await fetch(`${baseUrl}/health`, {
        signal: AbortSignal.timeout(1_000),
      });
      if (response.ok) {
        assert.deepEqual(await response.json(), { ok: true });
        return;
      }
      lastError = new Error(`GET /health returned HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await delay(50);
  }

  throw new Error(
    `Rust server did not become healthy: ${lastError?.message ?? "timed out"}\n${processDiagnostics(server)}`,
    { cause: lastError },
  );
}

async function stopServer(server) {
  if (server === undefined) return;
  const { child } = server;
  if (child.exitCode !== null || child.signalCode !== null) return;

  const closed = new Promise((resolve) => child.once("close", resolve));
  child.kill();
  if ((await Promise.race([closed.then(() => true), delay(2_000, false)])) === true) return;

  child.kill("SIGKILL");
  await Promise.race([closed, delay(2_000)]);
}

async function createRoom(baseUrl) {
  const response = await fetch(`${baseUrl}/api/rooms`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ displayName: "Socket.IO test" }),
  });
  const body = await response.text();
  assert.equal(response.status, 201, `POST /api/rooms returned ${response.status}: ${body}`);

  const credentials = JSON.parse(body);
  assert.equal(typeof credentials.roomCode, "string");
  assert.equal(typeof credentials.participantId, "string");
  assert.equal(typeof credentials.reconnectToken, "string");
  return credentials;
}

async function connectSocket(baseUrl, credentials, transports) {
  const socket = io(baseUrl, {
    auth: {
      roomCode: credentials.roomCode,
      participantId: credentials.participantId,
      reconnectToken: credentials.reconnectToken,
    },
    autoConnect: false,
    forceNew: true,
    reconnection: false,
    transports,
  });

  try {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        cleanup();
        reject(new Error("Socket.IO connection timed out"));
      }, 5_000);
      const cleanup = () => {
        clearTimeout(timeout);
        socket.off("connect", onConnect);
        socket.off("connect_error", onConnectError);
      };
      const onConnect = () => {
        cleanup();
        resolve();
      };
      const onConnectError = (error) => {
        cleanup();
        reject(error);
      };

      socket.once("connect", onConnect);
      socket.once("connect_error", onConnectError);
      socket.connect();
    });
    return socket;
  } catch (error) {
    socket.disconnect();
    throw error;
  }
}

async function connectAndSyncImmediately(baseUrl, credentials, transports) {
  const socket = io(baseUrl, {
    auth: {
      roomCode: credentials.roomCode,
      participantId: credentials.participantId,
      reconnectToken: credentials.reconnectToken,
    },
    autoConnect: false,
    forceNew: true,
    reconnection: false,
    transports,
  });

  try {
    const response = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        cleanup();
        reject(new Error("Immediate room.sync acknowledgment timed out"));
      }, 5_000);
      const cleanup = () => {
        clearTimeout(timeout);
        socket.off("connect_error", onConnectError);
      };
      const onConnectError = (error) => {
        cleanup();
        reject(error);
      };

      socket.once("connect_error", onConnectError);
      socket.once("connect", () => {
        socket.emit("room.sync", (acknowledgment) => {
          cleanup();
          resolve(acknowledgment);
        });
      });
      socket.connect();
    });
    return { socket, response };
  } catch (error) {
    socket.disconnect();
    throw error;
  }
}

async function connectThenDisconnectImmediately(baseUrl, credentials) {
  const socket = io(baseUrl, {
    auth: {
      roomCode: credentials.roomCode,
      participantId: credentials.participantId,
      reconnectToken: credentials.reconnectToken,
    },
    autoConnect: false,
    forceNew: true,
    reconnection: false,
    transports: ["websocket"],
  });
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Rapid connection timed out")), 5_000);
    socket.once("connect_error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    socket.once("connect", () => {
      clearTimeout(timeout);
      socket.disconnect();
      resolve();
    });
    socket.connect();
  });
}

async function waitForPublicConnectionState(baseUrl, credentials, connected) {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const response = await fetch(`${baseUrl}/api/rooms/${credentials.roomCode}`);
    assert.equal(response.status, 200);
    const room = await response.json();
    const participant = room.participants.find((item) => item.id === credentials.participantId);
    if (participant?.connected === connected) return;
    await delay(20);
  }
  assert.fail(`Participant did not become ${connected ? "connected" : "disconnected"}`);
}

function syncRoom(socket) {
  return new Promise((resolve, reject) => {
    socket.timeout(5_000).emit("room.sync", (error, response) => {
      if (error) {
        reject(new Error(`room.sync acknowledgment timed out: ${error.message}`, { cause: error }));
        return;
      }
      resolve(response);
    });
  });
}

function emitWithAck(socket, event, payload) {
  return new Promise((resolve, reject) => {
    socket.timeout(5_000).emit(event, payload, (error, response) => {
      if (error) {
        reject(new Error(`${event} acknowledgment timed out: ${error.message}`, { cause: error }));
        return;
      }
      resolve(response);
    });
  });
}

function waitForSnapshotVersion(socket, version) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      socket.off("room.snapshot", onSnapshot);
      reject(new Error(`room.snapshot version ${version} timed out`));
    }, 5_000);
    const onSnapshot = (snapshot) => {
      if (snapshot?.version !== version) return;
      clearTimeout(timeout);
      socket.off("room.snapshot", onSnapshot);
      resolve(snapshot);
    };
    socket.on("room.snapshot", onSnapshot);
  });
}

async function expectInvalidCredentials(baseUrl, credentials) {
  const lastCharacter = credentials.reconnectToken.at(-1);
  const badToken = `${credentials.reconnectToken.slice(0, -1)}${lastCharacter === "A" ? "B" : "A"}`;
  const socket = io(baseUrl, {
    auth: {
      roomCode: credentials.roomCode,
      participantId: credentials.participantId,
      reconnectToken: badToken,
    },
    autoConnect: false,
    forceNew: true,
    reconnection: false,
    transports: ["websocket"],
  });
  try {
    const error = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("Invalid credentials were accepted")), 5_000);
      socket.once("connect", () => {
        clearTimeout(timeout);
        reject(new Error("Invalid credentials were accepted"));
      });
      socket.once("connect_error", (cause) => {
        clearTimeout(timeout);
        resolve(cause);
      });
      socket.connect();
    });
    const payload = JSON.parse(error.message);
    assert.equal(payload.code, "INVALID_CREDENTIALS");
  } finally {
    socket.disconnect();
  }
}

test("Rust server is compatible with socket.io-client 4.8.3", { timeout: 120_000 }, async (context) => {
  let server;
  let socket;

  try {
    await buildServer();
    const port = await getAvailablePort();
    const baseUrl = `http://127.0.0.1:${port}`;
    server = startServer(port);
    await waitForHealth(baseUrl, server);

    const credentials = await createRoom(baseUrl);
    await expectInvalidCredentials(baseUrl, credentials);
    const immediate = await connectAndSyncImmediately(baseUrl, credentials, ["websocket"]);
    socket = immediate.socket;
    assert.equal(socket.io.engine.transport.name, "websocket");

    const response = immediate.response;
    assert.equal(response?.ok, true);
    assert.equal(response?.version, 1);
    assert.equal(response?.snapshot?.version, response.version);
    assert.equal(response?.snapshot?.roomCode, credentials.roomCode);
    assert.deepEqual(
      {
        participantId: response?.snapshot?.self?.participantId,
        role: response?.snapshot?.self?.role,
        seat: response?.snapshot?.self?.seat,
      },
      {
        participantId: credentials.participantId,
        role: credentials.role,
        seat: credentials.seat,
      },
    );

    const publicationOrder = [];
    const observeReadySnapshot = (snapshot) => {
      if (snapshot?.version === 2) publicationOrder.push("snapshot");
    };
    socket.on("room.snapshot", observeReadySnapshot);
    const readySnapshot = waitForSnapshotVersion(socket, 2);
    const readyPayload = { actionId: "socket-ready-action", version: 1, ready: true };
    const firstAttempt = emitWithAck(socket, "room.ready", readyPayload).then((acknowledgment) => {
      publicationOrder.push("ack");
      return acknowledgment;
    });
    const retryAttempt = emitWithAck(socket, "room.ready", readyPayload).then((acknowledgment) => {
      publicationOrder.push("ack");
      return acknowledgment;
    });
    const readyResponses = await Promise.all([firstAttempt, retryAttempt]);
    assert.deepEqual(
      readyResponses.map((acknowledgment) => acknowledgment.duplicate).sort(),
      [false, true],
    );
    assert.ok(readyResponses.every((acknowledgment) => acknowledgment.ok && acknowledgment.version === 2));
    assert.equal((await readySnapshot).self.ready, true);
    assert.equal(publicationOrder[0], "snapshot");
    socket.off("room.snapshot", observeReadySnapshot);

    const changedSeatSnapshot = waitForSnapshotVersion(socket, 3);
    const changedSeatResponse = await emitWithAck(socket, "room.change_seat", {
      actionId: "socket-change-seat",
      version: 2,
      seat: 3,
    });
    assert.equal(changedSeatResponse?.ok, true);
    assert.equal(changedSeatResponse?.version, 3);
    const changedSeat = await changedSeatSnapshot;
    assert.equal(changedSeat.self.seat, 3);
    assert.equal(changedSeat.self.ready, false);
    assert.equal(changedSeat.seats[0], null);
    assert.equal(changedSeat.seats[3], credentials.participantId);

    const staleResponse = await emitWithAck(socket, "room.ready", {
      actionId: "socket-stale-action",
      version: 1,
      ready: false,
    });
    assert.equal(staleResponse?.ok, false);
    assert.equal(staleResponse?.error?.code, "STALE_STATE");

    socket.disconnect();
    socket = await connectSocket(baseUrl, credentials, ["polling"]);
    assert.equal(socket.io.engine.transport.name, "polling");
    const pollingResponse = await syncRoom(socket);
    assert.equal(pollingResponse?.ok, true);
    assert.equal(pollingResponse?.snapshot?.self?.participantId, credentials.participantId);

    socket.disconnect();
    socket = undefined;
    for (let attempt = 0; attempt < 12; attempt += 1) {
      await connectThenDisconnectImmediately(baseUrl, credentials);
    }
    await waitForPublicConnectionState(baseUrl, credentials, false);
  } catch (error) {
    if (server !== undefined) {
      const diagnostics = processDiagnostics(server);
      if (diagnostics !== "") context.diagnostic(diagnostics);
    }
    throw error;
  } finally {
    socket?.disconnect();
    await stopServer(server);
  }
});
