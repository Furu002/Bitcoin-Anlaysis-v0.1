use crate::model::{Depth, Event, Fixed, Levels, Snapshot, Trade};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WsError, Message},
};

const WS_URL: &str =
    "wss://data-stream.binance.vision/stream?streams=btcusdt@trade/btcusdt@depth@100ms";
const REST_URL: &str = "https://data-api.binance.vision/api/v3/depth?symbol=BTCUSDT&limit=1000";

#[derive(Default)]
pub struct AccessPolicy {
    pub blocked: bool,
    pub until: Option<Instant>,
}
impl AccessPolicy {
    pub fn allowed(&self) -> Result<(), String> {
        if self.blocked {
            return Err(
                "Source denied access. Live mode is disabled for this app run; use simulation."
                    .into(),
            );
        }
        if self.until.is_some_and(|t| t > Instant::now()) {
            return Err("Exchange cooldown is active; use simulation until it expires.".into());
        }
        Ok(())
    }
}
pub type Policy = Arc<Mutex<AccessPolicy>>;

#[derive(Clone)]
pub struct Clock {
    pub base_ms: u64,
    pub started: Instant,
}
impl Clock {
    pub fn now(&self) -> u64 {
        self.base_ms + self.started.elapsed().as_millis() as u64
    }
}
pub struct Input {
    pub generation: u64,
    pub received_ms: u64,
    pub event: Event,
}

#[derive(Debug)]
struct FeedError {
    note: String,
    blocked: bool,
    cooldown: Option<u64>,
}
impl FeedError {
    fn transient(note: impl Into<String>) -> Self {
        Self {
            note: note.into(),
            blocked: false,
            cooldown: None,
        }
    }
    fn http(code: u16, retry: Option<u64>) -> Self {
        Self {
            note: format!("Public source returned HTTP {code}"),
            blocked: matches!(code, 403 | 451 | 418),
            cooldown: if code == 429 {
                Some(retry.unwrap_or(60).max(1))
            } else {
                None
            },
        }
    }
}

#[derive(Deserialize)]
struct WireSnapshot {
    #[serde(rename = "lastUpdateId")]
    last_id: u64,
    bids: Levels,
    asks: Levels,
}
#[derive(Deserialize)]
struct WireDepth {
    s: String,
    #[serde(rename = "U")]
    first_id: u64,
    #[serde(rename = "u")]
    last_id: u64,
    #[serde(rename = "E")]
    event_ms: u64,
    b: Levels,
    a: Levels,
}
#[derive(Deserialize)]
struct WireTrade {
    s: String,
    t: u64,
    #[serde(rename = "E")]
    event_ms: u64,
    #[serde(rename = "T")]
    trade_ms: u64,
    p: Fixed,
    q: Fixed,
    m: bool,
}

pub fn decode(json: &str) -> Result<Event, String> {
    if json.len() > 262_144 {
        return Err("Oversized stream message".into());
    }
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Malformed stream JSON: {e}"))?;
    let value = root.get("data").unwrap_or(&root);
    match value.get("e").and_then(|v| v.as_str()) {
        Some("depthUpdate") => {
            let d: WireDepth = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            if d.s != "BTCUSDT" {
                return Err("Unexpected market symbol".into());
            }
            Ok(Event::Depth(Depth {
                first_id: d.first_id,
                last_id: d.last_id,
                event_ms: d.event_ms,
                bids: d.b,
                asks: d.a,
            }))
        }
        Some("trade") => {
            let t: WireTrade = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            if t.s != "BTCUSDT" {
                return Err("Unexpected market symbol".into());
            }
            Ok(Event::Trade(Trade {
                id: t.t,
                event_ms: t.event_ms,
                trade_ms: t.trade_ms,
                price: t.p,
                qty: t.q,
                buyer_maker: t.m,
            }))
        }
        Some("serverShutdown") => Err("Exchange server shutdown: reconnect required".into()),
        _ => Err("Unexpected event on fixed market streams".into()),
    }
}

async fn send(
    tx: &mpsc::Sender<Input>,
    generation: u64,
    clock: &Clock,
    event: Event,
) -> Result<(), FeedError> {
    let received_ms = clock.now();
    tokio::time::timeout(
        Duration::from_millis(200),
        tx.send(Input {
            generation,
            received_ms,
            event,
        }),
    )
    .await
    .map_err(|_| FeedError::transient("Ingest queue overflow: resynchronization required"))?
    .map_err(|_| FeedError::transient("Engine stopped"))
}

async fn snapshot(client: &reqwest::Client) -> Result<Event, FeedError> {
    let res = client
        .get(REST_URL)
        .send()
        .await
        .map_err(|e| FeedError::transient(format!("Snapshot transport: {e}")))?;
    if !res.status().is_success() {
        let retry = res
            .headers()
            .get("retry-after")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok());
        return Err(FeedError::http(res.status().as_u16(), retry));
    }
    let bytes = res
        .bytes()
        .await
        .map_err(|e| FeedError::transient(e.to_string()))?;
    if bytes.len() > 1_048_576 {
        return Err(FeedError::transient("Oversized REST snapshot"));
    }
    let s: WireSnapshot = serde_json::from_slice(&bytes)
        .map_err(|e| FeedError::transient(format!("Snapshot parse: {e}")))?;
    Ok(Event::Snapshot(Snapshot {
        last_id: s.last_id,
        bids: s.bids,
        asks: s.asks,
    }))
}

