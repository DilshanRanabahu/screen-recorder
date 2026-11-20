//! System tray icon. Idle: grey dot, click opens the launcher.
//! Recording: red dot, click stops (docs/UI-DESIGN.md, section 4.7).

use std::sync::OnceLock;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

use crate::state::{self, AppState};
use crate::{HOTKEY_LABEL, PAUSE_HOTKEY_LABEL};

const TRAY_ID: &str = "main";
static TOGGLE_ITEM: OnceLock<MenuItem<Wry>> = OnceLock::new();
static PAUSE_ITEM: OnceLock<MenuItem<Wry>> = OnceLock::new();

/// Draws a filled, anti-aliased circle, so no icon files are needed.
fn dot_icon(rgb: [u8; 3]) -> Image<'static> {
    const SIZE: u32 = 32;
    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = 11.0;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let alpha = (radius + 0.5 - d).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (alpha * 255.0) as u8]);
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}

fn idle_icon() -> Image<'static> {
    dot_icon([0x9a, 0x9a, 0x9a])
}

fn recording_icon() -> Image<'static> {
    dot_icon([0xe5, 0x48, 0x4d])
}

fn paused_icon() -> Image<'static> {
    dot_icon([0xf5, 0xa5, 0x24])
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Start recording", true, Some(HOTKEY_LABEL))?;
    let pause = MenuItem::with_id(app, "pause", "Pause", false, Some(PAUSE_HOTKEY_LABEL))?;
    let show = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Open recordings folder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&toggle, &pause, &show, &folder, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let _ = TOGGLE_ITEM.set(toggle);
    let _ = PAUSE_ITEM.set(pause);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(idle_icon())
        .tooltip("Screen Recorder")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => state::toggle_recording(app),
            "pause" => {
                let _ = state::toggle_pause(app);
            }
            "show" => state::show_main(app),
            "folder" => crate::commands::open_folder(&recorder_core::default_output_dir()),
            "quit" => state::quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                use tauri::Manager;
                match app.state::<AppState>().ui() {
                    state::UiState::Recording { .. } => state::toggle_recording(app),
                    _ => state::show_main(app),
                }
            }
        })
        .build(app)?;
    Ok(())
}

pub fn update(app: &AppHandle, recording: bool, paused: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let icon = match (recording, paused) {
            (true, true) => paused_icon(),
            (true, false) => recording_icon(),
            _ => idle_icon(),
        };
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_tooltip(Some(match (recording, paused) {
            (true, true) => "Screen Recorder · Paused",
            (true, false) => "Screen Recorder · Recording (click to stop)",
            _ => "Screen Recorder",
        }));
    }
    if let Some(item) = TOGGLE_ITEM.get() {
        let _ = item.set_text(if recording { "Stop recording" } else { "Start recording" });
    }
    if let Some(item) = PAUSE_ITEM.get() {
        let _ = item.set_enabled(recording);
        let _ = item.set_text(if paused { "Resume" } else { "Pause" });
    }
}
