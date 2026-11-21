//! Lists top-level windows that are worth recording.

use serde::Serialize;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow};
use windows_capture::window::Window;

use crate::RecorderError;

/// A window that can be recorded.
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    /// The window handle. Valid until the window closes; not stable across runs.
    pub id: u64,
    pub title: String,
    /// Executable name, e.g. `chrome.exe`.
    pub process: String,
    /// Size in physical pixels, including the window frame.
    pub width: u32,
    pub height: u32,
    /// Minimized windows can be selected, but deliver no frames until restored.
    pub minimized: bool,
}

/// Visible, titled top-level windows, in Windows' z-order (front first).
///
/// Skips windows that are "cloaked" (UWP apps that are suspended or on
/// another virtual desktop look visible but are not shown), tiny windows,
/// and windows of this process (the recorder's own UI).
pub fn list_windows() -> Result<Vec<WindowInfo>, RecorderError> {
    let own_pid = std::process::id();
    let windows = Window::enumerate().map_err(|e| RecorderError::Capture(format!("list windows: {e}")))?;
    Ok(windows
        .into_iter()
        .filter_map(|w| {
            let hwnd = HWND(w.as_raw_hwnd());
            if w.process_id().ok()? == own_pid || is_cloaked(hwnd) {
                return None;
            }
            let title = w.title().ok()?.trim().to_string();
            if title.is_empty() {
                return None;
            }
            // Visible size (what gets recorded): the plain window rect also counts
            // invisible resize borders, about 8 px per side.
            let (width, height) = visible_size(hwnd).unwrap_or((w.width().ok()?, w.height().ok()?));
            let minimized = unsafe { IsIconic(hwnd) }.as_bool();
            if !minimized && (width < 50 || height < 50) {
                return None;
            }
            Some(WindowInfo {
                id: hwnd.0 as u64,
                title,
                process: w.process_name().unwrap_or_default(),
                width: width.max(0) as u32,
                height: height.max(0) as u32,
                minimized,
            })
        })
        .collect())
}

/// True while the window still exists.
pub fn window_exists(id: u64) -> bool {
    unsafe { IsWindow(Some(HWND(id as *mut _))) }.as_bool()
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), std::mem::size_of::<u32>() as u32)
    }
    .is_ok()
        && cloaked != 0
}

/// The window's visible size in physical pixels (DWM extended frame bounds).
fn visible_size(hwnd: HWND) -> Option<(i32, i32)> {
    let mut r = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut r).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    Some((r.right - r.left, r.bottom - r.top))
}
