use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use ashpd::desktop::{
    PersistMode,
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
};
use pipewire as pw;
use pw::properties::properties;
use pw::spa::pod::Pod;

use super::{CaptureError, CapturedImage, device_region, log_timing};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct PortalState {
    tx: Option<mpsc::Sender<CaptureRequest>>,
    starting: bool,
}

struct CaptureRequest {
    region: (i32, i32, u32, u32),
    reply: mpsc::Sender<Result<CapturedImage, CaptureError>>,
}

struct StreamData {
    format: pw::spa::param::video::VideoInfoRaw,
    format_logged: bool,
    pending: Arc<Mutex<Option<CaptureRequest>>>,
    origin: (i32, i32),
    logical: (u32, u32),
}

static PORTAL: OnceLock<Mutex<PortalState>> = OnceLock::new();

fn state() -> &'static Mutex<PortalState> {
    PORTAL.get_or_init(|| Mutex::new(PortalState::default()))
}

fn token_path() -> Option<PathBuf> {
    baras_core::paths::config_dir().map(|path| path.join("screen-capture-token"))
}

pub fn has_restore_token() -> bool {
    token_path().is_some_and(|path| path.is_file())
}

pub async fn enable() -> Result<(), CaptureError> {
    {
        let mut state = state().lock().unwrap_or_else(|p| p.into_inner());
        if state.tx.is_some() {
            return Ok(());
        }
        if state.starting {
            return Err(CaptureError::Failed(
                "screen capture is already starting".into(),
            ));
        }
        state.starting = true;
    }

    let result = start_portal().await;
    let mut state = state().lock().unwrap_or_else(|p| p.into_inner());
    state.starting = false;
    match result {
        Ok(tx) => {
            state.tx = Some(tx);
            Ok(())
        }
        Err(error) => {
            if matches!(&error, CaptureError::PermissionRequired(_)) {
                tracing::info!(target: "baras::capture", "screen capture permission not granted");
            } else {
                tracing::warn!(target: "baras::capture", error = %error, "screen capture portal unavailable");
            }
            Err(error)
        }
    }
}

async fn start_portal() -> Result<mpsc::Sender<CaptureRequest>, CaptureError> {
    let portal = Screencast::new()
        .await
        .map_err(|e| CaptureError::ConnectionFailed(format!("screen cast portal: {e}")))?;
    let session = portal
        .create_session(Default::default())
        .await
        .map_err(|e| CaptureError::Failed(format!("could not create screen cast: {e}")))?;
    let restore_token = token_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty());
    tracing::info!(
        target: "baras::capture",
        restore_requested = restore_token.is_some(),
        "opening screen capture portal"
    );

    portal
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::HIDDEN)
                .set_sources(SourceType::MONITOR)
                .set_multiple(false)
                .set_restore_token(restore_token.as_deref())
                .set_persist_mode(PersistMode::ExplicitlyRevoked),
        )
        .await
        .map_err(|e| CaptureError::Failed(format!("could not select a monitor: {e}")))?;

    let response = portal
        .start(&session, None, Default::default())
        .await
        .map_err(|e| CaptureError::Failed(format!("screen capture request failed: {e}")))?
        .response()
        .map_err(|e| {
            CaptureError::PermissionRequired(format!("screen capture was declined: {e}"))
        })?;
    let stream = response
        .streams()
        .first()
        .ok_or_else(|| CaptureError::Failed("the portal returned no monitor".into()))?;
    let node = stream.pipe_wire_node_id();
    let origin = stream.position().unwrap_or((0, 0));
    let logical = stream
        .size()
        .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)))
        .unwrap_or((0, 0));
    let new_token = response.restore_token().map(str::to_owned);
    let fd = portal
        .open_pipe_wire_remote(&session, Default::default())
        .await
        .map_err(|e| CaptureError::Failed(format!("could not open PipeWire: {e}")))?;
    let tx = spawn_stream(fd, node, origin, logical)?;

    tracing::info!(
        target: "baras::capture",
        backend = "pipewire-portal",
        node,
        origin = ?origin,
        logical_size = ?logical,
        restore_token = new_token.is_some(),
        on_demand = true,
        "screen capture portal ready"
    );

    if let (Some(path), Some(token)) = (token_path(), new_token) {
        let saved = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, token));
        if let Err(error) = saved {
            tracing::warn!(
                target: "baras::capture",
                error = %error,
                "could not save screen capture permission"
            );
        }
    }

    tokio::spawn(async move {
        let _portal = portal;
        let _session = session;
        std::future::pending::<()>().await;
    });
    Ok(tx)
}

