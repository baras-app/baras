//! Linux capture: Wayland where the session offers it, X11 otherwise.

#[cfg(target_os = "linux")]
mod portal;
mod wayland;
mod x11;

use super::{CaptureError, CapturedImage, log_timing};

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<CapturedImage, CaptureError> {
    // Keyed on the session, not on the overlay backend. On GNOME the overlay is
    // an Xwayland window, but only the compositor can read the screen, because
    // an X11 GetImage cannot see native Wayland surfaces.
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        match wayland::capture_region(x, y, width, height) {
            Ok(image) => Ok(image),
            #[cfg(target_os = "linux")]
            Err(CaptureError::Unsupported(_)) => portal::capture_region(x, y, width, height),
            Err(error) => Err(error),
        }
    } else {
        x11::capture_region(x, y, width, height)
    }
}

#[cfg(target_os = "linux")]
pub fn portal_required() -> bool {
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        super::log_backend("x11-get-image", true);
        return false;
    }
    let direct_supported = wayland::supported();
    super::log_backend(
        if direct_supported {
            "wayland-ext-image-copy"
        } else {
            "pipewire-portal"
        },
        direct_supported,
    );
    !direct_supported
}

#[cfg(target_os = "linux")]
pub fn has_portal_restore_token() -> bool {
    portal::has_restore_token()
}

#[cfg(target_os = "linux")]
pub async fn enable_portal() -> Result<(), CaptureError> {
    portal::enable().await
}

/// Map a global logical rectangle onto an output's capture buffer.
///
/// Buffers are device pixels, overlay geometry is logical. The factor comes from
/// the sizes the compositor reported — `wl_output`'s own scale is an integer and
/// wrong under fractional scaling.
///
/// Clamped to the buffer. `None` when the region lies outside it.
pub(crate) fn device_region(
    region: (i32, i32, u32, u32),
    output_origin: (i32, i32),
    logical_size: (u32, u32),
    buffer_size: (u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let (logical_w, logical_h) = logical_size;
    let (buffer_w, buffer_h) = buffer_size;
    if logical_w == 0 || logical_h == 0 || buffer_w == 0 || buffer_h == 0 {
        return None;
    }

    let scale_x = f64::from(buffer_w) / f64::from(logical_w);
    let scale_y = f64::from(buffer_h) / f64::from(logical_h);

    let left = (i64::from(region.0) - i64::from(output_origin.0)) as f64 * scale_x;
    let top = (i64::from(region.1) - i64::from(output_origin.1)) as f64 * scale_y;
    let right = left + f64::from(region.2) * scale_x;
    let bottom = top + f64::from(region.3) * scale_y;

    let x0 = left.round().clamp(0.0, f64::from(buffer_w)) as u32;
    let y0 = top.round().clamp(0.0, f64::from(buffer_h)) as u32;
    let x1 = right.round().clamp(0.0, f64::from(buffer_w)) as u32;
    let y1 = bottom.round().clamp(0.0, f64::from(buffer_h)) as u32;

    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scaling works on an unscaled machine and reads the wrong pixels
    /// everywhere else, so it is tested rather than left to a compositor.
    #[test]
    fn device_region_scales_the_rectangle() {
        // 1:1 — the region is already in device pixels.
        assert_eq!(
            device_region((100, 50, 400, 200), (0, 0), (1920, 1080), (1920, 1080)),
            Some((100, 50, 400, 200))
        );

        // 2x HiDPI.
        assert_eq!(
            device_region((100, 50, 400, 200), (0, 0), (1920, 1080), (3840, 2160)),
            Some((200, 100, 800, 400))
        );

        // 1.5x and 1.25x — what wl_output's integer scale gets wrong.
        assert_eq!(
            device_region((100, 50, 400, 200), (0, 0), (1280, 720), (1920, 1080)),
            Some((150, 75, 600, 300))
        );
        assert_eq!(
            device_region((100, 40, 400, 200), (0, 0), (1536, 864), (1920, 1080)),
            Some((125, 50, 500, 250))
        );
    }

    #[test]
    fn device_region_is_relative_to_its_output() {
        // Second monitor starting at x=1920, captured at 2x.
        assert_eq!(
            device_region((2020, 50, 400, 200), (1920, 0), (1920, 1080), (3840, 2160)),
            Some((200, 100, 800, 400))
        );
    }

    #[test]
    fn device_region_clamps_and_rejects_the_unusable() {
        // Overhanging rectangles keep the part on the output.
        assert_eq!(
            device_region((-50, -50, 200, 200), (0, 0), (1920, 1080), (1920, 1080)),
            Some((0, 0, 150, 150))
        );
        assert_eq!(
            device_region((4000, 0, 100, 100), (0, 0), (1920, 1080), (1920, 1080)),
            None
        );
        // A compositor that reported nothing usable.
        assert_eq!(device_region((0, 0, 100, 100), (0, 0), (0, 0), (1920, 1080)), None);
    }
}
