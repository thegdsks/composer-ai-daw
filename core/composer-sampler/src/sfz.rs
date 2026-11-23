//! SFZ reader. Facts: notes/ref-sampler-formats.yaml, section sfz.

use crate::wav::{decode_wav, WavSample};
use crate::{Limits, SamplerError};
use composer_engine::{Adsr, LoopMode, SampleBank, SampleData, Zone};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path};

type Opcodes = BTreeMap<String, String>;

/// Shortest release we allow, so a note off never clicks (SFZ default is 0).
const MIN_RELEASE: f32 = 0.002;

#[derive(Default)]
struct Scopes {
    control: Opcodes,
    global: Opcodes,
    master: Opcodes,
    group: Opcodes,
}

/// Where opcodes found right now are stored.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Control,
    Global,
    Master,
    Group,
    Region,
    Ignore,
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let b = text.as_bytes();
    let mut i = 0;
    let mut start = 0;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            out.push_str(&text[start..i]);
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            start = i;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            out.push_str(&text[start..i]);
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(b.len());
            out.push(' ');
            start = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[start..]);
    out
}

enum Token {
    Header(String),
    Opcode(String, String),
}

/// Splits comment free text into headers and `name=value` pairs. A value runs up to the
/// next whitespace that is followed by `name=` or `<`, so paths may hold spaces.
fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let at = |i: usize| chars.get(i).map(|c| c.1);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c.is_whitespace() {
            i += 1;
        } else if c == '<' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 != '>' {
                j += 1;
            }
            let name: String = chars[i + 1..j.min(chars.len())]
                .iter()
                .map(|c| c.1)
                .collect();
            out.push(Token::Header(name.trim().to_ascii_lowercase()));
            i = j + 1;
        } else {
            let mut j = i;
            while at(j).is_some_and(is_ident) {
                j += 1;
            }
            if j > i && at(j) == Some('=') {
                let name: String = chars[i..j].iter().map(|c| c.1).collect();
                let vstart = j + 1;
                let mut k = vstart;
                while k < chars.len() {
                    if chars[k].1.is_whitespace() {
                        let mut q = k;
                        while at(q).is_some_and(char::is_whitespace) {
                            q += 1;
                        }
                        if at(q) == Some('<') {
                            break;
                        }
                        let mut e = q;
                        while at(e).is_some_and(is_ident) {
                            e += 1;
                        }
                        if e > q && at(e) == Some('=') {
                            break;
                        }
                        k = q;
                    } else {
                        k += 1;
                    }
                }
                let value: String = chars[vstart..k.min(chars.len())]
                    .iter()
                    .map(|c| c.1)
                    .collect();
                out.push(Token::Opcode(
                    name.to_ascii_lowercase(),
                    value.trim().to_string(),
                ));
                i = k;
            } else {
                // Stray word (for example a directive): skip it.
                while at(i).is_some_and(|c| !c.is_whitespace()) {
                    i += 1;
                }
            }
        }
    }
    out
}

fn parse_note(s: &str) -> Result<u8, SamplerError> {
    if let Ok(n) = s.parse::<i32>() {
        return u8::try_from(n)
            .ok()
            .filter(|n| *n <= 127)
            .ok_or(SamplerError::Malformed);
    }
    let b = s.as_bytes();
    let base = match b.first().map(u8::to_ascii_lowercase) {
        Some(b'c') => 0,
        Some(b'd') => 2,
        Some(b'e') => 4,
        Some(b'f') => 5,
        Some(b'g') => 7,
        Some(b'a') => 9,
        Some(b'b') => 11,
        _ => return Err(SamplerError::Malformed),
    };
    let (shift, rest) = match b.get(1) {
        Some(b'#') => (1, &s[2..]),
        Some(b'b') | Some(b'B') if b.len() > 2 => (-1, &s[2..]),
        _ => (0, &s[1..]),
    };
    let octave: i32 = rest.parse().map_err(|_| SamplerError::Malformed)?;
    let n = (octave + 1) * 12 + base + shift;
    u8::try_from(n)
        .ok()
        .filter(|n| *n <= 127)
        .ok_or(SamplerError::Malformed)
}

fn parse_f(s: &str) -> Result<f64, SamplerError> {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or(SamplerError::Malformed)
}

fn parse_u(s: &str) -> Result<u32, SamplerError> {
    s.parse::<u32>().map_err(|_| SamplerError::Malformed)
}

/// A region's opcode view: closest scope wins.
struct View<'a> {
    levels: [&'a Opcodes; 4],
}

impl View<'_> {
    fn get(&self, names: &[&str]) -> Option<&str> {
        self.levels
            .iter()
            .find_map(|l| names.iter().find_map(|n| l.get(*n)))
            .map(String::as_str)
    }

    /// Like `get` but a `key` opcode at the same scope also answers.
    fn note(&self, name: &str) -> Result<Option<u8>, SamplerError> {
        for l in self.levels {
            if let Some(v) = l.get(name).or_else(|| l.get("key")) {
                return parse_note(v).map(Some);
            }
        }
        Ok(None)
    }

    fn float(&self, names: &[&str]) -> Result<Option<f64>, SamplerError> {
        self.get(names).map(parse_f).transpose()
    }
}