pub fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CapturedImage, CaptureError> {
    let tx = state()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .tx
        .clone()
        .ok_or_else(|| {
            CaptureError::PermissionRequired("screen capture permission is required".into())
        })?;
    let (reply, result) = mpsc::channel();
    let started = Instant::now();
    if tx
        .send(CaptureRequest {
            region: (x, y, width, height),
            reply,
        })
        .is_err()
    {
        state().lock().unwrap_or_else(|p| p.into_inner()).tx = None;
        return Err(CaptureError::PermissionRequired(
            "screen capture session ended".into(),
        ));
    }
    let image = result
        .recv_timeout(TIMEOUT)
        .map_err(|_| CaptureError::Failed("PipeWire did not return a frame in time".into()))??;
    log_timing(
        "pipewire-portal",
        Duration::ZERO,
        started.elapsed(),
        Duration::ZERO,
        started.elapsed(),
        (image.width, image.height),
    );
    Ok(image)
}

fn spawn_stream(
    fd: OwnedFd,
    node: u32,
    origin: (i32, i32),
    logical: (u32, u32),
) -> Result<mpsc::Sender<CaptureRequest>, CaptureError> {
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("baras-pipewire".into())
        .spawn(move || {
            if let Err(error) = run_stream(fd, node, origin, logical, rx, &ready_tx) {
                let _ = ready_tx.send(Err(error.to_string()));
                tracing::warn!(target: "baras::capture", error = %error, "PipeWire capture stopped");
            }
        })
        .map_err(|e| CaptureError::Failed(format!("could not start PipeWire: {e}")))?;
    ready_rx
        .recv_timeout(TIMEOUT)
        .map_err(|_| CaptureError::Failed("PipeWire did not start in time".into()))?
        .map_err(CaptureError::Failed)?;
    Ok(tx)
}

fn run_stream(
    fd: OwnedFd,
    node: u32,
    origin: (i32, i32),
    logical: (u32, u32),
    requests: mpsc::Receiver<CaptureRequest>,
    ready: &mpsc::SyncSender<Result<(), String>>,
) -> Result<(), pw::Error> {
    // Safe to call more than once; the library refuses to work without it.
    pw::init();
    let thread_loop = unsafe { pw::thread_loop::ThreadLoopBox::new(Some("baras-pipewire"), None)? };
    let context = pw::context::ContextBox::new(thread_loop.loop_(), None)?;
    let core = context.connect_fd(fd, None)?;
    let pending = Arc::new(Mutex::new(None));
    let stream = pw::stream::StreamBox::new(
        &core,
        "baras-screen-capture",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;
    let data = StreamData {
        format: Default::default(),
        format_logged: false,
        pending: pending.clone(),
        origin,
        logical,
    };
    let _listener = stream
        .add_local_listener_with_user_data(data)
        .param_changed(|_, data, id, param| {
            if id == pw::spa::param::ParamType::Format.as_raw()
                && let Some(param) = param
                && data.format.parse(param).is_ok()
            {
                if !data.format_logged {
                    let size = data.format.size();
                    let frame_mib =
                        f64::from(size.width) * f64::from(size.height) * 4.0 / (1024.0 * 1024.0);
                    tracing::info!(
                        target: "baras::capture",
                        backend = "pipewire-portal",
                        format = ?data.format.format(),
                        width = size.width,
                        height = size.height,
                        frame_mib,
                        "screen capture stream ready"
                    );
                    data.format_logged = true;
                }
            }
        })
        .process(|stream, data| {
            if data
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .is_none()
            {
                let _ = stream.set_active(false);
                return;
            }
            let Some(mut buffer) = stream.dequeue_buffer() else {
                // Spurious wakeup; stay active until a buffer arrives.
                return;
            };
            // KWin wakes consumers with cursor-metadata buffers carrying no
            // pixels (chunk size 0) even when the cursor is hidden. Skip them
            // and stay active for the next frame; the caller's timeout is the
            // backstop if none ever comes.
            let has_pixels = buffer.datas_mut().first().is_some_and(|pixels| {
                pixels.chunk().size() > 0
                    && !pixels
                        .chunk()
                        .flags()
                        .contains(pw::spa::buffer::ChunkFlags::CORRUPTED)
            });
            if !has_pixels {
                return;
            }
            let Some(request) = data
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take()
            else {
                let _ = stream.set_active(false);
                return;
            };
            let result = buffer
                .datas_mut()
                .first_mut()
                .ok_or_else(|| CaptureError::Failed("PipeWire returned an empty buffer".into()))
                .and_then(|pixels| {
                    frame_from_buffer(
                        pixels,
                        &data.format,
                        data.origin,
                        data.logical,
                        request.region,
                    )
                });
            let _ = stream.set_active(false);
            let _ = request.reply.send(result);
        })
        .register()?;

    let format = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaType,
            Id,
            pw::spa::param::format::MediaType::Video
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaSubtype,
            Id,
            pw::spa::param::format::MediaSubtype::Raw
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            pw::spa::param::video::VideoFormat::BGRx,
            pw::spa::param::video::VideoFormat::BGRx,
            pw::spa::param::video::VideoFormat::BGRA,
            pw::spa::param::video::VideoFormat::RGBx,
            pw::spa::param::video::VideoFormat::RGBA,
        ),
    );
    let values = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(format),
    )
    .map_err(|_| pw::Error::CreationFailed)?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&values).ok_or(pw::Error::CreationFailed)?];
    stream.connect(
        pw::spa::utils::Direction::Input,
        Some(node),
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::INACTIVE
            | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;
    thread_loop.start();
    let _ = ready.send(Ok(()));

    for request in requests {
        let mut slot = pending.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_some() {
            let _ = request.reply.send(Err(CaptureError::Failed(
                "capture is already running".into(),
            )));
            continue;
        }
        *slot = Some(request);
        drop(slot);
        let _guard = thread_loop.lock();
        if let Err(error) = stream.set_active(true) {
            if let Some(request) = pending.lock().unwrap_or_else(|p| p.into_inner()).take() {
                let _ = request
                    .reply
                    .send(Err(CaptureError::Failed(error.to_string())));
            }
        }
    }
    thread_loop.stop();
    Ok(())
}

