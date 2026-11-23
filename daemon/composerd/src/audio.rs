//! Audio methods (D43): import a file as a content addressed asset, list assets, read peaks.
//! Bytes logic lives in composer-audio; this file only parses, resolves paths (D16) and shapes JSON.

use crate::files::{self, FileConfig, Kind};
use crate::hub::{random_id, Inner};
use crate::rpc::{hex, parse_params, summary, with, RpcError};
use composer_audio::{AudioError, AudioStream, DirStore, WavStream};
use composer_commands::Command;
use composer_engine::TempoClock;
use composer_format::{AssetRef, ContentHash, MAX_ASSET_NAME_LEN};
use composer_model::{Id, RegionContent, Tick, MAX_TICK};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const ASSET_DIR: &str = "assets";
/// Peak bins returned when the client does not say.
const DEFAULT_PEAK_BINS: u32 = 512;

/// The asset store inside the base directory. The folder name is fixed, never client supplied,
/// and a symbolic link in its place is refused (D16).
pub(crate) fn store(cfg: &FileConfig) -> Result<DirStore, RpcError> {
    let dir = cfg.dir.join(ASSET_DIR);
    if let Ok(m) = fs::symlink_metadata(&dir) {
        if m.file_type().is_symlink() {
            return Err(RpcError::app(
                "path_outside_base",
                "the asset folder is a symbolic link",
            ));
        }
    }
    Ok(DirStore::new(dir, cfg.audio))
}

/// Like [`audio_error`] for the file the client named, where a missing file is a file error.
fn source_error(e: &AudioError) -> RpcError {
    match e {
        AudioError::NotFound => RpcError::app("file_not_found", "the file does not exist"),
        other => audio_error(other),
    }
}

fn audio_error(e: &AudioError) -> RpcError {
    match e {
        AudioError::NotFound => RpcError::app("asset_missing", e.to_string()),
        AudioError::BadRequest(_) => RpcError::invalid_params(e.to_string()),
        AudioError::TooLarge(_) => RpcError::app("file_too_large", e.to_string()),
        _ => RpcError::app("audio_invalid", format!("{}: {e}", e.code())),
    }
}

