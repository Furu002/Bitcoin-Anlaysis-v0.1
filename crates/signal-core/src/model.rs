use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const SCALE: i64 = 100_000_000;

/// Exact exchange decimal; all wire/storage representations are strings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed(pub i64);

impl Fixed {
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<_> = s.split('.').collect();
        if parts.is_empty()
            || parts.len() > 2
            || parts[0].is_empty()
            || !parts[0].bytes().all(|c| c.is_ascii_digit())
        {
            return Err(format!("Invalid nonnegative decimal: {s}"));
        }
        let frac = parts.get(1).copied().unwrap_or("");
        if frac.len() > 8
            || !frac.bytes().all(|c| c.is_ascii_digit())
            || (parts.len() == 2 && frac.is_empty())
        {
            return Err(format!("Invalid decimal precision: {s}"));
        }
        let whole = parts[0].parse::<i64>().map_err(|_| "Decimal overflow")?;
        let fraction = if frac.is_empty() {
            0
        } else {
            frac.parse::<i64>().map_err(|_| "Decimal overflow")? * 10_i64.pow(8 - frac.len() as u32)
        };
        whole
            .checked_mul(SCALE)
            .and_then(|v| v.checked_add(fraction))
            .map(Self)
            .ok_or_else(|| "Decimal overflow".into())
    }
}

pub fn decimal(value: i128) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.unsigned_abs();
    format!(
        "{sign}{}.{:08}",
        value / SCALE as u128,
        value % SCALE as u128
    )
}

impl Serialize for Fixed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&decimal(self.0 as i128))
    }
}
impl<'de> Deserialize<'de> for Fixed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

pub type Levels = Vec<(Fixed, Fixed)>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Depth {
    pub first_id: u64,
    pub last_id: u64,
    pub event_ms: u64,
    pub bids: Levels,
    pub asks: Levels,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub last_id: u64,
    pub bids: Levels,
    pub asks: Levels,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trade {
    pub id: u64,
    pub event_ms: u64,
    pub trade_ms: u64,
    pub price: Fixed,
    pub qty: Fixed,
    pub buyer_maker: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Event {
    Reset { reason: String },
    Status { state: String, note: String },
    Snapshot(Snapshot),
    Depth(Depth),
    Trade(Trade),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub received_ms: u64,
    pub event: Event,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_decimals_and_rejection() {
        assert_eq!(Fixed::parse("0.00000001").unwrap(), Fixed(1));
        assert_eq!(Fixed::parse("64000.12").unwrap(), Fixed(6_400_012_000_000));
        for s in [
            "-1",
            "NaN",
            "1e3",
            "1.000000001",
            ".1",
            "1.",
            "1..0",
            "9223372036854775807",
        ] {
            assert!(Fixed::parse(s).is_err(), "{s}");
        }
        assert_eq!(decimal(-1), "-0.00000001");
        let value = serde_json::to_string(&Fixed(123)).unwrap();
        assert_eq!(serde_json::from_str::<Fixed>(&value).unwrap(), Fixed(123));
    }
}
