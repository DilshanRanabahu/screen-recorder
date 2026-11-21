// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod logging;
mod settings;
mod state;
mod tray;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use state::AppState;

/// Start/stop hotkey. Ctrl+Shift+R is avoided because browsers use it for hard reload,
/// and Win+Shift+R belongs to the Snipping Tool.
pub const HOTKEY: &str = "ctrl+alt+r";
pub const HOTKEY_LABEL: &str = "Ctrl+Alt+R";
pub const PAUSE_HOTKEY: &str = "ctrl+alt+p";
pub const PAUSE_HOTKEY_LABEL: &str = "Ctrl+Alt+P";

fn main() {
    let _log_guard = logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    let result = tauri::Builder::default()
        // A second launch just brings the existing window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            state::show_main(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(load_state())
        .invoke_handler(tauri::generate_handler![
            commands::ui_info,
            commands::list_displays,
            commands::list_microphones,
            commands::get_levels,
            commands::set_monitoring,
            commands::start_preview,
            commands::stop_preview,
            commands::preview_ack,
            commands::set_preview_fps,
            commands::list_recordings,
            commands::storage_free,
            commands::get_state,
            commands::get_selection,
            commands::set_selection,
            commands::start_recording,
            commands::stop_recording,
            commands::toggle_pause,
            commands::get_view,
            commands::set_view,
            commands::open_path,
            commands::open_recordings_folder,
            commands::hide_window,
            commands::minimize_window,
            commands::toggle_maximize,
        ])
        .setup(|app| {
            let handle = app.handle();

            if let Some(window) = app.get_webview_window("main") {
                // Developer aid: make the window visible to screenshots/recordings.
                if std::env::var_os("SR_SHOW_IN_CAPTURE").is_some() {
                    tracing::warn!("SR_SHOW_IN_CAPTURE set: the app window will appear in captures");
                    window.set_content_protected(false)?;
                }
                state::set_view(handle, state::View::Studio);
                window.show()?;
            }

            tray::create(handle)?;

            handle.global_shortcut().on_shortcut(HOTKEY, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    state::toggle_recording(app);
                }
            })?;
            handle.global_shortcut().on_shortcut(PAUSE_HOTKEY, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    let _ = state::toggle_pause(app);
                }
            })?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it to the tray; Quit is in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        tracing::error!(error = %e, "app failed");
    }
}

/// Builds the app state from saved settings, fixing any choice that no longer
/// matches the hardware (unplugged monitor or microphone).
fn load_state() -> AppState {
    let path = settings::default_path();
    let mut saved = path.as_deref().map(settings::load).unwrap_or_default();
    // Enumerate hardware on a helper thread: WASAPI puts its thread into the
    // multithreaded COM mode, but the main thread must stay single-threaded
    // for the window (OLE drag and drop), or startup panics.
    let (displays, mics) = std::thread::spawn(|| {
        let displays = recorder_core::list_displays().map(|d| d.len()).unwrap_or(1);
        let mics: Vec<String> = recorder_core::list_microphones()
            .map(|m| m.into_iter().map(|d| d.id).collect())
            .unwrap_or_default();
        (displays, mics)
    })
    .join()
    .unwrap_or((1, Vec::new()));
    settings::validate(&mut saved.selection, displays, &mics);
    tracing::info!(selection = ?saved.selection, "settings loaded");
    AppState::new(path, saved.selection)
}