fn frame_from_buffer(
    data: &mut pw::spa::buffer::Data,
    format: &pw::spa::param::video::VideoInfoRaw,
    origin: (i32, i32),
    logical: (u32, u32),
    region: (i32, i32, u32, u32),
) -> Result<CapturedImage, CaptureError> {
    let size = format.size();
    let (width, height) = (size.width, size.height);
    if width == 0 || height == 0 {
        return Err(CaptureError::Failed(
            "PipeWire returned no frame size".into(),
        ));
    }
    let logical = if logical.0 == 0 || logical.1 == 0 {
        (width, height)
    } else {
        logical
    };
    let (crop_x, crop_y, crop_width, crop_height) =
        device_region(region, origin, logical, (width, height)).ok_or_else(|| {
            CaptureError::InvalidRegion(
                "the selected monitor does not contain the raid frames".into(),
            )
        })?;
    let offset = data.chunk().offset() as usize;
    // Some producers leave the chunk stride unset; the chunk size still spans
    // the whole frame, so the row length can be derived from it instead.
    let stride = match data.chunk().stride() {
        stride if stride > 0 => stride as usize,
        _ => data.chunk().size() as usize / height as usize,
    };
    if stride < width as usize * 4 {
        return Err(CaptureError::Failed(
            "PipeWire returned an unusable row layout".into(),
        ));
    }
    let bytes = data
        .data()
        .ok_or_else(|| CaptureError::Failed("PipeWire buffer is not mapped".into()))?;
    let mut rgba = Vec::with_capacity(crop_width as usize * crop_height as usize * 4);
    let swap = matches!(
        format.format(),
        pw::spa::param::video::VideoFormat::BGRx | pw::spa::param::video::VideoFormat::BGRA
    );
    for row in crop_y..crop_y + crop_height {
        let start = offset + row as usize * stride + crop_x as usize * 4;
        let end = start + crop_width as usize * 4;
        let pixels = bytes
            .get(start..end)
            .ok_or_else(|| CaptureError::Failed("PipeWire returned a short frame".into()))?;
        for pixel in pixels.chunks_exact(4) {
            let (r, b) = if swap {
                (pixel[2], pixel[0])
            } else {
                (pixel[0], pixel[2])
            };
            rgba.extend_from_slice(&[r, pixel[1], b, 255]);
        }
    }
    Ok(CapturedImage {
        width: crop_width,
        height: crop_height,
        rgba,
    })
}
