// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod logging;
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
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ui_info,
            commands::list_displays,
            commands::list_microphones,
            commands::get_levels,
            commands::get_state,
            commands::get_selection,
            commands::set_selection,
            commands::start_recording,
            commands::stop_recording,
            commands::toggle_pause,
            commands::open_path,
            commands::open_recordings_folder,
            commands::hide_window,
        ])
        .setup(|app| {
            let handle = app.handle();

            if let Some(window) = app.get_webview_window("main") {
                let mica = window_vibrancy::apply_mica(&window, None).is_ok();
                if !mica {
                    tracing::warn!("Mica backdrop unavailable; using a solid background");
                }
                app.state::<AppState>().set_mica(mica);
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
