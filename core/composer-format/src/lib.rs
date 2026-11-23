//! Project file format v0: bytes in, bytes out. No file system, no clock, so
//! it runs natively and in wasm. Callers do the I/O and supply the time stamps.
//!
//! # Byte layout (all integers little endian)
//!
//! ```text
//! header   16 bytes   magic[8] = 89 'C' 'M' 'P' 'F' 0D 0A 1A
//!                     version u16 (this build writes 0)
//!                     header flags u16 (must be 0)
//!                     section count u32
//! table    72 bytes per section, in file order
//!                     tag[4], flags u32 (bit 0 = required, rest 0),
//!                     key u128 (0 in v0), offset u64, length u64, sha256[32]
//! manifest hash 32    SHA-256 of header and table
//! sections            raw bytes, contiguous, in table order, nothing after
//! ```
//!
//! The header and table are the manifest. Offsets are absolute and must be
//! contiguous, so a lying length cannot make a reader skip or overlap data.
//! Every length is checked against the bytes present, and against [`Limits`],
//! before anything is allocated from it. A reader that sees a version above
//! [`FORMAT_VERSION`] returns [`FormatError::UnsupportedVersion`] before it
//! trusts any later field.
//!
//! v0 sections, each present exactly once:
//!
//! * `proj`: the `Project` as JSON. Field order follows the model structs and
//!   maps are ordered, so equal projects give identical bytes.
//! * `asst`: [`AssetTable`] as a JSON array sorted by id. Entries hold id,
//!   content hash (SHA-256 hex), byte length, original name, sample rate and
//!   channels. The audio bytes are not stored: they are found by hash, so a
//!   later store can stream large audio from separate files.
//! * `meta`: [`Meta`] as JSON (app version, created and modified stamps, optional transport prefs, D21).
//!
//! A section this build does not know is skipped (but still checksummed) when
//! its required flag is clear, and rejected when it is set.
//!
//! # Reading
//!
//! [`SectionIndex::parse`] reads only the manifest, so tools and streaming
//! loaders can learn the layout from the first few hundred bytes.
//! [`peek`] adds the small `meta` section and the project name from the first
//! bytes of `proj`; it never checksums or parses the whole project section.
//! [`read_project`] verifies each section checksum, parses, then replays the
//! project through the model's own change rules, which rejects duplicate ids,
//! out of range values and broken keys, and finally checks the id index.
//!
//! # Planned split per track (not implemented)
//!
//! A later version keeps `proj` for project level data (ids, name, tempo map,
//! track order) and stores each track as its own section with the track id in
//! the table `key` field. A save then rewrites only the sections whose
//! checksum changed plus the manifest, and a loader can parse tracks lazily or
//! in parallel. The `key` field and the contiguous layout rule exist for this;
//! in v0 a non zero key is rejected. The migration chain turns a v0 file into
//! the split layout on load.
#![forbid(unsafe_code)]

mod assets;
pub mod container;
mod error;
mod limits;
mod meta;
pub mod migrate;
mod peek;
pub mod sha256;
mod validate;

pub use assets::{
    AssetRef, AssetTable, ContentHash, MAX_ASSET_NAME_LEN, MAX_CHANNELS, MAX_SAMPLE_RATE,
};
pub use container::{Entry, SectionIndex, Tag, FORMAT_VERSION};
pub use error::FormatError;
pub use limits::Limits;
pub use meta::{
    LoopPatch, LoopRange, Meta, Metronome, MetronomePatch, PrefsError, TransportPatch,
    TransportPrefs, DEFAULT_MAX_COUNT_IN_BARS, MAX_APP_VERSION_LEN,
};
pub use peek::ProjectInfo;

use composer_model::Project;
use container::SectionSpec;
use migrate::RawSection;
use std::borrow::Cow;

/// A successfully read and validated file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub project: Project,
    pub assets: AssetTable,
    pub meta: Meta,
    /// Format version the file was written with, before any migration.
    pub version: u16,
    /// Optional sections this build did not know and ignored.
    pub skipped_sections: Vec<Tag>,
}

