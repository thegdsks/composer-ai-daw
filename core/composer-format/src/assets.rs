use crate::container::Tag;
use crate::error::FormatError;
use crate::sha256::{sha256, to_hex};
use composer_model::Id;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// Longest accepted original file name, in bytes.
pub const MAX_ASSET_NAME_LEN: usize = 1024;
pub const MAX_SAMPLE_RATE: u32 = 768_000;
pub const MAX_CHANNELS: u16 = 64;

/// SHA-256 of an asset's bytes. Serialized as 64 lowercase hex digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn of(data: &[u8]) -> Self {
        Self(sha256(data))
    }
}

impl ContentHash {
    /// Parses 64 lowercase hex digits.
    pub fn from_hex(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (o, pair) in out.iter_mut().zip(b.as_chunks::<2>().0) {
            let nib = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            };
            *o = (nib(pair[0])? << 4) | nib(pair[1])?;
        }
        Some(Self(out))
    }

    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", to_hex(&self.0))
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let b = s.as_bytes();
        if b.len() != 64 {
            return Err(de::Error::custom("hash must be 64 hex digits"));
        }
        let mut out = [0u8; 32];
        for (o, pair) in out.iter_mut().zip(b.as_chunks::<2>().0) {
            let nib = |c: u8| match c {
                b'0'..=b'9' => Ok(c - b'0'),
                b'a'..=b'f' => Ok(c - b'a' + 10),
                _ => Err(de::Error::custom("hash must be lowercase hex")),
            };
            *o = (nib(pair[0])? << 4) | nib(pair[1])?;
        }
        Ok(Self(out))
    }
}

/// A reference to one audio file. The bytes live outside the project file and
/// are found by `hash`, so large audio can be streamed from separate storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    pub id: Id,
    pub hash: ContentHash,
    /// Length of the asset in bytes.
    pub bytes: u64,
    /// Original file name, for display only.
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AssetRef {
    fn check(&self) -> Result<(), FormatError> {
        if self.name.len() > MAX_ASSET_NAME_LEN {
            return Err(FormatError::OutOfRange("asset name"));
        }
        if !(1..=MAX_SAMPLE_RATE).contains(&self.sample_rate) {
            return Err(FormatError::OutOfRange("asset sample rate"));
        }
        if !(1..=MAX_CHANNELS).contains(&self.channels) {
            return Err(FormatError::OutOfRange("asset channels"));
        }
        Ok(())
    }
}

/// Audio assets a project refers to, unique by id. Serialized sorted by id so
/// output is deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetTable {
    assets: BTreeMap<Id, AssetRef>,
}

impl AssetTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an asset. Fails on a duplicate id or an out of range field.
    pub fn insert(&mut self, a: AssetRef) -> Result<(), FormatError> {
        a.check()?;
        if self.assets.contains_key(&a.id) {
            return Err(FormatError::DuplicateId(a.id));
        }
        self.assets.insert(a.id, a);
        Ok(())
    }

    pub fn get(&self, id: Id) -> Option<&AssetRef> {
        self.assets.get(&id)
    }

    /// The asset with this content hash, if any.
    pub fn find_by_hash(&self, hash: &ContentHash) -> Option<&AssetRef> {
        self.assets.values().find(|a| a.hash == *hash)
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &AssetRef> {
        self.assets.values()
    }

    pub(crate) fn to_json(&self) -> Vec<u8> {
        let list: Vec<&AssetRef> = self.assets.values().collect();
        // Plain data with string keys cannot fail to serialize.
        serde_json::to_vec(&list).expect("asset table serializes")
    }

    pub(crate) fn from_json(bytes: &[u8], max_assets: u64) -> Result<Self, FormatError> {
        let list: Vec<AssetRef> =
            serde_json::from_slice(bytes).map_err(|e| FormatError::Malformed {
                tag: Tag::ASSETS,
                reason: e.to_string(),
            })?;
        if list.len() as u64 > max_assets {
            return Err(FormatError::LimitExceeded {
                what: "assets",
                limit: max_assets,
                found: list.len() as u64,
            });
        }
        let mut t = Self::new();
        for a in list {
            t.insert(a)?;
        }
        Ok(t)
    }
}
