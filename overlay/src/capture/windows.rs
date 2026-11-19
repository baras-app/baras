//! Windows capture via GDI `BitBlt`.
//!
//! Exclusive fullscreen may return a black image; windowed and borderless modes
//! use the composited desktop and work normally.

use std::time::Instant;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, HBITMAP, HDC, ReleaseDC, SRCCOPY, SelectObject,
};

use super::{CaptureError, CapturedImage};

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<CapturedImage, CaptureError> {
    let started = Instant::now();

    let screen_dc = unsafe { GetDC(None) };
    if screen_dc.is_invalid() {
        return Err(CaptureError::ConnectionFailed("GetDC(None) failed".into()));
    }

    let result = capture_with_dc(screen_dc, started, x, y, width, height);
    unsafe { ReleaseDC(None, screen_dc) };
    result
}

fn capture_with_dc(
    screen_dc: HDC,
    started: Instant,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CapturedImage, CaptureError> {
    let mem_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
    if mem_dc.is_invalid() {
        return Err(CaptureError::Failed("CreateCompatibleDC failed".into()));
    }

    // Negative height requests a top-down DIB, matching CapturedImage's layout.
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let bitmap: HBITMAP = match unsafe { CreateDIBSection(Some(mem_dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) } {
        Ok(b) => b,
        Err(e) => {
            unsafe {
                let _ = DeleteDC(mem_dc);
            };
            return Err(CaptureError::Failed(format!("CreateDIBSection failed: {e}")));
        }
    };

    if bits.is_null() {
        unsafe {
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(mem_dc);
        }
        return Err(CaptureError::Failed("DIB section has no backing memory".into()));
    }

    let old = unsafe { SelectObject(mem_dc, bitmap.into()) };

    let setup = started.elapsed();
    let blit_started = Instant::now();
    let blit_ok = unsafe {
        BitBlt(
            mem_dc,
            0,
            0,
            width as i32,
            height as i32,
            Some(screen_dc),
            x,
            y,
            SRCCOPY,
        )
    }
    .is_ok();

    let captured = blit_started.elapsed();

    let out = if blit_ok {
        let crop_started = Instant::now();
        let byte_len = (width as usize) * (height as usize) * 4;
        // SAFETY: CreateDIBSection allocated exactly this many bytes for a
        // 32-bit top-down bitmap of these dimensions, and the bitmap is still
        // selected into mem_dc so the memory is live.
        let src = unsafe { std::slice::from_raw_parts(bits as *const u8, byte_len) };

        // GDI hands back BGRA with an unused alpha byte; normalize to opaque RGBA.
        let mut rgba = vec![0u8; byte_len];
        for (dst, px) in rgba.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            dst[0] = px[2];
            dst[1] = px[1];
            dst[2] = px[0];
            dst[3] = 255;
        }

        if rgba.chunks_exact(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0) {
            Err(CaptureError::Failed(
                "captured region was entirely black — the game may be in exclusive fullscreen".into(),
            ))
        } else {
            super::log_timing(
                "windows-gdi",
                setup,
                captured,
                crop_started.elapsed(),
                started.elapsed(),
                (width, height),
            );
            Ok(CapturedImage { width, height, rgba })
        }
    } else {
        Err(CaptureError::Failed("BitBlt failed".into()))
    };

    unsafe {
        SelectObject(mem_dc, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem_dc);
    }

    out
}
