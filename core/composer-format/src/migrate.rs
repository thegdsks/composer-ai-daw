//! Upgrade chain. A file older than [`FORMAT_VERSION`] passes through one step
//! per version, each turning version N sections into version N+1 sections,
//! before the normal reader sees it. v0 is the first version, so the shipped
//! chain is empty; the machinery is tested with a fake step.

use crate::container::{Tag, FORMAT_VERSION};
use crate::error::FormatError;
use std::borrow::Cow;

/// A section held in memory during migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSection<'a> {
    pub tag: Tag,
    pub required: bool,
    pub data: Cow<'a, [u8]>,
}

/// Upgrades `sections` from version `from` to `from + 1`.
pub struct Step {
    pub from: u16,
    pub run: fn(&mut Vec<RawSection<'_>>) -> Result<(), String>,
}

/// Steps shipped with this build, ordered by `from`.
pub const STEPS: &[Step] = &[];

/// Runs the steps from `found` up to `target`. Each version needs exactly one step.
pub fn migrate_with(
    steps: &[Step],
    found: u16,
    target: u16,
    sections: &mut Vec<RawSection<'_>>,
) -> Result<(), FormatError> {
    for v in found..target {
        let step =
            steps
                .iter()
                .find(|s| s.from == v)
                .ok_or_else(|| FormatError::MigrationFailed {
                    from: v,
                    reason: "no upgrade step".into(),
                })?;
        (step.run)(sections).map_err(|reason| FormatError::MigrationFailed { from: v, reason })?;
    }
    Ok(())
}

/// Brings sections of any older version up to [`FORMAT_VERSION`].
pub fn migrate(found: u16, sections: &mut Vec<RawSection<'_>>) -> Result<(), FormatError> {
    migrate_with(STEPS, found, FORMAT_VERSION, sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_v0_to_v1(s: &mut Vec<RawSection<'_>>) -> Result<(), String> {
        let p = s
            .iter_mut()
            .find(|x| x.tag == Tag::PROJECT)
            .ok_or("no project")?;
        p.data.to_mut().extend_from_slice(b"+v1");
        Ok(())
    }

    fn fail(_: &mut Vec<RawSection<'_>>) -> Result<(), String> {
        Err("boom".into())
    }

    fn sample() -> Vec<RawSection<'static>> {
        vec![RawSection {
            tag: Tag::PROJECT,
            required: true,
            data: Cow::Borrowed(b"p"),
        }]
    }

    #[test]
    fn synthetic_step_runs_and_chain_is_ordered() {
        let steps = [Step {
            from: 0,
            run: fake_v0_to_v1,
        }];
        let mut s = sample();
        migrate_with(&steps, 0, 1, &mut s).unwrap();
        assert_eq!(&*s[0].data, b"p+v1");
        // Already current: nothing runs.
        let mut s = sample();
        migrate_with(&steps, 1, 1, &mut s).unwrap();
        assert_eq!(&*s[0].data, b"p");
    }

    #[test]
    fn missing_or_failing_step_is_typed() {
        let mut s = sample();
        let e = migrate_with(&[], 0, 1, &mut s).unwrap_err();
        assert_eq!(e.code(), "migration_failed");
        let steps = [Step { from: 0, run: fail }];
        let e = migrate_with(&steps, 0, 1, &mut s).unwrap_err();
        assert!(matches!(e, FormatError::MigrationFailed { from: 0, .. }));
    }

    #[test]
    fn shipped_chain_is_identity_at_current_version() {
        let mut s = sample();
        migrate(FORMAT_VERSION, &mut s).unwrap();
        assert_eq!(s, sample());
    }
}
