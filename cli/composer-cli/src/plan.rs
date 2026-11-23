//! Maps CLI words to daemon calls. This is a pure table: no editor logic.

use serde_json::{json, Map, Value};
use std::fmt;

/// One daemon call.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub method: String,
    pub params: Value,
}

#[derive(Debug, PartialEq)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn usage(m: impl Into<String>) -> UsageError {
    UsageError(m.into())
}

pub const USAGE: &str =
    "usage: composer [--daemon PATH] [--ws URL] [--dry-run] [--keep-going] <command>
commands:
  describe                      list commands the daemon accepts (commands.describe)
  state                         full project JSON (state.get)
  hash                          state hash only (state.hash)
  new NAME [--id HEX32]         replace the session with a new project (session.new)
  apply JSON                    run one command (command.apply)
  group LABEL JSON_ARRAY [--source user|ai|template|recipe]
                                run commands as one undo step (command.group)
  propose LABEL JSON_ARRAY [--note TEXT] [--author NAME]
                                post commands for review, nothing changes yet (proposal.post)
  proposals [pending|accepted|rejected|all]
                                list proposals (proposal.list)
  resolve ID accept|reject      apply a proposal as one ai step, or drop it (proposal.resolve)
  templates                     list the starter templates (templates.list)
  template ID [SEED]            add a starter template as one undo step (template.apply)
  recipes                       list recipes and their parameters (recipes.list)
  recipe NAME [JSON]            show the commands a recipe would run, changes nothing (recipe.plan)
  recipe-apply NAME [JSON]      run a recipe as one undo step (recipe.apply)
  undo [STEPS] | redo [STEPS]   move along history, default 1 step (undo, redo)
  history [LIMIT [OFFSET]]      list undo steps with source and cursor (history.get)
  enabled [JSON]                which commands can run now, JSON may hold context {track, region} (commands.enabled)
  tree [JSON]                   compact project tree, JSON with notes and track (project.tree)
  selection [JSON]              read the selection, or set it from JSON (selection.get, selection.set)
  playhead [TICK]               read the playhead, or move it (transport.get, transport.set)
  transport [JSON]              read transport state, or set tick, loop, metronome, count_in_bars, follow (transport.get, transport.set)
  meters [JSON]                 offline peak and rms per track and master (meters.measure)
  sounds [BUNDLE]               list the sound catalogue (sounds.list)
  scale ROOT SCALE              scale pitch classes, ROOT 0..11 (theory.scale)
  chords ROOT SCALE [OCTAVE]    diatonic triads and sevenths of a key (theory.chords)
  pitch ROOT SCALE PITCH        in key, degree and nearest in key pitch (theory.pitch)
  save PATH [--overwrite]       save the project (project.save)
  load PATH                     load a project file (project.load)
  export-midi PATH [--overwrite]  write a MIDI file (export.midi)
  import-midi PATH              add a MIDI file as one undo step (import.midi)
  import-audio PATH [TRACK [START]]  import a WAV as an asset, place a clip when TRACK is given (audio.import)
  assets                        list imported audio assets (audio.assets)
  peaks ASSET [FROM TO BINS]    min and max per bin for a waveform (audio.peaks)
  render PATH [JSON]            render a WAV, JSON may hold sample_rate, tail_seconds, format, overwrite (render.wav)
  rpc METHOD [JSON]             raw daemon method
  run FILE                      replay a script: one {\"method\",\"params\"} per line
JSON may be inline, @FILE, or - for stdin. Recipe JSON holds params, scope {track, region} and seed. Env: COMPOSER_DAEMON, COMPOSER_WS_URL, COMPOSER_TOKEN.
Each stdio invocation is a fresh session; use --ws or `run` to keep state across steps.";

/// Source of text for `@file` and `-` arguments, injected so tests need no disk.
pub trait Input {
    fn file(&self, path: &str) -> Result<String, UsageError>;
    fn stdin(&self) -> Result<String, UsageError>;
}

fn text(arg: &str, input: &dyn Input) -> Result<String, UsageError> {
    if arg == "-" {
        input.stdin()
    } else if let Some(p) = arg.strip_prefix('@') {
        input.file(p)
    } else {
        Ok(arg.to_string())
    }
}

fn json_arg(arg: &str, input: &dyn Input) -> Result<Value, UsageError> {
    serde_json::from_str(&text(arg, input)?).map_err(|e| usage(format!("bad JSON: {e}")))
}

fn with_dry(mut params: Map<String, Value>, dry: bool) -> Value {
    if dry {
        params.insert("dry_run".into(), json!(true));
    }
    Value::Object(params)
}

fn step(method: &str, params: Value) -> Step {
    Step {
        method: method.into(),
        params,
    }
}

fn object(v: Value) -> Result<Map<String, Value>, UsageError> {
    match v {
        Value::Object(m) => Ok(m),
        _ => Err(usage("JSON argument must be an object")),
    }
}

/// Turns the words after the global flags into daemon calls.
pub fn plan(words: &[String], dry: bool, input: &dyn Input) -> Result<Vec<Step>, UsageError> {
    let (name, rest) = words
        .split_first()
        .ok_or_else(|| usage("missing command"))?;
    let arity = |n: usize| {
        if rest.len() == n {
            Ok(())
        } else {
            Err(usage(format!("{name} takes {n} argument(s)")))
        }
    };
    match name.as_str() {
        "describe" => arity(0).map(|()| vec![step("commands.describe", json!({}))]),
        "state" => arity(0).map(|()| vec![step("state.get", json!({}))]),
        "hash" => arity(0).map(|()| vec![step("state.hash", json!({}))]),
        "undo" | "redo" => {
            let mut p = Map::new();
            match rest {
                [] => {}
                [n] => {
                    let steps: u32 =
                        n.parse().ok().filter(|s| *s >= 1).ok_or_else(|| {
                            usage(format!("{name} STEPS is a number, at least 1"))
                        })?;
                    p.insert("steps".into(), json!(steps));
                }
                _ => return Err(usage(format!("{name} [STEPS]"))),
            }
            Ok(vec![step(name, with_dry(p, dry))])
        }
        "history" => {
            let num = |s: &String, what: &str| {
                s.parse::<u32>()
                    .map_err(|_| usage(format!("history {what} is a number")))
            };
            let mut p = Map::new();
            match rest {
                [] => {}
                [l] => {
                    p.insert("limit".into(), json!(num(l, "LIMIT")?));
                }
                [l, o] => {
                    p.insert("limit".into(), json!(num(l, "LIMIT")?));
                    p.insert("offset".into(), json!(num(o, "OFFSET")?));
                }
                _ => return Err(usage("history [LIMIT [OFFSET]]")),
            }
            Ok(vec![step("history.get", Value::Object(p))])
        }
        "enabled" => {
            let p = match rest {
                [] => Map::new(),
                [j] => object(json_arg(j, input)?)?,
                _ => return Err(usage("enabled [JSON]")),
            };
            Ok(vec![step("commands.enabled", Value::Object(p))])
        }
        "new" => {
            let (n, tail) = rest
                .split_first()
                .ok_or_else(|| usage("new needs a NAME"))?;
            let mut p = Map::new();
            p.insert("name".into(), json!(n));
            match tail {
                [] => {}
                [flag, id] if flag == "--id" => {
                    p.insert("id".into(), json!(id));
                }
                _ => return Err(usage("new NAME [--id HEX32]")),
            }
            Ok(vec![step("session.new", with_dry(p, dry))])
        }
        "apply" => {
            arity(1)?;
            let mut p = Map::new();
            p.insert("command".into(), json_arg(&rest[0], input)?);
            Ok(vec![step("command.apply", with_dry(p, dry))])
        }
        "group" => {
            let mut p = Map::new();
            match rest {
                [l, c] => {
                    p.insert("label".into(), json!(l));
                    p.insert("commands".into(), json_arg(c, input)?);
                }
                [l, c, flag, s] if flag == "--source" => {
                    p.insert("label".into(), json!(l));
                    p.insert("commands".into(), json_arg(c, input)?);
                    p.insert("source".into(), json!(s));
                }
                _ => return Err(usage("group LABEL JSON_ARRAY [--source NAME]")),
            }
            Ok(vec![step("command.group", with_dry(p, dry))])
        }
        "propose" => {
            let (l, c, opts) = match rest {
                [l, c, opts @ ..] => (l, c, opts),
                _ => {
                    return Err(usage(
                        "propose LABEL JSON_ARRAY [--note TEXT] [--author NAME]",
                    ))
                }
            };
            let mut p = Map::new();
            p.insert("label".into(), json!(l));
            p.insert("commands".into(), json_arg(c, input)?);
            let mut it = opts.iter();
            while let Some(flag) = it.next() {
                let key = match flag.as_str() {
                    "--note" => "note",
                    "--author" => "author",
                    _ => {
                        return Err(usage(
                            "propose LABEL JSON_ARRAY [--note TEXT] [--author NAME]",
                        ))
                    }
                };
                let v = it
                    .next()
                    .ok_or_else(|| usage(format!("{flag} needs a value")))?;
                p.insert(key.into(), json!(v));
            }
            Ok(vec![step("proposal.post", with_dry(p, dry))])
        }
        "proposals" => {
            let mut p = Map::new();
            match rest {
                [] => {}
                [s] => {
                    p.insert("status".into(), json!(s));
                }
                _ => return Err(usage("proposals [pending|accepted|rejected|all]")),
            }
            Ok(vec![step("proposal.list", Value::Object(p))])
        }
        "resolve" => match rest {
            [id, action] => {
                let mut p = Map::new();
                p.insert("id".into(), json!(id));
                p.insert("action".into(), json!(action));
                Ok(vec![step("proposal.resolve", with_dry(p, dry))])
            }
            _ => Err(usage("resolve ID accept|reject")),
        },
        "templates" => arity(0).map(|()| vec![step("templates.list", json!({}))]),
        "recipes" => arity(0).map(|()| vec![step("recipes.list", json!({}))]),
        "template" => {
            let mut p = Map::new();
            match rest {
                [id] => {
                    p.insert("template".into(), json!(id));
                }
                [id, seed] => {
                    let seed: u64 = seed
                        .parse()
                        .map_err(|_| usage("template ID [SEED]: SEED is a whole number"))?;
                    p.insert("template".into(), json!(id));
                    p.insert("seed".into(), json!(seed));
                }
                _ => return Err(usage("template ID [SEED]")),
            }
            Ok(vec![step("template.apply", with_dry(p, dry))])
        }
        "recipe" | "recipe-apply" => {
            let (recipe, mut p) = match rest {
                [r] => (r, Map::new()),
                [r, j] => (r, object(json_arg(j, input)?)?),
                _ => return Err(usage(format!("{name} NAME [JSON]"))),
            };
            p.insert("recipe".into(), json!(recipe));
            if name == "recipe" {
                Ok(vec![step("recipe.plan", Value::Object(p))])
            } else {
                Ok(vec![step("recipe.apply", with_dry(p, dry))])
            }
        }
        "tree" => {
            let p = match rest {
                [] => Map::new(),
                [j] => object(json_arg(j, input)?)?,
                _ => return Err(usage("tree [JSON]")),
            };
            Ok(vec![step("project.tree", Value::Object(p))])
        }
        "selection" => match rest {
            [] => Ok(vec![step("selection.get", json!({}))]),
            [j] => Ok(vec![step(
                "selection.set",
                with_dry(object(json_arg(j, input)?)?, dry),
            )]),
            _ => Err(usage("selection [JSON]")),
        },
        "playhead" => match rest {
            [] => Ok(vec![step("transport.get", json!({}))]),
            [t] => {
                let tick: i64 = t.parse().map_err(|_| usage("playhead TICK is a number"))?;
                let mut p = Map::new();
                p.insert("tick".into(), json!(tick));
                Ok(vec![step("transport.set", with_dry(p, dry))])
            }
            _ => Err(usage("playhead [TICK]")),
        },
        "transport" => match rest {
            [] => Ok(vec![step("transport.get", json!({}))]),
            [j] => Ok(vec![step(
                "transport.set",
                with_dry(object(json_arg(j, input)?)?, dry),
            )]),
            _ => Err(usage("transport [JSON]")),
        },
        "meters" => {
            let p = match rest {
                [] => Map::new(),
                [j] => object(json_arg(j, input)?)?,
                _ => return Err(usage("meters [JSON]")),
            };
            Ok(vec![step("meters.measure", Value::Object(p))])
        }
        "sounds" => {
            let mut p = Map::new();
            match rest {
                [] => {}
                [b] => {
                    p.insert("bundle".into(), json!(b));
                }
                _ => return Err(usage("sounds [BUNDLE]")),
            }
            Ok(vec![step("sounds.list", Value::Object(p))])
        }
        "scale" | "chords" | "pitch" => {
            let usage_text = match name.as_str() {
                "scale" => "scale ROOT SCALE",
                "chords" => "chords ROOT SCALE [OCTAVE]",
                _ => "pitch ROOT SCALE PITCH",
            };
            let max = if name == "scale" { 2 } else { 3 };
            if rest.len() < 2 || rest.len() > max || (name == "pitch" && rest.len() != 3) {
                return Err(usage(usage_text));
            }
            let num = |s: &String| {
                s.parse::<i64>()
                    .map_err(|_| usage(format!("{usage_text}: {s} is not a whole number")))
            };
            let mut p = Map::new();
            p.insert("root".into(), json!(num(&rest[0])?));
            p.insert("scale".into(), json!(rest[1]));
            if let Some(extra) = rest.get(2) {
                let key = if name == "pitch" { "pitch" } else { "octave" };
                p.insert(key.into(), json!(num(extra)?));
            }
            Ok(vec![step(
                match name.as_str() {
                    "scale" => "theory.scale",
                    "chords" => "theory.chords",
                    _ => "theory.pitch",
                },
                Value::Object(p),
            )])
        }
        "save" | "export-midi" => {
            let method = if name == "save" {
                "project.save"
            } else {
                "export.midi"
            };
            let (path, tail) = rest
                .split_first()
                .ok_or_else(|| usage(format!("{name} needs a PATH")))?;
            let mut p = Map::new();
            p.insert("path".into(), json!(path));
            match tail {
                [] => {}
                [f] if f == "--overwrite" => {
                    p.insert("overwrite".into(), json!(true));
                }
                _ => return Err(usage(format!("{name} PATH [--overwrite]"))),
            }
            Ok(vec![step(method, with_dry(p, dry))])
        }
        "load" | "import-midi" => {
            arity(1)?;
            let method = if name == "load" {
                "project.load"
            } else {
                "import.midi"
            };
            let mut p = Map::new();
            p.insert("path".into(), json!(rest[0]));
            Ok(vec![step(method, with_dry(p, dry))])
        }
        "import-audio" => {
            if rest.is_empty() || rest.len() > 3 {
                return Err(usage("import-audio PATH [TRACK [START]]"));
            }
            let mut p = Map::new();
            p.insert("path".into(), json!(rest[0]));
            if let Some(t) = rest.get(1) {
                p.insert("track".into(), json!(t));
            }
            if let Some(st) = rest.get(2) {
                let n: i64 = st
                    .parse()
                    .map_err(|_| usage("START must be a tick number"))?;
                p.insert("start".into(), json!(n));
            }
            Ok(vec![step("audio.import", with_dry(p, dry))])
        }
        "assets" => {
            arity(0)?;
            Ok(vec![step("audio.assets", json!({}))])
        }
        "peaks" => {
            if rest.is_empty() || rest.len() == 2 || rest.len() > 4 {
                return Err(usage("peaks ASSET [FROM TO [BINS]]"));
            }
            let mut p = Map::new();
            p.insert("asset".into(), json!(rest[0]));
            let num = |s: &String| -> Result<u64, UsageError> {
                s.parse()
                    .map_err(|_| usage("FROM, TO and BINS are whole numbers"))
            };
            if rest.len() >= 3 {
                p.insert("from".into(), json!(num(&rest[1])?));
                p.insert("to".into(), json!(num(&rest[2])?));
            }
            if let Some(b) = rest.get(3) {
                p.insert("bins".into(), json!(num(b)?));
            }
            Ok(vec![step("audio.peaks", Value::Object(p))])
        }
        "render" => {
            let (path, tail) = rest
                .split_first()
                .ok_or_else(|| usage("render needs a PATH"))?;
            let mut p = match tail {
                [] => Map::new(),
                [j] => object(json_arg(j, input)?)?,
                _ => return Err(usage("render PATH [JSON]")),
            };
            p.insert("path".into(), json!(path));
            Ok(vec![step("render.wav", with_dry(p, dry))])
        }
        "rpc" => {
            let (m, tail) = rest
                .split_first()
                .ok_or_else(|| usage("rpc needs a METHOD"))?;
            let params = match tail {
                [] => json!({}),
                [j] => json_arg(j, input)?,
                _ => return Err(usage("rpc METHOD [JSON]")),
            };
            Ok(vec![step(m, params)])
        }
        "run" => {
            arity(1)?;
            parse_script(&text(&rest[0], input)?)
        }
        other => Err(usage(format!("unknown command {other}"))),
    }
}

/// One JSON object per line; blank lines and `#` comments are skipped.
pub fn parse_script(src: &str) -> Result<Vec<Step>, UsageError> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |m: String| usage(format!("script line {}: {m}", i + 1));
        let v: Value = serde_json::from_str(line).map_err(|e| at(e.to_string()))?;
        let method = v
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| at("missing method".into()))?;
        let params = v.get("params").cloned().unwrap_or_else(|| json!({}));
        out.push(step(method, params));
    }
    Ok(out)
}
