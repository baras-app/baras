//! Loopback-only web mirror for the native overlay windows.
//!
//! Native overlays already render their final pixels.  This module caches those
//! small transparent frames and serves a tiny page that positions them at the
//! same monitor-relative coordinates.  No combat or overlay UI logic is
//! duplicated in the browser.

use std::collections::HashMap;
use std::io::Cursor;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use baras_overlay::Overlay;
use baras_types::{WEB_OVERLAY_PORT, WEB_OVERLAY_URL};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex as AsyncMutex, oneshot};

use crate::overlay::OverlayType;

const OVERLAY_PAGE: &str = r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Baras Web Overlay</title>
  <style>
    html, body, #scene {
      margin: 0;
      width: 100%;
      height: 100%;
      overflow: hidden;
      background: transparent;
    }
    #scene { position: relative; }
    .overlay {
      position: absolute;
      pointer-events: none;
      user-select: none;
    }
  </style>
</head>
<body>
  <div id="scene"></div>
  <script>
    const scene = document.getElementById('scene');
    const overlays = new Map();

    function clearScene() {
      for (const image of overlays.values()) image.remove();
      overlays.clear();
    }

    async function refresh() {
      try {
        const response = await fetch('/scene', { cache: 'no-store' });
        if (!response.ok) throw new Error('Web overlay unavailable');
        const frames = await response.json();
        const active = new Set();

        for (const frame of frames) {
          active.add(frame.id);
          let image = overlays.get(frame.id);
          if (!image) {
            image = document.createElement('img');
            image.className = 'overlay';
            image.alt = '';
            image.draggable = false;
            overlays.set(frame.id, image);
            scene.appendChild(image);
          }

          image.style.left = `${frame.x}px`;
          image.style.top = `${frame.y}px`;
          image.style.width = `${frame.width}px`;
          image.style.height = `${frame.height}px`;

          const revision = String(frame.revision);
          if (image.dataset.revision !== revision) {
            image.dataset.revision = revision;
            image.src = `/frame/${frame.id}.png?v=${revision}`;
          }
        }

        for (const [id, image] of overlays) {
          if (!active.has(id)) {
            image.remove();
            overlays.delete(id);
          }
        }
      } catch (_) {
        clearScene();
      }

      window.setTimeout(refresh, 100);
    }

    refresh();
  </script>
</body>
</html>
"#;

static CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static FRAME_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static FRAME_CACHE: OnceLock<Mutex<HashMap<String, CachedFrame>>> = OnceLock::new();

fn frame_cache() -> &'static Mutex<HashMap<String, CachedFrame>> {
    FRAME_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_frames() -> std::sync::MutexGuard<'static, HashMap<String, CachedFrame>> {
    frame_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct CachedFrame {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    revision: u64,
    rgba: Vec<u8>,
    png: Option<Vec<u8>>,
}

#[derive(Serialize)]
struct SceneFrame {
    id: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    revision: u64,
}

/// Cache the pixels produced by a native overlay render.
pub fn capture_overlay<O: Overlay>(kind: OverlayType, overlay: &mut O) {
    if !CAPTURE_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let position = overlay.position();
    let (monitor_x, monitor_y) = overlay
        .frame()
        .window()
        .current_monitor()
        .map(|monitor| (monitor.x, monitor.y))
        .unwrap_or((0, 0));

    let Some(rgba) = overlay.frame_mut().window_mut().snapshot_rgba() else {
        return;
    };
    if rgba.len() != (position.width * position.height * 4) as usize {
        return;
    }

    let frame = CachedFrame {
        x: position.x - monitor_x,
        y: position.y - monitor_y,
        width: position.width,
        height: position.height,
        revision: FRAME_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        rgba,
        png: None,
    };
    lock_frames().insert(kind.config_key().to_string(), frame);
}

/// Remove an overlay as soon as its native window closes.
pub fn remove_frame(kind: OverlayType) {
    lock_frames().remove(kind.config_key());
}

fn clear_frames() {
    lock_frames().clear();
}

