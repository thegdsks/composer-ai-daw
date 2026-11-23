//! `cargo run --release -p composer-bench` prints the measurements as YAML on
//! stdout and exits 1 when a budget set in the environment is exceeded.

use composer_bench::{report, run_all, spec};
use std::env;
use std::process::ExitCode;

fn sweep(name: &str, fallback: &[u64]) -> Vec<u64> {
    env::var(name)
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .filter(|v: &Vec<u64>| !v.is_empty())
        .unwrap_or_else(|| fallback.to_vec())
}

fn main() -> ExitCode {
    let s = spec::Spec::from_env();
    let rs = spec::RenderSpec::from_env();
    let regions = sweep(
        "COMPOSER_BENCH_SWEEP_REGIONS",
        &[1_000, 4_000, 16_000, 64_000],
    );
    let tracks = sweep("COMPOSER_BENCH_SWEEP_TRACKS", &[64, 256, 1_024, 4_000]);
    let lookups = sweep("COMPOSER_BENCH_LOOKUPS", &[200_000])[0];
    let r = run_all(&s, &rs, &regions, &tracks, lookups);
    print!("{}", report::yaml(&r));
    let v = report::check(&r, &spec::Budgets::from_env());
    for x in &v {
        eprintln!(
            "BUDGET EXCEEDED {} ({}): limit {} actual {:.2}",
            x.budget, x.env, x.limit, x.actual
        );
    }
    if v.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
