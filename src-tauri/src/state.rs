//! App-wide recording state. The Rust side owns it; every UI surface (window,
//! tray, hotkeys) reads it from here and is told about changes through the
//! `recorder-state` event, so they never disagree.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use recorder_core::{
    AudioConfig, AudioMeter, AudioTrack, PreviewEncoding, PreviewSession, QualityPreset, RecordOptions, Recorder, RecordingSummary,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize};

use crate::tray;

/// What the user picked in the studio. Used by the hotkey too, and saved
/// between runs (see `settings.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Selection {
    pub display: usize,
    pub preset: QualityPreset,
    pub system_audio: bool,
    pub microphone: bool,
    /// `None` = Windows' default microphone.
    pub microphone_id: Option<String>,
    /// Live preview in the studio (off saves CPU; recording is unaffected).
    pub preview: bool,
    /// Record a whole display or a single window.
    pub capture_mode: CaptureMode,
    /// The window to record in `Window` mode. The id changes between runs, so the
    /// UI re-finds the window by app and title after a restart.
    pub window: Option<WindowChoice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    #[default]
    Screen,
    Window,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowChoice {
    pub id: u64,
    pub title: String,
    pub process: String,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            display: 1,
            preset: QualityPreset::High,
            system_audio: true,
            microphone: false,
            microphone_id: None,
            preview: true,
            capture_mode: CaptureMode::Screen,
            window: None,
        }
    }
}

/// Sent to the frontend as the `recorder-state` event.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum UiState {
    Idle {
        last: Option<RecordingSummary>,
        error: Option<String>,
    },
    Recording {
        /// Recorded time (pauses excluded) at `at_ms`.
        elapsed_ms: u64,
        /// Wall-clock time this state was sent, in ms since the Unix epoch.
        at_ms: u64,
        paused: bool,
        path: String,
        tracks: Vec<AudioTrack>,
        warnings: Vec<String>,
    },
    Starting,
    Stopping,
}

/// Which layout the main window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    /// Full window: preview, sources, mixer, controls.
    #[default]
    Studio,
    /// Small always-on-top control bar while recording.
    Pill,
}

#[derive(Default)]
pub struct AppState {
    /// Where settings are saved; `None` if there is no config folder.
    settings_path: Option<PathBuf>,
    recorder: Mutex<Option<Recorder>>,
    ui: Mutex<Option<UiState>>,
    selection: Mutex<Selection>,
    view: Mutex<View>,
    /// Studio window position and size, restored when leaving the pill.
    studio_bounds: Mutex<Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>>,
    /// Level meters while idle (mic check). Closed while recording.
    meter: Mutex<Option<AudioMeter>>,
    monitoring: AtomicBool,
    /// Codecs whose hardware encoder has been warmed up (see `warm_up_encoder`).
    warmed: Arc<Mutex<Vec<recorder_core::Codec>>>,
    /// Live preview stream (GPU capture → small JPEGs → UI). At most one runs.
    preview: Mutex<Option<ActivePreview>>,
    /// Source of preview session ids.
    next_preview_id: AtomicU32,
    /// Listens for microphones being plugged in or out (kept alive here).
    device_watcher: Mutex<Option<recorder_core::AudioDeviceWatcher>>,
}

/// The running preview and what identifies it.
///
/// Every session gets an id. `stop_preview` and `preview_ack` name the session
/// they mean, so a late "stop" or "ack" from a previous session (for example
/// right after switching displays) can never affect the new one.
struct ActivePreview {
    id: u32,
    display: usize,
    session: PreviewSession,
    /// Frames sent to the UI but not yet drawn.
    in_flight: Arc<AtomicU32>,
}

impl AppState {
    /// Starts with saved choices; later changes are written to `settings_path`.
    pub fn new(settings_path: Option<PathBuf>, selection: Selection) -> Self {
        Self { settings_path, selection: Mutex::new(selection), ..Default::default() }
    }