async fn connection(
    tx: &mpsc::Sender<Input>,
    generation: u64,
    clock: &Clock,
    client: &reqwest::Client,
) -> Result<(), FeedError> {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(WS_URL))
        .await
        .map_err(|_| FeedError::transient("WebSocket connection timeout"))?
        .map_err(|e| match e {
            WsError::Http(r) => FeedError::http(
                r.status().as_u16(),
                r.headers()
                    .get("retry-after")
                    .and_then(|s| s.to_str().ok())
                    .and_then(|s| s.parse().ok()),
            ),
            other => FeedError::transient(format!("WebSocket transport: {other}")),
        })?;
    send(
        tx,
        generation,
        clock,
        Event::Reset {
            reason: "New WebSocket epoch; buffering depth for REST snapshot".into(),
        },
    )
    .await?;
    send(
        tx,
        generation,
        clock,
        Event::Status {
            state: "connected".into(),
            note: "Binance public spot · awaiting snapshot bridge".into(),
        },
    )
    .await?;
    // The reader remains active while this independent REST future is pending.
    let mut snapshot_future = Box::pin(snapshot(client));
    let mut snapshot_pending = true;
    let opened = Instant::now();
    let mut last_frame = Instant::now();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            s = &mut snapshot_future, if snapshot_pending => {
                snapshot_pending = false; send(tx, generation, clock, s?).await?;
            },
            msg = socket.next() => {
                last_frame = Instant::now();
                match msg {
                    Some(Ok(Message::Text(t))) => send(tx, generation, clock, decode(t.as_str()).map_err(FeedError::transient)?).await?,
                    // Tungstenite queues an identical-payload pong automatically. Flush promptly.
                    Some(Ok(Message::Ping(_))) => tokio::time::timeout(Duration::from_secs(2), socket.flush()).await
                        .map_err(|_| FeedError::transient("Pong write timeout"))?.map_err(|e| FeedError::transient(e.to_string()))?,
                    Some(Ok(Message::Pong(_))) => {},
                    Some(Ok(Message::Close(_))) | None => return Err(FeedError::transient("Exchange closed the stream")),
                    Some(Err(e)) => return Err(FeedError::transient(e.to_string())),
                    _ => return Err(FeedError::transient("Unexpected binary WebSocket frame")),
                }
            },
            _ = heartbeat.tick() => {
                if last_frame.elapsed() > Duration::from_secs(15) { return Err(FeedError::transient("Transport stalled for 15 seconds")); }
                if opened.elapsed() > Duration::from_secs(23 * 3600 + 55 * 60) { return Err(FeedError::transient("Scheduled reconnect before 24-hour lifetime")); }
            }
        }
    }
}

pub async fn run(
    tx: mpsc::Sender<Input>,
    generation: u64,
    clock: Clock,
    policy: Policy,
    initial_delay: bool,
) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .https_only(true)
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut attempt = 0_u32;
    if initial_delay {
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    while !tx.is_closed() {
        let cooldown = policy
            .lock()
            .ok()
            .and_then(|p| p.until)
            .and_then(|t| t.checked_duration_since(Instant::now()));
        if let Some(delay) = cooldown {
            tokio::time::sleep(delay).await;
        }
        if policy.lock().map_or(true, |p| p.blocked) {
            return;
        }
        let started = Instant::now();
        let err = match connection(&tx, generation, &clock, &client).await {
            Ok(()) => return,
            Err(e) => e,
        };
        if started.elapsed() > Duration::from_secs(60) {
            attempt = 0;
        }
        if err.blocked {
            if let Ok(mut p) = policy.lock() {
                p.blocked = true;
            }
            let _ = tx
                .send(Input {
                    generation,
                    received_ms: clock.now(),
                    event: Event::Status {
                        state: "blocked".into(),
                        note: format!(
                            "{}. Live source stopped; simulation is available.",
                            err.note
                        ),
                    },
                })
                .await;
            return;
        }
        let state = if err.cooldown.is_some() {
            "cooldown"
        } else {
            "syncing"
        };
        if let Some(seconds) = err.cooldown {
            if let Ok(mut p) = policy.lock() {
                p.until = Instant::now().checked_add(Duration::from_secs(seconds));
            }
        }
        if tx
            .send(Input {
                generation,
                received_ms: clock.now(),
                event: Event::Status {
                    state: state.into(),
                    note: err.note,
                },
            })
            .await
            .is_err()
        {
            return;
        }
        attempt = (attempt + 1).min(5);
        let jitter = (generation.wrapping_mul(137) + attempt as u64 * 173) % 500;
        tokio::time::sleep(Duration::from_millis((1_u64 << attempt) * 1000 + jitter)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_trade_mapping_and_symbol_validation() {
        let json = r#"{"stream":"btcusdt@trade","data":{"e":"trade","s":"BTCUSDT","t":7,"E":1000,"T":990,"p":"64000.01","q":"0.00000001","m":true}}"#;
        let Event::Trade(t) = decode(json).unwrap() else {
            panic!()
        };
        assert!(t.buyer_maker);
        assert_eq!(t.qty, Fixed(1));
        assert!(decode(&json.replace("BTCUSDT", "ETHUSDT")).is_err());
        assert!(decode(&json.replace("0.00000001", "-1")).is_err());
        assert!(decode("{}").is_err());
    }
    #[test]
    fn access_denials_and_rate_limits_persist_across_restarts() {
        for code in [403, 451, 418] {
            assert!(FeedError::http(code, None).blocked);
        }
        assert_eq!(FeedError::http(429, Some(120)).cooldown, Some(120));
        assert_eq!(FeedError::http(429, None).cooldown, Some(60));
        let p = AccessPolicy {
            blocked: false,
            until: Some(Instant::now() + Duration::from_secs(60)),
        };
        assert!(p.allowed().is_err());
    }
}
