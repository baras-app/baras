//! X11 capture via `GetImage` on the root window.
//!
//! The server crops for us, so there is no separate crop pass. Under XWayland
//! this cannot see native Wayland surfaces, hence Wayland is tried first.

use std::time::Instant;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, ImageOrder};

use super::{CaptureError, CapturedImage};

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<CapturedImage, CaptureError> {
    let started = Instant::now();

    let bounds = |what: &str| CaptureError::InvalidRegion(format!("{what} is out of X11 range"));
    let (x, y) = (
        i16::try_from(x).map_err(|_| bounds("x"))?,
        i16::try_from(y).map_err(|_| bounds("y"))?,
    );
    let (w, h) = (
        u16::try_from(width).map_err(|_| bounds("width"))?,
        u16::try_from(height).map_err(|_| bounds("height"))?,
    );

    let (conn, screen) =
        x11rb::connect(None).map_err(|e| CaptureError::ConnectionFailed(format!("no X display: {e}")))?;
    let setup = conn.setup();
    let root = setup.roots[screen].root;

    let failed = |e: &dyn std::fmt::Display| CaptureError::Failed(format!("GetImage failed: {e}"));
    let reply = conn
        .get_image(ImageFormat::Z_PIXMAP, root, x, y, w, h, !0)
        .map_err(|e| failed(&e))?
        .reply()
        .map_err(|e| failed(&e))?;
    let captured = started.elapsed();

    let bits = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == reply.depth)
        .map(|format| format.bits_per_pixel)
        .ok_or_else(|| CaptureError::Failed(format!("no format for depth {}", reply.depth)))?;
    if bits != 32 {
        return Err(CaptureError::Unsupported(format!(
            "{bits}-bit displays are not supported"
        )));
    }

    let expected = width as usize * height as usize * 4;
    if reply.data.len() < expected {
        return Err(CaptureError::Failed("GetImage returned short data".into()));
    }

    let converted = Instant::now();
    // Z_PIXMAP is B,G,R,X little-endian and X,R,G,B the other way round.
    let lsb = setup.image_byte_order == ImageOrder::LSB_FIRST;
    let mut rgba = Vec::with_capacity(expected);
    for px in reply.data[..expected].chunks_exact(4) {
        let (r, g, b) = if lsb {
            (px[2], px[1], px[0])
        } else {
            (px[1], px[2], px[3])
        };
        rgba.extend_from_slice(&[r, g, b, 255]);
    }

    super::log_timing(
        "x11-get-image",
        std::time::Duration::ZERO,
        captured,
        converted.elapsed(),
        started.elapsed(),
        (width, height),
    );

    Ok(CapturedImage { width, height, rgba })
}
