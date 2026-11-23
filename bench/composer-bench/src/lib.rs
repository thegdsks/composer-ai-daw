//! Scale benchmarks for composer. Everything is built and edited through the
//! public command API; thresholds are read from the environment (see `spec`).

pub mod gen;
pub mod measure;
pub mod report;
pub mod spec;

use report::Results;
use spec::{RenderSpec, Spec};

/// Runs every measurement. `scaling_regions` and `scaling_tracks` are the sweep sizes.
pub fn run_all(
    spec: &Spec,
    render: &RenderSpec,
    scaling_regions: &[u64],
    scaling_tracks: &[u64],
    lookups: u64,
) -> Results {
    let (mut ed, _ids, apply) = measure::apply(spec);
    let state_hash_ms = measure::state_hash_ms(ed.project(), 5);
    let persist = measure::persist(ed.project());
    let undo_redo = measure::undo_redo(&mut ed, spec.undo_depth);
    drop(ed);
    let render = measure::render(render, spec.seed);
    let scaling = measure::scaling(scaling_regions, scaling_tracks, lookups);
    Results {
        apply,
        undo_redo,
        state_hash_ms,
        persist,
        render,
        scaling,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spec::Budgets;

    fn tiny() -> Spec {
        Spec {
            tracks: 4,
            regions_per_track: 3,
            notes_per_region: 5,
            undo_depth: 20,
            seed: 1,
        }
    }

    #[test]
    fn generator_counts_and_history() {
        let (ed, ids, a) = measure::apply(&tiny());
        assert_eq!(ids.tracks.len(), 4);
        assert_eq!(ids.regions.len(), 12);
        assert_eq!(ids.notes.len(), 60);
        assert_eq!(a.counts.commands, 4 + 12 + 60);
        assert!(ed.project().index_is_consistent());
    }

    #[test]
    fn undo_redo_restores_state() {
        let (mut ed, _, _) = measure::apply(&tiny());
        let u = measure::undo_redo(&mut ed, 20);
        assert!(u.hash_restored);
        assert_eq!(u.full_history_steps, 76);
        assert_eq!(ed.redo_depth(), 0);
    }

    #[test]
    fn persist_roundtrips() {
        let (ed, _, _) = measure::apply(&tiny());
        let p = measure::persist(ed.project());
        assert!(p.roundtrip_equal && p.bytes > 0);
    }

    #[test]
    fn full_run_and_budget_check() {
        let rs = RenderSpec {
            tracks: 2,
            seconds: 8,
            sample_rate: 8_000,
        };
        let r = run_all(&tiny(), &rs, &[100, 400], &[8, 32], 500);
        assert!(r.render.audio_seconds >= 8.0);
        assert!(report::yaml(&r).contains("ns_per_command"));
        assert!(report::check(&r, &Budgets::default()).is_empty());
        let tight = Budgets {
            max_apply_ns: Some(0.0),
            ..Budgets::default()
        };
        let v = report::check(&r, &tight);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].env, "COMPOSER_BENCH_MAX_APPLY_NS");
    }
}
