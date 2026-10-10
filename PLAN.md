# SIGNAL v0.1 — implementation plan

## Scope and evidence

A Windows/macOS desktop analysis terminal: Tauri 2, React, TypeScript and Rust. BTC/USDT spot only. Start offline in deterministic simulation. Live mode explicitly connects to Binance's public market-data-only trade and diff-depth streams plus a REST snapshot. No account, API key, private endpoint, order entry or execution.

The workspace was empty on 2026-10-10. The linked conversation returned five user turns, but assistant answers were unresolved content references. Recoverable context: Bitcoin focus, interest in market data sources, possible Binance account, development in Codex. This plan does not invent missing requirements or copy the referenced developer's code.

## Phases

1. **Runnable vertical slice (this delivery).** One authoritative Rust engine shared by Tauri and the browser development preview. Seeded simulation, Binance adapter, snapshot/delta synchronization, explicit feed health, top-20 order book, trade tape, session CVD and imbalance, SQLite record/replay, pause/speed controls, injected depth gap, functional tests. A desktop build and offline smoke test where this host permits it.
2. **Reliability hardening.** Long-running soak tests, captured exchange fixtures, network fault proxy, storage capacity/retention settings, crash recovery UX, replay pagination/checkpoints/seek, p95 ingest/render latency measurements. Acceptance: disconnects, stalled snapshots, full queues and disk failures cannot silently publish healthy data.
3. **Analysis tools.** Time-bucketed CVD, trade-size filters, footprints and depth history, configurable layouts and accessible keyboard navigation. Every metric declares source, units, window and completeness; no predictive profit claims.
4. **Distribution readiness.** CI on Windows MSVC and both macOS architectures, signed installers, notarization, dependency/license review and documented permission to distribute exchange data. Acceptance: installation and replay tested on each OS; commercial data use cleared before release.

## First-slice contract

- Rust owns parsing, integer arithmetic, book state, tape, metrics, feed lifecycle and SQLite. React receives bounded views at 10 Hz. Browser preview uses the same Rust engine over a loopback-only development bridge, not a second algorithm.
- Prices and BTC quantities parse into signed 64-bit integers at 8 decimal places. Transport and storage preserve decimals as strings; floating point is only for plotted/UI values. Reject negative, excessive-precision, overflowing and malformed input.
- Open WebSocket first and buffer depth while requesting snapshot. Drop `u <= snapshot.lastUpdateId`; the next accepted range must cover `lastUpdateId + 1`. Apply absolute quantities; zero deletes. Duplicate/stale deltas do not advance the book. Any forward gap, malformed message, crossed book, coverage exhaustion or overflow invalidates derived book values and triggers a fresh connection and snapshot. No futures `pu` logic on spot.
- Snapshot is deliberately bounded (1,000 levels per side); tracked original coverage bounds prevent unknown outer levels from masquerading as a complete top-20 book. Memory caps force resynchronization.
- Trade `m` means **buyer is maker**: `m=false` → buy aggressor, `+q`; `m=true` → sell aggressor, `-q`. Session CVD is observed buy BTC minus observed sell BTC since mode/session start. Trade IDs deduplicate; a forward ID gap or reconnect marks CVD incomplete for the rest of that session. No inferred buy/sell from price movement and no claim of a full-day CVD.
- Imbalance = `(sum top-20 bid BTC - sum top-20 ask BTC) / (sum bid BTC + sum ask BTC)`. Publish only for a synchronized, fresh, sufficiently covered book. This is displayed liquidity, not a trading signal or a promise.
- Health separates transport/book/trade state; local receive age governs staleness. Event-vs-receive offset is labeled clock-dependent, not one-way latency. Book stale after 2 seconds, trades after 5; reconnect on a 15-second transport stall. Planned reconnect before the exchange's 24-hour lifetime. Respect ping/pong and server shutdown.
- Recording captures versioned, normalized inputs, receive order/time, snapshots, resets and status boundaries in a local SQLite WAL database. Start recording with a fresh feed epoch so replay has a snapshot bootstrap. Persist in bounded transactions; surface errors and stop recording, never silently discard events. Replay reads sequence order and advances a virtual receive clock, using the exact same engine. v0.1 replay is capped at 100,000 events and 64 MiB of encoded payload; larger sessions receive an explicit error pending pagination.
- Live endpoints are fixed to the documented market-data-only URLs. Rate limits/Retry-After are respected; access-denied responses stop live mode. No fallback domains, proxy rotation or region-bypass behavior. No market-data redistribution/export feature in this slice. See DATA_SOURCES.md for release gates.

## Verification

Rust: exact decimal validation; bootstrap/overlap/stale/zero-delete/forward gap; crossed/partial book handling; aggressor signs and dedupe; trade completeness; independent feed ages; recording/replay equivalence; invalid commands and bounded sessions. Frontend: type/build checks and controls/empty/stale states. End-to-end: simulation → record → stop → replay → compare final book/tape/CVD; inject gap and observe invalidation/recovery. Real live connectivity is separate from offline correctness and is not claimed when blocked by geography or network.

## Sources checked 2026-10-10

- [Binance Spot WebSocket specification](https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md)
- [Binance Spot REST specification](https://github.com/binance/binance-spot-api-docs/blob/master/rest-api.md)
- [Market-data-only endpoints](https://github.com/binance/binance-spot-api-docs/blob/master/faqs/market_data_only.md)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri Rust commands](https://v2.tauri.app/develop/calling-rust/)
