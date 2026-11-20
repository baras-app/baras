//! Wayland capture via `ext-image-copy-capture-v1`.
//!
//! The source is the output the region sits on, never the game window:
//! `ext_foreign_toplevel_handle_v1` carries no geometry, so a window source
//! could not be placed against the overlay's global rectangle.

use std::ffi::c_void;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use rustix::fs::{MemfdFlags, memfd_create};
use rustix::mm::{MapFlags, ProtFlags, mmap, munmap};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_output::{Transform, WlOutput};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{self, ExtImageCopyCaptureManagerV1},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::ZxdgOutputManagerV1,
    zxdg_output_v1::{self, ZxdgOutputV1},
};

use super::{CaptureError, CapturedImage, device_region};

/// A first frame is never delayed, so this only trips on a compositor that
/// stopped answering. Without it the overlay thread would hang for good.
const TIMEOUT: Duration = Duration::from_secs(5);

pub fn supported() -> bool {
    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, _)) = registry_queue_init::<Capture>(&conn) else {
        return false;
    };
    let required = [
        WlShm::interface().name,
        ZxdgOutputManagerV1::interface().name,
        ExtOutputImageCaptureSourceManagerV1::interface().name,
        ExtImageCopyCaptureManagerV1::interface().name,
    ];
    globals.contents().with_list(|list| {
        required
            .iter()
            .all(|name| list.iter().any(|global| global.interface == *name))
    })
}

pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<CapturedImage, CaptureError> {
    let started = Instant::now();
    let deadline = started + TIMEOUT;

    let conn =
        Connection::connect_to_env().map_err(|e| CaptureError::ConnectionFailed(format!("no Wayland display: {e}")))?;
    let (globals, mut queue) = registry_queue_init::<Capture>(&conn)
        .map_err(|e| CaptureError::ConnectionFailed(format!("registry init failed: {e}")))?;
    let qh = queue.handle();

    let missing = |what: &str| CaptureError::Unsupported(format!("compositor does not implement {what}"));
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).map_err(|_| missing("wl_shm"))?;
    let xdg_outputs: ZxdgOutputManagerV1 = globals.bind(&qh, 1..=3, ()).map_err(|_| missing("xdg-output"))?;
    let sources: ExtOutputImageCaptureSourceManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| missing("ext-image-capture-source-v1"))?;
    let capture: ExtImageCopyCaptureManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|_| missing("ext-image-copy-capture-v1"))?;

    // xdg-output reports logical geometry; wl_output only knows device pixels.
    let mut state = Capture::default();
    globals.contents().with_list(|list| {
        for global in list.iter().filter(|g| g.interface == WlOutput::interface().name) {
            let output = globals
                .registry()
                .bind::<WlOutput, _, _>(global.name, global.version.min(4), &qh, ());
            state.outputs.push(Output::new(output));
        }
    });
    for (index, output) in state.outputs.iter().enumerate() {
        xdg_outputs.get_xdg_output(&output.output, &qh, index);
    }
    queue
        .roundtrip(&mut state)
        .map_err(|e| CaptureError::ConnectionFailed(format!("output query failed: {e}")))?;

    let region = (x, y, width, height);
    let outside = || CaptureError::InvalidRegion(format!("{width}x{height} at ({x},{y}) is not on any output"));
    let picked = state
        .outputs
        .iter()
        .enumerate()
        .max_by_key(|(_, output)| output.overlap(region))
        .filter(|(_, output)| output.overlap(region) > 0)
        .map(|(index, _)| index)
        .ok_or_else(outside)?;
    let (origin, logical) = (state.outputs[picked].origin, state.outputs[picked].logical);

    let source = sources.create_source(&state.outputs[picked].output, &qh, ());
    let session = capture.create_session(&source, ext_image_copy_capture_manager_v1::Options::empty(), &qh, ());

    let setup = started.elapsed();
    let copy_started = Instant::now();

    while !state.constraints_done {
        pump(&mut queue, &mut state, deadline)?;
    }
    let (buffer_width, buffer_height) = state
        .buffer_size
        .ok_or_else(|| CaptureError::Failed("compositor advertised no buffer size".into()))?;
    let format = state
        .format
        .ok_or_else(|| CaptureError::Unsupported("compositor offered no readable shm format".into()))?;

    // Fail before the round trip if the region cannot land on the buffer.
    let (crop_x, crop_y, crop_width, crop_height) =
        device_region(region, origin, logical, (buffer_width, buffer_height)).ok_or_else(outside)?;

    let len = (buffer_width as usize) * (buffer_height as usize) * 4;
    let fd = memfd_create(c"baras-capture", MemfdFlags::CLOEXEC)
        .map_err(|e| CaptureError::Failed(format!("memfd_create failed: {e}")))?;
    rustix::fs::ftruncate(&fd, len as u64)
        .map_err(|e| CaptureError::Failed(format!("could not size capture buffer: {e}")))?;
    let mapping = Mapping {
        ptr: unsafe {
            mmap(
                std::ptr::null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                fd.as_fd(),
                0,
            )
            .map_err(|e| CaptureError::Failed(format!("could not map capture buffer: {e}")))?
        },
        len,
    };

    let pool = shm.create_pool(fd.as_fd(), len as i32, &qh, ());
    let buffer = pool.create_buffer(
        0,
        buffer_width as i32,
        buffer_height as i32,
        (buffer_width * 4) as i32,
        format,
        &qh,
        (),
    );

    let frame = session.create_frame(&qh, ());
    frame.attach_buffer(&buffer);
    frame.damage_buffer(0, 0, buffer_width as i32, buffer_height as i32);
    frame.capture();

    while state.outcome.is_none() {
        pump(&mut queue, &mut state, deadline)?;
    }
    if let Some(Err(reason)) = state.outcome.as_ref() {
        return Err(CaptureError::Failed(reason.clone()));
    }
    // A rotated output swaps the axes the crop was computed in.
    if !matches!(state.transform, None | Some(WEnum::Value(Transform::Normal))) {
        return Err(CaptureError::Unsupported(
            "rotated or flipped outputs are not supported".into(),
        ));
    }
    let captured = copy_started.elapsed();

    let crop_started = Instant::now();
    // SAFETY: `ready` means the compositor has finished writing, and the mapping
    // covers exactly `buffer_width * buffer_height * 4` bytes.
    let src = unsafe { std::slice::from_raw_parts(mapping.ptr as *const u8, mapping.len) };
    // Crop on the way out, so only the overlay's rows are ever touched.
    let mut rgba = Vec::with_capacity(crop_width as usize * crop_height as usize * 4);
    let swap = matches!(format, Format::Xrgb8888 | Format::Argb8888);
    for row in crop_y..crop_y + crop_height {
        let start = (row as usize * buffer_width as usize + crop_x as usize) * 4;
        for px in src[start..start + crop_width as usize * 4].as_chunks::<4>().0 {
            // Xrgb/Argb arrive as B,G,R,A; the bgr variants are already in order.
            let (r, b) = if swap { (px[2], px[0]) } else { (px[0], px[2]) };
            rgba.extend_from_slice(&[r, px[1], b, 255]);
        }
    }

    super::log_timing(
        "wayland-ext-image-copy",
        setup,
        captured,
        crop_started.elapsed(),
        started.elapsed(),
        (crop_width, crop_height),
    );

    Ok(CapturedImage {
        width: crop_width,
        height: crop_height,
        rgba,
    })
}

