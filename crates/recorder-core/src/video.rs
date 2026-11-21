//! Constant-frame-rate video.
//!
//! Windows.Graphics.Capture only delivers a frame when the screen changes.
//! Each new frame is copied (on the GPU) into a small ring of textures, and a
//! pacing thread submits the latest one to the encoder on every tick of the
//! media clock. A static screen therefore repeats the last frame, and every
//! frame's timestamp is exactly `tick / fps`, which keeps video in step with
//! audio.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use tracing::{debug, warn};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_SAMPLE_DESC};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use crate::RecorderError;
use crate::clock::MediaClock;
use crate::writer::Writer;

const RING_SIZE: usize = 8;
const NONE: usize = usize::MAX;

/// GPU textures holding the most recent captured frames.
pub struct FrameRing {
    textures: Vec<ID3D11Texture2D>,
    next: AtomicUsize,
    latest: AtomicUsize,
    width: u32,
    height: u32,
}

// SAFETY: D3D11 devices and resources are free-threaded, and the device has
// multithread protection enabled (see `Writer::create`).
unsafe impl Send for FrameRing {}
unsafe impl Sync for FrameRing {}

impl FrameRing {
    pub fn new(device: &ID3D11Device, width: u32, height: u32, format: DXGI_FORMAT) -> Result<Self, RecorderError> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut textures = Vec::with_capacity(RING_SIZE);
        for _ in 0..RING_SIZE {
            let mut texture = None;
            unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
                .map_err(|e| RecorderError::Capture(format!("create frame texture: {e}")))?;
            textures.push(texture.ok_or_else(|| RecorderError::Capture("no texture".into()))?);
        }
        Ok(Self { textures, next: AtomicUsize::new(0), latest: AtomicUsize::new(NONE), width, height })
    }

    /// Copies a captured frame into the next slot (GPU copy). Capture thread only.
    pub fn push(&self, context: &ID3D11DeviceContext, src: &ID3D11Texture2D, src_width: u32, src_height: u32) {
        let slot = self.next.fetch_add(1, Ordering::Relaxed) % RING_SIZE;
        // Clip to the ring size: handles odd sizes and mid-recording resolution changes.
        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: src_width.min(self.width),
            bottom: src_height.min(self.height),
            back: 1,
        };
        unsafe { context.CopySubresourceRegion(&self.textures[slot], 0, 0, 0, 0, src, 0, Some(&region)) };
        self.latest.store(slot, Ordering::Release);
    }

    fn latest(&self) -> Option<&ID3D11Texture2D> {
        match self.latest.load(Ordering::Acquire) {
            NONE => None,
            slot => Some(&self.textures[slot]),
        }
    }
}

#[derive(Default)]
pub struct PacerStats {
    pub frames_written: AtomicU64,
    pub frames_dropped: AtomicU64,
}

/// Submits one frame per tick of the media clock until `stop` is set.
pub fn spawn_pacer(
    fps: u32,
    clock: Arc<MediaClock>,
    ring: Arc<OnceLock<FrameRing>>,
    writer: Arc<Mutex<Option<Writer>>>,
    stop: Arc<AtomicBool>,
    stats: Arc<PacerStats>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name("video-pacer".into()).spawn(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // Keep frame pacing on time when the PC is busy.
        let _mmcss = crate::system::MmcssGuard::register("Capture");
        let frame_100ns = 10_000_000 / i64::from(fps);
        let mut next_tick: u64 = 0;

        while !stop.load(Ordering::Relaxed) {
            let (Some(now), Some(ring)) = (clock.now(), ring.get()) else {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            };
            let due_tick = (now.as_nanos() * u128::from(fps) / 1_000_000_000) as u64;

            // Submit every tick that is due (normally one). Catch up a little
            // after a hiccup, but skip ahead after a long stall.
            let mut budget = 8;
            while next_tick <= due_tick && budget > 0 {
                if let Some(texture) = ring.latest() {
                    let time = next_tick as i64 * 10_000_000 / i64::from(fps);
                    let mut guard = writer.lock().unwrap();
                    let Some(w) = guard.as_mut() else { return };
                    if let Err(e) = w.write_video(texture, time, frame_100ns) {
                        warn!(error = %e, "failed to write video frame");
                    } else {
                        stats.frames_written.fetch_add(1, Ordering::Relaxed);
                    }
                }
                next_tick += 1;
                budget -= 1;
            }
            if next_tick <= due_tick {
                let skipped = due_tick + 1 - next_tick;
                stats.frames_dropped.fetch_add(skipped, Ordering::Relaxed);
                debug!(skipped, "video pacer fell behind; skipping ahead");
                next_tick = due_tick + 1;
            }

            if clock.is_paused() {
                std::thread::sleep(Duration::from_millis(5));
            } else {
                let next_ns = next_tick as u128 * 1_000_000_000 / u128::from(fps);
                let wait = next_ns.saturating_sub(now.as_nanos()) as u64;
                std::thread::sleep(Duration::from_nanos(wait.min(20_000_000)));
            }
        }
    })
}
