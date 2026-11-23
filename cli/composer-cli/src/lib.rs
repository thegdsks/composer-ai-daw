//! `composer`: a thin client of composerd. It owns no editor logic; every
//! word maps to a daemon method and the daemon's JSON comes back unchanged.

pub mod plan;
pub mod transport;

use plan::Step;
use serde_json::{json, Value};
use transport::{Reply, Transport, TransportError};

/// Runs steps in order and returns one envelope per step:
/// `{"method", "result"}` or `{"method", "error"}`. Stops at the first
/// daemon error unless `keep_going`. The bool is true when all succeeded.
pub fn execute(
    t: &mut dyn Transport,
    steps: &[Step],
    keep_going: bool,
) -> Result<(Vec<Value>, bool), TransportError> {
    let mut out = Vec::with_capacity(steps.len());
    let mut all_ok = true;
    for s in steps {
        match t.call(&s.method, s.params.clone())? {
            Reply::Result(r) => out.push(json!({ "method": s.method, "result": r })),
            Reply::Error(e) => {
                all_ok = false;
                out.push(json!({ "method": s.method, "error": e }));
                if !keep_going {
                    break;
                }
            }
        }
    }
    Ok((out, all_ok))
}
