use crate::model::{Depth, Fixed, Levels, Snapshot};
use std::collections::{BTreeMap, VecDeque};

pub const BUFFER_CAP: usize = 4096;
const BOOK_CAP: usize = 20_000;

#[derive(Default)]
pub struct Book {
    pub bids: BTreeMap<Fixed, Fixed>,
    pub asks: BTreeMap<Fixed, Fixed>,
    pub last_id: Option<u64>,
    pub synced: bool,
    pending: VecDeque<Depth>,
    bid_floor: Option<Fixed>,
    ask_ceiling: Option<Fixed>,
    pub duplicates: u64,
}

impl Book {
    pub fn invalidate(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.pending.clear();
        self.last_id = None;
        self.synced = false;
        self.bid_floor = None;
        self.ask_ceiling = None;
    }

    fn validate_levels(levels: &Levels) -> Result<(), String> {
        if levels.iter().any(|(p, q)| p.0 <= 0 || q.0 < 0) {
            Err("Invalid price or quantity".into())
        } else {
            Ok(())
        }
    }

    fn update(map: &mut BTreeMap<Fixed, Fixed>, levels: &Levels) {
        for &(p, q) in levels {
            if q.0 == 0 {
                map.remove(&p);
            } else {
                map.insert(p, q);
            }
        }
    }

    fn check(&self) -> Result<(), String> {
        if self.bids.len() + self.asks.len() > BOOK_CAP {
            return Err("Book capacity exceeded".into());
        }
        if let (Some((bid, _)), Some((ask, _))) =
            (self.bids.last_key_value(), self.asks.first_key_value())
        {
            if bid >= ask {
                return Err("Crossed or locked book".into());
            }
        } else {
            return Err("Empty side; snapshot coverage exhausted".into());
        }
        Ok(())
    }

    pub fn covered(&self, n: usize) -> bool {
        self.bids
            .iter()
            .rev()
            .nth(n - 1)
            .zip(self.asks.iter().nth(n - 1))
            .is_some_and(|((bid, _), (ask, _))| {
                self.bid_floor.is_some_and(|p| *bid >= p)
                    && self.ask_ceiling.is_some_and(|p| *ask <= p)
            })
    }

    pub fn snapshot(&mut self, s: Snapshot) -> Result<bool, String> {
        Self::validate_levels(&s.bids)?;
        Self::validate_levels(&s.asks)?;
        // Build a new map so no partially parsed snapshot can become observable.
        self.bids.clear();
        self.asks.clear();
        Self::update(&mut self.bids, &s.bids);
        Self::update(&mut self.asks, &s.asks);
        self.bid_floor = self.bids.first_key_value().map(|(p, _)| *p);
        self.ask_ceiling = self.asks.last_key_value().map(|(p, _)| *p);
        self.last_id = Some(s.last_id);
        self.synced = false;
        self.check()?;
        let pending = std::mem::take(&mut self.pending);
        let mut advanced = false;
        for delta in pending {
            advanced |= self.depth(delta)?;
        }
        Ok(advanced)
    }

    pub fn depth(&mut self, d: Depth) -> Result<bool, String> {
        if d.first_id > d.last_id {
            return Err("Invalid update range".into());
        }
        Self::validate_levels(&d.bids)?;
        Self::validate_levels(&d.asks)?;
        let Some(last) = self.last_id else {
            if self.pending.len() >= BUFFER_CAP {
                return Err("Depth bootstrap buffer exceeded".into());
            }
            self.pending.push_back(d);
            return Ok(false);
        };
        if d.last_id <= last {
            self.duplicates += 1;
            return Ok(false);
        }
        let expected = last.checked_add(1).ok_or("Update ID overflow")?;
        if d.first_id > expected {
            return Err(format!(
                "Depth gap: expected {expected}, received {}",
                d.first_id
            ));
        }
        Self::update(&mut self.bids, &d.bids);
        Self::update(&mut self.asks, &d.asks);
        self.check()?;
        self.last_id = Some(d.last_id);
        self.synced = true;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn f(n: i64) -> Fixed {
        Fixed(n * 100_000_000)
    }
    fn snap(id: u64) -> Snapshot {
        Snapshot {
            last_id: id,
            bids: vec![(f(99), f(2)), (f(98), f(3))],
            asks: vec![(f(101), f(4)), (f(102), f(5))],
        }
    }
    fn delta(u: u64, end: u64) -> Depth {
        Depth {
            first_id: u,
            last_id: end,
            event_ms: 0,
            bids: vec![],
            asks: vec![],
        }
    }
    #[test]
    fn buffer_snapshot_bridge_overlap_duplicates_and_delete() {
        let mut b = Book::default();
        b.depth(delta(8, 10)).unwrap();
        let mut d = delta(10, 12);
        d.bids = vec![(f(99), f(0)), (f(98), f(7))];
        b.depth(d).unwrap();
        assert!(b.snapshot(snap(10)).unwrap());
        assert_eq!(b.last_id, Some(12));
        assert!(!b.bids.contains_key(&f(99)));
        assert_eq!(b.bids[&f(98)], f(7)); // absolute, not additive
        assert!(!b.depth(delta(10, 12)).unwrap());
        assert!(b.depth(delta(12, 14)).unwrap());
        assert_eq!(b.last_id, Some(14));
    }
    #[test]
    fn too_old_snapshot_and_forward_gap_fail() {
        let mut b = Book::default();
        b.depth(delta(15, 16)).unwrap();
        assert!(b.snapshot(snap(10)).is_err());
        b.invalidate();
        b.snapshot(snap(10)).unwrap();
        assert!(b.depth(delta(12, 12)).is_err());
    }
    #[test]
    fn snapshot_alone_is_not_synced_and_crosses_fail() {
        let mut b = Book::default();
        b.snapshot(snap(10)).unwrap();
        assert!(!b.synced);
        assert!(!b.covered(20));
        let mut d = delta(11, 11);
        d.bids = vec![(f(101), f(1))];
        assert!(b.depth(d).is_err());
    }
    #[test]
    fn unknown_outer_levels_do_not_extend_snapshot_coverage() {
        let mut b = Book::default();
        b.snapshot(snap(10)).unwrap();
        let mut d = delta(11, 11);
        d.bids = vec![(f(97), f(1)), (f(99), f(0)), (f(98), f(0))];
        b.depth(d).unwrap();
        assert!(!b.covered(1));
    }
    #[test]
    fn bootstrap_is_bounded() {
        let mut b = Book::default();
        for i in 0..BUFFER_CAP {
            b.depth(delta(i as u64, i as u64)).unwrap();
        }
        assert!(b.depth(delta(5000, 5000)).is_err());
    }
}
