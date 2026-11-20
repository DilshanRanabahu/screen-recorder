//! Commands called from the frontend (`src/lib/api.ts`).

use std::path::Path;
use std::process::Command;
use std::time::UNIX_EPOCH;

use recorder_core::{AudioDevice, Display};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};

use crate::state::{self, AppState, Selection, UiState, View};

#[derive(Serialize)]
pub struct UiInfo {
    hotkey: &'static str,
    pause_hotkey: &'static str,
    output_dir: String,
}

#[tauri::command]
pub fn ui_info() -> UiInfo {
    UiInfo {
        hotkey: crate::HOTKEY_LABEL,
        pause_hotkey: crate::PAUSE_HOTKEY_LABEL,
        output_dir: recorder_core::default_output_dir().display().to_string(),
    }
}

#[tauri::command]
pub fn list_displays() -> Result<Vec<Display>, String> {
    recorder_core::list_displays().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_microphones() -> Result<Vec<AudioDevice>, String> {
    recorder_core::list_microphones().map_err(|e| e.to_string())
}

/// Peak (system, microphone) levels since the last call, 0.0–1.0.
#[tauri::command]
pub fn get_levels(state: State<'_, AppState>) -> (f32, f32) {
    state.levels()
}

/// Turns the idle level meters (mic check) on or off.
#[tauri::command]
pub fn set_monitoring(state: State<'_, AppState>, on: bool) {
    state.set_monitoring(on);
}

/// Starts streaming the live preview of `display` to `channel`. Each message is
/// a 9-byte header followed by JPEG (or raw RGBA) data; see
/// `AppState::start_preview`. The UI calls `preview_ack` after drawing each
/// frame; at most two frames are in flight, so a busy UI never builds a backlog.
#[tauri::command]
pub async fn start_preview(
    state: State<'_, AppState>,
    display: usize,
    max_width: u32,
    fps: u32,
    raw: bool,
    channel: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    state.start_preview(display, max_width, fps, raw, channel).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_preview(state: State<'_, AppState>) -> Result<(), String> {
    state.stop_preview();
    Ok(())
}

#[tauri::command]
pub fn set_preview_fps(state: State<'_, AppState>, fps: u32) {
    state.set_preview_fps(fps);
}

#[tauri::command]
pub fn preview_ack(state: State<'_, AppState>) {
    state.preview_ack();
}

#[derive(Serialize)]
pub struct RecordingFile {
    name: String,
    path: String,
    bytes: u64,
    modified_ms: u64,
}

/// Most recent recordings in the output folder, newest first.
#[tauri::command]
pub fn list_recordings(limit: usize) -> Vec<RecordingFile> {
    let dir = recorder_core::default_output_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut files: Vec<RecordingFile> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4")))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified_ms = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
            Some(RecordingFile {
                name: e.file_name().to_string_lossy().into_owned(),
                path: e.path().display().to_string(),
                bytes: meta.len(),
                modified_ms,
            })
        })
        .collect();
    files.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    files.truncate(limit);
    files
}

/// Free space on the recordings drive, in bytes.
#[tauri::command]
pub fn storage_free() -> Option<u64> {
    recorder_core::free_space(&recorder_core::default_output_dir())
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> UiState {
    state.ui()
}

#[tauri::command]
pub fn get_selection(state: State<'_, AppState>) -> Selection {
    state.selection()
}

#[tauri::command]
pub fn set_selection(state: State<'_, AppState>, selection: Selection) {
    state.set_selection(selection);
}

#[tauri::command]
pub fn start_recording(app: AppHandle) -> Result<(), String> {
    state::start_recording(&app)
}

#[tauri::command]
pub fn stop_recording(app: AppHandle) -> Result<(), String> {
    state::stop_recording(&app)
}

#[tauri::command]
pub fn toggle_pause(app: AppHandle) -> Result<(), String> {
    state::toggle_pause(&app)
}

#[tauri::command]
pub fn get_view(state: State<'_, AppState>) -> View {
    state.view()
}

#[tauri::command]
pub fn set_view(app: AppHandle, view: View) {
    state::set_view(&app, view);
}

/// Opens a recording in the default player, or selects it in Explorer.
/// Only paths inside the recordings folder are accepted.
#[tauri::command]
pub fn open_path(path: String, reveal: bool) -> Result<(), String> {
    let path = Path::new(&path);
    let root = recorder_core::default_output_dir();
    let inside = path
        .canonicalize()
        .ok()
        .zip(root.canonicalize().ok())
        .is_some_and(|(p, r)| p.starts_with(r));
    if !inside {
        return Err("That file is not in the recordings folder.".into());
    }

    let result = if reveal {
        Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn()
    } else {
        Command::new("explorer.exe").arg(path).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_recordings_folder() {
    open_folder(&recorder_core::default_output_dir());
}

pub fn open_folder(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    if let Err(e) = Command::new("explorer.exe").arg(dir).spawn() {
        tracing::warn!(error = %e, "failed to open folder");
    }
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn minimize_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }
}

#[tauri::command]
pub fn toggle_maximize(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let maximized = window.is_maximized().unwrap_or(false);
        let _ = if maximized { window.unmaximize() } else { window.maximize() };
    }
}
