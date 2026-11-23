//! Edit latency scenario (D47): cost of one edit as seen by an interactive client, which
//! asks for the document hash around every edit. Reports the median of add_note, undo and
//! redo at several project sizes and checks the cost does not grow with the size.
//!
//! Env: COMPOSER_BENCH_EDIT_SIZES (notes, default 1000,20000,150000), COMPOSER_BENCH_EDIT_OPS
//! (default 200), COMPOSER_BENCH_SEED, and budgets COMPOSER_BENCH_MAX_EDIT_MS (largest size,
//! any of the three) and COMPOSER_BENCH_MAX_EDIT_RATIO (largest median over smallest).

use composer_bench::gen::{generate, Picker, REGION_TICKS};
use composer_bench::spec::Spec;
use composer_commands::{Command, Editor};
use composer_model::{IdGen, Project};
use std::env;
use std::process::ExitCode;
use std::time::Instant;

fn env_u64(name: &str, fallback: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(fallback)
}

fn env_f64(name: &str) -> Option<f64> {
    env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

struct Row {
    notes: u64,
    add_ms: f64,
    undo_ms: f64,
    redo_ms: f64,
    /// Command planning and apply alone, no hash, for comparison.
    exec_only_ms: f64,
    /// First edit after the initial hash: pays the one time table builds.
    cold_ms: f64,
}

/// One edit the way the wasm editor serves it: hash before the edit for the change feed,
/// hash to learn whether it changed, hash after for the reply.
fn edit_with_hashes(ed: &mut Editor, f: impl FnOnce(&mut Editor)) -> f64 {
    let t = Instant::now();
    let base = ed.state_hash();
    let before = ed.state_hash();
    f(ed);
    let after = ed.state_hash();
    std::hint::black_box((base, before, after));
    t.elapsed().as_secs_f64() * 1e3
}

fn measure(notes: u64, ops: usize, seed: u64) -> Row {
    let spec = Spec {
        tracks: (notes / 500).max(1),
        regions_per_track: 10,
        notes_per_region: 50,
        undo_depth: 0,
        seed,
    };
    let mut gen = IdGen::new(seed);
    let mut ed = Editor::new(Project::new(gen.next_id(), "edit-latency"));
    let ids = generate(&mut ed, &mut gen, &spec);
    // Warm the hash state once, as a loaded project would be.
    std::hint::black_box(ed.state_hash());
    let mut pick = Picker::new(seed ^ 0xA5A5);
    let (mut add, mut undo, mut redo, mut exec) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for _ in 0..ops {
        let (_, region) = ids.regions[pick.below(ids.regions.len())];
        let cmd = Command::AddNote {
            id: gen.next_id(),
            region,
            pitch: 40 + pick.below(40) as u8,
            velocity: 90,
            start: pick.below((REGION_TICKS - 20) as usize) as i64,
            duration: 10,
        };
        add.push(edit_with_hashes(&mut ed, |e| {
            let t = Instant::now();
            e.execute(&cmd).expect("valid add_note");
            exec.push(t.elapsed().as_secs_f64() * 1e3);
        }));
        undo.push(edit_with_hashes(&mut ed, |e| {
            e.undo().expect("undo");
        }));
        redo.push(edit_with_hashes(&mut ed, |e| {
            e.redo().expect("redo");
        }));
    }
    let cold_ms = add.first().copied().unwrap_or(0.0);
    Row {
        notes: ids.notes.len() as u64,
        add_ms: median(&mut add),
        undo_ms: median(&mut undo),
        redo_ms: median(&mut redo),
        exec_only_ms: median(&mut exec),
        cold_ms,
    }
}

fn main() -> ExitCode {
    let sizes: Vec<u64> = env::var("COMPOSER_BENCH_EDIT_SIZES")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .filter(|v: &Vec<u64>| !v.is_empty())
        .unwrap_or_else(|| vec![1_000, 20_000, 150_000]);
    let ops = env_u64("COMPOSER_BENCH_EDIT_OPS", 200) as usize;
    let seed = env_u64("COMPOSER_BENCH_SEED", 1);
    let rows: Vec<Row> = sizes.iter().map(|&n| measure(n, ops, seed)).collect();
    println!("edit_latency:");
    println!("  ops_per_size: {ops}");
    println!("  rows:");
    for r in &rows {
        println!(
            "    - {{ notes: {}, add_note_ms: {:.4}, undo_ms: {:.4}, redo_ms: {:.4}, execute_only_ms: {:.4}, first_edit_cold_ms: {:.1} }}",
            r.notes, r.add_ms, r.undo_ms, r.redo_ms, r.exec_only_ms, r.cold_ms
        );
    }
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return ExitCode::SUCCESS;
    };
    let ratio = last.add_ms / first.add_ms.max(1e-6);
    println!("  add_note_ratio_largest_over_smallest: {ratio:.2}");
    let mut failed = false;
    if let Some(max) = env_f64("COMPOSER_BENCH_MAX_EDIT_MS") {
        let worst = last.add_ms.max(last.undo_ms).max(last.redo_ms);
        if worst > max {
            eprintln!(
                "BUDGET EXCEEDED edit_ms (COMPOSER_BENCH_MAX_EDIT_MS): limit {max} actual {worst:.4}"
            );
            failed = true;
        }
    }
    if let Some(max) = env_f64("COMPOSER_BENCH_MAX_EDIT_RATIO") {
        if ratio > max {
            eprintln!(
                "BUDGET EXCEEDED edit_ratio (COMPOSER_BENCH_MAX_EDIT_RATIO): limit {max} actual {ratio:.2}"
            );
            failed = true;
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
