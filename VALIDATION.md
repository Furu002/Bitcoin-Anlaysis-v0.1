# Validation — 2026-10-10

## Observed locally

- Frontend TypeScript check and Vite production build: passed.
- Rust core: 15 tests passed, including exact decimals, buffered snapshot bridge, overlapping/stale deltas, absolute quantities and zero deletion, gaps, crossed books, snapshot coverage, aggressor signs, deduplication, CVD completeness and receive-age behavior.
- Offline integration smoke: simulation → SQLite record → replay matched exact CVD, buy/sell volumes, trade count, book ID/levels and tape. Injected depth gap invalidated book/imbalance, then recovered while retaining the incomplete CVD label.
- Browser UI: simulation appeared with source label; pause, fresh recording, session list, replay and 8× speed worked; replay reached the complete event count.
- Final offline smoke after moving live receive timestamps ahead of the ingest queue: passed.
- Windows native Tauri development build (Rust 1.99, x86_64 GNU, WebView2): compiled, launched and rendered the terminal. The definitions dialog confirmed the native platform app-data SQLite path, proving use of Tauri IPC rather than the development preview bridge.
- Public Binance live connection in the native desktop: passed on this host. REST snapshot and WebSocket depth bridged, health became healthy, raw trade tape/CVD/imbalance updated, and depth/trade receive ages were displayed independently. This was a brief connection smoke, not a 24-hour reliability test or a guarantee of access elsewhere.
- Notion specification: all 16 sections replaced with the SIGNAL scope; title, content and GitHub link read back successfully.

## Remaining verification and limits

- macOS execution, Windows MSVC release build, both macOS architectures, installer signing/notarization and soak/load/disk-failure tests are not locally verified.
- Replay is bounded to 100,000 events / 64 MiB and has no seek/checkpoint pagination.
- Recording uses bounded synchronous SQLite transactions; prolonged disk stalls still need measured load tests. Abrupt process termination can lose the uncommitted batch (normally up to approximately 300ms); no power-loss guarantee is claimed.
- Development preview is a loopback bridge for the same Rust engine. Production desktop uses IPC. The local isolated GNU compiler is kept outside the repository.
- Exchange source availability and permission for external/commercial display, storage or redistribution are separate from software correctness.