    pub fn ui(&self) -> UiState {
        self.ui.lock().unwrap().clone().unwrap_or(UiState::Idle { last: None, error: None })
    }

    pub fn selection(&self) -> Selection {
        self.selection.lock().unwrap().clone()
    }

    pub fn set_selection(&self, selection: Selection) {
        let changed = {
            let mut current = self.selection.lock().unwrap();
            let changed = *current != selection;
            *current = selection.clone();
            changed
        };
        if !changed {
            return;
        }
        self.warm_up_encoder(selection.preset.codec());
        if let Some(path) = &self.settings_path {
            let settings = crate::settings::Settings { selection, ..Default::default() };
            if let Err(e) = crate::settings::save(path, &settings) {
                tracing::warn!(error = %e, "failed to save settings");
            }
        }
        self.refresh_meter();
    }

    pub fn view(&self) -> View {
        *self.view.lock().unwrap()
    }

    /// Peak (system, microphone) levels since the last call: from the
    /// recording if there is one, otherwise from the idle meter.
    pub fn levels(&self) -> (f32, f32) {
        if let Some(r) = self.recorder.lock().unwrap().as_ref() {
            return r.levels();
        }
        self.meter.lock().unwrap().as_ref().map(|m| m.levels()).unwrap_or((0.0, 0.0))
    }

    /// Turns the idle level meters on or off (the UI does this while visible).
    pub fn set_monitoring(&self, on: bool) {
        self.monitoring.store(on, Ordering::Relaxed);
        self.refresh_meter();
    }

    /// (Re)opens the meter devices to match the selection, or closes them.
    pub fn refresh_meter(&self) {
        let mut meter = self.meter.lock().unwrap();
        *meter = None; // close devices first
        let idle = self.recorder.lock().unwrap().is_none();
        let s = self.selection();
        if !(idle && self.monitoring.load(Ordering::Relaxed) && (s.system_audio || s.microphone)) {
            return;
        }
        let config = AudioConfig { system: s.system_audio, microphone: s.microphone, microphone_id: s.microphone_id };
        match AudioMeter::start(&config) {
            Ok(m) => *meter = Some(m),
            Err(e) => tracing::warn!(error = %e, "failed to start level meters"),
        }
    }

    /// Initializes the hardware encoder for `codec` on a background thread, once
    /// per codec. The first recording in a process otherwise takes 1.7–3.9 s to
    /// start instead of about 0.3 s.
    pub fn warm_up_encoder(&self, codec: recorder_core::Codec) {
        {
            let mut warmed = self.warmed.lock().unwrap();
            if warmed.contains(&codec) {
                return;
            }
            warmed.push(codec);
        }
        let warmed = Arc::clone(&self.warmed);
        std::thread::spawn(move || match recorder_core::warm_up_encoder(codec) {
            Ok(took) => tracing::info!(?codec, ms = took.as_millis() as u64, "encoder warmed up"),
            Err(e) => {
                tracing::warn!(?codec, error = %e, "encoder warm-up failed");
                warmed.lock().unwrap().retain(|c| *c != codec);
            }
        });
    }

    fn close_meter(&self) {
        *self.meter.lock().unwrap() = None;
    }