/// Serializes a project. The output is deterministic: equal inputs give equal bytes.
/// The project is not validated here; [`read_project`] rejects an invalid one.
pub fn write_project(project: &Project, assets: &AssetTable, meta: &Meta) -> Vec<u8> {
    // Plain data with string keys cannot fail to serialize.
    let proj = serde_json::to_vec(project).expect("project serializes");
    let asst = assets.to_json();
    let meta = serde_json::to_vec(meta).expect("meta serializes");
    container::encode(
        FORMAT_VERSION,
        &[
            SectionSpec {
                tag: Tag::PROJECT,
                required: true,
                key: 0,
                data: &proj,
            },
            SectionSpec {
                tag: Tag::ASSETS,
                required: true,
                key: 0,
                data: &asst,
            },
            SectionSpec {
                tag: Tag::META,
                required: true,
                key: 0,
                data: &meta,
            },
        ],
    )
}

// v0 is the oldest version, so today this is always false; the chain is ready for v1.
#[allow(clippy::absurd_extreme_comparisons)]
fn needs_migration(version: u16) -> bool {
    version < FORMAT_VERSION
}

fn is_known(tag: Tag) -> bool {
    matches!(tag, Tag::PROJECT | Tag::ASSETS | Tag::META)
}

fn malformed(tag: Tag, e: serde_json::Error) -> FormatError {
    FormatError::Malformed {
        tag,
        reason: e.to_string(),
    }
}

/// Reads and fully validates a project file. Fails closed: any problem
/// returns an error and no partial project.
pub fn read_project(bytes: &[u8], limits: &Limits) -> Result<Loaded, FormatError> {
    let index = SectionIndex::parse(bytes, limits)?;
    index.check_extent(bytes)?;
    if let Some(e) = index
        .entries()
        .iter()
        .find(|e| e.required && !is_known(e.tag))
    {
        return Err(FormatError::UnknownRequiredSection { tag: e.tag });
    }
    let mut sections = Vec::new();
    let mut skipped = Vec::new();
    for e in index.entries() {
        // Every section is checksummed, including ones we skip.
        let data = index.section(bytes, e)?;
        if is_known(e.tag) {
            sections.push(RawSection {
                tag: e.tag,
                required: e.required,
                data: Cow::Borrowed(data),
            });
        } else {
            skipped.push(e.tag);
        }
    }
    if needs_migration(index.version()) {
        migrate::migrate(index.version(), &mut sections)?;
    }
    let find = |tag: Tag| {
        sections
            .iter()
            .find(|s| s.tag == tag)
            .map(|s| &*s.data)
            .ok_or(FormatError::MissingSection { tag })
    };
    let proj_bytes = find(Tag::PROJECT)?;
    let asst_bytes = find(Tag::ASSETS)?;
    let meta_bytes = find(Tag::META)?;

    let parsed: Project =
        serde_json::from_slice(proj_bytes).map_err(|e| malformed(Tag::PROJECT, e))?;
    let project = validate::validate(parsed, limits)?;
    let assets = AssetTable::from_json(asst_bytes, limits.max_assets)?;
    for t in &project.tracks {
        for r in t.regions.values() {
            if let composer_model::RegionContent::Clip { asset, .. } = &r.content {
                let known = ContentHash::from_hex(asset)
                    .and_then(|h| assets.find_by_hash(&h))
                    .is_some();
                if !known {
                    return Err(FormatError::DanglingReference(
                        "audio clip names an asset that is not in the asset table",
                    ));
                }
            }
        }
    }
    let meta: Meta = serde_json::from_slice(meta_bytes).map_err(|e| malformed(Tag::META, e))?;
    meta.check()?;
    Ok(Loaded {
        project,
        assets,
        meta,
        version: index.version(),
        skipped_sections: skipped,
    })
}

/// Summary of a file using default [`Limits`]. See [`peek_with`].
pub fn peek(bytes: &[u8]) -> Result<ProjectInfo, FormatError> {
    peek_with(bytes, &Limits::default())
}

/// Summary of a file from the manifest, the `meta` section and the start of
/// `proj`. It does not checksum or parse the project section, so its cost does
/// not grow with project size. What is and is not verified is listed on
/// [`ProjectInfo`]; use [`read_project`] before trusting the content.
pub fn peek_with(bytes: &[u8], limits: &Limits) -> Result<ProjectInfo, FormatError> {
    let index = SectionIndex::parse(bytes, limits)?;
    index.check_extent(bytes)?;
    if let Some(e) = index
        .entries()
        .iter()
        .find(|e| e.required && !is_known(e.tag))
    {
        return Err(FormatError::UnknownRequiredSection { tag: e.tag });
    }
    peek::summarize(bytes, &index, index.version() == FORMAT_VERSION)
}
