//! A single recording: start → (pause ⇄ resume) → stop.
//!
//! Threads:
//! - capture (windows-capture): copies each new screen frame into a GPU ring
//! - video pacer: submits the latest frame to the encoder at a constant rate
//! - audio capture ×N + audio pacer: one AAC track per source
//! - Media Foundation's own encoder threads
//!
//! Video and audio are both timed by one [`MediaClock`], which also
//! implements pause.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::{info, warn};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::RecorderError;
use crate::audio::{AudioConfig, AudioPipeline, AudioTrack};
use crate::clock::MediaClock;
use crate::options::{QualityPreset, RecordOptions, timestamped_file_name};
use crate::video::{FrameRing, PacerStats, spawn_pacer};
use crate::writer::{VideoFormat, Writer};

type HandlerError = Box<dyn std::error::Error + Send + Sync>;

/// State shared by all recording threads.
struct Shared {
    writer: Arc<Mutex<Option<Writer>>>,
    ring: Arc<OnceLock<FrameRing>>,
    clock: Arc<MediaClock>,
    /// Set once the encoder is ready (or failed to start).
    ready: OnceLock<Result<(), String>>,
    screen_updates: AtomicU64,
}

/// D3D11 objects handed to the capture thread.
struct D3D {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
}
// SAFETY: D3D11 devices are free-threaded; the immediate context is only used
// on the capture thread, with multithread protection enabled on the device.
unsafe impl Send for D3D {}

struct HandlerConfig {
    path: PathBuf,
    preset: QualityPreset,
    tracks: Vec<AudioTrack>,
    shared: Arc<Shared>,
}

struct Handler {
    config: HandlerConfig,
    d3d: D3D,
}

impl Handler {
    /// First frame: build the frame ring and the encoder, then start the clock.
    fn init(&self, frame: &Frame) -> Result<(), RecorderError> {
        let shared = &self.config.shared;
        let preset = self.config.preset;
        // H.264/HEVC need even dimensions.
        let (width, height) = (frame.width() & !1, frame.height() & !1);
        let bitrate = preset.bitrate(width, height);

        let ring = FrameRing::new(&self.d3d.device, width, height, frame.desc().Format)?;
        let started = Instant::now();
        let writer = Writer::create(
            &self.config.path,
            &self.d3d.device,
            &VideoFormat { width, height, fps: preset.fps(), bitrate, codec: preset.codec() },
            &self.config.tracks,
        )?;
        info!(
            width,
            height,
            fps = preset.fps(),
            bitrate_mbps = bitrate as f64 / 1e6,
            codec = ?preset.codec(),
            tracks = ?self.config.tracks,
            setup_ms = started.elapsed().as_millis() as u64,
            path = %self.config.path.display(),
            "encoder ready"
        );

        *shared.writer.lock().unwrap() = Some(writer);
        let _ = shared.ring.set(ring);
        shared.clock.start();
        Ok(())
    }
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (HandlerConfig, D3DSlot);
    type Error = HandlerError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (config, _) = ctx.flags;
        Ok(Self { config, d3d: D3D { device: ctx.device, context: ctx.device_context } })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _control: InternalCaptureControl) -> Result<(), Self::Error> {
        let shared = &self.config.shared;
        if shared.ready.get().is_none() {
            let result = self.init(frame).map_err(|e| e.to_string());
            let failed = result.clone().err();
            let _ = shared.ready.set(result);
            if let Some(e) = failed {
                return Err(e.into());
            }
        }

        if let Some(ring) = shared.ring.get() {
            ring.push(&self.d3d.context, frame.as_raw_texture(), frame.width(), frame.height());
            shared.screen_updates.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        warn!("capture item closed (display disconnected?)");
        Ok(())
    }
}

/// Placeholder so `Flags` can carry future per-session D3D options.
#[derive(Default)]
struct D3DSlot;