fn scene_json() -> Vec<u8> {
    let frames = lock_frames();
    let mut scene: Vec<_> = frames
        .iter()
        .map(|(id, frame)| SceneFrame {
            id: id.clone(),
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
            revision: frame.revision,
        })
        .collect();
    scene.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    serde_json::to_vec(&scene).unwrap_or_else(|_| b"[]".to_vec())
}

fn frame_png(id: &str) -> Option<Vec<u8>> {
    let (revision, width, height, rgba) = {
        let frames = lock_frames();
        let frame = frames.get(id)?;
        if let Some(png) = &frame.png {
            return Some(png.clone());
        }
        (frame.revision, frame.width, frame.height, frame.rgba.clone())
    };

    let png = encode_png(width, height, rgba).ok()?;
    let mut frames = lock_frames();
    if let Some(frame) = frames.get_mut(id)
        && frame.revision == revision
    {
        frame.png = Some(png.clone());
    }
    Some(png)
}

/// tiny-skia stores premultiplied RGBA; PNG/browser input expects straight RGBA.
fn encode_png(width: u32, height: u32, mut rgba: Vec<u8>) -> Result<Vec<u8>, String> {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = (((*channel as u32 * 255) + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }

    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut output), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer.write_image_data(&rgba).map_err(|error| error.to_string())?;
    }
    Ok(output)
}

struct RunningServer {
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Clone, Default)]
pub struct WebOverlayServer {
    running: Arc<AsyncMutex<Option<RunningServer>>>,
}

impl WebOverlayServer {
    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.lock().await;
        if running.is_some() {
            return Ok(());
        }

        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, WEB_OVERLAY_PORT));
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| format!("Could not start web overlay on {WEB_OVERLAY_URL}: {error}"))?;
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        CAPTURE_ENABLED.store(true, Ordering::Relaxed);
        let task = tokio::spawn(run_server(listener, shutdown_rx));
        *running = Some(RunningServer {
            shutdown: shutdown_tx,
            task,
        });
        tracing::info!(url = WEB_OVERLAY_URL, "Web overlay server started");
        Ok(())
    }

    pub async fn stop(&self) {
        let server = self.running.lock().await.take();
        CAPTURE_ENABLED.store(false, Ordering::Relaxed);
        clear_frames();

        if let Some(server) = server {
            let _ = server.shutdown.send(());
            let _ = server.task.await;
            tracing::info!("Web overlay server stopped");
        }
    }
}

async fn run_server(listener: TcpListener, mut shutdown: oneshot::Receiver<()>) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        tokio::spawn(async move {
                            if let Err(error) = handle_connection(stream).await {
                                tracing::debug!(error = %error, "Web overlay connection closed");
                            }
                        });
                    }
                    Err(error) => {
                        tracing::warn!(error = %error, "Web overlay accept failed");
                    }
                }
            }
        }
    }
}

async fn handle_connection(mut stream: TcpStream) -> std::io::Result<()> {
    let mut request = [0_u8; 4096];
    let read = stream.read(&mut request).await?;
    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&request[..read]);
    let mut request_parts = request.lines().next().unwrap_or_default().split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let target = request_parts.next().unwrap_or_default();
    let path = target.split('?').next().unwrap_or(target);

    let (status, content_type, body) = if method != "GET" {
        (
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method not allowed".to_vec(),
        )
    } else if matches!(path, "/" | "/overlay" | "/overlay/") {
        ("200 OK", "text/html; charset=utf-8", OVERLAY_PAGE.as_bytes().to_vec())
    } else if path == "/scene" {
        ("200 OK", "application/json", scene_json())
    } else if let Some(id) = path
        .strip_prefix("/frame/")
        .and_then(|value| value.strip_suffix(".png"))
    {
        match frame_png(id) {
            Some(png) => ("200 OK", "image/png", png),
            None => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"Frame not found".to_vec(),
            ),
        }
    } else {
        ("404 Not Found", "text/plain; charset=utf-8", b"Not found".to_vec())
    };

    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_encoder_preserves_transparency() {
        let png = encode_png(1, 1, vec![0, 0, 0, 0]).expect("PNG should encode");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn overlay_page_is_transparent() {
        assert!(OVERLAY_PAGE.contains("background: transparent"));
        assert!(OVERLAY_PAGE.contains("fetch('/scene'"));
    }
}
