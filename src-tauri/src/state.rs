//! App-wide recording state. The Rust side owns it; every UI surface (window,
//! tray, hotkeys) reads it from here and is told about changes through the
//! `recorder-state` event, so they never disagree.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use recorder_core::{AudioTrack, QualityPreset, RecordOptions, Recorder, RecordingSummary};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

use crate::tray;

/// What the user picked in the launcher. Used by the hotkey too.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub display: usize,
    pub preset: QualityPreset,
    pub system_audio: bool,
    pub microphone: bool,
    /// `None` = Windows' default microphone.
    pub microphone_id: Option<String>,
}

impl Default for Selection {
    fn default() -> Self {
        Self { display: 1, preset: QualityPreset::High, system_audio: true, microphone: false, microphone_id: None }
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

#[derive(Default)]
pub struct AppState {
    recorder: Mutex<Option<Recorder>>,
    ui: Mutex<Option<UiState>>,
    selection: Mutex<Selection>,
    mica: AtomicBool,
}

impl AppState {
    pub fn ui(&self) -> UiState {
        self.ui.lock().unwrap().clone().unwrap_or(UiState::Idle { last: None, error: None })
    }

    pub fn selection(&self) -> Selection {
        self.selection.lock().unwrap().clone()
    }

    pub fn set_selection(&self, selection: Selection) {
        *self.selection.lock().unwrap() = selection;
    }

    pub fn mica(&self) -> bool {
        self.mica.load(Ordering::Relaxed)
    }

    pub fn set_mica(&self, mica: bool) {
        self.mica.store(mica, Ordering::Relaxed);
    }

    /// Peak (system, microphone) levels since the last call.
    pub fn levels(&self) -> (f32, f32) {
        self.recorder.lock().unwrap().as_ref().map(|r| r.levels()).unwrap_or((0.0, 0.0))
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
    resize_main(app, &ui);
    if let Err(e) = app.emit("recorder-state", &ui) {
        tracing::warn!(error = %e, "failed to emit state");
    }
}

/// Full launcher while idle; a small control pill at the top centre while recording.
fn resize_main(app: &AppHandle, ui: &UiState) {
    let Some(window) = app.get_webview_window("main") else { return };
    let result = match ui {
        UiState::Idle { .. } => window.set_size(LogicalSize::new(380.0, 500.0)).and_then(|_| window.center()),
        UiState::Starting | UiState::Recording { .. } | UiState::Stopping => {
            let (w, h) = (300.0, 52.0);
            window.set_size(LogicalSize::new(w, h)).and_then(|_| {
                let monitor = window.current_monitor()?.or(window.primary_monitor()?);
                match monitor {
                    Some(m) => {
                        let scale = m.scale_factor();
                        let pos = m.position().to_logical::<f64>(scale);
                        let size = m.size().to_logical::<f64>(scale);
                        window.set_position(LogicalPosition::new(pos.x + (size.width - w) / 2.0, pos.y + 16.0))
                    }
                    None => Ok(()),
                }
            })
        }
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to resize window");
    }
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
    let options = RecordOptions {
        display: s.display,
        preset: s.preset,
        system_audio: s.system_audio,
        microphone: s.microphone,
        microphone_id: s.microphone_id,
        ..Default::default()
    };

    let app = app.clone();
    std::thread::spawn(move || {
        let ui = match Recorder::start(options) {
            Ok(recorder) => {
                let ui = recording_state(&recorder);
                *app.state::<AppState>().recorder.lock().unwrap() = Some(recorder);
                ui
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to start recording");
                UiState::Idle { last: None, error: Some(e.to_string()) }
            }
        };
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
