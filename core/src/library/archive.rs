//! Cold storage for logs: zstd-compressed copies under
//! `~/.config/baras/archive/<filename>.zst`. The history JSON and user notes
//! stay where they are, so archived pulls remain in the dashboard; the area
//! index moves with the file so the Library can still classify it. Restoring
//! decompresses back into the log directory with the original mtime, which
//! keeps the history header current.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use baras_types::history::ArchiveReport;

use super::LibraryMut;

/// Combat logs are repetitive text; this level roughly matches 9 on size at
/// half the time, and an archive is a one-off.
const LEVEL: i32 = 6;
const EXT: &str = "zst";

pub fn archive_dir() -> io::Result<PathBuf> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baras")
        .join("archive");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// One compressed log on disk.
pub struct ArchivedLog {
    /// Original log filename (without `.zst`)
    pub filename: String,
    pub path: PathBuf,
    pub compressed_size: u64,
    /// Uncompressed size from the zstd frame header; None for archives
    /// written without it
    pub original_size: Option<u64>,
}

/// The frame header carries the content size when the encoder pledged it.
fn frame_content_size(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut head = [0u8; 32];
    let n = File::open(path).ok()?.read(&mut head).ok()?;
    zstd::zstd_safe::get_frame_content_size(&head[..n]).ok().flatten()
}

/// Every log in the archive directory, unsorted.
pub fn archived_logs() -> Vec<ArchivedLog> {
    let Ok(dir) = archive_dir() else {
        return Vec::new();
    };
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read.flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some(EXT) {
                return None;
            }
            let filename = path.file_stem()?.to_str()?.to_owned();
            let compressed_size = e.metadata().ok()?.len();
            let original_size = frame_content_size(&path);
            Some(ArchivedLog { filename, path, compressed_size, original_size })
        })
        .collect()
}

/// Compress the given logs into the archive and remove the originals. The
/// live log and favorites are refused. Failures leave the original in place.
pub fn archive(lib: &mut LibraryMut<'_>, paths: &[PathBuf]) -> ArchiveReport {
    let mut report = ArchiveReport::default();
    let Ok(dir) = archive_dir() else {
        report.failed = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        return report;
    };
    for path in paths {
        let Some(filename) = lib.removable(path) else {
            report.failed.push(path.to_string_lossy().into_owned());
            continue;
        };
        let dest = dir.join(format!("{filename}.{EXT}"));
        match compress(path, &dest) {
            Ok((before, after)) => {
                report.archived += 1;
                report.bytes_before += before;
                report.bytes_after += after;
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to archive log");
                report.failed.push(path.to_string_lossy().into_owned());
                continue;
            }
        }
        if let Some(areas) = lib.areas.get(path).cloned() {
            lib.areas.insert(dest, areas);
        }
        lib.forget(path, filename);
    }
    report
}

/// Decompress `filename` from the archive into `log_dir` and drop the archive
/// copy. Refuses to overwrite an existing log.
pub fn restore(lib: &mut LibraryMut<'_>, log_dir: &Path, filename: &str) -> io::Result<PathBuf> {
    let src = archive_dir()?.join(format!("{filename}.{EXT}"));
    let dest = log_dir.join(filename);
    if dest.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "log already in directory"));
    }
    let mtime = fs::metadata(&src)?.modified()?;
    let tmp = dest.with_extension("restoring");
    let result = (|| {
        let mut out = BufWriter::new(File::create(&tmp)?);
        zstd::stream::copy_decode(BufReader::new(File::open(&src)?), &mut out)?;
        let out = out.into_inner().map_err(|e| e.into_error())?;
        out.set_modified(mtime)?;
        fs::rename(&tmp, &dest)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let _ = fs::remove_file(&src);
    if let Some(areas) = lib.areas.get(&src).cloned() {
        lib.areas.remove(&src);
        lib.areas.insert(dest.clone(), areas);
    }
    lib.directory.add_file(&dest);
    Ok(dest)
}

/// Write `src` compressed to `dest` (atomically, via a temp file), then remove
/// `src`. Returns (original, compressed) sizes. The archive keeps the
/// original mtime so `restore` can hand it back.
fn compress(src: &Path, dest: &Path) -> io::Result<(u64, u64)> {
    let meta = fs::metadata(src)?;
    let tmp = dest.with_extension("zst.tmp");
    let result = (|| {
        // Pledging the size puts it in the frame header, so the Library can
        // show what an archive saved without decompressing it
        let mut enc = zstd::stream::Encoder::new(BufWriter::new(File::create(&tmp)?), LEVEL)?;
        enc.set_pledged_src_size(Some(meta.len()))?;
        enc.include_contentsize(true)?;
        io::copy(&mut BufReader::new(File::open(src)?), &mut enc)?;
        let out = enc.finish()?.into_inner().map_err(|e| e.into_error())?;
        out.set_modified(meta.modified()?)?;
        fs::rename(&tmp, dest)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::remove_file(src)?;
    let after = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    Ok((meta.len(), after))
}
