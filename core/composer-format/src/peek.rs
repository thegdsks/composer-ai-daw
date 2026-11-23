//! Cheap summary of a project file. Only the manifest, the small `meta`
//! section and a short prefix of `proj` are touched, so cost does not grow
//! with project size.

use crate::container::{SectionIndex, Tag};
use crate::{FormatError, Meta};
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use std::cell::RefCell;
use std::fmt;

/// How much of `proj` is scanned for the project name. The name is the second
/// field of the serialized project, so a short prefix is enough.
const NAME_SCAN_BYTES: usize = 4096;

/// Summary from [`crate::peek`].
///
/// Verified (fail closed): the manifest checksum and layout, the announced
/// file extent (no short read, no trailing bytes), no unknown required
/// section, and for `meta` its checksum, JSON shape and size rules.
///
/// Not verified: the `proj` and `asst` checksums, and every project rule. The
/// `name` is a best effort read of the first bytes of `proj` and may be wrong
/// or `None` in a corrupt file. Use [`crate::read_project`] before trusting
/// content. Track, region and note counts are not offered because producing
/// them needs a pass over the whole project section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub version: u16,
    /// Exact file size the manifest announces (already checked against the input).
    pub file_bytes: u64,
    /// Length of the `proj` section as announced by the manifest.
    pub project_bytes: u64,
    /// Length of the `asst` section as announced by the manifest.
    pub assets_bytes: u64,
    /// Checksummed and parsed. `None` when the file is an older layout.
    pub meta: Option<Meta>,
    /// Unverified, see the type docs.
    pub name: Option<String>,
}

pub(crate) fn summarize(
    bytes: &[u8],
    index: &SectionIndex,
    current: bool,
) -> Result<ProjectInfo, FormatError> {
    let proj = index
        .entry(Tag::PROJECT)
        .ok_or(FormatError::MissingSection { tag: Tag::PROJECT })?;
    let assets_bytes = index.entry(Tag::ASSETS).map_or(0, |e| e.len);
    let (meta, name) = if current {
        let e = index
            .entry(Tag::META)
            .ok_or(FormatError::MissingSection { tag: Tag::META })?;
        let m: Meta = serde_json::from_slice(index.section(bytes, e)?).map_err(|e| {
            FormatError::Malformed {
                tag: Tag::META,
                reason: e.to_string(),
            }
        })?;
        m.check()?;
        (Some(m), scan_name(bytes, proj.offset, proj.len))
    } else {
        // Older layouts need the migration chain to be read, which peek does not run.
        (None, None)
    };
    Ok(ProjectInfo {
        version: index.version(),
        file_bytes: index.total_len(),
        project_bytes: proj.len,
        assets_bytes,
        meta,
        name,
    })
}

fn scan_name(bytes: &[u8], offset: u64, len: u64) -> Option<String> {
    let start = usize::try_from(offset).ok()?;
    let len = usize::try_from(len).ok()?;
    let end = start.checked_add(len.min(NAME_SCAN_BYTES))?;
    let prefix = bytes.get(start..end)?;
    let found = RefCell::new(None);
    let mut de = serde_json::Deserializer::from_slice(prefix);
    // The walk stops at the name, so the deserializer's own result is ignored.
    let _ = de.deserialize_map(NameFinder { found: &found });
    found.into_inner()
}

struct NameFinder<'a> {
    found: &'a RefCell<Option<String>>,
}

impl<'de> Visitor<'de> for NameFinder<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a project object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        while let Some(key) = a.next_key::<String>()? {
            if key == "name" {
                *self.found.borrow_mut() = Some(a.next_value::<String>()?);
                return Ok(());
            }
            a.next_value::<IgnoredAny>()?;
        }
        Ok(())
    }
}
