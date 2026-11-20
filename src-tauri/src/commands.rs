//! Commands called from the frontend (`src/lib/api.ts`).

use std::path::Path;
use std::process::Command;

use recorder_core::{AudioDevice, Display};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::state::{self, AppState, Selection, UiState};

#[derive(Serialize)]
pub struct UiInfo {
    mica: bool,
    hotkey: &'static str,
    pause_hotkey: &'static str,
}

#[tauri::command]
pub fn ui_info(state: State<'_, AppState>) -> UiInfo {
    UiInfo { mica: state.mica(), hotkey: crate::HOTKEY_LABEL, pause_hotkey: crate::PAUSE_HOTKEY_LABEL }
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
