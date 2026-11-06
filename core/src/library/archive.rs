//! Cold storage for logs: zstd-compressed copies under
//! `~/.config/baras/archive/<filename>.zst`. The history JSON and user notes
//! stay where they are, so archived pulls remain in the dashboard; the area
//! index moves with the file so the Library can still classify it. The
//! archive keeps the original mtime and pledges the original length in the
//! frame header, so the history header written from the raw log still
//! describes the archive and a backfill can tell current from stale without
//! decompressing. Restoring hands the log back with the same mtime.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

use baras_types::history::ArchiveReport;

use super::LibraryMut;
use crate::history::{HistoryHeader, mtime_secs};

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
    /// Unix seconds; the original log's mtime, carried over by `compress`
    pub mtime: i64,
}

impl ArchivedLog {
    fn from_path(path: PathBuf) -> Option<Self> {
        if path.extension().and_then(|x| x.to_str()) != Some(EXT) {
            return None;
        }
        let filename = path.file_stem()?.to_str()?.to_owned();
        let meta = fs::metadata(&path).ok()?;
        let original_size = frame_content_size(&path);
        Some(ArchivedLog { filename, compressed_size: meta.len(), original_size, mtime: mtime_secs(&meta), path })
    }

    /// True when `header` was written from the log this archive holds: same
    /// store version, length and mtime as an in-directory check.
    pub fn history_current(&self, header: &HistoryHeader) -> bool {
        self.original_size.is_some_and(|len| header.matches(len, self.mtime))
    }

    /// Decompress into `dir/<filename>` with the archive's mtime, so a history
    /// written from the copy carries the same header as one from the original.
    pub fn extract(&self, dir: &Path) -> io::Result<PathBuf> {
        let dest = dir.join(&self.filename);
        decompress(&self.path, &dest)?;
        Ok(dest)
    }
}

/// The archive of `filename`, if one exists.
pub fn archived_log(filename: &str) -> Option<ArchivedLog> {
    ArchivedLog::from_path(archive_dir().ok()?.join(format!("{filename}.{EXT}")))
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
    read.flatten().filter_map(|e| ArchivedLog::from_path(e.path())).collect()
}

/// A log compressed into the archive whose original is still in place.
/// Compression is the slow part and needs no index, so it runs without the
/// library locks; `commit` then swaps the original out under them.
pub struct Compressed {
    pub src: PathBuf,
    dest: PathBuf,
    before: u64,
    after: u64,
}

/// Write `path`'s compressed copy into the archive, leaving `path` as it is.
pub fn compress_log(path: &Path) -> io::Result<Compressed> {
    let filename = path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log has no filename"))?;
    let dest = archive_dir()?.join(format!("{filename}.{EXT}"));
    let (before, after) = compress(path, &dest)?;
    Ok(Compressed { src: path.to_path_buf(), dest, before, after })
}

/// Remove the original of a compressed log and move its index entries to
/// the archive. The live log and favorites are refused: the copy is dropped
/// and the original kept.
pub fn commit(lib: &mut LibraryMut<'_>, c: Compressed, report: &mut ArchiveReport) {
    let Some(filename) = lib.removable(&c.src) else {
        let _ = fs::remove_file(&c.dest);
        report.failed.push(c.src.to_string_lossy().into_owned());
        return;
    };
    if let Err(e) = fs::remove_file(&c.src) {
        tracing::warn!(path = %c.src.display(), error = %e, "Failed to remove archived log");
        let _ = fs::remove_file(&c.dest);
        report.failed.push(c.src.to_string_lossy().into_owned());
        return;
    }
    report.archived += 1;
    report.bytes_before += c.before;
    report.bytes_after += c.after;
    if let Some(areas) = lib.areas.get(&c.src).cloned() {
        lib.areas.insert(c.dest, areas);
    }
    lib.forget(&c.src, filename);
}

/// Decompress `filename` from the archive into `log_dir` and drop the archive
/// copy. Refuses to overwrite an existing log.
pub fn restore(lib: &mut LibraryMut<'_>, log_dir: &Path, filename: &str) -> io::Result<PathBuf> {
    let src = archive_dir()?.join(format!("{filename}.{EXT}"));
    let dest = log_dir.join(filename);
    if dest.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "log already in directory"));
    }
    decompress(&src, &dest)?;
    let _ = fs::remove_file(&src);
    if let Some(areas) = lib.areas.get(&src).cloned() {
        lib.areas.remove(&src);
        lib.areas.insert(dest.clone(), areas);
    }
    lib.directory.add_file(&dest);
    Ok(dest)
}

/// Inflate `src` to `dest` atomically, keeping the archive's mtime.
fn decompress(src: &Path, dest: &Path) -> io::Result<()> {
    let mtime = fs::metadata(src)?.modified()?;
    let tmp = dest.with_extension("restoring");
    let result = (|| {
        let mut out = BufWriter::new(File::create(&tmp)?);
        zstd::stream::copy_decode(BufReader::new(File::open(src)?), &mut out)?;
        let out = out.into_inner().map_err(|e| e.into_error())?;
        out.set_modified(mtime)?;
        fs::rename(&tmp, dest)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Write `src` compressed to `dest` (atomically, via a temp file). Returns
/// (original, compressed) sizes. The archive keeps the original mtime so
/// `restore` can hand it back.
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
    let after = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    Ok((meta.len(), after))
}
