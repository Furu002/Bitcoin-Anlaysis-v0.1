use crate::model::{Depth, Event, Fixed, Levels, Snapshot, Trade, SCALE};

pub struct Simulation {
    seed: u64,
    pub depth_id: u64,
    trade_id: u64,
    mid: i64,
    old_bids: Levels,
    old_asks: Levels,
}
impl Default for Simulation {
    fn default() -> Self {
        Self {
            seed: 0x51_6e_a1,
            depth_id: 1000,
            trade_id: 1,
            mid: 64_320 * SCALE,
            old_bids: vec![],
            old_asks: vec![],
        }
    }
}
impl Simulation {
    fn random(&mut self) -> u64 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.seed >> 32
    }
    fn ladder(&mut self) -> (Levels, Levels) {
        let mut bids = vec![];
        let mut asks = vec![];
        for i in 0..64_i64 {
            let buy = 2_000_000 + (self.random() % 150_000_000) as i64;
            let sell = 2_000_000 + (self.random() % 140_000_000) as i64;
            bids.push((Fixed(self.mid - i * SCALE), Fixed(buy)));
            asks.push((Fixed(self.mid + (i + 1) * SCALE), Fixed(sell)));
        }
        (bids, asks)
    }
    pub fn bootstrap(&mut self) -> Vec<Event> {
        let (bids, asks) = self.ladder();
        self.old_bids = bids.clone();
        self.old_asks = asks.clone();
        vec![
            Event::Reset {
                reason: "Seeded offline simulation · synthetic BTC/USDT".into(),
            },
            Event::Status {
                state: "connected".into(),
                note: "Synthetic feed · seed 0x516ea1".into(),
            },
            Event::Snapshot(Snapshot {
                last_id: self.depth_id,
                bids,
                asks,
            }),
        ]
    }
    pub fn step(&mut self, at_ms: u64) -> Vec<Event> {
        self.depth_id += 1;
        self.mid += (self.random() as i64 % 5 - 2) * (SCALE / 4);
        let (bids, asks) = self.ladder();
        let mut delta_b: Levels = self.old_bids.iter().map(|(p, _)| (*p, Fixed(0))).collect();
        let mut delta_a: Levels = self.old_asks.iter().map(|(p, _)| (*p, Fixed(0))).collect();
        delta_b.extend(bids.clone());
        delta_a.extend(asks.clone());
        self.old_bids = bids;
        self.old_asks = asks;
        let mut events = vec![Event::Depth(Depth {
            first_id: self.depth_id,
            last_id: self.depth_id,
            event_ms: at_ms,
            bids: delta_b,
            asks: delta_a,
        })];
        for _ in 0..3 {
            let maker = self.random() % 100 < 47;
            let qty = Fixed(100_000 + (self.random() % 25_000_000) as i64);
            events.push(Event::Trade(Trade {
                id: self.trade_id,
                event_ms: at_ms,
                trade_ms: at_ms,
                price: Fixed(self.mid + if maker { 0 } else { SCALE }),
                qty,
                buyer_maker: maker,
            }));
            self.trade_id += 1;
        }
        events
    }
    pub fn skip_depth(&mut self) {
        self.depth_id += 1;
    }
}
