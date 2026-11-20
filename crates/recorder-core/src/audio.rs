//! Audio capture.
//!
//! - One WASAPI capture thread per source: system audio (loopback of the
//!   default output device) and the microphone. Both ask Windows for 48 kHz
//!   stereo float, so any device format is converted for us.
//! - One pacing thread driven by the shared [`MediaClock`]. Every 10 ms it
//!   emits, for each source, exactly the samples that are due, filling with
//!   silence where a source had nothing (loopback sends no data while nothing
//!   is playing). Each source becomes its own track in the file. While paused
//!   the clock stands still, so nothing is emitted and incoming audio is
//!   discarded.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use tracing::{debug, info, warn};
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

use crate::RecorderError;
use crate::clock::MediaClock;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// While recording, keep at most this much unsent audio per source; older
/// samples are dropped. Bounds the delay if a device clock runs slightly fast.
const MAX_BUFFERED: usize = SAMPLE_RATE as usize * CHANNELS / 5; // 200 ms
/// Before the encoder is ready (it can take over a second to create), keep
/// more, so audio from time zero onwards isn't lost.
const MAX_BUFFERED_STARTUP: usize = SAMPLE_RATE as usize * CHANNELS * 5; // 5 s
const TICK: Duration = Duration::from_millis(10);

/// An audio input device.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Lists microphones and other capture devices.
pub fn list_microphones() -> Result<Vec<AudioDevice>, RecorderError> {
    let err = |e: wasapi::WasapiError| RecorderError::Audio(e.to_string());
    let _ = wasapi::initialize_mta();
    let enumerator = DeviceEnumerator::new().map_err(err)?;
    let default_id = enumerator.get_default_device(Direction::Capture).and_then(|d| d.get_id()).ok();
    let collection = enumerator.get_device_collection(Direction::Capture).map_err(err)?;

    let mut devices = Vec::new();
    for i in 0..collection.get_nbr_devices().map_err(err)? {
        let Ok(device) = collection.get_device_at_index(i) else { continue };
        let (Ok(id), Ok(name)) = (device.get_id(), device.get_friendlyname()) else { continue };
        devices.push(AudioDevice { is_default: Some(&id) == default_id.as_ref(), id, name });
    }
    Ok(devices)
}

/// An audio track in the output file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioTrack {
    System,
    Microphone,
}

/// Peak level of each track since it was last read, stored as f32 bits.
#[derive(Default)]
pub struct Levels {
    system: AtomicU32,
    microphone: AtomicU32,
}

impl Levels {
    fn slot(&self, track: AudioTrack) -> &AtomicU32 {
        match track {
            AudioTrack::System => &self.system,
            AudioTrack::Microphone => &self.microphone,
        }
    }

    fn raise(&self, track: AudioTrack, peak: f32) {
        let slot = self.slot(track);
        let mut current = slot.load(Ordering::Relaxed);
        while peak > f32::from_bits(current) {
            match slot.compare_exchange_weak(current, peak.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(v) => current = v,
            }
        }
    }

    /// Returns (system, microphone) peaks, 0.0–1.0, and resets them.
    pub fn take(&self) -> (f32, f32) {
        (
            f32::from_bits(self.system.swap(0, Ordering::Relaxed)),
            f32::from_bits(self.microphone.swap(0, Ordering::Relaxed)),
        )
    }
}

/// Opens one WASAPI stream and forwards interleaved f32 samples.
fn spawn_capture(
    track: AudioTrack,
    device_id: Option<String>,
    stop: Arc<AtomicBool>,
    levels: Arc<Levels>,
) -> Result<(JoinHandle<()>, Receiver<Vec<f32>>), RecorderError> {
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(256);
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::Builder::new()
        .name(format!("audio-{track:?}").to_lowercase())
        .spawn(move || {
            if let Err(e) = capture_loop(track, device_id, &stop, &levels, &tx, &ready_tx) {
                // If `ready` was already sent, this is a runtime failure.
                if ready_tx.send(Err(e.clone())).is_err() {
                    warn!(?track, error = %e, "audio capture stopped");
                }
            }
        })
        .map_err(|e| RecorderError::Audio(e.to_string()))?;

    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok((handle, rx)),
        Ok(Err(e)) => Err(RecorderError::Audio(e)),
        Err(_) => Err(RecorderError::Audio(format!("{track:?} audio did not start in time"))),
    }
}

