//! Text recognition for prepared raid-frame crops.
//!
//! Recognition is told the whole crop is one line, so it can never answer "no
//! text here". Detection narrows the crop first. Both models are downloaded on
//! first use and loaded once per process.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use rten_imageproc::{Rect, RotatedRect};
use sha2::{Digest, Sha256};

use crate::analysis::PreparedCrop;

// Upstream ocrs bucket first; our mirror at baras-app/ocr-models is the
// fallback when the bucket is unreachable or serves a different hash.
const RECOGNITION_MODEL_URLS: &[&str] = &[
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten",
    "https://raw.githubusercontent.com/baras-app/ocr-models/main/text-recognition.rten",
];

const RECOGNITION_MODEL_SHA256: &str = "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e";

const DETECTION_MODEL_URLS: &[&str] = &[
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten",
    "https://raw.githubusercontent.com/baras-app/ocr-models/main/text-detection.rten",
];

const DETECTION_MODEL_SHA256: &str = "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca";

const MIN_MODEL_BYTES: u64 = 1_000_000;

/// Peak text probability a column must reach. Glyphs measure 0.94 and up,
/// border and icons around 0.56. The word boxes are useless here: binarized far
/// lower, so an icon touching the last letter joins its box.
const TEXT_COLUMN_CONFIDENCE: f32 = 0.65;

/// Columns kept either side of the detected text, for antialiased edges.
const SPAN_PAD: u32 = 2;

/// The detection model's input, which it pads up to and shrinks down to. A
/// batch stays inside it so nothing is scaled.
const CANVAS_ROWS: u32 = 800;
const CANVAS_COLS: u32 = 600;

/// Black rows between packed crops, so one crop's mask cannot reach the next.
const BATCH_GAP: u32 = 8;