fn pump(queue: &mut EventQueue<Capture>, state: &mut Capture, deadline: Instant) -> Result<(), CaptureError> {
    if Instant::now() >= deadline {
        return Err(CaptureError::Failed(
            "compositor did not answer the capture request in time".into(),
        ));
    }
    queue
        .blocking_dispatch(state)
        .map(|_| ())
        .map_err(|e| CaptureError::Failed(format!("Wayland dispatch failed: {e}")))
}

/// Unmaps however the function exits.
struct Mapping {
    ptr: *mut c_void,
    len: usize,
}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.ptr, self.len);
        }
    }
}

struct Output {
    output: WlOutput,
    origin: (i32, i32),
    logical: (u32, u32),
}

impl Output {
    fn new(output: WlOutput) -> Self {
        Self {
            output,
            origin: (0, 0),
            logical: (0, 0),
        }
    }

    /// Logical area shared with `region`, picking the output an overlay
    /// straddling two monitors mostly sits on.
    fn overlap(&self, region: (i32, i32, u32, u32)) -> i64 {
        let (left, top) = (i64::from(self.origin.0), i64::from(self.origin.1));
        let (right, bottom) = (left + i64::from(self.logical.0), top + i64::from(self.logical.1));

        let x0 = i64::from(region.0).max(left);
        let y0 = i64::from(region.1).max(top);
        let x1 = (i64::from(region.0) + i64::from(region.2)).min(right);
        let y1 = (i64::from(region.1) + i64::from(region.3)).min(bottom);

        (x1 - x0).max(0) * (y1 - y0).max(0)
    }
}

#[derive(Default)]
struct Capture {
    outputs: Vec<Output>,
    buffer_size: Option<(u32, u32)>,
    format: Option<Format>,
    constraints_done: bool,
    transform: Option<WEnum<Transform>>,
    /// `Some` once the copy is done or refused.
    outcome: Option<Result<(), String>>,
}

impl Dispatch<WlRegistry, GlobalListContents> for Capture {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // One-shot capture; globals cannot change underneath it.
    }
}

impl Dispatch<ZxdgOutputV1, usize> for Capture {
    fn event(
        state: &mut Self,
        _: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state.outputs.get_mut(*index) else {
            return;
        };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => output.origin = (x, y),
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                output.logical = (width.max(0) as u32, height.max(0) as u32)
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for Capture {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                state.buffer_size = Some((width, height))
            }
            // Take the first format that maps onto opaque RGBA without a table.
            ext_image_copy_capture_session_v1::Event::ShmFormat { format }
                if state.format.is_none()
                    && matches!(
                        format,
                        WEnum::Value(Format::Xrgb8888 | Format::Argb8888 | Format::Xbgr8888 | Format::Abgr8888)
                    ) =>
            {
                state.format = format.into_result().ok();
            }
            ext_image_copy_capture_session_v1::Event::Done => state.constraints_done = true,
            ext_image_copy_capture_session_v1::Event::Stopped => {
                state.constraints_done = true;
                state.outcome = Some(Err("compositor stopped the capture session".into()));
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for Capture {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_frame_v1::Event::Transform { transform } => state.transform = Some(transform),
            ext_image_copy_capture_frame_v1::Event::Ready => state.outcome = Some(Ok(())),
            ext_image_copy_capture_frame_v1::Event::Failed { reason } => {
                state.outcome = Some(Err(format!("compositor refused the capture: {reason:?}")))
            }
            _ => {}
        }
    }
}

delegate_noop!(Capture: ignore WlShm);
delegate_noop!(Capture: ignore WlOutput);
delegate_noop!(Capture: ignore WlBuffer);
delegate_noop!(Capture: WlShmPool);
delegate_noop!(Capture: ZxdgOutputManagerV1);
delegate_noop!(Capture: ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(Capture: ExtImageCaptureSourceV1);
delegate_noop!(Capture: ExtImageCopyCaptureManagerV1);