/// Result of a finished recording.
#[derive(Debug, Clone, Serialize)]
pub struct RecordingSummary {
    pub path: PathBuf,
    /// Length of the video (pauses excluded).
    pub duration: Duration,
    /// Frames in the file (constant frame rate).
    pub frames: u64,
    /// Ticks skipped because the system stalled. Should be 0.
    pub dropped_frames: u64,
    pub bytes: u64,
    /// Audio tracks in the file, in order.
    pub audio_tracks: Vec<AudioTrack>,
    /// Problems that didn't stop the recording, e.g. "Microphone unavailable".
    pub warnings: Vec<String>,
}

/// A recording in progress. Call [`Recorder::stop`] to finish the file.
///
/// Dropping a `Recorder` without calling `stop` also stops it and finalizes
/// the file, so a recording is never left running in the background.
pub struct Recorder {
    control: Option<CaptureControl<Handler, HandlerError>>,
    audio: Option<AudioPipeline>,
    pacer: Option<JoinHandle<()>>,
    pacer_stop: Arc<AtomicBool>,
    pacer_stats: Arc<PacerStats>,
    shared: Arc<Shared>,
    tracks: Vec<AudioTrack>,
    warnings: Vec<String>,
    path: PathBuf,
}

impl Recorder {
    /// Starts recording and returns once the encoder is running.
    pub fn start(options: RecordOptions) -> Result<Self, RecorderError> {
        let monitor = Monitor::from_index(options.display)
            .map_err(|_| RecorderError::DisplayNotFound(options.display))?;

        std::fs::create_dir_all(&options.output_dir).map_err(|source| RecorderError::OutputDir {
            path: options.output_dir.clone(),
            source,
        })?;
        let path = options.output_dir.join(options.file_name.clone().unwrap_or_else(timestamped_file_name));

        let shared = Arc::new(Shared {
            writer: Arc::new(Mutex::new(None)),
            ring: Arc::new(OnceLock::new()),
            clock: Arc::new(MediaClock::default()),
            ready: OnceLock::new(),
            screen_updates: AtomicU64::new(0),
        });

        // Audio first: the devices start buffering while the encoder spins up,
        // and the encoder needs to know which tracks exist.
        let audio_config = AudioConfig {
            system: options.system_audio,
            microphone: options.microphone,
            microphone_id: options.microphone_id.clone(),
        };
        let audio_writer = Arc::clone(&shared.writer);
        let audio = AudioPipeline::start(&audio_config, Arc::clone(&shared.clock), move |track, pcm, time| {
            let mut guard = audio_writer.lock().unwrap();
            match guard.as_mut() {
                Some(w) => {
                    if let Err(e) = w.write_audio(track, pcm, time) {
                        warn!(error = %e, "failed to write audio");
                    }
                    true
                }
                None => false, // finalized
            }
        })?;
        let tracks = audio.tracks().to_vec();
        let warnings = audio.warnings.clone();

        let fps = options.preset.fps();
        let settings = Settings::new(
            monitor,
            if options.capture_cursor {
                CursorCaptureSettings::WithCursor
            } else {
                CursorCaptureSettings::WithoutCursor
            },
            // Windows 11 lets us hide the yellow capture border.
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            // No point capturing faster than we encode.
            MinimumUpdateIntervalSettings::Custom(Duration::from_secs(1) / fps),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            (
                HandlerConfig {
                    path: path.clone(),
                    preset: options.preset,
                    tracks: tracks.clone(),
                    shared: Arc::clone(&shared),
                },
                D3DSlot,
            ),
        );
        let control = Handler::start_free_threaded(settings).map_err(|e| RecorderError::Capture(format!("{e:?}")))?;

        let pacer_stop = Arc::new(AtomicBool::new(false));
        let pacer_stats = Arc::new(PacerStats::default());
        let pacer = spawn_pacer(
            fps,
            Arc::clone(&shared.clock),
            Arc::clone(&shared.ring),
            Arc::clone(&shared.writer),
            Arc::clone(&pacer_stop),
            Arc::clone(&pacer_stats),
        )
        .map_err(|e| RecorderError::Capture(e.to_string()))?;

        let mut recorder = Self {
            control: Some(control),
            audio: Some(audio),
            pacer: Some(pacer),
            pacer_stop,
            pacer_stats,
            shared,
            tracks,
            warnings,
            path,
        };

        // Report encoder problems now rather than at stop.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match recorder.shared.ready.get() {
                Some(Ok(())) => break,
                Some(Err(e)) => {
                    let e = RecorderError::Encoder(e.clone());
                    recorder.shutdown_threads();
                    let _ = std::fs::remove_file(&recorder.path);
                    return Err(e);
                }
                None if Instant::now() > deadline => {
                    recorder.shutdown_threads();
                    return Err(RecorderError::Capture("no frame arrived from the display within 10 s".into()));
                }
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }

        info!(
            display = options.display,
            preset = ?options.preset,
            audio_tracks = ?recorder.tracks,
            "recording started"
        );
        Ok(recorder)
    }

    /// Recorded time so far, excluding pauses.
    pub fn elapsed(&self) -> Duration {
        self.shared.clock.now().unwrap_or_default()
    }

    pub fn pause(&self) -> bool {
        let paused = self.shared.clock.pause();
        if paused {
            info!(at = ?self.elapsed(), "paused");
        }
        paused
    }

    pub fn resume(&self) -> bool {
        let resumed = self.shared.clock.resume();
        if resumed {
            info!(at = ?self.elapsed(), "resumed");
        }
        resumed
    }

    pub fn is_paused(&self) -> bool {
        self.shared.clock.is_paused()
    }

    /// Frames written to the file so far.
    pub fn frames(&self) -> u64 {
        self.pacer_stats.frames_written.load(Ordering::Relaxed)
    }

    /// Peak audio levels (system, microphone) since the last call, 0.0–1.0.
    pub fn levels(&self) -> (f32, f32) {
        self.audio.as_ref().map(|a| a.levels.take()).unwrap_or((0.0, 0.0))
    }

    pub fn audio_tracks(&self) -> &[AudioTrack] {
        &self.tracks
    }

    /// Problems so far that didn't stop the recording.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Where the file is being written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stops capturing, flushes the encoder and returns the finished file.
    pub fn stop(mut self) -> Result<RecordingSummary, RecorderError> {
        self.finish()
    }

    /// Stops every producer thread. Safe to call more than once.
    fn shutdown_threads(&mut self) -> Option<String> {
        self.pacer_stop.store(true, Ordering::Relaxed);
        if let Some(p) = self.pacer.take() {
            let _ = p.join();
        }
        if let Some(a) = self.audio.take() {
            a.stop();
        }
        self.control.take().and_then(|c| c.stop().err()).map(|e| format!("{e:?}"))
    }

    fn finish(&mut self) -> Result<RecordingSummary, RecorderError> {
        let duration = self.elapsed();
        // Video and audio stop at the same media time; then the file is closed.
        let capture_error = self.shutdown_threads();
        if let Some(e) = &capture_error {
            warn!(error = %e, "capture thread ended with an error");
        }

        let writer = self.shared.writer.lock().unwrap().take().ok_or(RecorderError::NoFrames)?;
        writer.finalize()?;

        let frames = self.frames();
        let dropped_frames = self.pacer_stats.frames_dropped.load(Ordering::Relaxed);
        let bytes = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        info!(
            path = %self.path.display(),
            frames,
            dropped_frames,
            screen_updates = self.shared.screen_updates.load(Ordering::Relaxed),
            bytes,
            secs = duration.as_secs_f64(),
            "recording saved"
        );

        Ok(RecordingSummary {
            path: self.path.clone(),
            duration,
            frames,
            dropped_frames,
            bytes,
            audio_tracks: self.tracks.clone(),
            warnings: self.warnings.clone(),
        })
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.shared.writer.lock().map(|w| w.is_some()).unwrap_or(false)
            && let Err(e) = self.finish()
        {
            warn!(error = %e, "failed to finish recording on drop");
        }
        self.shutdown_threads();
    }
}
