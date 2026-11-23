//! Safe file access for the daemon's save, load, export and render methods (D16).
//! Clients name files relative to one base directory; this module is the only
//! place that turns such a name into a real path.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// File settings. Every field has an env override, see `Config`.
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// `COMPOSERD_FILES_DIR`: the only directory clients may read or write in. Default `.`.
    pub dir: PathBuf,
    /// `COMPOSERD_SOUNDS_FILE`: the sound catalogue. Default `sounds/catalogue.yaml`.
    pub sounds_file: PathBuf,
    /// `COMPOSERD_MAX_FILE_BYTES`: largest file read, default 256 MiB.
    pub max_file_bytes: u64,
    /// `COMPOSERD_MAX_RENDER_SECONDS`: longest render or meter pass, default 600.
    pub max_render_seconds: u64,
    /// `COMPOSERD_RENDER_SAMPLE_RATE`: default 48000.
    pub render_sample_rate: u32,
    /// `COMPOSERD_RENDER_TAIL_SECONDS`: silence after the last note, default 2.
    pub render_tail_seconds: u32,
    /// `COMPOSERD_MAX_COUNT_IN_BARS`: largest transport count in, default 4.
    pub max_count_in_bars: u32,
    /// `COMPOSERD_MAX_HISTORY_PAGE`: most entries one `history.get` returns, default 200.
    pub max_history_page: usize,
    /// `COMPOSERD_GROUP_TIMEOUT_MS`: an open group is rolled back after this long, default 30000.
    pub group_timeout_ms: u64,
    /// `COMPOSERD_FEED_HISTORY`: change feeds kept for `changes.since`, default 64.
    pub feed_history: usize,
    /// `COMPOSERD_MAX_FEED_CHANGES`: a feed over this many changes is sent as `full`, default 5000.
    pub max_feed_changes: usize,
    /// Audio caps (`COMPOSER_AUDIO_MAX_*`, see `composer_audio::AudioLimits`).
    pub audio: composer_audio::AudioLimits,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            dir: PathBuf::from("."),
            sounds_file: PathBuf::from("sounds/catalogue.yaml"),
            max_file_bytes: 256 << 20,
            max_render_seconds: 600,
            render_sample_rate: 48_000,
            render_tail_seconds: 2,
            max_count_in_bars: composer_format::DEFAULT_MAX_COUNT_IN_BARS,
            max_history_page: 200,
            group_timeout_ms: 30_000,
            feed_history: 64,
            max_feed_changes: 5_000,
            audio: composer_audio::AudioLimits::default(),
        }
    }
}

/// What a method does with a file; it fixes the allowed extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Project,
    Midi,
    Wav,
}

impl Kind {
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Kind::Project => &["cmpf"],
            Kind::Midi => &["mid", "midi"],
            Kind::Wav => &["wav"],
        }
    }
}

/// A file problem with a stable machine readable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    pub code: &'static str,
    pub message: String,
}

fn fail(code: &'static str, message: impl Into<String>) -> FileError {
    FileError {
        code,
        message: message.into(),
    }
}

fn io(e: std::io::Error) -> FileError {
    let code = if e.kind() == std::io::ErrorKind::NotFound {
        "file_not_found"
    } else {
        "io_error"
    };
    fail(code, e.to_string())
}

const MAX_NAME_BYTES: usize = 255;

/// Checks the shape of a client supplied name without touching the disk.
fn check_name(name: &str, kind: Kind) -> Result<PathBuf, FileError> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.contains(['\0', '\\']) {
        return Err(fail(
            "path_invalid",
            "empty, too long or unusable characters",
        ));
    }
    let p = Path::new(name);
    if !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(fail(
            "path_invalid",
            "must be a relative name without . or .. parts",
        ));
    }
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !kind.extensions().contains(&ext.as_str()) {
        return Err(fail(
            "path_invalid",
            format!("extension must be one of: {}", kind.extensions().join(", ")),
        ));
    }
    Ok(p.to_path_buf())
}

/// Resolves `name` inside the base directory. The folder it sits in must exist
/// (nothing is created), must not lead outside the base through a symlink, and
/// the file itself must not be a symlink.
pub fn resolve(cfg: &FileConfig, name: &str, kind: Kind) -> Result<PathBuf, FileError> {
    let rel = check_name(name, kind)?;
    let base = fs::canonicalize(&cfg.dir)
        .map_err(|e| fail("files_dir_unavailable", format!("base directory: {e}")))?;
    let joined = base.join(&rel);
    let (Some(parent), Some(file)) = (joined.parent(), joined.file_name()) else {
        return Err(fail("path_invalid", "no file name"));
    };
    let parent = fs::canonicalize(parent).map_err(io)?;
    if !parent.starts_with(&base) {
        return Err(fail(
            "path_outside_base",
            "resolves outside the base directory",
        ));
    }
    let full = parent.join(file);
    if let Ok(meta) = fs::symlink_metadata(&full) {
        if meta.file_type().is_symlink() {
            return Err(fail("path_invalid", "symbolic links are not followed"));
        }
        if !meta.is_file() {
            return Err(fail("path_invalid", "not a regular file"));
        }
    }
    Ok(full)
}

