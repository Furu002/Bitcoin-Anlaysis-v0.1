use crate::{
    book::Book,
    model::{decimal, Envelope, Event, Fixed},
};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Serialize)]
pub struct TapeRow {
    pub id: String,
    pub at_ms: u64,
    pub price: Fixed,
    pub qty: Fixed,
    pub aggressor: String,
}
#[derive(Clone, Serialize)]
pub struct Sample {
    pub at_ms: u64,
    pub price: Option<Fixed>,
    pub cvd: String,
}

#[derive(Serialize)]
pub struct View {
    pub mode: String,
    pub health: String,
    pub note: String,
    pub book_state: String,
    pub trade_state: String,
    pub book_age_ms: Option<u64>,
    pub trade_age_ms: Option<u64>,
    pub event_offset_ms: Option<i64>,
    pub update_id: Option<String>,
    pub bids: Vec<(Fixed, Fixed)>,
    pub asks: Vec<(Fixed, Fixed)>,
    pub spread: Option<Fixed>,
    pub imbalance: Option<f64>,
    pub last_price: Option<Fixed>,
    pub cvd: String,
    pub buy_volume: String,
    pub sell_volume: String,
    pub cvd_complete: bool,
    pub trade_count: u64,
    pub duplicates: u64,
    pub gaps: u64,
    pub tape: Vec<TapeRow>,
    pub history: Vec<Sample>,
    pub now_ms: u64,
}

pub struct Engine {
    pub book: Book,
    pub tape: VecDeque<TapeRow>,
    pub history: VecDeque<Sample>,
    pub cvd: i128,
    pub buy_volume: i128,
    pub sell_volume: i128,
    pub cvd_complete: bool,
    pub trade_count: u64,
    pub trade_duplicates: u64,
    pub gaps: u64,
    pub last_trade_id: Option<u64>,
    pub last_price: Option<Fixed>,
    pub depth_received: Option<u64>,
    pub trade_received: Option<u64>,
    pub event_offset_ms: Option<i64>,
    pub transport: String,
    pub note: String,
    bootstrap_depth_received: Option<u64>,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            book: Book::default(),
            tape: VecDeque::new(),
            history: VecDeque::new(),
            cvd: 0,
            buy_volume: 0,
            sell_volume: 0,
            cvd_complete: true,
            trade_count: 0,
            trade_duplicates: 0,
            gaps: 0,
            last_trade_id: None,
            last_price: None,
            depth_received: None,
            trade_received: None,
            event_offset_ms: None,
            transport: "syncing".into(),
            note: "Waiting for snapshot and depth bridge".into(),
            bootstrap_depth_received: None,
        }
    }
}

impl Engine {
    pub fn invalidate(&mut self, reason: String) {
        self.book.invalidate();
        self.depth_received = None;
        self.bootstrap_depth_received = None;
        self.transport = "syncing".into();
        self.note = reason;
        self.gaps += 1;
        self.cvd_complete = false;
    }

    pub fn apply(&mut self, frame: &Envelope) -> Result<(), String> {
        let result = match &frame.event {
            Event::Reset { reason } => {
                self.book.invalidate();
                self.depth_received = None;
                self.bootstrap_depth_received = None;
                self.transport = "syncing".into();
                self.note = reason.clone();
                if self.trade_count > 0 {
                    self.cvd_complete = false;
                }
                Ok(())
            }
            Event::Status { state, note } => {
                self.transport = state.clone();
                self.note = note.clone();
                if state != "connected" {
                    self.book.invalidate();
                    self.depth_received = None;
                    self.bootstrap_depth_received = None;
                    if self.trade_count > 0 {
                        self.cvd_complete = false;
                    }
                }
                Ok(())
            }
            Event::Snapshot(s) => self.book.snapshot(s.clone()).map(|advanced| {
                if advanced {
                    self.depth_received = self.bootstrap_depth_received;
                }
                self.bootstrap_depth_received = None;
                if advanced && self.note.contains("awaiting snapshot") {
                    self.note = "Binance public spot · snapshot synchronized".into();
                }
            }),
            Event::Depth(d) => {
                if self.book.last_id.is_none() {
                    self.bootstrap_depth_received = Some(frame.received_ms);
                }
                self.book.depth(d.clone()).map(|advanced| {
                    if advanced {
                        self.depth_received = Some(frame.received_ms);
                    }
                    if advanced && self.note.contains("awaiting snapshot") {
                        self.note = "Binance public spot · snapshot synchronized".into();
                    }
                    self.event_offset_ms = Some(
                        (frame.received_ms as i128 - d.event_ms as i128)
                            .clamp(i64::MIN as i128, i64::MAX as i128)
                            as i64,
                    );
                })
            }
            Event::Trade(t) => {
                if t.price.0 <= 0 || t.qty.0 <= 0 {
                    let reason = "Invalid trade price or quantity".to_string();
                    self.invalidate(reason.clone());
                    return Err(reason);
                }
                if self.last_trade_id.is_some_and(|id| t.id <= id) {
                    self.trade_duplicates += 1;
                    return Ok(());
                }
                if self
                    .last_trade_id
                    .is_some_and(|id| t.id > id.saturating_add(1))
                {
                    self.cvd_complete = false;
                    self.gaps += 1;
                    self.note = "Trade ID gap: CVD includes observed trades only".into();
                }
                self.last_trade_id = Some(t.id);
                self.trade_count += 1;
                self.last_price = Some(t.price);
                self.trade_received = Some(frame.received_ms);
                // Buyer-maker=true means the seller crossed the bid: sell aggressor.
                if t.buyer_maker {
                    self.sell_volume += t.qty.0 as i128;
                    self.cvd -= t.qty.0 as i128;
                } else {
                    self.buy_volume += t.qty.0 as i128;
                    self.cvd += t.qty.0 as i128;
                }
                self.tape.push_front(TapeRow {
                    id: t.id.to_string(),
                    at_ms: t.trade_ms,
                    price: t.price,
                    qty: t.qty,
                    aggressor: if t.buyer_maker { "sell" } else { "buy" }.into(),
                });
                self.tape.truncate(100);
                Ok(())
            }
        };
        let result = result.and_then(|()| {
            if self.book.synced && !self.book.covered(20) {
                Err("Top-20 snapshot coverage exhausted".into())
            } else {
                Ok(())
            }
        });
        if let Err(err) = &result {
            self.invalidate(err.clone());
        }
        result
    }