    /// Starts the live preview of `display`, replacing any running one, and
    /// returns the new session's id.
    pub fn start_preview(
        &self,
        display: usize,
        window: Option<u64>,
        max_width: u32,
        fps: u32,
        raw: bool,
        channel: Channel<InvokeResponseBody>,
    ) -> Result<u32, recorder_core::RecorderError> {
        // (`display` is a helper name inside tracing macros, so log `screen`.)
        let screen = display;
        let encoding = if raw { PreviewEncoding::Raw } else { PreviewEncoding::Jpeg };
        let id = self.next_preview_id.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let mut preview = self.preview.lock().unwrap();
        if let Some(old) = preview.take() {
            tracing::debug!(old_id = old.id, old_screen = old.display, new_id = id, screen, "preview replaced");
            old.session.stop();
        }
        let in_flight = Arc::new(AtomicU32::new(0));
        let frames_in_flight = Arc::clone(&in_flight);
        let (mut sent, mut dropped, mut bytes, mut since) = (0u32, 0u32, 0usize, std::time::Instant::now());
        let (started, mut first_frame) = (std::time::Instant::now(), true);
        let session = PreviewSession::start(display, window, max_width, fps.clamp(1, 60), encoding, move |frame| {
            let in_flight = &frames_in_flight;
            if first_frame {
                first_frame = false;
                tracing::debug!(id, screen, after_ms = started.elapsed().as_millis() as u64, "preview first frame");
            }
            if since.elapsed().as_secs() >= 5 {
                let secs = since.elapsed().as_secs_f32();
                tracing::debug!(
                    id,
                    screen,
                    fps = format!("{:.1}", sent as f32 / secs),
                    dropped,
                    avg_kb = if sent > 0 { bytes / sent as usize / 1024 } else { 0 },
                    size = ?(frame.width, frame.height),
                    "preview stream"
                );
                (sent, dropped, bytes, since) = (0, 0, 0, std::time::Instant::now());
            }
            // Drop the frame if the UI is still busy with two earlier ones.
            if in_flight.fetch_add(1, Ordering::AcqRel) >= 2 {
                in_flight.fetch_sub(1, Ordering::AcqRel);
                dropped += 1;
                return;
            }
            // Header (13 bytes): [encoding u8: 0 = JPEG, 1 = RGBA][width u32 LE]
            // [height u32 LE][session id u32 LE], then the data.
            let mut msg = Vec::with_capacity(13 + frame.data.len());
            msg.push(u8::from(frame.encoding == PreviewEncoding::Raw));
            msg.extend_from_slice(&frame.width.to_le_bytes());
            msg.extend_from_slice(&frame.height.to_le_bytes());
            msg.extend_from_slice(&id.to_le_bytes());
            msg.extend_from_slice(frame.data);
            if channel.send(InvokeResponseBody::Raw(msg)).is_err() {
                in_flight.fetch_sub(1, Ordering::AcqRel);
            } else {
                sent += 1;
                bytes += frame.data.len();
            }
        })?;
        tracing::debug!(id, screen, "preview session active");
        *preview = Some(ActivePreview { id, display, session, in_flight });
        Ok(id)
    }

    pub fn set_preview_fps(&self, fps: u32) {
        tracing::debug!(fps, "preview rate");
        if let Some(p) = self.preview.lock().unwrap().as_ref() {
            p.session.set_fps(fps);
        }
    }

    /// Stops preview session `id`. A stop for an older session (one already
    /// replaced by a newer `start_preview`) is ignored.
    pub fn stop_preview(&self, id: u32) {
        let mut preview = self.preview.lock().unwrap();
        match preview.as_ref() {
            Some(p) if p.id == id => {
                let p = preview.take().expect("checked above");
                tracing::debug!(id, screen = p.display, "preview stopped");
                p.session.stop();
            }
            Some(p) => tracing::debug!(stale_id = id, current_id = p.id, "ignored stop for an older preview session"),
            None => {}
        }
    }