fn capture_loop(
    track: AudioTrack,
    device_id: Option<String>,
    stop: &AtomicBool,
    levels: &Levels,
    tx: &SyncSender<Vec<f32>>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    let s = |e: wasapi::WasapiError| e.to_string();
    let _ = wasapi::initialize_mta();
    let enumerator = DeviceEnumerator::new().map_err(s)?;
    let device = match (track, &device_id) {
        // Loopback: capture from the default *output* device.
        (AudioTrack::System, _) => enumerator.get_default_device(Direction::Render),
        (AudioTrack::Microphone, Some(id)) => enumerator.get_device(id),
        (AudioTrack::Microphone, None) => enumerator.get_default_device(Direction::Capture),
    }
    .map_err(s)?;
    let name = device.get_friendlyname().unwrap_or_default();

    let mut client = device.get_iaudioclient().map_err(s)?;
    let format = WaveFormat::new(32, 32, SampleType::Float, SAMPLE_RATE as usize, CHANNELS, None);
    // 100 ms device buffer, polled every 5 ms. Polling works for loopback on
    // all Windows versions, unlike event-driven loopback.
    let mode = StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: 1_000_000 };
    client.initialize_client(&format, Direction::Capture, mode, None).map_err(s)?;
    let capture = client.get_audiocaptureclient().map_err(s)?;
    client.start_stream().map_err(s)?;

    info!(?track, device = %name, "audio capture started");
    let _ = ready.send(Ok(()));

    let mut bytes: VecDeque<u8> = VecDeque::with_capacity(64 * 1024);
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(5));
        while let Some(frames) = capture.get_next_packet_size().map_err(s)?
            && frames > 0
        {
            capture.read_from_device_to_deque(&mut bytes).map_err(s)?;
        }
        if bytes.is_empty() {
            continue;
        }
        let samples: Vec<f32> = bytes
            .make_contiguous()
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        bytes.clear();
        let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        levels.raise(track, peak.min(1.0));
        match tx.try_send(samples) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => break,
        }
    }

    let _ = client.stop_stream();
    debug!(?track, "audio capture stopped");
    Ok(())
}

struct Source {
    track: AudioTrack,
    rx: Receiver<Vec<f32>>,
    buf: VecDeque<f32>,
}

impl Source {
    fn pull(&mut self, max: usize) {
        while let Ok(chunk) = self.rx.try_recv() {
            self.buf.extend(chunk);
        }
        self.keep_latest(max);
    }

    fn keep_latest(&mut self, max: usize) {
        if self.buf.len() > max {
            let excess = (self.buf.len() - max) / CHANNELS * CHANNELS; // whole frames
            self.buf.drain(..excess);
        }
    }
}

/// Which audio to record.
#[derive(Debug, Clone, Default)]
pub struct AudioConfig {
    pub system: bool,
    pub microphone: bool,
    /// `None` = Windows' default microphone.
    pub microphone_id: Option<String>,
}

/// Running audio capture. Dropping it stops all threads.
pub struct AudioPipeline {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    tracks: Vec<AudioTrack>,
    pub levels: Arc<Levels>,
    /// Problems that didn't stop the recording, e.g. "no microphone".
    pub warnings: Vec<String>,
}

