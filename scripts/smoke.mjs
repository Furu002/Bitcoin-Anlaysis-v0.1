import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";

const dir = mkdtempSync(join(tmpdir(), "signal-smoke-"));
const executable =
  process.env.SIGNAL_ENGINE ||
  resolve(
    "target/debug",
    process.platform === "win32" ? "signal-engine.exe" : "signal-engine",
  );
const child = spawn(executable, [], {
  windowsHide: true,
  env: { ...process.env, SIGNAL_DB: join(dir, "smoke.sqlite3") },
});
const waiting = [];
createInterface({ input: child.stdout }).on("line", (line) => {
  const reply = JSON.parse(line),
    item = waiting.shift();
  if (item)
    reply.ok ? item.resolve(reply.data) : item.reject(new Error(reply.error));
});
child.stderr.on("data", (d) => process.stderr.write(d));
child.on("error", (e) => {
  for (const item of waiting.splice(0)) item.reject(e);
});
const call = (request) =>
  new Promise((resolve, reject) => {
    waiting.push({ resolve, reject });
    child.stdin.write(JSON.stringify(request) + "\n");
  });
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const deadline = setTimeout(() => {
  child.kill();
  console.error("Smoke test timed out");
  process.exitCode = 1;
}, 25_000);

try {
  const first = await call({ action: "view" });
  assert.equal(first.mode, "simulation");
  assert.equal(first.health, "healthy");
  await call({ action: "record", enabled: true });
  await delay(800);
  await call({ action: "pause", paused: true });
  const recorded = await call({ action: "record", enabled: false });
  const sessions = await call({ action: "sessions" });
  assert.ok(sessions[0].event_count > 0);
  assert.equal(sessions[0].closed, true);
  await call({ action: "replay", session_id: sessions[0].id });
  await call({ action: "speed", speed: 8 });
  let replay;
  for (let i = 0; i < 30; i++) {
    replay = await call({ action: "view" });
    if (replay.replay.finished) break;
    await delay(100);
  }
  assert.equal(replay.replay.finished, true);
  for (const key of [
    "cvd",
    "buy_volume",
    "sell_volume",
    "cvd_complete",
    "trade_count",
    "update_id",
    "bids",
    "asks",
    "tape",
  ]) {
    assert.deepEqual(replay[key], recorded[key], `Replay differs: ${key}`);
  }
  await call({ action: "mode", mode: "simulation" });
  await call({ action: "inject_gap" });
  await delay(200);
  const gap = await call({ action: "view" });
  assert.equal(gap.book_state, "syncing");
  assert.equal(gap.imbalance, null);
  await delay(1400);
  const recovered = await call({ action: "view" });
  assert.equal(recovered.health, "healthy");
  assert.equal(recovered.cvd_complete, false);
  await call({ action: "shutdown" });
  console.log(
    "PASS: offline simulation, SQLite recording, exact replay, gap invalidation and recovery.",
  );
} finally {
  clearTimeout(deadline);
  child.stdin.end();
  await new Promise((resolve) =>
    child.exitCode === null ? child.once("exit", resolve) : resolve(),
  );
  // The absolute target was created by mkdtempSync under the OS temporary directory.
  rmSync(dir, { recursive: true, force: true });
}
