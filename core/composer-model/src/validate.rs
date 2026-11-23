//! Whole project validation: one pass over the tree, no cloning, using the same
//! rule functions as `Project::apply`.

use crate::automation::MAX_AUTOMATION_POINTS;
use crate::effects::MAX_CHAIN_EFFECTS;
use crate::id::Id;
use crate::project::{Loc, Project, RegionContent};
use crate::rules::{self, Rule};
use std::collections::HashSet;
use std::fmt;

/// Where in the tree a rule was broken. Unset parts do not apply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Locator {
    pub track_index: Option<usize>,
    pub region: Option<Id>,
    pub note: Option<Id>,
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("project")?;
        if let Some(i) = self.track_index {
            write!(f, ".tracks[{i}]")?;
        }
        if let Some(r) = self.region {
            write!(f, ".regions[{r}]")?;
        }
        if let Some(n) = self.note {
            write!(f, ".notes[{n}]")?;
        }
        Ok(())
    }
}

/// The first rule a project broke, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationError {
    pub rule: Rule,
    pub at: Locator,
}

impl ValidationError {
    /// Stable machine readable code of the broken rule.
    pub fn code(&self) -> &'static str {
        self.rule.code()
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} ({})",
            self.at,
            self.rule.code(),
            self.rule.what()
        )?;
        if let Rule::DuplicateId(id) | Rule::NoteKeyMismatch(id) = self.rule {
            write!(f, " id {id}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

struct Walk<'a> {
    p: &'a Project,
    seen: HashSet<Id>,
    visits: u64,
    /// First index mismatch, reported only if nothing else is wrong (a duplicate id makes the index lie).
    index_err: Option<ValidationError>,
}

impl Walk<'_> {
    /// Records an id; checks it is new and that the index agrees on where it lives.
    fn claim(&mut self, id: Id, loc: Loc, at: Locator) -> Result<(), ValidationError> {
        self.visits += 1;
        let err = |rule| ValidationError { rule, at };
        if !self.seen.insert(id) {
            return Err(err(Rule::DuplicateId(id)));
        }
        if self.p.index.ids.get(&id) != Some(&loc) {
            self.index_err.get_or_insert(err(Rule::IndexInconsistent));
        }
        Ok(())
    }
}

impl Project {
    /// Checks the whole project against every rule `apply` enforces, plus map key
    /// and id index consistency. One O(size) pass, nothing cloned.
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.validate_counted().0
    }

    /// Like [`Project::validate`], also returning how many tree nodes were visited
    /// (project, tempo entries, tracks, regions, notes), so tests can assert a single pass.
    pub fn validate_counted(&self) -> (Result<(), ValidationError>, u64) {
        let mut w = Walk {
            p: self,
            seen: HashSet::new(),
            visits: 0,
            index_err: None,
        };
        let r = self.walk(&mut w);
        (r, w.visits)
    }

    fn walk(&self, w: &mut Walk) -> Result<(), ValidationError> {
        let root = Locator::default();
        let at = |rule, at| ValidationError { rule, at };
        w.claim(self.id, Loc::Project, root)?;
        rules::name(&self.name).map_err(|r| at(r, root))?;
        let tm = &self.tempo_map;
        for (&tick, &v) in &tm.tempos {
            w.visits += 1;
            rules::tempo_entry(tick, v).map_err(|r| at(r, root))?;
        }
        for (&tick, v) in &tm.meters {
            w.visits += 1;
            rules::meter_entry(tick, v).map_err(|r| at(r, root))?;
        }
        for (&tick, k) in &tm.keys {
            w.visits += 1;
            rules::key_entry(tick, k).map_err(|r| at(r, root))?;
        }
        if !tm.tempos.contains_key(&0) || !tm.meters.contains_key(&0) {
            return Err(at(Rule::OriginMissing, root));
        }
        rules::master(&self.master).map_err(|r| at(r, root))?;
        if self.markers.len() > rules::MAX_MARKERS {
            return Err(at(Rule::EntryTick, root));
        }
        for (&tick, name) in &self.markers {
            w.visits += 1;
            rules::marker_entry(tick, name).map_err(|r| at(r, root))?;
        }
        let mut prev = None;
        for lane in &self.automation {
            w.visits += 1;
            let sorted = prev.is_none_or(|p| p < lane.target);
            let known = lane.target.track().is_none_or(|t| self.track(t).is_some());
            let sized = !lane.points.is_empty() && lane.points.len() <= MAX_AUTOMATION_POINTS;
            if !(sorted && known && sized) {
                return Err(at(Rule::Automation, root));
            }
            prev = Some(lane.target);
            for (&tick, p) in &lane.points {
                w.visits += 1;
                rules::automation_point(&lane.target, tick, p).map_err(|r| at(r, root))?;
            }
        }
        let mut prev_owner = None;
        for chain in &self.effects {
            w.visits += 1;
            let sorted = prev_owner.is_none_or(|p| p < chain.owner);
            let known = chain.owner.track().is_none_or(|t| self.track(t).is_some());
            let sized = !chain.effects.is_empty() && chain.effects.len() <= MAX_CHAIN_EFFECTS;
            if !(sorted && known && sized) {
                return Err(at(Rule::Effect, root));
            }
            prev_owner = Some(chain.owner);
            for e in &chain.effects {
                w.claim(e.id, Loc::Effect { owner: chain.owner }, root)?;
                rules::effect(e).map_err(|r| at(r, root))?;
            }
        }
        for (i, t) in self.tracks.iter().enumerate() {
            let tl = Locator {
                track_index: Some(i),
                ..root
            };
            for (key, r) in &t.regions {
                if *key != r.id {
                    let rl = Locator {
                        region: Some(*key),
                        ..tl
                    };
                    return Err(at(Rule::RegionKeyMismatch, rl));
                }
            }
            w.claim(t.id, Loc::Track, tl)?;
            if self.track_index(t.id) != Some(i) {
                w.index_err.get_or_insert(at(Rule::IndexInconsistent, tl));
            }
            rules::name(&t.name).map_err(|r| at(r, tl))?;
            rules::mix(&t.mix).map_err(|r| at(r, tl))?;
            rules::reverb_send(t.reverb_send).map_err(|r| at(r, tl))?;
            if let Some(sound) = &t.sound {
                rules::sound_id(sound).map_err(|r| at(r, tl))?;
            }
            for r in t.regions.values() {
                let rl = Locator {
                    region: Some(r.id),
                    ..tl
                };
                w.claim(r.id, Loc::Region { track: t.id }, rl)?;
                rules::region_fields(r).map_err(|e| at(e, rl))?;
                rules::content_fits(&r.content, t.kind).map_err(|e| at(e, rl))?;
                let RegionContent::Midi { notes } = &r.content else {
                    continue;
                };
                for (key, n) in notes {
                    let nl = Locator {
                        note: Some(n.id),
                        ..rl
                    };
                    if *key != n.id {
                        return Err(at(Rule::NoteKeyMismatch(n.id), nl));
                    }
                    w.claim(n.id, Loc::Note { region: r.id }, nl)?;
                    rules::note_fields(n).map_err(|e| at(e, nl))?;
                    rules::inside(n.start, n.duration, r.length).map_err(|e| at(e, nl))?;
                }
            }
        }
        // Every claimed id matched the index, so equal sizes mean no stray entries.
        let stray =
            self.index.ids.len() != w.seen.len() || self.index.track_pos.len() != self.tracks.len();
        if stray {
            w.index_err.get_or_insert(at(Rule::IndexInconsistent, root));
        }
        w.index_err.map_or(Ok(()), Err)
    }
}
