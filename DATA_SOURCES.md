# Data-source and release constraints

The only live source in v0.1 is Binance BTCUSDT **spot**, via documented public market-data-only endpoints:

- `wss://data-stream.binance.vision/stream?streams=btcusdt@trade/btcusdt@depth@100ms`
- `https://data-api.binance.vision/api/v3/depth?symbol=BTCUSDT&limit=1000`

No API keys or Binance account are needed for these endpoints according to [Binance's public-data documentation](https://github.com/binance/binance-spot-api-docs/blob/master/faqs/market_data_only.md). No credentials are requested, stored or transmitted. TLS certificate verification stays enabled. Simulation does not make exchange requests.

Public technical access does not establish a license to redistribute, resell or commercially display the data. Before any public or commercial release, review the then-current [Binance Terms](https://www.binance.com/en/terms), applicable regional/entity terms and data-use permissions with qualified counsel. The terms page available during implementation did not expose the substantive regional clauses to the research tool; no affirmative legal-permission claim is made. Jurisdiction is not inferred from this machine's locale. Local SQLite storage is an engineering feature whose permitted use must also be checked before release.

The adapter stops on HTTP 403/451/418 and on access-denied WebSocket handshakes. HTTP 429 suspends retries for at least `Retry-After` (60 seconds if missing); transient errors use bounded exponential backoff. A manual mode switch does not defeat an in-process exchange cooldown. There is no region bypass, domain switching, scraping, proxy rotation or automatic account connection. Live availability can vary by region and exchange policy. A blocked source leaves simulation usable.

Binance is one venue. Its order book, tape and CVD are not the entire Bitcoin market. CVD is session-scoped observed aggressor volume; gaps remain visible. Displayed liquidity can change or be canceled. SIGNAL v0.1 provides descriptive market analysis; it has no trade execution or profit guarantee.

SQLite recordings stay on the user's machine. No telemetry, cloud upload or built-in redistribution is implemented. Release work must include dependency licenses, a defined storage-retention policy and installation/signing checks on Windows and macOS.
