//! Container layer: header, section table (the manifest) and section slicing.
//! It knows nothing about projects, so tests and tools can build odd files.

use crate::error::FormatError;
use crate::limits::Limits;
use crate::sha256::sha256;
use std::fmt;

pub const MAGIC: [u8; 8] = [0x89, b'C', b'M', b'P', b'F', 0x0D, 0x0A, 0x1A];
/// Version this build writes and fully understands.
pub const FORMAT_VERSION: u16 = 0;
pub const HEADER_LEN: usize = 16;
pub const ENTRY_LEN: usize = 72;
pub const HASH_LEN: usize = 32;
const FLAG_REQUIRED: u32 = 1;

/// Four byte section name.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag(pub [u8; 4]);

impl Tag {
    pub const PROJECT: Tag = Tag(*b"proj");
    pub const ASSETS: Tag = Tag(*b"asst");
    pub const META: Tag = Tag(*b"meta");
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            if b.is_ascii_graphic() {
                write!(f, "{}", char::from(b))?;
            } else {
                write!(f, "\\x{b:02x}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

/// One row of the section table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub tag: Tag,
    /// A reader that does not know a required section must refuse the file.
    pub required: bool,
    /// Instance key for repeated sections. Always 0 in v0.
    pub key: u128,
    pub offset: u64,
    pub len: u64,
    pub sha256: [u8; 32],
}

impl Entry {
    /// Byte range of the section data, once the whole file is present.
    fn range(&self) -> Option<std::ops::Range<usize>> {
        let start = usize::try_from(self.offset).ok()?;
        let end = usize::try_from(self.offset.checked_add(self.len)?).ok()?;
        Some(start..end)
    }
}

/// Section to be written by [`encode`].
#[derive(Debug, Clone)]
pub struct SectionSpec<'a> {
    pub tag: Tag,
    pub required: bool,
    pub key: u128,
    pub data: &'a [u8],
}

/// Low level writer: header, manifest with checksums, then the sections in order.
pub fn encode(version: u16, sections: &[SectionSpec<'_>]) -> Vec<u8> {
    let manifest_len = HEADER_LEN + sections.len() * ENTRY_LEN + HASH_LEN;
    let data_len: usize = sections.iter().map(|s| s.data.len()).sum();
    let mut out = Vec::with_capacity(manifest_len + data_len);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
    let mut offset = manifest_len as u64;
    for s in sections {
        out.extend_from_slice(&s.tag.0);
        out.extend_from_slice(&u32::from(s.required).to_le_bytes());
        out.extend_from_slice(&s.key.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(s.data.len() as u64).to_le_bytes());
        out.extend_from_slice(&sha256(s.data));
        offset += s.data.len() as u64;
    }
    let manifest_hash = sha256(&out);
    out.extend_from_slice(&manifest_hash);
    for s in sections {
        out.extend_from_slice(s.data);
    }
    out
}

fn le_u16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn le_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}
fn le_u128(b: &[u8]) -> u128 {
    let mut a = [0u8; 16];
    a.copy_from_slice(&b[..16]);
    u128::from_le_bytes(a)
}

fn check_header(bytes: &[u8]) -> Result<(u16, u32), FormatError> {
    if bytes.len() < HEADER_LEN {
        let n = bytes.len().min(MAGIC.len());
        return Err(if bytes[..n] == MAGIC[..n] {
            FormatError::Truncated { what: "header" }
        } else {
            FormatError::BadMagic
        });
    }
    if bytes[..8] != MAGIC {
        return Err(FormatError::BadMagic);
    }
    let version = le_u16(&bytes[8..]);
    if version > FORMAT_VERSION {
        return Err(FormatError::UnsupportedVersion {
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    if le_u16(&bytes[10..]) != 0 {
        return Err(FormatError::BadHeader("reserved flags are set"));
    }
    Ok((version, le_u32(&bytes[12..])))
}

/// The parsed manifest: version and section table, without any section data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionIndex {
    version: u16,
    entries: Vec<Entry>,
    manifest_len: usize,
    total_len: u64,
}

impl SectionIndex {
    /// Bytes the manifest occupies, from the first [`HEADER_LEN`] bytes of a file.
    /// A caller that streams can fetch the header, call this, then fetch the manifest.
    pub fn manifest_len(header: &[u8], limits: &Limits) -> Result<usize, FormatError> {
        let (_, count) = check_header(header)?;
        if count > limits.max_sections {
            return Err(FormatError::LimitExceeded {
                what: "sections",
                limit: u64::from(limits.max_sections),
                found: u64::from(count),
            });
        }
        Ok((count as usize)
            .saturating_mul(ENTRY_LEN)
            .saturating_add(HEADER_LEN + HASH_LEN))
    }

    /// Parses the manifest from the start of `bytes`, which may be a prefix
    /// holding only the manifest. Section data is neither read nor checked.
    pub fn parse(bytes: &[u8], limits: &Limits) -> Result<Self, FormatError> {
        let manifest_len = Self::manifest_len(bytes, limits)?;
        if bytes.len() < manifest_len {
            return Err(FormatError::Truncated { what: "manifest" });
        }
        let (body, hash) = bytes[..manifest_len].split_at(manifest_len - HASH_LEN);
        if sha256(body) != hash {
            return Err(FormatError::ManifestCorrupt);
        }
        let (version, count) = check_header(bytes)?;
        let mut entries: Vec<Entry> = Vec::with_capacity(count as usize);
        let mut cursor = manifest_len as u64;
        for row in body[HEADER_LEN..].as_chunks::<ENTRY_LEN>().0 {
            let flags = le_u32(&row[4..]);
            if flags & !FLAG_REQUIRED != 0 {
                return Err(FormatError::BadHeader("reserved section flags are set"));
            }
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&row[..4]);
            let mut sha = [0u8; 32];
            sha.copy_from_slice(&row[40..72]);
            let e = Entry {
                tag: Tag(tag),
                required: flags & FLAG_REQUIRED != 0,
                key: le_u128(&row[8..]),
                offset: le_u64(&row[24..]),
                len: le_u64(&row[32..]),
                sha256: sha,
            };
            if e.key != 0 {
                return Err(FormatError::BadLayout(
                    "keyed sections need a newer version",
                ));
            }
            if entries.iter().any(|o| o.tag == e.tag && o.key == e.key) {
                return Err(FormatError::DuplicateSection { tag: e.tag });
            }
            if e.offset != cursor {
                return Err(FormatError::BadLayout("sections must be contiguous"));
            }
            cursor = cursor
                .checked_add(e.len)
                .ok_or(FormatError::BadLayout("section length overflows"))?;
            let limit = limit_for(e.tag, limits);
            if e.len > limit {
                return Err(FormatError::SectionTooLarge {
                    tag: e.tag,
                    len: e.len,
                    limit,
                });
            }
            entries.push(e);
        }
        if cursor > limits.max_file_bytes {
            return Err(FormatError::LimitExceeded {
                what: "file bytes",
                limit: limits.max_file_bytes,
                found: cursor,
            });
        }
        Ok(Self {
            version,
            entries,
            manifest_len,
            total_len: cursor,
        })
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, tag: Tag) -> Option<&Entry> {
        self.entries.iter().find(|e| e.tag == tag)
    }

    pub fn manifest_bytes(&self) -> usize {
        self.manifest_len
    }

    /// Exact file size the manifest announces.
    pub fn total_len(&self) -> u64 {
        self.total_len
    }

    /// Requires `bytes` to be exactly the announced file: no short read, no trailing bytes.
    pub fn check_extent(&self, bytes: &[u8]) -> Result<(), FormatError> {
        let have = bytes.len() as u64;
        if have < self.total_len {
            Err(FormatError::Truncated { what: "sections" })
        } else if have > self.total_len {
            Err(FormatError::TrailingBytes)
        } else {
            Ok(())
        }
    }

    /// Section data for `entry`, checked against its checksum.
    pub fn section<'a>(&self, bytes: &'a [u8], entry: &Entry) -> Result<&'a [u8], FormatError> {
        let data = entry
            .range()
            .and_then(|r| bytes.get(r))
            .ok_or(FormatError::Truncated { what: "section" })?;
        if sha256(data) != entry.sha256 {
            return Err(FormatError::SectionCorrupt { tag: entry.tag });
        }
        Ok(data)
    }
}

fn limit_for(tag: Tag, l: &Limits) -> u64 {
    match tag {
        Tag::PROJECT => l.max_project_bytes,
        Tag::ASSETS => l.max_assets_bytes,
        Tag::META => l.max_meta_bytes,
        _ => l.max_other_section_bytes,
    }
}
