//! Reader for the flat sound catalogue (`sounds/catalogue.yaml`, D18).
//!
//! The file is a list of entries under `entries:` with scalar fields only, plus
//! a top level `gaps:` list of strings. This reader accepts exactly that shape,
//! so no YAML dependency is needed, and fails closed on anything else.
#![forbid(unsafe_code)]

use serde::Serialize;
use std::fmt;

/// Longest catalogue accepted, in bytes.
pub const MAX_CATALOGUE_BYTES: usize = 4 << 20;
/// Most entries accepted.
pub const MAX_ENTRIES: usize = 10_000;

/// One sound source. Optional fields are `None` when the file says `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub url: String,
    pub licence: String,
    pub redistribution_allowed: String,
    pub commercial_use_allowed: String,
    pub attribution_required: String,
    pub attribution_text: Option<String>,
    pub approx_size: String,
    pub instruments: String,
    pub quality_note: String,
    /// `yes`, `maybe` or `no`.
    pub bundle: String,
    pub bundle_reason: String,
    /// `listed`, `fetched` or `wired` (S01).
    pub status: String,
    /// Path of the fetched file under sounds/packs, when there is one.
    pub file: Option<String>,
    /// sha256 of the fetched archive, when there is one.
    pub checksum: Option<String>,
    pub licence_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Catalogue {
    pub entries: Vec<Entry>,
    /// Known holes in the catalogue, in the file's own words.
    pub gaps: Vec<String>,
}

/// Why the file was refused. `code` is stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueError {
    pub line: usize,
    pub reason: &'static str,
}

impl CatalogueError {
    pub fn code(&self) -> &'static str {
        "catalogue_malformed"
    }
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for CatalogueError {}

fn err(line: usize, reason: &'static str) -> CatalogueError {
    CatalogueError { line, reason }
}

/// A scalar: `null`, a double quoted string with `\"` and `\\` escapes, or bare text.
fn scalar(raw: &str, line: usize) -> Result<Option<String>, CatalogueError> {
    let raw = raw.trim();
    if raw == "null" || raw == "~" {
        return Ok(None);
    }
    let Some(inner) = raw.strip_prefix('"') else {
        return Ok(Some(raw.to_string()));
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    loop {
        match chars.next() {
            None => return Err(err(line, "unterminated string")),
            Some('"') => {
                return if chars.as_str().trim().is_empty() {
                    Ok(Some(out))
                } else {
                    Err(err(line, "text after closing quote"))
                };
            }
            Some('\\') => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                _ => return Err(err(line, "unsupported escape")),
            },
            Some(c) => out.push(c),
        }
    }
}

fn set(e: &mut Entry, key: &str, v: Option<String>, line: usize) -> Result<(), CatalogueError> {
    let text = || v.clone().unwrap_or_default();
    match key {
        "id" => e.id = text(),
        "name" => e.name = text(),
        "kind" => e.kind = text(),
        "url" => e.url = text(),
        "licence" => e.licence = text(),
        "redistribution_allowed" => e.redistribution_allowed = text(),
        "commercial_use_allowed" => e.commercial_use_allowed = text(),
        "attribution_required" => e.attribution_required = text(),
        "attribution_text" => e.attribution_text = v,
        "approx_size" => e.approx_size = text(),
        "instruments" => e.instruments = text(),
        "quality_note" => e.quality_note = text(),
        "bundle" => e.bundle = text(),
        "bundle_reason" => e.bundle_reason = text(),
        "status" => e.status = text(),
        "file" => e.file = v,
        "checksum" => e.checksum = v,
        "licence_url" => e.licence_url = v,
        // Fields this reader does not expose (download_url, format) are read and dropped.
        "download_url" | "format" => {}
        _ => return Err(err(line, "unknown field")),
    }
    Ok(())
}