    pub fn sample(&mut self, now: u64) {
        self.history.push_back(Sample {
            at_ms: now,
            price: self.last_price,
            cvd: decimal(self.cvd),
        });
        if self.history.len() > 1200 {
            self.history.pop_front();
        }
    }

    pub fn view(&self, mode: &str, now: u64) -> View {
        let book_age = self.depth_received.map(|t| now.saturating_sub(t));
        let trade_age = self.trade_received.map(|t| now.saturating_sub(t));
        let book_fresh = self.book.synced && book_age.is_some_and(|a| a <= 2000);
        let covered = self.book.covered(20);
        let usable = book_fresh && covered && self.transport == "connected";
        let trade_fresh = trade_age.is_some_and(|a| a <= 5000);
        let bids: Vec<_> = if usable {
            self.book
                .bids
                .iter()
                .rev()
                .take(20)
                .map(|(&p, &q)| (p, q))
                .collect()
        } else {
            vec![]
        };
        let asks: Vec<_> = if usable {
            self.book
                .asks
                .iter()
                .take(20)
                .map(|(&p, &q)| (p, q))
                .collect()
        } else {
            vec![]
        };
        let b: i128 = bids.iter().map(|(_, q)| q.0 as i128).sum();
        let a: i128 = asks.iter().map(|(_, q)| q.0 as i128).sum();
        let book_state = if !self.book.synced {
            "syncing"
        } else if !book_fresh {
            "stale"
        } else if !covered {
            "partial"
        } else {
            "synced"
        };
        let health = if self.transport == "blocked" || self.transport == "cooldown" {
            self.transport.as_str()
        } else if usable && trade_fresh {
            "healthy"
        } else if self.transport == "connected" && self.book.synced {
            "degraded"
        } else {
            "syncing"
        };
        View {
            mode: mode.into(),
            health: health.into(),
            note: self.note.clone(),
            book_state: book_state.into(),
            trade_state: if trade_fresh {
                "fresh"
            } else if trade_age.is_some() {
                "stale"
            } else {
                "waiting"
            }
            .into(),
            book_age_ms: book_age,
            trade_age_ms: trade_age,
            event_offset_ms: self.event_offset_ms,
            update_id: self.book.last_id.map(|v| v.to_string()),
            spread: bids
                .first()
                .zip(asks.first())
                .map(|((bid, _), (ask, _))| Fixed(ask.0 - bid.0)),
            imbalance: if usable && b + a > 0 {
                Some((b - a) as f64 / (b + a) as f64)
            } else {
                None
            },
            bids,
            asks,
            last_price: self.last_price,
            cvd: decimal(self.cvd),
            buy_volume: decimal(self.buy_volume),
            sell_volume: decimal(self.sell_volume),
            cvd_complete: self.cvd_complete,
            trade_count: self.trade_count,
            duplicates: self.book.duplicates + self.trade_duplicates,
            gaps: self.gaps,
            tape: self.tape.iter().cloned().collect(),
            history: self.history.iter().cloned().collect(),
            now_ms: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Trade, SCALE};
    fn trade(id: u64, qty: i64, maker: bool) -> Envelope {
        Envelope {
            received_ms: 1000,
            event: Event::Trade(Trade {
                id,
                event_ms: 990,
                trade_ms: 985,
                price: Fixed(100 * SCALE),
                qty: Fixed(qty),
                buyer_maker: maker,
            }),
        }
    }
    #[test]
    fn aggressor_semantics_dedupe_and_completeness() {
        let mut e = Engine::default();
        e.apply(&trade(1, 3 * SCALE, false)).unwrap();
        e.apply(&trade(2, SCALE, true)).unwrap();
        e.apply(&trade(2, SCALE, true)).unwrap();
        assert_eq!(e.cvd, 2 * SCALE as i128);
        assert_eq!(e.trade_count, 2);
        assert_eq!(e.tape.front().unwrap().aggressor, "sell");
        e.apply(&trade(4, 1, false)).unwrap();
        assert!(!e.cvd_complete);
        e.apply(&trade(5, 1, false)).unwrap();
        assert!(!e.cvd_complete);
    }
    #[test]
    fn reset_keeps_observed_cvd_but_marks_incomplete() {
        let mut e = Engine::default();
        e.apply(&trade(1, SCALE, false)).unwrap();
        e.apply(&Envelope {
            received_ms: 1100,
            event: Event::Reset {
                reason: "reconnect".into(),
            },
        })
        .unwrap();
        assert_eq!(e.cvd, SCALE as i128);
        assert!(!e.cvd_complete);
    }
    #[test]
    fn trade_age_is_independent_and_invalid_trade_is_rejected() {
        let mut e = Engine::default();
        e.apply(&trade(1, SCALE, false)).unwrap();
        let v = e.view("live", 7001);
        assert_eq!(v.trade_state, "stale");
        assert!(v.imbalance.is_none());
        assert!(e.apply(&trade(2, 0, false)).is_err());
        assert_eq!(e.trade_count, 1);
        assert!(!e.cvd_complete);
        assert_eq!(e.gaps, 1);
    }
    #[test]
    fn delayed_snapshot_and_duplicate_depth_cannot_hide_staleness() {
        use crate::model::{Depth, Snapshot};
        let mut e = Engine::default();
        let depth = Depth {
            first_id: 11,
            last_id: 11,
            event_ms: 990,
            bids: vec![],
            asks: vec![],
        };
        e.apply(&Envelope {
            received_ms: 1000,
            event: Event::Status {
                state: "connected".into(),
                note: "test".into(),
            },
        })
        .unwrap();
        e.apply(&Envelope {
            received_ms: 1000,
            event: Event::Depth(depth.clone()),
        })
        .unwrap();
        let s = Snapshot {
            last_id: 10,
            bids: (1..=20)
                .map(|i| (Fixed((100 - i) * SCALE), Fixed(SCALE)))
                .collect(),
            asks: (1..=20)
                .map(|i| (Fixed((100 + i) * SCALE), Fixed(SCALE)))
                .collect(),
        };
        e.apply(&Envelope {
            received_ms: 5000,
            event: Event::Snapshot(s),
        })
        .unwrap();
        assert_eq!(e.view("live", 5000).book_state, "stale");
        e.apply(&Envelope {
            received_ms: 6000,
            event: Event::Depth(depth),
        })
        .unwrap();
        assert_eq!(e.view("live", 6000).book_age_ms, Some(5000));
        assert!(e.view("live", 6000).imbalance.is_none());
    }
    #[test]
    fn top20_imbalance_matches_independent_sum_and_depth_age() {
        use crate::model::{Depth, Snapshot};
        let mut e = Engine::default();
        e.apply(&Envelope {
            received_ms: 1000,
            event: Event::Status {
                state: "connected".into(),
                note: "test".into(),
            },
        })
        .unwrap();
        e.apply(&Envelope {
            received_ms: 1000,
            event: Event::Snapshot(Snapshot {
                last_id: 10,
                bids: (1..=20)
                    .map(|i| (Fixed((100 - i) * SCALE), Fixed(3 * SCALE)))
                    .collect(),
                asks: (1..=20)
                    .map(|i| (Fixed((100 + i) * SCALE), Fixed(2 * SCALE)))
                    .collect(),
            }),
        })
        .unwrap();
        e.apply(&Envelope {
            received_ms: 1100,
            event: Event::Depth(Depth {
                first_id: 11,
                last_id: 11,
                event_ms: 1100,
                bids: vec![],
                asks: vec![],
            }),
        })
        .unwrap();
        assert!((e.view("live", 1200).imbalance.unwrap() - 0.2).abs() < 1e-12);
        e.apply(&trade(1, SCALE, false)).unwrap();
        let stale = e.view("live", 3201);
        assert_eq!(stale.book_state, "stale");
        assert_eq!(stale.trade_state, "fresh");
        assert!(stale.bids.is_empty());
    }
}
