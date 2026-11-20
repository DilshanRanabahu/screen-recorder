use serde::Serialize;
use windows_capture::monitor::Monitor;

use crate::RecorderError;

/// A monitor that can be recorded.
#[derive(Debug, Clone, Serialize)]
pub struct Display {
    /// One-based index, as used by [`crate::RecordOptions::display`].
    pub index: usize,
    /// Friendly name, for example "Generic PnP Monitor".
    pub name: String,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Refresh rate in Hz.
    pub refresh_rate: u32,
}

/// Lists the connected monitors in Windows' display order.
pub fn list_displays() -> Result<Vec<Display>, RecorderError> {
    let monitors = Monitor::enumerate().map_err(|e| RecorderError::Displays(e.to_string()))?;

    Ok(monitors
        .iter()
        .enumerate()
        .map(|(i, m)| Display {
            index: i + 1,
            name: m
                .name()
                .or_else(|_| m.device_string())
                .unwrap_or_else(|_| format!("Display {}", i + 1)),
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            refresh_rate: m.refresh_rate().unwrap_or(0),
        })
        .collect())
}