struct Loaded {
    wav: WavSample,
}

/// Reads an SFZ instrument. `read` returns the bytes of a sample path as written in the file
/// (relative, `/` separated, `default_path` already applied); it is the only door to storage.
pub fn load_sfz(
    text: &[u8],
    limits: &Limits,
    read: &mut dyn FnMut(&str) -> Result<Vec<u8>, SamplerError>,
) -> Result<SampleBank, SamplerError> {
    if text.len() > limits.max_file_bytes {
        return Err(SamplerError::TooLarge);
    }
    let text = strip_comments(&String::from_utf8_lossy(text));
    let mut sc = Scopes::default();
    let mut region = Opcodes::new();
    let mut target = Target::Ignore;
    let mut regions: Vec<Opcodes> = Vec::new();
    // Each region snapshots the inherited scopes at the moment it closes.
    let mut snapshots: Vec<[Opcodes; 3]> = Vec::new();
    let flush = |region: &mut Opcodes,
                 sc: &Scopes,
                 target: Target,
                 regions: &mut Vec<Opcodes>,
                 snaps: &mut Vec<[Opcodes; 3]>|
     -> Result<(), SamplerError> {
        if target == Target::Region {
            if regions.len() >= limits.max_zones {
                return Err(SamplerError::TooLarge);
            }
            regions.push(std::mem::take(region));
            snaps.push([sc.group.clone(), sc.master.clone(), sc.global.clone()]);
        }
        Ok(())
    };
    for tok in tokens(&text) {
        match tok {
            Token::Header(h) => {
                flush(&mut region, &sc, target, &mut regions, &mut snapshots)?;
                target = match h.as_str() {
                    "control" => {
                        sc.control.clear();
                        Target::Control
                    }
                    "global" => {
                        sc.global.clear();
                        sc.master.clear();
                        sc.group.clear();
                        Target::Global
                    }
                    "master" => {
                        sc.master.clear();
                        sc.group.clear();
                        Target::Master
                    }
                    "group" => {
                        sc.group.clear();
                        Target::Group
                    }
                    "region" => Target::Region,
                    _ => Target::Ignore,
                };
            }
            Token::Opcode(k, v) => {
                let map = match target {
                    Target::Control => &mut sc.control,
                    Target::Global => &mut sc.global,
                    Target::Master => &mut sc.master,
                    Target::Group => &mut sc.group,
                    Target::Region => &mut region,
                    Target::Ignore => continue,
                };
                if map.len() >= 4096 {
                    return Err(SamplerError::TooLarge);
                }
                map.insert(k, v);
            }
        }
    }
    flush(&mut region, &sc, target, &mut regions, &mut snapshots)?;

    let default_path = sc.control.get("default_path").cloned().unwrap_or_default();
    let mut wavs: BTreeMap<String, Loaded> = BTreeMap::new();
    let mut slots: BTreeMap<(String, u32, u32), u32> = BTreeMap::new();
    let mut samples: Vec<SampleData> = Vec::new();
    let mut zones: Vec<Zone> = Vec::new();
    let mut total = 0usize;
    for (r, snap) in regions.iter().zip(&snapshots) {
        let view = View {
            levels: [r, &snap[0], &snap[1], &snap[2]],
        };
        let Some(name) = view.get(&["sample"]) else {
            continue;
        };
        // Generator samples such as *sine are not files.
        if name.starts_with('*') {
            continue;
        }
        let path = format!("{default_path}{name}").replace('\\', "/");
        if !wavs.contains_key(&path) {
            let bytes = read(&path)?;
            wavs.insert(
                path.clone(),
                Loaded {
                    wav: decode_wav(&bytes, limits)?,
                },
            );
        }
        let wav = &wavs[&path].wav;
        let n = wav.len_frames() as u32;
        let off = view.get(&["offset"]).map(parse_u).transpose()?.unwrap_or(0);
        let last = view
            .get(&["end"])
            .map(parse_u)
            .transpose()?
            .map_or(n, |e| e.saturating_add(1).min(n));
        if off >= last {
            continue;
        }
        let len = last - off;
        let key = (path.clone(), off, last);
        let root = view.note("pitch_keycenter")?.unwrap_or(60);
        let index = match slots.get(&key) {
            Some(i) => *i,
            None => {
                total += len as usize * usize::from(wav.channels);
                if total > limits.max_total_frames.saturating_mul(2) {
                    return Err(SamplerError::TooLarge);
                }
                let ch = usize::from(wav.channels);
                let data = wav.frames[off as usize * ch..last as usize * ch].to_vec();
                let s = SampleData::new(wav.rate, wav.channels, root, data)
                    .map_err(|_| SamplerError::Malformed)?;
                samples.push(s);
                let i = (samples.len() - 1) as u32;
                slots.insert(key, i);
                i
            }
        };
        let lo = view.note("lokey")?.unwrap_or(0);
        let hi = view.note("hikey")?.unwrap_or(127);
        let vlo = view.get(&["lovel"]).map(parse_u).transpose()?.unwrap_or(0);
        let vhi = view
            .get(&["hivel"])
            .map(parse_u)
            .transpose()?
            .unwrap_or(127);
        if lo > hi || vlo > vhi || vhi > 127 {
            continue;
        }
        let mode_name = view.get(&["loop_mode", "loopmode"]);
        let wav_loop = wav
            .loop_points
            .map(|(s, e)| (s as i64 - i64::from(off), e as i64 - i64::from(off)));
        let mode = match mode_name {
            Some("no_loop") => LoopMode::None,
            Some("one_shot") => LoopMode::OneShot,
            Some("loop_continuous") => LoopMode::Continuous,
            Some("loop_sustain") => LoopMode::Sustain,
            Some(_) => return Err(SamplerError::Malformed),
            None if wav_loop.is_some() => LoopMode::Continuous,
            None => LoopMode::None,
        };
        let (mut ls, mut le) = (0u32, 0u32);
        if matches!(mode, LoopMode::Continuous | LoopMode::Sustain) {
            let s = match view.get(&["loop_start", "loopstart"]) {
                Some(v) => i64::from(parse_u(v)?) - i64::from(off),
                None => wav_loop.map_or(0, |l| l.0),
            };
            let e = match view.get(&["loop_end", "loopend"]) {
                Some(v) => i64::from(parse_u(v)?) + 1 - i64::from(off),
                None => wav_loop.map_or(i64::from(len), |l| l.1),
            };
            if s < 0 || s >= e || e > i64::from(len) {
                return Err(SamplerError::BadLoop);
            }
            ls = s as u32;
            le = e as u32;
        }
        let transpose = view.float(&["transpose"])?.unwrap_or(0.0);
        let tune = view.float(&["tune", "pitch"])?.unwrap_or(0.0);
        let keytrack = view.float(&["pitch_keytrack"])?.unwrap_or(100.0);
        let secs = |name: &str| -> Result<f32, SamplerError> {
            Ok(view.float(&[name])?.unwrap_or(0.0).clamp(0.0, 100.0) as f32)
        };
        let sustain = view.float(&["ampeg_sustain"])?.unwrap_or(100.0);
        let volume = view
            .float(&["volume", "gain"])?
            .unwrap_or(0.0)
            .clamp(-144.0, 48.0);
        let pan = view.float(&["pan"])?.unwrap_or(0.0);
        zones.push(Zone {
            sample: index,
            key_lo: lo,
            key_hi: hi,
            vel_lo: vlo as u8,
            vel_hi: vhi as u8,
            root_key: root,
            tune_cents: (transpose * 100.0 + tune)
                .round()
                .clamp(-48_000.0, 48_000.0) as i32,
            scale_cents: keytrack.round().clamp(-1200.0, 1200.0) as i16,
            loop_mode: mode,
            loop_start: ls,
            loop_end: le,
            adsr: Adsr {
                attack: secs("ampeg_attack")?,
                decay: secs("ampeg_decay")?,
                sustain: (sustain / 100.0).clamp(0.0, 1.0) as f32,
                release: secs("ampeg_release")?.max(MIN_RELEASE),
            },
            gain: 10f64.powf(volume / 20.0) as f32,
            pan: (pan / 100.0).clamp(-1.0, 1.0) as f32,
        });
    }
    if zones.is_empty() {
        return Err(SamplerError::NoZones);
    }
    Ok(SampleBank::new(samples, zones)?)
}

