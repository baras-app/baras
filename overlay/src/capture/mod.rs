//! Platform screen capture used by raid-frame detection.
//!
//! Captures include the overlay itself, so callers blank it first.

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as backend;

#[cfg(all(unix, not(target_os = "macos")))]
mod unix;
#[cfg(all(unix, not(target_os = "macos")))]
use unix as backend;

/// Whether this platform has a capture backend at all. macOS has none, so
/// callers can hide detection UI rather than fail at press time.
pub const SUPPORTED: bool = cfg!(any(target_os = "windows", all(unix, not(target_os = "macos"))));

fn log_backend(backend: &str, direct_supported: bool) {
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        tracing::info!(
            target: "baras::capture",
            backend,
            direct_supported,
            "screen capture backend selected"
        );
    });
}

#[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
mod backend {
    use super::{CaptureError, CapturedImage};

    pub fn capture_region(_x: i32, _y: i32, _width: u32, _height: u32) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::Unsupported(
            "raid-frame capture is not available on this platform".into(),
        ))
    }
}

/// Capture cost, on its own target so it can be filtered on or off by itself:
/// `RUST_LOG=baras::capture=info`. Fires once per detect press.
#[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
fn log_timing(
    backend: &str,
    setup: std::time::Duration,
    captured: std::time::Duration,
    cropped: std::time::Duration,
    total: std::time::Duration,
    region: (u32, u32),
) {
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    tracing::info!(
        target: "baras::capture",
        backend,
        setup_ms = ms(setup),
        capture_ms = ms(captured),
        crop_ms = ms(cropped),
        total_ms = ms(total),
        region = format!("{}x{}", region.0, region.1),
        "raid-frame capture"
    );
}

/// A captured region, top-down, 4 bytes per pixel.
///
/// A scaled display returns more pixels than were asked for; downsampling would
/// throw away detail recognition wants. Compare `width` against the requested
/// width to recover the factor.
#[derive(Debug)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    /// RGBA, row-major, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl CapturedImage {
    /// Pixel at `(x, y)` as `(r, g, b, a)`, or `None` when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize).checked_mul(4)?;
        let pixel = self.rgba.get(i..i.checked_add(4)?)?;
        Some((pixel[0], pixel[1], pixel[2], pixel[3]))
    }

    /// Copy a sub-rectangle out of this image, clamped to its bounds.
    ///
    /// Returns `None` when the requested rectangle lies entirely outside.
    pub fn crop(&self, x: i32, y: i32, width: u32, height: u32) -> Option<CapturedImage> {
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)?
            .checked_mul(4)?;
        if self.rgba.len() < expected {
            return None;
        }

        let x0 = i64::from(x).clamp(0, i64::from(self.width)) as u32;
        let y0 = i64::from(y).clamp(0, i64::from(self.height)) as u32;
        let x1 = (i64::from(x) + i64::from(width)).clamp(0, i64::from(self.width)) as u32;
        let y1 = (i64::from(y) + i64::from(height)).clamp(0, i64::from(self.height)) as u32;

        if x0 >= x1 || y0 >= y1 {
            return None;
        }

        let (w, h) = (x1 - x0, y1 - y0);
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for row in y0..y1 {
            let start = (row as usize * self.width as usize + x0 as usize) * 4;
            let end = start + w as usize * 4;
            rgba.extend_from_slice(&self.rgba[start..end]);
        }

        Some(CapturedImage {
            width: w,
            height: h,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_clamps_without_overflowing() {
        let image = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        };

        let crop = image.crop(-1, -1, u32::MAX, u32::MAX).unwrap();
        assert_eq!((crop.width, crop.height), (2, 2));
    }

    #[test]
    fn malformed_image_is_rejected() {
        let image = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![255; 3],
        };

        assert_eq!(image.pixel(0, 0), None);
        assert!(image.crop(0, 0, 1, 1).is_none());
    }

    #[test]
    fn oversized_capture_is_rejected_before_the_backend() {
        assert!(matches!(
            capture_region(0, 0, u32::MAX, 1),
            Err(CaptureError::InvalidRegion(_))
        ));
    }
}

#[derive(Debug)]
pub enum CaptureError {
    /// Display server or graphics API unreachable.
    ConnectionFailed(String),
    /// The platform or compositor does not support region capture.
    Unsupported(String),
    /// Screen capture needs user approval.
    PermissionRequired(String),
    /// Requested region is empty or off-screen.
    InvalidRegion(String),
    /// Capture was attempted but produced nothing usable.
    Failed(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::ConnectionFailed(s) => write!(f, "Capture connection failed: {s}"),
            CaptureError::Unsupported(s) => write!(f, "Capture unsupported: {s}"),
            CaptureError::PermissionRequired(s) => write!(f, "Capture permission required: {s}"),
            CaptureError::InvalidRegion(s) => write!(f, "Invalid capture region: {s}"),
            CaptureError::Failed(s) => write!(f, "Capture failed: {s}"),
        }
    }
}

#[cfg(target_os = "linux")]
pub fn portal_required() -> bool {
    unix::portal_required()
}

#[cfg(not(target_os = "linux"))]
pub fn portal_required() -> bool {
    #[cfg(target_os = "windows")]
    log_backend("windows-gdi", true);
    #[cfg(target_os = "macos")]
    log_backend("none", false);
    false
}

#[cfg(target_os = "linux")]
pub fn has_portal_restore_token() -> bool {
    unix::has_portal_restore_token()
}

#[cfg(not(target_os = "linux"))]
pub fn has_portal_restore_token() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub async fn enable_portal() -> Result<(), CaptureError> {
    unix::enable_portal().await
}

#[cfg(not(target_os = "linux"))]
pub async fn enable_portal() -> Result<(), CaptureError> {
    Err(CaptureError::Unsupported(
        "screen cast portal is only available on Linux".into(),
    ))
}

impl std::error::Error for CaptureError {}

/// Capture a screen region in global desktop coordinates.
///
/// The overlay must be hidden first — see the module docs.
pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<CapturedImage, CaptureError> {
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidRegion(format!("{width}x{height} has no area")));
    }
    if width > i32::MAX as u32
        || height > i32::MAX as u32
        || (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none()
    {
        return Err(CaptureError::InvalidRegion(format!("{width}x{height} is too large")));
    }
    backend::capture_region(x, y, width, height)
}
