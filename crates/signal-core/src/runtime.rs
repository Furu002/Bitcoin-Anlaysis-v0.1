use crate::{
    engine::Engine,
    feed::{self, AccessPolicy, Input, Policy},
    model::{Envelope, Event},
    simulation::Simulation,
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    View,
    Mode { mode: String },
    Pause { paused: bool },
    Speed { speed: u32 },
    InjectGap,
    Record { enabled: bool },
    Sessions,
    Replay { session_id: i64 },
    Shutdown,
}
struct Rpc {
    request: Request,
    reply: oneshot::Sender<Result<Value, String>>,
}
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Rpc>,
}
impl Handle {
    pub async fn start(path: PathBuf) -> Result<Self, String> {
        let store = Store::open(&path)?;
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(Actor::new(store, path).run(rx));
        Ok(Self { tx })
    }
    pub async fn call(&self, request: Request) -> Result<Value, String> {
        let (reply, result) = oneshot::channel();
        self.tx
            .send(Rpc { request, reply })
            .await
            .map_err(|_| "Engine is unavailable")?;
        tokio::time::timeout(Duration::from_secs(10), result)
            .await
            .map_err(|_| "Engine request timed out")?
            .map_err(|_| "Engine stopped")?
    }
}

struct Replay {
    id: i64,
    frames: Vec<Envelope>,
    cursor: usize,
    clock: u64,
    end: u64,
}
struct Actor {
    engine: Engine,
    sim: Simulation,
    store: Store,
    db_path: PathBuf,
    mode: String,
    paused: bool,
    speed: u32,
    now: u64,
    base_ms: u64,
    base_clock: Instant,
    replay: Option<Replay>,
    policy: Policy,
    generation: u64,
    feed_task: Option<JoinHandle<()>>,
    input_tx: mpsc::Sender<Input>,
    input_rx: mpsc::Receiver<Input>,
    recording_error: Option<String>,
    recovery_ticks: u32,
    ticks: u64,
}
impl Actor {
    fn new(store: Store, path: PathBuf) -> Self {
        let (input_tx, input_rx) = mpsc::channel(8192);
        let base_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let mut actor = Self {
            engine: Engine::default(),
            sim: Simulation::default(),
            store,
            db_path: path,
            mode: "simulation".into(),
            paused: false,
            speed: 1,
            now: base_ms.saturating_sub(12000),
            base_ms,
            base_clock: Instant::now(),
            replay: None,
            policy: Arc::new(Mutex::new(AccessPolicy::default())),
            generation: 0,
            feed_task: None,
            input_tx,
            input_rx,
            recording_error: None,
            recovery_ticks: 0,
            ticks: 0,
        };
        for event in actor.sim.bootstrap() {
            actor.process(event);
        }
        for _ in 0..120 {
            actor.now += 100;
            actor.sim_tick();
            actor.engine.sample(actor.now);
        }
        actor
    }
    fn wall(&self) -> u64 {
        self.base_ms + self.base_clock.elapsed().as_millis() as u64
    }
    fn process(&mut self, event: Event) {
        let frame = Envelope {
            received_ms: self.now,
            event,
        };
        self.process_frame(frame);
    }
    fn process_frame(&mut self, frame: Envelope) {
        if let Err(e) = self.store.push(&frame) {
            self.recording_error = Some(format!("Recording stopped: {e}"));
            self.store.fail();
        }
        if let Err(reason) = self.engine.apply(&frame) {
            // Recovery is a recorded boundary, so replay cannot invent continuity.
            let reset = Envelope {
                received_ms: self.now,
                event: Event::Reset {
                    reason: reason.clone(),
                },
            };
            if let Err(e) = self.store.push(&reset) {
                self.recording_error = Some(e);
                self.store.fail();
            }
            let _ = self.engine.apply(&reset);
            if self.mode == "live" {
                self.restart_feed(true);
            } else if self.mode == "simulation" {
                self.recovery_ticks = 10;
            }
        }
    }
    fn restart_feed(&mut self, delay: bool) {
        if let Some(task) = self.feed_task.take() {
            task.abort();
        }
        self.generation += 1;
        let clock = feed::Clock {
            base_ms: self.base_ms,
            started: self.base_clock,
        };
        self.feed_task = Some(tokio::spawn(feed::run(
            self.input_tx.clone(),
            self.generation,
            clock,
            self.policy.clone(),
            delay,
        )));
    }
    fn sim_tick(&mut self) {
        if self.recovery_ticks > 0 {
            self.recovery_ticks -= 1;
            if self.recovery_ticks == 0 {
                for event in self.sim.bootstrap() {
                    self.process(event);
                }
            }
            return;
        }
        for event in self.sim.step(self.now) {
            self.process(event);
        }
    }
    fn reset_mode(&mut self, mode: &str) {
        if let Some(task) = self.feed_task.take() {
            task.abort();
        }
        self.generation += 1;
        self.engine = Engine::default();
        self.mode = mode.into();
        self.paused = false;
        self.speed = 1;
        self.replay = None;
        self.recovery_ticks = 0;
        self.now = self.wall();
        self.sim = Simulation::default();
        if mode == "simulation" {
            for event in self.sim.bootstrap() {
                self.process(event);
            }
        } else if mode == "live" {
            self.restart_feed(false);
        }
    }
    fn view(&self) -> Value {
        let mut value =
            serde_json::to_value(self.engine.view(&self.mode, self.now)).unwrap_or(Value::Null);
        value["paused"] = json!(self.paused);
        value["speed"] = json!(self.speed);
        value["recording"] = json!(self.store.active());
        value["recorded_events"] = json!(self.store.count());
        value["recording_error"] = json!(self.recording_error);
        value["db_path"] = json!(self.db_path);
        value["replay"] = self.replay.as_ref().map_or(Value::Null, |r| json!({"session_id":r.id,"cursor":r.cursor,"total":r.frames.len(),"finished":r.cursor==r.frames.len()}));
        value
    }
    fn request(&mut self, request: Request) -> Result<Value, String> {
        match request {
            Request::View => return Ok(self.view()),
            Request::Mode { mode } => {
                if !["simulation", "live"].contains(&mode.as_str()) {
                    return Err("Unknown mode".into());
                }
                if mode == "live" {
                    self.policy
                        .lock()
                        .map_err(|_| "Policy unavailable")?
                        .allowed()?;
                }
                self.store.stop()?;
                self.reset_mode(&mode);
            }
            Request::Pause { paused } => {
                if self.mode == "live" {
                    return Err(
                        "Live ingestion cannot be paused; switch to simulation or replay".into(),
                    );
                }
                self.paused = paused;
            }
            Request::Speed { speed } => {
                if self.mode != "replay" || ![1, 2, 4, 8].contains(&speed) {
                    return Err("Replay speed must be 1, 2, 4 or 8".into());
                }
                self.speed = speed;
            }
            Request::InjectGap => {
                if self.mode != "simulation" {
                    return Err("Gap injection is available in simulation".into());
                }
                self.sim.skip_depth();
            }
            Request::Record { enabled } => {
                if enabled {
                    if self.mode == "replay" {
                        return Err("Replay cannot be recorded".into());
                    }
                    if self.store.active().is_some() {
                        return Ok(self.view());
                    }
                    if self.mode == "live" {
                        self.policy
                            .lock()
                            .map_err(|_| "Policy unavailable")?
                            .allowed()?;
                    }
                    self.recording_error = None;
                    self.store.start(&self.mode, self.wall())?;
                    let mode = self.mode.clone();
                    self.reset_mode(&mode);
                } else {
                    self.store.stop()?;
                }
            }
            Request::Sessions => {
                self.store.flush()?;
                return serde_json::to_value(self.store.sessions()?).map_err(|e| e.to_string());
            }
            Request::Replay { session_id } => {
                self.store.stop()?;
                let frames = self.store.load(session_id)?;
                self.reset_mode("replay");
                let start = frames[0].received_ms;
                let end = frames.last().unwrap().received_ms;
                self.now = start;
                self.replay = Some(Replay {
                    id: session_id,
                    frames,
                    cursor: 0,
                    clock: start,
                    end,
                });
                self.replay_tick(0);
            }
            Request::Shutdown => {
                self.store.stop()?;
                return Ok(json!({"shutdown":true}));
            }
        }
        Ok(self.view())
    }
    fn replay_tick(&mut self, advance: u64) {
        let Some(r) = &mut self.replay else {
            return;
        };
        r.clock = r.clock.saturating_add(advance).min(r.end);
        // Bound per-tick replay work; unfinished same-time bursts are continued next tick.
        let mut budget = 4096;
        while r.cursor < r.frames.len() && r.frames[r.cursor].received_ms <= r.clock && budget > 0 {
            let frame = &r.frames[r.cursor];
            self.now = frame.received_ms;
            let _ = self.engine.apply(frame);
            r.cursor += 1;
            budget -= 1;
        }
        self.now = r.clock;
        if r.cursor == r.frames.len() {
            self.paused = true;
        }
    }
    async fn run(mut self, mut rx: mpsc::Receiver<Rpc>) {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                rpc = rx.recv() => {
                    let Some(rpc) = rpc else { break; };
                    let shutdown = matches!(rpc.request, Request::Shutdown);
                    let result = self.request(rpc.request); let _ = rpc.reply.send(result);
                    if shutdown { break; }
                },
                input = self.input_rx.recv(), if self.mode == "live" => {
                    if let Some(input) = input { if input.generation == self.generation { self.now = self.wall(); self.process_frame(Envelope { received_ms: input.received_ms, event: input.event }); } }
                },
                _ = tick.tick() => {
                    self.ticks += 1;
                    if self.mode == "live" { self.now = self.wall(); }
                    else if !self.paused {
                        if self.mode == "simulation" { self.now += 100; self.sim_tick(); }
                        else { self.replay_tick(100 * self.speed as u64); }
                    }
                    if !self.paused { self.engine.sample(self.now); }
                    if self.ticks % 3 == 0 {
                        if let Err(e) = self.store.flush() { self.recording_error = Some(format!("Recording stopped: {e}")); self.store.fail(); }
                    }
                }
            }
        }
        if let Some(task) = self.feed_task {
            task.abort();
        }
        let _ = self.store.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn controls_validate_and_simulation_recovers_from_gap() {
        let dir = tempfile::tempdir().unwrap();
        let handle = Handle::start(dir.path().join("runtime.sqlite3"))
            .await
            .unwrap();
        assert!(handle
            .call(Request::Mode {
                mode: "futures".into()
            })
            .await
            .is_err());
        assert!(handle.call(Request::Speed { speed: 3 }).await.is_err());
        let initial = handle.call(Request::View).await.unwrap();
        assert_eq!(initial["mode"], "simulation");
        assert_eq!(initial["health"], "healthy");
        handle.call(Request::InjectGap).await.unwrap();
        tokio::time::sleep(Duration::from_millis(220)).await;
        let gap = handle.call(Request::View).await.unwrap();
        assert_eq!(gap["book_state"], "syncing");
        assert!(gap["imbalance"].is_null());
        tokio::time::sleep(Duration::from_millis(1300)).await;
        assert_eq!(
            handle.call(Request::View).await.unwrap()["health"],
            "healthy"
        );
        handle.call(Request::Shutdown).await.unwrap();
    }
}