/// Refuses a command list that names an asset the session does not hold (D43).
pub(crate) fn check_assets(g: &Inner, cmds: &[Command]) -> Result<(), RpcError> {
    for c in cmds {
        if let Command::AddAudioClip { asset, .. } = c {
            let known = ContentHash::from_hex(asset)
                .and_then(|h| g.assets.find_by_hash(&h))
                .is_some();
            if !known {
                return Err(RpcError::app(
                    "asset_not_found",
                    "import the audio file first (audio.import)",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    path: String,
    track: Option<Id>,
    start: Option<Tick>,
    length: Option<Tick>,
    id: Option<Id>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AssetsParams {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeaksParams {
    asset: String,
    from: Option<u64>,
    to: Option<u64>,
    bins: Option<u32>,
}

/// Smallest tick length that covers `frames` source frames from `start`, by the tempo map.
fn ticks_for(map: &composer_model::TempoMap, start: Tick, rate: u32, frames: u64) -> Tick {
    let clock = TempoClock::new(map, rate);
    let target = clock.frame_at(start) + frames;
    let (mut lo, mut hi) = (1, MAX_TICK - start.max(0));
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if clock.frame_at(start + mid) >= target {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo.max(1)
}

fn asset_json(a: &AssetRef, frames: Option<u64>, present: bool) -> Value {
    json!({
        "id": a.id,
        "hash": a.hash.to_hex(),
        "name": a.name,
        "bytes": a.bytes,
        "sample_rate": a.sample_rate,
        "channels": a.channels,
        "frames": frames,
        "seconds": frames.map(|f| f as f64 / f64::from(a.sample_rate)),
        "present": present,
    })
}

pub(crate) fn import(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: ImportParams = parse_params(params)?;
    let src = files::resolve(&g.files, &p.path, Kind::Wav)?;
    if p.length.is_some() && p.track.is_none() {
        return Err(RpcError::invalid_params("length needs a track"));
    }
    let store = store(&g.files)?;
    if p.dry_run {
        // Read the header only: hashing a long file is the real import's job.
        let f = fs::File::open(&src).map_err(|e| source_error(&AudioError::from(e)))?;
        let info = WavStream::open(f, &g.files.audio)
            .map_err(|e| audio_error(&e))?
            .info();
        return Ok(json!({
            "dry_run": true,
            "path": p.path,
            "sample_rate": info.sample_rate,
            "channels": info.channels,
            "frames": info.frames,
            "seconds": info.frames as f64 / f64::from(info.sample_rate),
            "would_place": p.track.is_some(),
        }));
    }
    let stored = store.import_file(&src).map_err(|e| source_error(&e))?;
    let existing = g.assets.find_by_hash(&stored.hash).cloned();
    let asset = match existing {
        Some(a) => a,
        None => {
            let mut name = Path::new(&p.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("audio.wav")
                .to_string();
            name.truncate(MAX_ASSET_NAME_LEN);
            let a = AssetRef {
                id: random_id(),
                hash: stored.hash,
                bytes: stored.bytes,
                name,
                sample_rate: stored.info.sample_rate,
                channels: stored.info.channels,
            };
            g.assets
                .insert(a.clone())
                .map_err(|e| RpcError::app(e.code(), e.to_string()))?;
            a
        }
    };
    let asset_value = asset_json(&asset, Some(stored.info.frames), true);
    let Some(track) = p.track else {
        return Ok(with(
            summary(&g.editor),
            json!({ "asset": asset_value, "placed": false }),
        ));
    };
    let start = p.start.unwrap_or(0);
    let length = p.length.unwrap_or_else(|| {
        ticks_for(
            &g.editor.project().tempo_map,
            start,
            stored.info.sample_rate,
            stored.info.frames,
        )
    });
    let clip = p.id.unwrap_or_else(random_id);
    let cmd = Command::AddAudioClip {
        id: clip,
        track,
        start,
        length,
        asset: asset.hash.to_hex(),
        offset: None,
        gain: None,
    };
    g.editor
        .execute(&cmd)
        .map_err(|e| RpcError::command(&e, None))?;
    Ok(with(
        summary(&g.editor),
        json!({ "asset": asset_value, "placed": true, "clip": clip, "start": start, "length": length }),
    ))
}

pub(crate) fn assets(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let _: AssetsParams = parse_params(params)?;
    let store = store(&g.files)?;
    let project = g.editor.project();
    let list: Vec<Value> = g
        .assets
        .iter()
        .map(|a| {
            let hash = a.hash.to_hex();
            let frames = store.open_wav(&hash).ok().map(|s| s.info().frames);
            let clips: Vec<Id> = project
                .tracks
                .iter()
                .flat_map(|t| t.regions.values())
                .filter(
                    |r| matches!(&r.content, RegionContent::Clip { asset, .. } if *asset == hash),
                )
                .map(|r| r.id)
                .collect();
            let mut v = asset_json(a, frames, store.contains(&hash));
            v["clips"] = json!(clips);
            v
        })
        .collect();
    Ok(json!({ "count": list.len(), "assets": list, "state_hash": hex(project.state_hash()) }))
}

pub(crate) fn peaks(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let p: PeaksParams = parse_params(params)?;
    let asset = match p.asset.len() {
        64 => ContentHash::from_hex(&p.asset).and_then(|h| g.assets.find_by_hash(&h)),
        32 => p.asset.parse::<Id>().ok().and_then(|id| g.assets.get(id)),
        _ => None,
    }
    .ok_or_else(|| RpcError::app("asset_not_found", "no such asset in this session"))?;
    let hash = asset.hash.to_hex();
    let store = store(&g.files)?;
    let bins = p.bins.unwrap_or(DEFAULT_PEAK_BINS);
    let r = store
        .peaks(&hash, p.from.unwrap_or(0), p.to, bins)
        .map_err(|e| audio_error(&e))?;
    let round = |v: &[f32]| -> Vec<f64> {
        v.iter()
            .map(|x| (f64::from(*x) * 10_000.0).round() / 10_000.0)
            .collect()
    };
    Ok(json!({
        "asset": hash,
        "from": r.from,
        "to": r.to,
        "bins": r.bins,
        "level_frames": r.level_frames,
        "min": round(&r.min),
        "max": round(&r.max),
    }))
}
