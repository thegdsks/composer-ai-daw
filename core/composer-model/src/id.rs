use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Stable 128 bit identifier. Serialized as 32 lowercase hex digits so JSON
/// clients never lose precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub u128);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// Error for text that is not 1 to 32 hex digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdError;

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("id must be 1 to 32 hex digits")
    }
}

impl std::error::Error for ParseIdError {}

impl FromStr for Id {
    type Err = ParseIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s.len() > 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseIdError);
        }
        u128::from_str_radix(s, 16)
            .map(Id)
            .map_err(|_| ParseIdError)
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct IdVisitor;
        impl Visitor<'_> for IdVisitor {
            type Value = Id;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a hex id string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Id, E> {
                v.parse().map_err(E::custom)
            }
        }
        d.deserialize_str(IdVisitor)
    }
}

/// Seeded id source (splitmix64). Same seed gives the same ids, which keeps
/// tests and replays deterministic.
#[derive(Debug, Clone)]
pub struct IdGen {
    state: u64,
}

impl IdGen {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_id(&mut self) -> Id {
        let hi = u128::from(self.next_u64());
        let lo = u128::from(self.next_u64());
        Id((hi << 64) | lo)
    }
}
