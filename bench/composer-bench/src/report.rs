//! YAML emission and budget checks. Hand written to keep the crate free of new dependencies.

use crate::measure::{Apply, Persist, Render, Scaling, UndoRedo};
use crate::spec::Budgets;
use std::fmt::Write;

pub struct Results {
    pub apply: Apply,
    pub undo_redo: UndoRedo,
    pub state_hash_ms: f64,
    pub persist: Persist,
    pub render: Render,
    pub scaling: Scaling,
}

/// One budget that was exceeded.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    pub budget: &'static str,
    pub env: &'static str,
    pub limit: f64,
    pub actual: f64,
}

/// Compares results with the bounds that are set; unset bounds are skipped.
pub fn check(r: &Results, b: &Budgets) -> Vec<Violation> {
    type Row = (&'static str, &'static str, Option<f64>, f64);
    let rows: [Row; 9] = [
        (
            "apply_ns_per_command",
            "COMPOSER_BENCH_MAX_APPLY_NS",
            b.max_apply_ns,
            r.apply.ns_per_command,
        ),
        (
            "undo_ns_per_step",
            "COMPOSER_BENCH_MAX_UNDO_NS",
            b.max_undo_ns,
            r.undo_redo.undo_ns_per_step,
        ),
        (
            "redo_ns_per_step",
            "COMPOSER_BENCH_MAX_REDO_NS",
            b.max_redo_ns,
            r.undo_redo.redo_ns_per_step,
        ),
        (
            "state_hash_ms",
            "COMPOSER_BENCH_MAX_STATE_HASH_MS",
            b.max_state_hash_ms,
            r.state_hash_ms,
        ),
        (
            "save_ms",
            "COMPOSER_BENCH_MAX_SAVE_MS",
            b.max_save_ms,
            r.persist.save_ms,
        ),
        (
            "load_ms",
            "COMPOSER_BENCH_MAX_LOAD_MS",
            b.max_load_ms,
            r.persist.load_ms,
        ),
        (
            "render_rtf",
            "COMPOSER_BENCH_MAX_RENDER_RTF",
            b.max_render_rtf,
            r.render.rtf,
        ),
        (
            "lookup_ratio",
            "COMPOSER_BENCH_MAX_LOOKUP_RATIO",
            b.max_lookup_ratio,
            r.scaling.lookup_ratio(),
        ),
        (
            "apply_ratio",
            "COMPOSER_BENCH_MAX_APPLY_RATIO",
            b.max_apply_ratio,
            r.scaling.apply_ratio(),
        ),
    ];
    rows.into_iter()
        .filter_map(|(budget, env, limit, actual)| {
            let limit = limit?;
            (actual > limit).then_some(Violation {
                budget,
                env,
                limit,
                actual,
            })
        })
        .collect()
}

pub fn yaml(r: &Results) -> String {
    let mut s = String::new();
    let a = &r.apply;
    let _ = writeln!(s, "apply:");
    let _ = writeln!(s, "  tracks: {}", a.counts.tracks);
    let _ = writeln!(s, "  regions: {}", a.counts.regions);
    let _ = writeln!(s, "  notes: {}", a.counts.notes);
    let _ = writeln!(s, "  commands: {}", a.counts.commands);
    let _ = writeln!(s, "  total_ms: {:.1}", a.total_ms);
    let _ = writeln!(s, "  ns_per_command: {:.0}", a.ns_per_command);
    let u = &r.undo_redo;
    let _ = writeln!(s, "undo_redo:");
    let _ = writeln!(s, "  cycled_depth: {}", u.depth);
    let _ = writeln!(s, "  undo_ns_per_step: {:.0}", u.undo_ns_per_step);
    let _ = writeln!(s, "  redo_ns_per_step: {:.0}", u.redo_ns_per_step);
    let _ = writeln!(s, "  full_history_steps: {}", u.full_history_steps);
    let _ = writeln!(
        s,
        "  full_undo_ns_per_step_first_decile: {:.0}",
        u.full_undo_ns_first_decile
    );
    let _ = writeln!(
        s,
        "  full_undo_ns_per_step_last_decile: {:.0}",
        u.full_undo_ns_last_decile
    );
    let _ = writeln!(s, "  full_redo_ms: {:.1}", u.full_redo_ms);
    let _ = writeln!(s, "  hash_restored: {}", u.hash_restored);
    let _ = writeln!(s, "state_hash_ms: {:.2}", r.state_hash_ms);
    let p = &r.persist;
    let _ = writeln!(s, "persist:");
    let _ = writeln!(s, "  bytes: {}", p.bytes);
    let _ = writeln!(s, "  save_ms: {:.1}", p.save_ms);
    let _ = writeln!(s, "  load_ms: {:.1}", p.load_ms);
    let _ = writeln!(s, "  peek_ms: {:.3}", p.peek_ms);
    let _ = writeln!(s, "  roundtrip_equal: {}", p.roundtrip_equal);
    let n = &r.render;
    let _ = writeln!(s, "render:");
    let _ = writeln!(s, "  tracks: {}", n.tracks);
    let _ = writeln!(s, "  notes: {}", n.notes);
    let _ = writeln!(s, "  audio_seconds: {:.1}", n.audio_seconds);
    let _ = writeln!(s, "  render_ms: {:.1}", n.render_ms);
    let _ = writeln!(s, "  rtf: {:.4}", n.rtf);
    let _ = writeln!(s, "  sample_bytes: {}", n.sample_bytes);
    let c = &r.scaling;
    let _ = writeln!(s, "scaling:");
    let _ = writeln!(s, "  by_regions:");
    for pt in &c.by_regions {
        let _ = writeln!(
            s,
            "    - {{ regions: {}, lookup_ns: {:.1}, add_note_ns: {:.0} }}",
            pt.size, pt.lookup_ns, pt.apply_ns
        );
    }
    let _ = writeln!(s, "  by_tracks:");
    for pt in &c.by_tracks {
        let _ = writeln!(
            s,
            "    - {{ tracks: {}, lookup_ns: {:.1}, front_insert_or_delete_ns: {:.0} }}",
            pt.size, pt.lookup_ns, pt.apply_ns
        );
    }
    let _ = writeln!(s, "  lookup_ratio_regions: {:.2}", c.lookup_ratio());
    let _ = writeln!(s, "  apply_ratio_regions: {:.2}", c.apply_ratio());
    let _ = writeln!(s, "  lookup_ratio_tracks: {:.2}", c.track_lookup_ratio());
    let _ = writeln!(s, "  apply_ratio_tracks: {:.2}", c.track_apply_ratio());
    s
}
