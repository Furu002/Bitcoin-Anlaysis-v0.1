# SIGNAL v0.1

A local desktop market-analysis terminal built with **Tauri 2, React, TypeScript and Rust**. BTC/USDT spot, with simulation as the default. The first slice includes synchronized depth, a trade tape, session CVD, top-20 imbalance, feed health and local SQLite recording/replay.

## Run

Install Node.js 24 and Rust stable. Windows: Microsoft's C++ Build Tools with Desktop development with C++ and WebView2. macOS: Xcode Command Line Tools. See [Tauri's platform prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run desktop
```

The app starts in offline simulation. Select **Connect live** to explicitly start the public Binance feed. No exchange account or API key is required. There are no trading or account endpoints.

For this Codex Windows workspace, an isolated Rust/MinGW compiler was placed in the surrounding `work/toolchain` directory without changing the system PATH. The local convenience launcher detects that compiler:

```powershell
.\scripts\start.ps1
```

Normal clones use the installed toolchain from PATH. Windows MSVC and macOS are the standard release build paths; this host's GNU build is an additional development check, not a substitute for release validation.

## Browser development preview

```sh
npm run engine
npm run dev
```

Open `http://127.0.0.1:1420`. This development-only bridge runs the **same Rust engine** as Tauri, including SQLite; it is not a frontend mock. The server binds to loopback and accepts JSON requests from its own origin. Production Tauri uses local IPC and does not start a web server. On Windows, close preview tabs and stop the preview server before rebuilding a running engine executable to avoid file locks.

## Try the vertical slice

1. Watch synthetic trades, price and CVD. `B` means buy aggressor and `S` means sell aggressor.
2. Select **Inject depth gap**. The order book and imbalance become unavailable, then recover from a fresh snapshot. Session CVD becomes visibly incomplete across the reset boundary.
3. Select **Record session**. A fresh session begins so the recording includes a snapshot bootstrap. Stop recording after a few seconds.
4. Open **Recordings**, select **Replay**, and choose 1×/2×/4×/8×. Replay freezes its virtual clock when paused or finished.
5. Open **Data & definitions** for metric semantics, freshness thresholds and the local database path.

## Correctness contracts

- Open WebSocket before REST; buffer deltas; discard stale updates; accept the bridge only when `U <= lastUpdateId + 1 <= u`. Apply absolute quantities and remove zero levels. Gaps, malformed data, crossed books, insufficient top-20 coverage and capacity limits force resynchronization. Snapshot alone is not a synchronized book.
- All prices and quantities use exact integers with eight decimal places. Wire/storage decimals remain strings. Floating point is only for plotting and the displayed ratio.
- Binance's `m` is **buyer-is-maker**: `false` = buy aggressor (`+q`), `true` = sell aggressor (`−q`). CVD is session observed volume, in BTC. Trade IDs deduplicate; missing IDs/reconnections permanently mark the current session incomplete. Starting a new mode or recording resets session metrics.
- Imbalance is `(bid BTC − ask BTC)/(bid BTC + ask BTC)` over the best **20 levels per side**; 10 levels per side are shown. Unusable books yield no derived values.
- Depth/trade receive freshness is independent. Depth stale after 2s; trades after 5s. A monotonic-anchored receive clock prevents wall-clock adjustments and engine queue delay from pretending data is fresh. Exchange-event offset is explicitly clock-dependent, not a measured network latency.
- Feed retries are bounded, old generations are ignored, ping/pong is handled, 15s transport stalls reconnect, and connections rotate before 24 hours. HTTP 403/451/418 stop live access; 429 respects `Retry-After` across manual mode switches.
- SQLite records ordered versioned normalized inputs, snapshots and reset/status boundaries in WAL transactions. Replay preserves receive order/time through the same engine. Storage errors stop recording with a visible error.

## Verify

```sh
npm run build
cargo test -p signal-core --locked
npm run engine
npm run smoke
cargo check -p signal-desktop --locked
```

The offline smoke test compares recorded and replayed book levels, tape, exact CVD, volume and trade counts, then checks gap invalidation and recovery. GitHub Actions includes Windows and macOS checks. See [VALIDATION.md](VALIDATION.md) for actual observed results; workflow configuration itself does not establish a pass.

## Storage and limits

Tauri stores `signal.sqlite3` in its platform local app-data directory. Browser preview uses `.signal/preview.sqlite3` in the project. `SIGNAL_DB` can select a path for the CLI; recordings never enter Git.

Replay currently loads at most **100,000 events and 64 MiB of encoded payload**. Tape is capped at 100 rows, chart history at 1,200 samples, snapshot at 1,000 levels per side and bootstrap buffer at 4,096 deltas. Recording flushes at 128 events or approximately 300ms and on normal shutdown. Abrupt process termination can lose the uncommitted batch; a full crash-recovery/retention UI, storage quotas, replay seek and pagination are later work. Normal close uses an explicit flush; abrupt termination is not a durability guarantee.

## Project map

```text
crates/signal-core/   exact types, book, metrics, simulator, Binance, SQLite, runtime, CLI
src/                 React terminal, typed IPC client, charts and controls
src-tauri/           Tauri desktop shell, local database location and capability/CSP config
scripts/             offline end-to-end smoke test and Windows launcher
docs/PRODUCT_SPEC.md Korean product/technical specification mirrored to Notion
PLAN.md              phases, acceptance criteria and architecture contracts
DATA_SOURCES.md       public source, access restrictions and release gates
VALIDATION.md         measured checks and remaining limits
```

Public technical access does not establish commercial display, recording or redistribution rights. Before external release, review current exchange/entity terms, regional availability and data permissions. See [DATA_SOURCES.md](DATA_SOURCES.md). This terminal describes one venue's observed activity and makes no profit claims.
