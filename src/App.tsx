import { useEffect, useState } from "react";
import { request } from "./api";
import type { Level, Request, Sample, Session, View } from "./types";

const price = (v: string | number | null) =>
  v === null
    ? "—"
    : Number(v).toLocaleString("en-US", {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2,
      });
const qty = (v: string | number) => Number(v).toFixed(4);
const signed = (v: string | number) => (Number(v) >= 0 ? "+" : "") + qty(v);
const time = (ms: number) => new Date(ms).toISOString().slice(11, 19);
const age = (ms: number | null) =>
  ms === null ? "—" : ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;

function Icon({ name }: { name: "terminal" | "library" | "info" | "pulse" }) {
  const paths = {
    terminal: "M3 4h18v16H3z M7 8l4 4-4 4 M13 16h4",
    library: "M4 3h5v18H4z M12 3h5v18h-5z M20 5l3 15",
    info: "M12 8h.01 M12 11v6 M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0",
    pulse: "M2 12h5l3-8 4 16 3-8h5",
  };
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={paths[name]} />
    </svg>
  );
}

function Chart({
  samples,
  field,
  color,
  unit,
}: {
  samples: Sample[];
  field: "price" | "cvd";
  color: string;
  unit: string;
}) {
  const values = samples.filter((s) => s[field] !== null);
  if (values.length < 2)
    return (
      <div className="chart-empty">
        Collecting observed {field === "cvd" ? "volume" : "trades"}…
      </div>
    );
  const data = values.map((s) => Number(s[field]));
  const low = Math.min(...data),
    high = Math.max(...data),
    pad = Math.max((high - low) * 0.18, field === "price" ? 0.5 : 0.05);
  const min = low - pad,
    max = high + pad;
  const start = values[0].at_ms,
    finish = Math.max(values.at(-1)!.at_ms, start + 1);
  const x = (at: number) => 18 + ((at - start) / (finish - start)) * 642;
  const y = (v: number) => 18 + ((max - v) / (max - min)) * 180;
  const line = values
    .map(
      (s, i) =>
        `${i ? "L" : "M"}${x(s.at_ms).toFixed(1)},${y(data[i]).toFixed(1)}`,
    )
    .join(" ");
  const area = `${line} L660,208 L18,208 Z`;
  return (
    <svg
      className="chart"
      viewBox="0 0 760 245"
      role="img"
      aria-label={`${field} chart in ${unit}, observed session only`}
      preserveAspectRatio="none"
    >
      <defs>
        <linearGradient id={`fill-${field}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity=".15" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      {[0, 1, 2, 3, 4].map((i) => (
        <g key={i}>
          <line
            x1="18"
            x2="663"
            y1={18 + i * 45}
            y2={18 + i * 45}
            stroke="#202731"
            strokeDasharray="3 5"
          />
          <text x="683" y={22 + i * 45} fill="#788696" fontSize="10">
            {field === "price"
              ? price(max - ((max - min) * i) / 4)
              : (max - ((max - min) * i) / 4).toFixed(2)}
          </text>
        </g>
      ))}
      <path d={area} fill={`url(#fill-${field})`} />
      <path
        d={line}
        fill="none"
        stroke={color}
        strokeWidth="1.7"
        vectorEffect="non-scaling-stroke"
      />
      <circle cx="660" cy={y(data.at(-1)!)} r="3" fill={color} />
      {[0, 0.25, 0.5, 0.75, 1].map((t) => (
        <text
          key={t}
          x={18 + 642 * t}
          y="232"
          textAnchor={t === 0 ? "start" : t === 1 ? "end" : "middle"}
          fill="#667484"
          fontSize="10"
        >
          {time(start + (finish - start) * t)}
        </text>
      ))}
    </svg>
  );
}

function BookRows({ levels, side }: { levels: Level[]; side: "buy" | "sell" }) {
  const max = Math.max(...levels.map(([, q]) => Number(q)), 0.001);
  let cumulative = 0;
  return (
    <>
      {levels.slice(0, 10).map(([p, q]) => {
        cumulative += Number(q);
        return (
          <div className={`book-row ${side}`} key={p}>
            <span
              className="depth-fill"
              style={{ width: `${(Number(q) / max) * 100}%` }}
            />
            <span>{price(p)}</span>
            <span>{qty(q)}</span>
            <span>{cumulative.toFixed(4)}</span>
          </div>
        );
      })}
    </>
  );
}

export function App() {
  const [v, setV] = useState<View | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"health" | "recordings">("health"),
    [sessions, setSessions] = useState<Session[]>([]);
  const [info, setInfo] = useState(false),
    [online, setOnline] = useState(true);
  useEffect(() => {
    let stopped = false,
      timeout: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = await request<View>({ action: "view" });
        if (!stopped) {
          setV(next);
          setOnline(true);
        }
      } catch (e) {
        if (!stopped) {
          setOnline(false);
          setError(e instanceof Error ? e.message : String(e));
        }
      }
      if (!stopped) timeout = setTimeout(poll, 100);
    };
    void poll();
    return () => {
      stopped = true;
      clearTimeout(timeout);
    };
  }, []);
  async function refreshSessions() {
    try {
      setSessions(await request<Session[]>({ action: "sessions" }));
    } catch (e) {
      setError(String(e));
    }
  }
  useEffect(() => {
    if (tab === "recordings") void refreshSessions();
  }, [tab, v?.recording]);
  async function act(command: Request) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      setV(await request<View>(command));
      setOnline(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  const mode = v?.mode || "simulation",
    healthy = v?.health === "healthy" && online;
  const source =
    mode === "simulation"
      ? "SYNTHETIC DATA"
      : mode === "replay"
        ? "LOCAL REPLAY"
        : "BINANCE SPOT";
  const usable = online && (v?.bids.length || 0) > 0;
  const cvdPositive = Number(v?.cvd || 0) >= 0;
  return (
    <div className="app-shell">
      <aside className="rail">
        <a className="brand-mark" href="#" aria-label="SIGNAL terminal">
          S<span>·</span>
        </a>
        <button
          className={tab === "health" ? "rail-button active" : "rail-button"}
          title="Terminal"
          aria-label="Terminal"
          onClick={() => setTab("health")}
        >
          <Icon name="terminal" />
        </button>
        <button
          className={
            tab === "recordings" ? "rail-button active" : "rail-button"
          }
          title="Recordings"
          aria-label="Recordings"
          onClick={() => setTab("recordings")}
        >
          <Icon name="library" />
        </button>
        <button
          className="rail-button rail-bottom"
          title="Metric definitions"
          aria-label="Metric definitions"
          onClick={() => setInfo(true)}
        >
          <Icon name="info" />
        </button>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="wordmark">
            SIGNAL <span>TERMINAL</span>
            <small>v0.1</small>
          </div>
          <div className="top-actions">
            <span className="read-only">ANALYSIS ONLY</span>
            <span className="utc-clock">
              {v ? time(v.now_ms) : "--:--:--"} <small>UTC</small>
            </span>
          </div>
        </header>
        <section className="toolbar">
          <div className="instrument">
            <div className="coin">₿</div>
            <div>
              <h1>
                BTC <span>/ USDT</span>
              </h1>
              <div className="instrument-sub">
                {mode === "live" ? "Binance" : "Offline workspace"}{" "}
                <span>•</span> Spot market
              </div>
            </div>
          </div>
          <div className="mode-switch" aria-label="Data mode">
            <button
              disabled={busy}
              className={mode === "simulation" ? "selected" : ""}
              onClick={() => act({ action: "mode", mode: "simulation" })}
            >
              Simulation
            </button>
            <button
              disabled={busy}
              className={mode === "live" ? "selected" : ""}
              onClick={() => act({ action: "mode", mode: "live" })}
            >
              Connect live ↗
            </button>
            {mode === "replay" && (
              <span className="selected replay-label">Replay</span>
            )}
          </div>
          <div className="feed-badge">
            <span className={`status-dot ${healthy ? "green" : "amber"}`} />
            {online
              ? v?.paused
                ? "PAUSED"
                : (v?.health || "STARTING").toUpperCase()
              : "ENGINE OFFLINE"}
          </div>
        </section>
        <div className={`source-banner ${mode}`}>
          <span>
            <span className="banner-tag">{source}</span>
            {mode === "simulation"
              ? "Seeded simulation. All prices, trades and liquidity are generated locally."
              : mode === "replay"
                ? `Recorded session #${v?.replay?.session_id} · virtual receive clock`
                : "Public market data · BTC/USDT on one venue · source access rules apply"}
          </span>
          <button onClick={() => setInfo(true)}>
            Data & definitions <span>↗</span>
          </button>
        </div>
        {error && (
          <div className="error-banner" role="alert">
            {error}
            <button onClick={() => setError("")} aria-label="Dismiss error">
              ×
            </button>
          </div>
        )}
        {v?.recording_error && (
          <div className="error-banner" role="alert">
            {v.recording_error}
          </div>
        )}
        <section className="metrics">
          <div className="metric">
            <label>
              LAST OBSERVED PRICE <span>USDT</span>
            </label>
            <strong className={!online ? "muted" : ""}>
              {price(v?.last_price ?? null)}
            </strong>
            <small>
              {v?.trade_state === "fresh" && online
                ? "Latest received trade"
                : `Trade feed ${v?.trade_state || "waiting"}`}
            </small>
          </div>
          <div className="metric">
            <label>
              SESSION CVD <span>BTC</span>
            </label>
            <strong className={cvdPositive ? "positive" : "negative"}>
              {v ? signed(v.cvd) : "—"}
            </strong>
            <small className={v && !v.cvd_complete ? "warning" : ""}>
              {v?.cvd_complete
                ? "Observed buy − sell volume"
                : "Incomplete · observed trades only"}
            </small>
          </div>
          <div className="metric">
            <label>
              BOOK IMBALANCE <span>TOP 20</span>
            </label>
            <strong
              className={
                v?.imbalance && v.imbalance < 0 ? "negative" : "positive"
              }
            >
              {online && v?.imbalance !== null && v?.imbalance !== undefined
                ? `${v.imbalance >= 0 ? "+" : ""}${(v.imbalance * 100).toFixed(1)}%`
                : "—"}
            </strong>
            <small>Displayed bid / ask quantity</small>
          </div>
          <div className="metric">
            <label>
              SPREAD <span>USDT</span>
            </label>
            <strong>{online ? price(v?.spread ?? null) : "—"}</strong>
            <small>
              {online
                ? `Book ${v?.book_state || "waiting"}`
                : "Engine disconnected"}
            </small>
          </div>
          <div className="metric">
            <label>
              OBSERVED TRADES <span>SESSION</span>
            </label>
            <strong>{(v?.trade_count || 0).toLocaleString()}</strong>
            <small>
              {v
                ? `${qty(Number(v.buy_volume) + Number(v.sell_volume))} BTC total`
                : "Collecting trades"}
            </small>
          </div>
        </section>
        <main className="terminal-grid">
          <section className="chart-stack panel">
            <div className="panel-heading">
              <div>
                <h2>PRICE ACTION</h2>
                <span className="panel-sub">Observed trades · USDT</span>
              </div>
              <span className="chip">LAST 120s</span>
            </div>
            <div className="price-chart">
              {v && (
                <Chart
                  samples={v.history}
                  field="price"
                  color="#82aaf5"
                  unit="USDT"
                />
              )}
            </div>
            <div className="chart-divider" />
            <div className="panel-heading compact">
              <div>
                <h2>CUMULATIVE VOLUME DELTA</h2>
                <span className="panel-sub">
                  Session · aggressor volume in BTC
                </span>
              </div>
              <span className={cvdPositive ? "positive" : "negative"}>
                {v ? signed(v.cvd) : "—"}
              </span>
            </div>
            <div className="cvd-chart">
              {v && (
                <Chart
                  samples={v.history}
                  field="cvd"
                  color="#3ed1ac"
                  unit="BTC"
                />
              )}
            </div>
            <div className="chart-foot">
              <span>
                <i className="legend-dot blue" />
                Last trade
              </span>
              <span>
                <i className="legend-dot green" />
                Buy − sell volume
              </span>
              <span>
                {v?.cvd_complete
                  ? "Session coverage"
                  : "⚠ Incomplete session coverage"}
              </span>
            </div>
          </section>
          <section className="panel orderbook">
            <div className="panel-heading">
              <div>
                <h2>ORDER BOOK</h2>
                <span className="panel-sub">
                  10 shown · 20 used for imbalance
                </span>
              </div>
              <span className="chip">L2</span>
            </div>
            <div className="table-head">
              <span>PRICE (USDT)</span>
              <span>SIZE (BTC)</span>
              <span>TOTAL</span>
            </div>
            {usable ? (
              <>
                <div className="book-asks">
                  <BookRows levels={v!.asks} side="sell" />
                </div>
                <div className="mid-price">
                  <strong>{price(v!.last_price)}</strong>
                  <span>Spread {price(v!.spread)}</span>
                </div>
                <BookRows levels={v!.bids} side="buy" />
              </>
            ) : (
              <div className="book-empty">
                <Icon name="pulse" />
                <strong>
                  {online ? "Book unavailable" : "Engine disconnected"}
                </strong>
                <span>
                  {v?.book_state === "stale"
                    ? "Depth is stale. Derived liquidity values are hidden."
                    : "Waiting for a fresh snapshot and contiguous depth updates."}
                </span>
              </div>
            )}
            <div className="book-foot">
              <span>Sequence</span>
              <span>{v?.update_id || "—"}</span>
            </div>
          </section>
          <section className="panel tape">
            <div className="panel-heading">
              <div>
                <h2>TRADE TAPE</h2>
                <span className="panel-sub">Raw trades · latest 100</span>
              </div>
              <span className="tiny-dot" />
            </div>
            <div className="table-head">
              <span>TIME · UTC</span>
              <span>PRICE</span>
              <span>BTC / SIDE</span>
            </div>
            <div className="tape-scroll">
              {v?.tape.map((t) => (
                <div className="tape-row" key={t.id}>
                  <span>{time(t.at_ms)}</span>
                  <span
                    className={t.aggressor === "buy" ? "positive" : "negative"}
                  >
                    {price(t.price)}
                  </span>
                  <span>
                    {qty(t.qty)}{" "}
                    <b
                      className={
                        t.aggressor === "buy" ? "positive" : "negative"
                      }
                    >
                      {t.aggressor === "buy" ? "B" : "S"}
                    </b>
                  </span>
                </div>
              ))}
              {!v?.tape.length && (
                <div className="small-empty">Waiting for observed trades…</div>
              )}
            </div>
            <div className="tape-foot">
              <span className="positive">B = buy aggressor</span>
              <span className="negative">S = sell aggressor</span>
            </div>
          </section>
        </main>
        <section className="bottom-panel panel">
          <div className="bottom-bar">
            <div className="tabs">
              <button
                className={tab === "health" ? "active" : ""}
                onClick={() => setTab("health")}
              >
                Feed health
              </button>
              <button
                className={tab === "recordings" ? "active" : ""}
                onClick={() => setTab("recordings")}
              >
                Recordings
              </button>
            </div>
            <div className="controls">
              {mode === "simulation" && (
                <button
                  disabled={busy}
                  onClick={() => act({ action: "inject_gap" })}
                >
                  Inject depth gap
                </button>
              )}
              {mode === "replay" && (
                <select
                  aria-label="Replay speed"
                  value={v?.speed}
                  disabled={busy}
                  onChange={(e) =>
                    act({ action: "speed", speed: Number(e.target.value) })
                  }
                >
                  {[1, 2, 4, 8].map((n) => (
                    <option key={n} value={n}>
                      {n}× speed
                    </option>
                  ))}
                </select>
              )}
              {mode !== "live" && (
                <button
                  disabled={busy || !!v?.replay?.finished}
                  onClick={() => act({ action: "pause", paused: !v?.paused })}
                >
                  {v?.paused ? "▶ Resume" : "Ⅱ Pause"}
                </button>
              )}
              {mode !== "replay" && (
                <button
                  className={v?.recording ? "record active" : "record"}
                  disabled={busy}
                  onClick={() =>
                    act({ action: "record", enabled: !v?.recording })
                  }
                >
                  <span className="record-dot" />
                  {v?.recording ? "Stop recording" : "Record session"}
                </button>
              )}
            </div>
          </div>
          {tab === "health" ? (
            <div className="health-content">
              <div className="health-cells">
                <div>
                  <label>DEPTH RECEIVE AGE</label>
                  <strong>{age(v?.book_age_ms ?? null)}</strong>
                  <span>{v?.book_state || "waiting"}</span>
                </div>
                <div>
                  <label>TRADE RECEIVE AGE</label>
                  <strong>{age(v?.trade_age_ms ?? null)}</strong>
                  <span>{v?.trade_state || "waiting"}</span>
                </div>
                <div>
                  <label>EVENT CLOCK OFFSET</label>
                  <strong>
                    {v?.event_offset_ms ?? "—"} <small>ms</small>
                  </strong>
                  <span>Clock-dependent · not latency</span>
                </div>
                <div>
                  <label>GAPS / DUPLICATES</label>
                  <strong>
                    {v?.gaps || 0} <small>/ {v?.duplicates || 0}</small>
                  </strong>
                  <span>Session diagnostics</span>
                </div>
                <div>
                  <label>LOCAL RECORDER</label>
                  <strong>
                    {v?.recording ? `REC #${v.recording}` : "IDLE"}
                  </strong>
                  <span>
                    {v?.recording
                      ? `${v.recorded_events.toLocaleString()} events`
                      : "SQLite · WAL"}
                  </span>
                </div>
              </div>
              <div className="health-note">
                <Icon name="pulse" />
                <span>{v?.note || "Starting the shared Rust engine…"}</span>
                {v?.replay && (
                  <span className="replay-progress">
                    {v.replay.cursor.toLocaleString()} /{" "}
                    {v.replay.total.toLocaleString()}
                    {v.replay.finished ? " · complete" : ""}
                  </span>
                )}
              </div>
            </div>
          ) : (
            <div className="recordings-content">
              <div className="recordings-intro">
                <span>
                  Local SQLite sessions. Recording starts a fresh session with a
                  snapshot bootstrap.
                </span>
                <button onClick={refreshSessions}>Refresh ↻</button>
              </div>
              {sessions.length ? (
                <div className="session-list">
                  {sessions.map((s) => (
                    <div className="session-row" key={s.id}>
                      <span className="session-id">#{s.id}</span>
                      <span>{s.source.toUpperCase()}</span>
                      <span>
                        {new Date(s.started_ms)
                          .toISOString()
                          .replace("T", " ")
                          .slice(0, 19)}{" "}
                        UTC
                      </span>
                      <span>{s.event_count.toLocaleString()} events</span>
                      <button
                        disabled={busy || !s.closed}
                        onClick={() =>
                          act({ action: "replay", session_id: s.id })
                        }
                      >
                        {s.closed ? "Replay →" : "Recording…"}
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="small-empty">
                  No recordings yet. Select “Record session”, then stop to
                  replay it.
                </div>
              )}
            </div>
          )}
        </section>
        <footer className="statusbar">
          <span>
            <span className={`status-dot ${healthy ? "green" : "amber"}`} />
            {source} <span className="footer-sep">/</span>{" "}
            {online ? "Rust engine" : "Disconnected"}
          </span>
          <span>
            BTC/USDT · SPOT <span className="footer-sep">/</span> Local first{" "}
            <span className="footer-sep">/</span> No order execution
          </span>
        </footer>
      </div>
      {info && (
        <div className="modal-backdrop" onClick={() => setInfo(false)}>
          <section
            className="definitions-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="definitions-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-heading">
              <h2 id="definitions-title">Data & metric definitions</h2>
              <button
                aria-label="Close definitions"
                onClick={() => setInfo(false)}
              >
                ×
              </button>
            </div>
            <dl>
              <dt>Session CVD · BTC</dt>
              <dd>
                Observed buy-aggressor quantity minus sell-aggressor quantity
                since this session started. Binance’s buyer-maker flag{" "}
                <code>m=false</code> means buy aggressor; <code>m=true</code>{" "}
                means sell aggressor. Duplicate trade IDs are ignored. A missing
                ID or reconnect marks coverage incomplete until a new session.
              </dd>
              <dt>Book imbalance · top 20</dt>
              <dd>
                (Bid BTC − ask BTC) / (bid BTC + ask BTC), using the best 20
                levels per side. Stale, unsynchronized or insufficiently covered
                books show no value.
              </dd>
              <dt>Feed health</dt>
              <dd>
                Depth is stale after 2 seconds without an accepted update;
                trades after 5 seconds. These are local receive ages. Event
                clock offset includes clock error and is not a measured one-way
                latency. Pausing simulation or replay freezes virtual time.
              </dd>
              <dt>Recording & replay</dt>
              <dd>
                Local SQLite stores ordered normalized events, snapshots and
                reset boundaries. Recording starts a new session. Replay uses
                the same Rust engine and virtual receive times. Limit: 100,000
                events / 64 MiB per replay in v0.1.
              </dd>
              <dt>Live source & use</dt>
              <dd>
                Binance public market-data-only streams and REST, BTC/USDT spot.
                Source access restrictions and cooldowns are enforced. Public
                access does not grant redistribution rights; review current
                terms before commercial use.
              </dd>
              <dt>Local database</dt>
              <dd>
                <code className="db-path">{v?.db_path || "Starting…"}</code>
              </dd>
            </dl>
            <p className="modal-note">
              Simulation is generated data. Venue liquidity and CVD describe
              observed activity; they do not promise price direction or
              investment results.
            </p>
            <button className="modal-done" onClick={() => setInfo(false)}>
              Back to terminal
            </button>
          </section>
        </div>
      )}
    </div>
  );
}
