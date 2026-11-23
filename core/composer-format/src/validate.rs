//! Fail closed checks for a project that came from untrusted bytes.
//!
//! The model's own `Project::validate` decides validity, so these are the same
//! rules `Project::apply` enforces. This module adds only the size limits and
//! maps model rules to stable format errors.

use crate::error::FormatError;
use crate::limits::Limits;
use composer_model::{Project, RegionContent, Rule};

fn map_rule(rule: Rule) -> FormatError {
    match rule {
        Rule::DuplicateId(id) | Rule::NoteKeyMismatch(id) => FormatError::DuplicateId(id),
        Rule::NoteOutside => FormatError::OutOfRange("note"),
        Rule::ContentKind => FormatError::Inconsistent("region content does not fit track"),
        Rule::RegionKeyMismatch => {
            FormatError::DanglingReference("region key differs from region id")
        }
        Rule::OriginMissing => FormatError::Inconsistent("tempo map lacks an entry at tick 0"),
        Rule::IndexInconsistent => FormatError::Inconsistent("id index"),
        other => FormatError::OutOfRange(other.what()),
    }
}

fn check_counts(p: &Project, limits: &Limits) -> Result<(), FormatError> {
    let over = |what, limit: u64, found: u64| {
        (found > limit).then_some(FormatError::LimitExceeded { what, limit, found })
    };
    if let Some(e) = over("tracks", limits.max_tracks, p.tracks.len() as u64) {
        return Err(e);
    }
    let (mut regions, mut notes) = (0u64, 0u64);
    for t in &p.tracks {
        regions += t.regions.len() as u64;
        for r in t.regions.values() {
            if let RegionContent::Midi { notes: n } = &r.content {
                notes += n.len() as u64;
            }
        }
    }
    if let Some(e) = over("regions", limits.max_regions, regions) {
        return Err(e);
    }
    if let Some(e) = over("notes", limits.max_notes, notes) {
        return Err(e);
    }
    Ok(())
}

/// Returns the project when it is within the size limits and passes every model rule.
pub(crate) fn validate(parsed: Project, limits: &Limits) -> Result<Project, FormatError> {
    check_counts(&parsed, limits)?;
    parsed.validate().map_err(|e| map_rule(e.rule))?;
    Ok(parsed)
}