/// Reads an SFZ file and its samples from disk. Sample paths must stay inside the folder of
/// the sfz: no absolute paths, no `..`, and a symlink may not lead out.
pub fn load_sfz_file(path: &Path, limits: &Limits) -> Result<SampleBank, SamplerError> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let base = base
        .canonicalize()
        .map_err(|_| SamplerError::MissingSample)?;
    let text = read_capped(path, limits.max_file_bytes)?;
    let mut read = |rel: &str| -> Result<Vec<u8>, SamplerError> {
        let p = Path::new(rel);
        if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(SamplerError::MissingSample);
        }
        let full = base
            .join(p)
            .canonicalize()
            .map_err(|_| SamplerError::MissingSample)?;
        if !full.starts_with(&base) {
            return Err(SamplerError::MissingSample);
        }
        read_capped(&full, limits.max_file_bytes)
    };
    load_sfz(&text, limits, &mut read)
}

fn read_capped(path: &Path, cap: usize) -> Result<Vec<u8>, SamplerError> {
    let f = std::fs::File::open(path).map_err(|_| SamplerError::MissingSample)?;
    let mut buf = Vec::new();
    f.take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|_| SamplerError::MissingSample)?;
    if buf.len() > cap {
        return Err(SamplerError::TooLarge);
    }
    Ok(buf)
}