#[derive(Debug, Clone)]
pub enum OcrError {
    ModelUnavailable(String),
    Recognition(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrError::ModelUnavailable(s) => write!(f, "OCR model unavailable: {s}"),
            OcrError::Recognition(s) => write!(f, "Recognition failed: {s}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// Where downloaded models are cached.
fn models_dir() -> Option<PathBuf> {
    baras_core::paths::config_dir().map(|p| p.join("models"))
}

/// Where the downloaded recognition model is cached.
pub fn model_path() -> Option<PathBuf> {
    models_dir().map(|d| d.join("text-recognition.rten"))
}

/// Where the downloaded detection model is cached.
pub fn detection_model_path() -> Option<PathBuf> {
    models_dir().map(|d| d.join("text-detection.rten"))
}

/// Whether both models have already been downloaded.
pub fn model_is_present() -> bool {
    [model_path(), detection_model_path()]
        .iter()
        .all(|p| p.as_ref().is_some_and(|p| p.exists()))
}

/// Download whichever models are not already cached.
pub async fn ensure_model() -> Result<(), OcrError> {
    let recognition = model_path().ok_or_else(|| OcrError::ModelUnavailable("no config directory".into()))?;
    let detection = detection_model_path().ok_or_else(|| OcrError::ModelUnavailable("no config directory".into()))?;

    ensure_one(
        &recognition,
        "recognition",
        RECOGNITION_MODEL_URLS,
        RECOGNITION_MODEL_SHA256,
    )
    .await?;
    ensure_one(&detection, "detection", DETECTION_MODEL_URLS, DETECTION_MODEL_SHA256).await
}

/// Download one model if the cached copy is missing or unreadable.
async fn ensure_one(path: &std::path::Path, kind: &str, urls: &[&str], sha256: &str) -> Result<(), OcrError> {
    // A cached file of plausible size is trusted: it was hash-checked before it
    // was written, and `engine()` reports one that fails to load.
    if let Ok(metadata) = std::fs::metadata(path)
        && metadata.len() >= MIN_MODEL_BYTES
    {
        return Ok(());
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| OcrError::ModelUnavailable(format!("cannot replace {path:?}: {e}")))?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| OcrError::ModelUnavailable(format!("cannot create {parent:?}: {e}")))?;
    }

    tracing::info!("Downloading OCR {kind} model to {path:?}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| OcrError::ModelUnavailable(format!("cannot build download client: {e}")))?;

    let mut bytes = None;
    let mut failures = Vec::new();
    for url in urls {
        match fetch_model(&client, url, sha256).await {
            Ok(fetched) => {
                bytes = Some(fetched);
                break;
            }
            Err(e) => {
                // Log failed mirrors even when a fallback works.
                tracing::warn!("OCR {kind} model not available from {url}: {e}");
                failures.push(format!("{url}: {e}"));
            }
        }
    }
    let Some(bytes) = bytes else {
        return Err(OcrError::ModelUnavailable(format!(
            "every source failed ({})",
            failures.join("; ")
        )));
    };

    // Write beside the target and rename, so an interrupted download cannot
    // leave a truncated file where a model is expected.
    let temp = path.with_extension("part");
    std::fs::write(&temp, &bytes).map_err(|e| OcrError::ModelUnavailable(format!("cannot write model: {e}")))?;
    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(OcrError::ModelUnavailable(format!("cannot finalize model: {e}")));
    }

    Ok(())
}

async fn fetch_model(client: &reqwest::Client, url: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let bytes = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    if bytes.len() < MIN_MODEL_BYTES as usize {
        return Err(format!("download was only {} bytes", bytes.len()));
    }

    let digest = hex(&Sha256::digest(&bytes));
    if digest != sha256 {
        return Err(format!("served a different model (sha256 {digest})"));
    }

    Ok(bytes.to_vec())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Process-wide engine, built once on first use.
static ENGINE: OnceLock<Mutex<Option<Arc<OcrEngine>>>> = OnceLock::new();

/// Load a cached model, naming it when it is missing.
fn load(path: Option<PathBuf>, kind: &str) -> Result<Model, OcrError> {
    let path = path.ok_or_else(|| OcrError::ModelUnavailable("no config directory".into()))?;
    if !path.exists() {
        return Err(OcrError::ModelUnavailable(format!(
            "{kind} model has not been downloaded"
        )));
    }
    Model::load_file(&path).map_err(|e| OcrError::ModelUnavailable(format!("cannot load {path:?}: {e}")))
}

fn engine() -> Result<Arc<OcrEngine>, OcrError> {
    let cell = ENGINE.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap_or_else(|p| p.into_inner());

    if let Some(existing) = guard.as_ref() {
        return Ok(existing.clone());
    }

    let recognition = load(model_path(), "recognition")?;
    let detection = load(detection_model_path(), "detection")?;

    let built = OcrEngine::new(OcrEngineParams {
        // Bands come from our own detection; this only narrows them.
        detection_model: Some(detection),
        recognition_model: Some(recognition),
        ..Default::default()
    })
    .map_err(|e| OcrError::ModelUnavailable(format!("cannot build engine: {e}")))?;

    let built = Arc::new(built);
    *guard = Some(built.clone());
    Ok(built)
}

/// Load the engine now, on this thread.
///
/// This holds the engine lock.
/// RTen loads on its own thread pool.
pub fn warm() -> Result<(), OcrError> {
    engine().map(|_| ())
}

/// Recognize the text in many prepared crops at once.
///
/// Each crop is treated as a single text line covering the whole crop, since
/// band detection already isolated it. The crops are stacked onto one canvas
/// and handed to recognition together: ocrs groups lines of similar width into
/// one model run, where reading them one at a time ran the model once per crop
/// and held one set of activations per crop in flight.
///
/// Readings line up with `crops`; a crop with nothing legible reads as `""`.
pub fn recognize_many(crops: &[&PreparedCrop]) -> Result<Vec<String>, OcrError> {
    if crops.is_empty() {
        return Ok(Vec::new());
    }
    let engine = engine()?;
    let (canvas, tops) = pack(crops);
    let lines: Vec<Vec<RotatedRect>> = crops
        .iter()
        .zip(&tops)
        .map(|(crop, &top)| {
            vec![RotatedRect::from_rect(Rect::from_tlhw(
                top as f32,
                0.0,
                crop.height as f32,
                crop.width as f32,
            ))]
        })
        .collect();

    let rgb = canvas.to_rgb();
    let source = ImageSource::from_bytes(&rgb, (canvas.width, canvas.height))
        .map_err(|e| OcrError::Recognition(format!("bad image source: {e}")))?;
    let input = engine
        .prepare_input(source)
        .map_err(|e| OcrError::Recognition(format!("prepare_input failed: {e}")))?;
    let recognized = engine
        .recognize_text(&input, &lines)
        .map_err(|e| OcrError::Recognition(format!("recognize_text failed: {e}")))?;

    Ok(recognized
        .into_iter()
        .map(|line| line.map_or_else(String::new, |l| l.to_string().trim().to_string()))
        .collect())
}

/// Stack crops top to bottom on one black canvas, `BATCH_GAP` rows apart.
///
/// Returns the canvas and each crop's top row on it. Unused canvas stays
/// black, which both models read as "nothing here".
fn pack(crops: &[&PreparedCrop]) -> (PreparedCrop, Vec<u32>) {
    let width = crops.iter().map(|c| c.width).max().unwrap_or(0);
    let height = crops.iter().map(|c| c.height).sum::<u32>() + BATCH_GAP * crops.len().saturating_sub(1) as u32;
    let mut gray = vec![0u8; (width * height) as usize];
    let mut tops = Vec::with_capacity(crops.len());
    let mut top = 0u32;
    for crop in crops {
        for row in 0..crop.height {
            let from = (row * crop.width) as usize;
            let to = ((top + row) * width) as usize;
            gray[to..to + crop.width as usize].copy_from_slice(&crop.gray[from..from + crop.width as usize]);
        }
        tops.push(top);
        top += crop.height + BATCH_GAP;
    }
    (PreparedCrop { width, height, gray }, tops)
}

/// Columns the detection model is confident hold text, for many crops.
///
/// `Some(None)` means no text at all: an empty frame, which recognition would
/// answer with a stray letter. The outer `None` is a crop we could not judge;
/// callers keep it whole rather than lose the slot.
///
/// Detection runs the full network on a fixed canvas whatever the input, so
/// crops are packed onto one canvas per inference instead of one each.
pub fn text_spans(crops: &[&PreparedCrop]) -> Vec<Option<Option<Span>>> {
    let mut out = vec![None; crops.len()];
    let mut batch: Vec<usize> = Vec::new();
    let (mut rows, mut cols) = (0u32, 0u32);

    for (i, crop) in crops.iter().enumerate() {
        // Too big to pack: ocrs shrinks it to the canvas on its own.
        if crop.width > CANVAS_COLS || crop.height > CANVAS_ROWS {
            out[i] = text_span(crop).ok();
            continue;
        }
        let gap = if batch.is_empty() { 0 } else { BATCH_GAP };
        if rows + gap + crop.height > CANVAS_ROWS || cols.max(crop.width) > CANVAS_COLS {
            detect_batch(crops, &batch, &mut out);
            batch.clear();
            (rows, cols) = (0, 0);
        }
        rows += if batch.is_empty() { 0 } else { BATCH_GAP } + crop.height;
        cols = cols.max(crop.width);
        batch.push(i);
    }
    detect_batch(crops, &batch, &mut out);
    out
}

/// One crop's span, on its own canvas.
pub fn text_span(crop: &PreparedCrop) -> Result<Option<Span>, OcrError> {
    let engine = engine()?;
    let spans = detect(&engine, crop, &[(0, crop.width, crop.height)])?;
    Ok(spans.into_iter().next().flatten())
}

/// Stack a batch onto one canvas, detect once, then read each crop's rows back.
fn detect_batch(crops: &[&PreparedCrop], batch: &[usize], out: &mut [Option<Option<Span>>]) {
    let (Some(&first), Ok(engine)) = (batch.first(), engine()) else {
        return;
    };
    if batch.len() == 1 {
        out[first] = text_span(crops[first]).ok();
        return;
    }

    let members: Vec<&PreparedCrop> = batch.iter().map(|&i| crops[i]).collect();
    let (packed, tops) = pack(&members);
    if packed.width == 0 || packed.height == 0 {
        return;
    }
    let regions: Vec<Region> = members
        .iter()
        .zip(&tops)
        .map(|(crop, &top)| (top, crop.width, crop.height))
        .collect();
    let Ok(spans) = detect(&engine, &packed, &regions) else {
        return;
    };
    for (&i, span) in batch.iter().zip(spans) {
        out[i] = Some(span);
    }
}

/// Detect once, then read a span out of each region of the mask.
///
/// The mask stays in here: naming its type would mean depending on
/// `rten-tensor` directly, and every caller wants spans anyway.
fn detect(engine: &OcrEngine, crop: &PreparedCrop, regions: &[Region]) -> Result<Vec<Option<Span>>, OcrError> {
    let rgb = crop.to_rgb();
    let source = ImageSource::from_bytes(&rgb, (crop.width, crop.height))
        .map_err(|e| OcrError::Recognition(format!("bad image source: {e}")))?;
    let input = engine
        .prepare_input(source)
        .map_err(|e| OcrError::Recognition(format!("prepare_input failed: {e}")))?;
    let mask = engine
        .detect_text_pixels(&input)
        .map_err(|e| OcrError::Recognition(format!("detect_text_pixels failed: {e}")))?;

    Ok(regions
        .iter()
        .map(|&(top, width, height)| {
            // Peak, not mean: a glyph fills only part of a column's height.
            let lit: Vec<bool> = (0..width as usize)
                .map(|x| {
                    (top as usize..(top + height) as usize)
                        .map(|y| mask[[y, x]])
                        .fold(0.0f32, f32::max)
                        >= TEXT_COLUMN_CONFIDENCE
                })
                .collect();
            span_from_lit(&lit, width, height)
        })
        .collect())
}

/// The widest run of text columns, bridging word spaces.
fn span_from_lit(lit: &[bool], width: u32, height: u32) -> Option<Span> {
    // Word spaces read as unlit. Bridge them; anything wider is the icon gap.
    // Measured at the crop's fixed height.
    let max_gap = (height.max(1) / 2) as usize;
    let mut best: Option<Run> = None;
    let mut current: Option<Run> = None;
    let mut gap = 0usize;

    for (x, &on) in lit.iter().enumerate() {
        match (on, current) {
            (true, Some((start, _))) => current = Some((start, x)),
            (true, None) => current = Some((x, x)),
            (false, Some((start, end))) => {
                gap += 1;
                if gap > max_gap {
                    best = wider(best, Some((start, end)));
                    current = None;
                    gap = 0;
                }
            }
            (false, None) => {}
        }
        if on {
            gap = 0;
        }
    }

    wider(best, current).map(|(start, end)| {
        let left = (start as u32).saturating_sub(SPAN_PAD);
        let right = (end as u32 + 1 + SPAN_PAD).min(width);
        (left, right)
    })
}

/// Left and right column of a crop's text.
pub type Span = (u32, u32);

/// One crop's place on a packed canvas: `(top, width, height)`.
type Region = (u32, u32, u32);

/// Inclusive first and last lit column of a run.
type Run = (usize, usize);

/// The longer of two runs.
fn wider(a: Option<Run>, b: Option<Run>) -> Option<Run> {
    match (a, b) {
        (Some(a), Some(b)) if b.1 - b.0 > a.1 - a.0 => Some(b),
        (Some(a), _) => Some(a),
        (None, b) => b,
    }
}