impl AudioPipeline {
    /// Opens the devices and starts buffering. Samples are emitted to `sink`
    /// as `(track, 16-bit stereo PCM, time in 100 ns)` once the clock starts.
    pub fn start(
        config: &AudioConfig,
        clock: Arc<MediaClock>,
        sink: impl FnMut(AudioTrack, &[u8], i64) -> bool + Send + 'static,
    ) -> Result<Self, RecorderError> {
        let stop = Arc::new(AtomicBool::new(false));
        let levels = Arc::new(Levels::default());
        let mut threads = Vec::new();
        let mut sources = Vec::new();
        let mut warnings = Vec::new();

        let wanted = [
            (config.system, AudioTrack::System, None),
            (config.microphone, AudioTrack::Microphone, config.microphone_id.clone()),
        ];
        for (on, track, id) in wanted {
            if !on {
                continue;
            }
            match spawn_capture(track, id, Arc::clone(&stop), Arc::clone(&levels)) {
                Ok((handle, rx)) => {
                    threads.push(handle);
                    sources.push(Source { track, rx, buf: VecDeque::new() });
                }
                Err(e) => {
                    let msg = match track {
                        AudioTrack::System => format!("System audio unavailable: {e}"),
                        AudioTrack::Microphone => format!("Microphone unavailable: {e}"),
                    };
                    warn!("{msg}");
                    warnings.push(msg);
                }
            }
        }
        let tracks = sources.iter().map(|s| s.track).collect();

        if !sources.is_empty() {
            let pacer_stop = Arc::clone(&stop);
            let pacer = std::thread::Builder::new()
                .name("audio-pacer".into())
                .spawn(move || pace_loop(sources, &clock, &pacer_stop, sink))
                .map_err(|e| RecorderError::Audio(e.to_string()))?;
            threads.push(pacer);
        }

        Ok(Self { stop, threads, tracks, levels, warnings })
    }

    /// Tracks that actually started; the file gets one audio track for each.
    pub fn tracks(&self) -> &[AudioTrack] {
        &self.tracks
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for AudioPipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn pace_loop(
    mut sources: Vec<Source>,
    clock: &MediaClock,
    stop: &AtomicBool,
    mut sink: impl FnMut(AudioTrack, &[u8], i64) -> bool,
) {
    let mut produced: u64 = 0; // frames (one sample per channel), same for every track
    let mut out: Vec<u8> = Vec::new();
    let mut aligned = false;

    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(TICK);
        let max = if aligned { MAX_BUFFERED } else { MAX_BUFFERED_STARTUP };
        for s in &mut sources {
            s.pull(max);
        }

        // Before time zero: keep buffering.
        let Some(now) = clock.now() else { continue };

        if !aligned {
            // Buffers end "now"; drop whatever was captured before time zero.
            let since_t0 = (now.as_secs_f64() * f64::from(SAMPLE_RATE)) as usize * CHANNELS;
            for s in &mut sources {
                s.keep_latest(since_t0);
            }
            aligned = true;
        }

        if clock.is_paused() {
            for s in &mut sources {
                s.buf.clear();
            }
            continue;
        }

        let due = (now.as_secs_f64() * f64::from(SAMPLE_RATE)) as u64;
        let frames = due.saturating_sub(produced) as usize;
        if frames == 0 {
            continue;
        }
        let n = frames * CHANNELS;
        let time = (produced as i128 * 10_000_000 / i128::from(SAMPLE_RATE)) as i64;

        for s in &mut sources {
            let take = n.min(s.buf.len());
            out.clear();
            out.reserve(n * 2);
            for v in s.buf.drain(..take) {
                let i = (v.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                out.extend_from_slice(&i.to_le_bytes());
            }
            // Silence for whatever the device didn't deliver.
            out.resize(n * 2, 0);
            if !sink(s.track, &out, time) {
                debug!("audio sink closed");
                return;
            }
        }
        produced += frames as u64;
    }
}

/// Opens audio devices only to show level meters (no recording). Used before
/// recording so you can check the microphone. Dropping it closes the devices.
pub struct AudioMeter {
    pipeline: AudioPipeline,
}

impl AudioMeter {
    pub fn start(config: &AudioConfig) -> Result<Self, RecorderError> {
        // A clock that never starts: samples are buffered (bounded) and discarded.
        let pipeline = AudioPipeline::start(config, Arc::new(MediaClock::default()), |_, _, _| true)?;
        Ok(Self { pipeline })
    }

    /// Returns (system, microphone) peaks since the last call, 0.0–1.0.
    pub fn levels(&self) -> (f32, f32) {
        self.pipeline.levels.take()
    }

    pub fn warnings(&self) -> &[String] {
        &self.pipeline.warnings
    }
}