/// Parses the catalogue text.
pub fn parse(text: &str) -> Result<Catalogue, CatalogueError> {
    if text.len() > MAX_CATALOGUE_BYTES {
        return Err(err(0, "file too large"));
    }
    #[derive(PartialEq)]
    enum Section {
        None,
        Entries,
        Gaps,
        Other,
    }
    let mut section = Section::None;
    let mut cat = Catalogue::default();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') {
            section = match trimmed {
                "entries:" => Section::Entries,
                "gaps:" => Section::Gaps,
                t if t.ends_with(':') => Section::Other,
                _ => return Err(err(n, "unexpected top level line")),
            };
            continue;
        }
        match section {
            Section::None => return Err(err(n, "indented line before any section")),
            Section::Other => {}
            Section::Gaps => {
                let item = trimmed
                    .strip_prefix("- ")
                    .ok_or_else(|| err(n, "expected a list item"))?;
                cat.gaps.push(scalar(item, n)?.unwrap_or_default());
            }
            Section::Entries => {
                let (key, rest) = if let Some(r) = trimmed.strip_prefix("- ") {
                    if cat.entries.len() >= MAX_ENTRIES {
                        return Err(err(n, "too many entries"));
                    }
                    cat.entries.push(Entry::default());
                    r.split_once(':')
                        .ok_or_else(|| err(n, "expected key: value"))?
                } else {
                    trimmed
                        .split_once(':')
                        .ok_or_else(|| err(n, "expected key: value"))?
                };
                let entry = cat
                    .entries
                    .last_mut()
                    .ok_or_else(|| err(n, "field before the first entry"))?;
                set(entry, key.trim(), scalar(rest, n)?, n)?;
            }
        }
    }
    if cat.entries.iter().any(|e| e.id.is_empty()) {
        return Err(err(0, "an entry has no id"));
    }
    Ok(cat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_catalogue() {
        let text = include_str!("../../../sounds/catalogue.yaml");
        let c = parse(text).unwrap();
        assert!(c.entries.len() >= 19);
        assert!(!c.gaps.is_empty());
        let first = &c.entries[0];
        assert_eq!(first.id, "freepats-upright-piano-kw");
        assert_eq!(first.licence, "CC0 1.0");
        assert_eq!(first.bundle, "yes");
        assert!(first.attribution_text.is_none());
        assert!(c.entries.iter().all(|e| !e.name.is_empty()));
    }

    #[test]
    fn real_catalogue_lists_the_sixteen_builtin_voices() {
        let c = parse(include_str!("../../../sounds/catalogue.yaml")).unwrap();
        let builtin: Vec<_> = c.entries.iter().filter(|e| e.kind == "builtin").collect();
        assert_eq!(builtin.len(), 16);
        for e in builtin {
            assert!(e.id.starts_with("builtin-"), "{}", e.id);
            assert_eq!(e.bundle, "yes");
            assert_eq!(e.redistribution_allowed, "yes");
            assert_eq!(e.commercial_use_allowed, "yes");
            assert!(!e.licence.is_empty());
        }
    }

    #[test]
    fn handles_escapes_null_and_bare_values() {
        let c = parse(
            "entries:\n  - id: a\n    name: \"Say \\\"hi\\\"\"\n    attribution_text: null\n    kind: sfz\n",
        )
        .unwrap();
        assert_eq!(c.entries[0].name, "Say \"hi\"");
        assert_eq!(c.entries[0].kind, "sfz");
    }

    #[test]
    fn fails_closed_on_bad_shapes() {
        for bad in [
            "entries:\n    id: a\n",
            "entries:\n  - id: a\n    mystery: 1\n",
            "entries:\n  - id: a\n    name: \"open\n",
            "entries:\n  - name: x\n",
            "stray line\n",
            "  indented: 1\n",
            "entries:\n  - id: a\n    name: \"x\" tail\n",
            "gaps:\n  not a list\n",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(parse(&"x".repeat(MAX_CATALOGUE_BYTES + 1)).is_err());
    }

    #[test]
    fn unknown_sections_are_skipped() {
        let c = parse("entries:\n  - id: a\nrejected_unverified:\n  - \"x\"\n").unwrap();
        assert_eq!(c.entries.len(), 1);
    }
}
