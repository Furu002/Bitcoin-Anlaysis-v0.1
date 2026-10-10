use crate::model::Envelope;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

pub const REPLAY_CAP: usize = 100_000;
pub const REPLAY_BYTE_CAP: usize = 64 * 1024 * 1024;
const FORMAT_VERSION: i64 = 1;

#[derive(Serialize)]
pub struct Session {
    pub id: i64,
    pub source: String,
    pub started_ms: u64,
    pub event_count: u64,
    pub closed: bool,
}

pub struct Store {
    db: Connection,
    active: Option<i64>,
    seq: u64,
    batch: Vec<Envelope>,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(1))
            .map_err(|e| e.to_string())?;
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version != 0 && version != FORMAT_VERSION {
            return Err(format!("Unsupported recording schema {version}"));
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS sessions(id INTEGER PRIMARY KEY, source TEXT NOT NULL, started_ms INTEGER NOT NULL, closed INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS events(session_id INTEGER NOT NULL REFERENCES sessions(id), seq INTEGER NOT NULL, received_ms INTEGER NOT NULL, version INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(session_id,seq));
            PRAGMA user_version=1;
            UPDATE sessions SET closed=1 WHERE closed=0;").map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            active: None,
            seq: 0,
            batch: vec![],
        })
    }
    pub fn active(&self) -> Option<i64> {
        self.active
    }
    pub fn count(&self) -> u64 {
        self.seq + self.batch.len() as u64
    }
    pub fn start(&mut self, mode: &str, now: u64) -> Result<i64, String> {
        self.stop()?;
        self.db
            .execute(
                "INSERT INTO sessions(source,started_ms) VALUES (?1,?2)",
                params![mode, now as i64],
            )
            .map_err(|e| e.to_string())?;
        let id = self.db.last_insert_rowid();
        self.active = Some(id);
        self.seq = 0;
        Ok(id)
    }
    pub fn push(&mut self, frame: &Envelope) -> Result<(), String> {
        if self.active.is_none() {
            return Ok(());
        }
        self.batch.push(frame.clone());
        if self.batch.len() >= 128 {
            self.flush()?;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), String> {
        let Some(id) = self.active else {
            return Ok(());
        };
        if self.batch.is_empty() {
            return Ok(());
        }
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare("INSERT INTO events VALUES (?1,?2,?3,?4,?5)")
                .map_err(|e| e.to_string())?;
            for (i, frame) in self.batch.iter().enumerate() {
                stmt.execute(params![
                    id,
                    self.seq + i as u64,
                    frame.received_ms as i64,
                    FORMAT_VERSION,
                    serde_json::to_string(frame).map_err(|e| e.to_string())?
                ])
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        self.seq += self.batch.len() as u64;
        self.batch.clear();
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), String> {
        self.flush()?;
        if let Some(id) = self.active {
            self.db
                .execute("UPDATE sessions SET closed=1 WHERE id=?1", [id])
                .map_err(|e| e.to_string())?;
        }
        self.active = None;
        Ok(())
    }
    pub fn fail(&mut self) {
        if let Some(id) = self.active {
            let _ = self
                .db
                .execute("UPDATE sessions SET closed=1 WHERE id=?1", [id]);
        }
        self.active = None;
        self.batch.clear();
    }
    pub fn sessions(&self) -> Result<Vec<Session>, String> {
        let mut stmt = self.db.prepare("SELECT s.id,s.source,s.started_ms,s.closed,COUNT(e.seq) FROM sessions s LEFT JOIN events e ON s.id=e.session_id GROUP BY s.id ORDER BY s.id DESC LIMIT 30").map_err(|e| e.to_string())?;
        let result = stmt
            .query_map([], |r| {
                Ok(Session {
                    id: r.get(0)?,
                    source: r.get(1)?,
                    started_ms: r.get(2)?,
                    closed: r.get(3)?,
                    event_count: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        result
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    pub fn load(&self, id: i64) -> Result<Vec<Envelope>, String> {
        let (count, bytes): (usize, usize) = self
            .db
            .query_row(
                "SELECT COUNT(*),COALESCE(SUM(LENGTH(payload)),0) FROM events WHERE session_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("Session is empty or does not exist".into());
        }
        if count > REPLAY_CAP {
            return Err(format!("v0.1 replay limit is {REPLAY_CAP} events; this session has {count}. Pagination is planned."));
        }
        if bytes > REPLAY_BYTE_CAP {
            return Err("v0.1 replay payload limit is 64 MiB; pagination is planned".into());
        }
        let mut stmt = self.db.prepare("SELECT seq,received_ms,version,payload FROM events WHERE session_id=?1 ORDER BY seq").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, u64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut frames = Vec::with_capacity(count);
        for row in rows {
            let (seq, received_ms, version, json) = row.map_err(|e| e.to_string())?;
            if version != FORMAT_VERSION {
                return Err("Unsupported event format".into());
            }
            let frame: Envelope =
                serde_json::from_str(&json).map_err(|e| format!("Corrupt recording: {e}"))?;
            if seq != frames.len() as u64
                || received_ms != frame.received_ms
                || frames
                    .last()
                    .is_some_and(|p: &Envelope| p.received_ms > received_ms)
            {
                return Err(
                    "Corrupt recording: noncontiguous order or receive-time mismatch".into(),
                );
            }
            frames.push(frame);
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Engine, simulation::Simulation};
    #[test]
    fn sqlite_replay_preserves_book_tape_and_exact_cvd() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("test.sqlite3")).unwrap();
        let id = store.start("simulation", 1000).unwrap();
        let mut sim = Simulation::default();
        let mut e = Engine::default();
        let mut events: Vec<_> = sim
            .bootstrap()
            .into_iter()
            .map(|event| Envelope {
                received_ms: 1000,
                event,
            })
            .collect();
        for t in 1..40 {
            events.extend(sim.step(1000 + t * 100).into_iter().map(|event| Envelope {
                received_ms: 1000 + t * 100,
                event,
            }));
        }
        for frame in &events {
            e.apply(frame).unwrap();
            store.push(frame).unwrap();
        }
        store.stop().unwrap();
        let mut replay = Engine::default();
        for frame in store.load(id).unwrap() {
            replay.apply(&frame).unwrap();
        }
        assert_eq!(e.cvd, replay.cvd);
        assert_eq!(e.book.last_id, replay.book.last_id);
        assert_eq!(e.book.bids, replay.book.bids);
        assert_eq!(e.book.asks, replay.book.asks);
        assert_eq!(
            serde_json::to_value(&e.tape).unwrap(),
            serde_json::to_value(&replay.tape).unwrap()
        );
        assert_eq!(
            store.sessions().unwrap()[0].event_count,
            events.len() as u64
        );
        assert!(store.load(999).is_err());
    }
}