/// Reads a resolved file, refusing anything over the size cap before reading it.
pub fn read(cfg: &FileConfig, path: &Path) -> Result<Vec<u8>, FileError> {
    let meta = fs::metadata(path).map_err(io)?;
    if meta.len() > cfg.max_file_bytes {
        return Err(fail(
            "file_too_large",
            format!("{} bytes, limit {}", meta.len(), cfg.max_file_bytes),
        ));
    }
    fs::read(path).map_err(io)
}

/// True when something already sits at the path.
pub fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Writes through a temp file in the same folder and renames it into place, so
/// a crash never leaves half a file. Refuses to replace a file unless `overwrite`.
pub fn write(path: &Path, bytes: &[u8], overwrite: bool) -> Result<(), FileError> {
    if !overwrite && exists(path) {
        return Err(fail(
            "file_exists",
            "the file exists; pass overwrite true to replace it",
        ));
    }
    let tmp = path.with_extension("tmp-composerd");
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(io(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut b = [0u8; 8];
        getrandom::fill(&mut b).unwrap();
        let name: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let d = std::env::temp_dir().join(format!("composerd-{tag}-{name}"));
        fs::create_dir_all(&d).unwrap();
        fs::canonicalize(&d).unwrap()
    }

    fn cfg(dir: &Path) -> FileConfig {
        FileConfig {
            dir: dir.to_path_buf(),
            ..FileConfig::default()
        }
    }

    #[test]
    fn rejects_traversal_absolute_and_odd_names() {
        let d = temp_dir("names");
        let c = cfg(&d);
        for bad in [
            "../x.cmpf",
            "a/../../x.cmpf",
            "/etc/x.cmpf",
            "./x.cmpf",
            "",
            "x\\y.cmpf",
            "x\0.cmpf",
            "x.txt",
            "x",
            ".cmpf",
        ] {
            let e = resolve(&c, bad, Kind::Project).unwrap_err();
            assert_eq!(e.code, "path_invalid", "{bad:?}");
        }
        assert!(resolve(&c, &"a".repeat(300), Kind::Wav).is_err());
        // Each kind accepts only its own extensions, case insensitive.
        assert!(resolve(&c, "song.WAV", Kind::Wav).is_ok());
        assert!(resolve(&c, "song.mid", Kind::Midi).is_ok());
        assert!(resolve(&c, "song.wav", Kind::Midi).is_err());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn missing_folder_is_not_created() {
        let d = temp_dir("nofolder");
        let e = resolve(&cfg(&d), "sub/x.cmpf", Kind::Project).unwrap_err();
        assert_eq!(e.code, "file_not_found");
        assert!(!d.join("sub").exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_the_base() {
        use std::os::unix::fs::symlink;
        let d = temp_dir("link");
        let outside = temp_dir("outside");
        symlink(&outside, d.join("out")).unwrap();
        let c = cfg(&d);
        let e = resolve(&c, "out/x.cmpf", Kind::Project).unwrap_err();
        assert_eq!(e.code, "path_outside_base");
        // A symlinked file inside the base is refused too.
        fs::write(outside.join("real.cmpf"), b"x").unwrap();
        symlink(outside.join("real.cmpf"), d.join("link.cmpf")).unwrap();
        let e = resolve(&c, "link.cmpf", Kind::Project).unwrap_err();
        assert_eq!(e.code, "path_invalid");
        fs::remove_dir_all(&d).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn write_is_atomic_and_refuses_overwrite_by_default() {
        let d = temp_dir("write");
        let c = cfg(&d);
        let p = resolve(&c, "a.cmpf", Kind::Project).unwrap();
        write(&p, b"one", false).unwrap();
        assert_eq!(write(&p, b"two", false).unwrap_err().code, "file_exists");
        assert_eq!(read(&c, &p).unwrap(), b"one");
        write(&p, b"two", true).unwrap();
        assert_eq!(read(&c, &p).unwrap(), b"two");
        let leftovers: Vec<_> = fs::read_dir(&d).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temp file left behind");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn read_enforces_the_size_cap_and_reports_missing_files() {
        let d = temp_dir("read");
        let mut c = cfg(&d);
        let p = resolve(&c, "a.cmpf", Kind::Project).unwrap();
        assert_eq!(read(&c, &p).unwrap_err().code, "file_not_found");
        write(&p, &[0u8; 100], false).unwrap();
        c.max_file_bytes = 50;
        assert_eq!(read(&c, &p).unwrap_err().code, "file_too_large");
        fs::remove_dir_all(&d).unwrap();
    }
}