    /// The UI finished drawing a frame of session `id`.
    pub fn preview_ack(&self, id: u32) {
        if let Some(p) = self.preview.lock().unwrap().as_ref()
            && p.id == id
        {
            let _ = p.in_flight.try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn recording_state(recorder: &Recorder) -> UiState {
    UiState::Recording {
        elapsed_ms: recorder.elapsed().as_millis() as u64,
        at_ms: now_ms(),
        paused: recorder.is_paused(),
        path: recorder.path().display().to_string(),
        tracks: recorder.audio_tracks().to_vec(),
        warnings: recorder.warnings().to_vec(),
    }
}

/// Stores the new state and updates every surface.
fn set_ui(app: &AppHandle, ui: UiState) {
    let state = app.state::<AppState>();
    *state.ui.lock().unwrap() = Some(ui.clone());

    let (recording, paused) = match &ui {
        UiState::Recording { paused, .. } => (true, *paused),
        UiState::Starting | UiState::Stopping => (true, false),
        UiState::Idle { .. } => (false, false),
    };
    tray::update(app, recording, paused);
    if let Err(e) = app.emit("recorder-state", &ui) {
        tracing::warn!(error = %e, "failed to emit state");
    }
}

pub const STUDIO_MIN: (f64, f64) = (880.0, 580.0);
const STUDIO_DEFAULT: (f64, f64) = (1120.0, 700.0);
const PILL_SIZE: (f64, f64) = (340.0, 56.0);

/// Switches the main window between the studio layout and the recording pill.
pub fn set_view(app: &AppHandle, view: View) {
    let state = app.state::<AppState>();
    let Some(window) = app.get_webview_window("main") else { return };
    let previous = std::mem::replace(&mut *state.view.lock().unwrap(), view);

    let result: tauri::Result<()> = (|| {
        match view {
            View::Pill => {
                if previous == View::Studio {
                    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
                        tracing::debug!(?pos, ?size, maximized = ?window.is_maximized(), "saving studio bounds");
                        *state.studio_bounds.lock().unwrap() = Some((pos, size));
                    }
                }
                window.unmaximize()?;
                window.set_min_size(None::<LogicalSize<f64>>)?;
                window.set_resizable(false)?;
                window.set_always_on_top(true)?;
                window.set_size(LogicalSize::new(PILL_SIZE.0, PILL_SIZE.1))?;
                if let Some(m) = window.current_monitor()?.or(window.primary_monitor()?) {
                    let scale = m.scale_factor();
                    let pos = m.position().to_logical::<f64>(scale);
                    let size = m.size().to_logical::<f64>(scale);
                    window.set_position(LogicalPosition::new(pos.x + (size.width - PILL_SIZE.0) / 2.0, pos.y + 14.0))?;
                }
            }
            View::Studio => {
                window.set_always_on_top(false)?;
                window.set_resizable(true)?;
                window.set_min_size(Some(LogicalSize::new(STUDIO_MIN.0, STUDIO_MIN.1)))?;
                match *state.studio_bounds.lock().unwrap() {
                    Some((pos, size)) if previous == View::Pill => {
                        tracing::debug!(?pos, ?size, "restoring studio bounds");
                        window.set_size(size)?;
                        window.set_position(pos)?;
                    }
                    Some(_) => {}
                    None => {
                        window.set_size(LogicalSize::new(STUDIO_DEFAULT.0, STUDIO_DEFAULT.1))?;
                        window.center()?;
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to switch view");
    }
    let _ = app.emit("view", view);
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Switches to `Starting` right away (so a second press is ignored), then
/// starts on a background thread: waiting for the encoder takes 1–2 s, and
/// the caller may be the UI or hotkey thread. Errors arrive as an Idle state.
pub fn start_recording(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut ui = state.ui.lock().unwrap();
        if !matches!(ui.as_ref(), None | Some(UiState::Idle { .. })) {
            return Err("A recording is already in progress.".into());
        }
        *ui = Some(UiState::Starting);
    }
    set_ui(app, UiState::Starting);

    let s = state.selection();
    let window = match (s.capture_mode, &s.window) {
        (CaptureMode::Screen, _) => None,
        (CaptureMode::Window, Some(w)) => Some(w.id),
        (CaptureMode::Window, None) => {
            let ui = UiState::Idle { last: None, error: Some("Choose a window to record first.".into()) };
            set_ui(app, ui);
            return Err("Choose a window to record first.".into());
        }
    };
    let options = RecordOptions {
        display: s.display,
        window,
        preset: s.preset,
        system_audio: s.system_audio,
        microphone: s.microphone,
        microphone_id: s.microphone_id,
        ..Default::default()
    };

    let app = app.clone();
    std::thread::spawn(move || {
        // Release the devices the idle meter holds before recording opens them.
        app.state::<AppState>().close_meter();
        let ui = match Recorder::start(options) {
            Ok(recorder) => {
                let ui = recording_state(&recorder);
                *app.state::<AppState>().recorder.lock().unwrap() = Some(recorder);
                set_view(&app, View::Pill);
                watch_source_closed(&app);
                ui
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to start recording");
                UiState::Idle { last: None, error: Some(e.to_string()) }
            }
        };
        if matches!(ui, UiState::Idle { .. }) {
            app.state::<AppState>().refresh_meter();
        }
        set_ui(&app, ui);
        show_main(&app);
    });
    Ok(())
}

/// Stops on a background thread, because flushing the encoder can take a moment.
pub fn stop_recording(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let Some(recorder) = state.recorder.lock().unwrap().take() else {
        return Err("Nothing is being recorded.".into());
    };
    set_ui(app, UiState::Stopping);

    let app = app.clone();
    std::thread::spawn(move || {
        let ui = match recorder.stop() {
            Ok(summary) => UiState::Idle { last: Some(summary), error: None },
            Err(e) => {
                tracing::error!(error = %e, "failed to finish recording");
                UiState::Idle { last: None, error: Some(e.to_string()) }
            }
        };
        set_ui(&app, ui);
        set_view(&app, View::Studio);
        app.state::<AppState>().refresh_meter();
        show_main(&app);
    });
    Ok(())
}

pub fn toggle_pause(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let ui = {
        let guard = state.recorder.lock().unwrap();
        let Some(recorder) = guard.as_ref() else {
            return Err("Nothing is being recorded.".into());
        };
        if recorder.is_paused() {
            recorder.resume();
        } else {
            recorder.pause();
        }
        recording_state(recorder)
    };
    set_ui(app, ui);
    Ok(())
}

pub fn toggle_recording(app: &AppHandle) {
    let result = match app.state::<AppState>().ui() {
        UiState::Idle { .. } => start_recording(app),
        UiState::Recording { .. } => stop_recording(app),
        UiState::Starting | UiState::Stopping => Ok(()),
    };
    if let Err(e) = result {
        set_ui(app, UiState::Idle { last: None, error: Some(e) });
        show_main(app);
    }
}

/// Finishes any recording in progress, then exits. Used by the tray's Quit.
pub fn quit(app: &AppHandle) {
    if let Some(recorder) = app.state::<AppState>().recorder.lock().unwrap().take() {
        tracing::info!("finishing recording before quitting");
        if let Err(e) = recorder.stop() {
            tracing::error!(error = %e, "failed to finish recording on quit");
        }
    }
    app.exit(0);
}

/// Starts listening for audio devices being plugged in or out. On a change
/// the UI re-reads the microphone list (`audio-devices-changed`), and the idle
/// level meter is reopened so it follows the chosen or default microphone.
pub fn watch_audio_devices(app: &AppHandle) {
    let handle = app.clone();
    match recorder_core::watch_audio_devices(move || {
        if let Err(e) = handle.emit("audio-devices-changed", ()) {
            tracing::debug!(error = %e, "failed to emit audio-devices-changed");
        }
        handle.state::<AppState>().refresh_meter();
    }) {
        Ok(watcher) => *app.state::<AppState>().device_watcher.lock().unwrap() = Some(watcher),
        Err(e) => tracing::warn!(error = %e, "audio device notifications unavailable; the mic list updates only on restart"),
    }
}

/// Stops and saves the recording if its window is closed (or its display is
/// disconnected). Without this the video would keep repeating the last frame.
fn watch_source_closed(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let closed = match app.state::<AppState>().recorder.lock().unwrap().as_ref() {
                Some(r) => r.source_closed(),
                None => return, // recording already stopped
            };
            if closed {
                tracing::info!("recorded window closed; stopping the recording");
                let _ = stop_recording(&app);
                return;
            }
        }
    });
}
