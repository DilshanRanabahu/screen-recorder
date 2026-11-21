//! Settings saved between runs, as JSON in
//! `%APPDATA%\ScreenRecorder\settings.json`.
//!
//! - Missing fields fall back to defaults (`#[serde(default)]`), so files
//!   written by older versions keep loading.
//! - Writes go to a temporary file that is then renamed over the old one, so
//!   a crash mid-write never leaves a half-written file.
//! - A file that can't be parsed is renamed to `settings.json.bak` and the
//!   app starts with defaults instead of failing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::state::Selection;

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// Display, quality preset and audio choices.
    pub selection: Selection,
}

impl Default for Settings {
    fn default() -> Self {
        Self { version: CURRENT_VERSION, selection: Selection::default() }
    }
}

/// `%APPDATA%\ScreenRecorder\settings.json`
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ScreenRecorder").join("settings.json"))
}

/// Loads settings from `path`, or returns defaults if the file is missing or
/// unreadable (a broken file is kept as `.bak` for inspection).
pub fn load(path: &Path) -> Settings {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "could not read settings; using defaults");
            return Settings::default();
        }
    };
    // Editors like Windows PowerShell 5 and old Notepad add a UTF-8 byte-order mark.
    match serde_json::from_str::<Settings>(text.trim_start_matches('\u{feff}')) {
        Ok(s) => s,
        Err(e) => {
            let backup = path.with_extension("json.bak");
            tracing::warn!(error = %e, backup = %backup.display(), "settings file is invalid; using defaults");
            let _ = std::fs::rename(path, &backup);
            Settings::default()
        }
    }
}

/// Saves settings atomically (temp file + rename).
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Fixes choices that no longer match the hardware, e.g. a monitor or
/// microphone that was unplugged since the settings were saved.
pub fn validate(selection: &mut Selection, display_count: usize, mic_ids: &[String]) {
    if selection.display == 0 || selection.display > display_count.max(1) {
        tracing::info!(saved = selection.display, "saved display not found; using display 1");
        selection.display = 1;
    }
    if let Some(id) = &selection.microphone_id
        && !mic_ids.contains(id)
    {
        tracing::info!("saved microphone not found; using the Windows default");
        selection.microphone_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recorder_core::QualityPreset;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sr-settings-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(load(&temp_path("missing")), Settings::default());
    }

    #[test]
    fn round_trip() {
        let path = temp_path("roundtrip");
        let mut s = Settings::default();
        s.selection.display = 2;
        s.selection.preset = QualityPreset::Maximum;
        s.selection.microphone = true;
        s.selection.microphone_id = Some("{mic-id}".into());
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
        assert!(!path.with_extension("json.tmp").exists(), "temp file must be renamed away");
    }

    #[test]
    fn missing_fields_use_defaults() {
        let path = temp_path("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{ "selection": { "preset": "standard" } }"#).unwrap();
        let s = load(&path);
        assert_eq!(s.selection.preset, QualityPreset::Standard);
        assert_eq!(s.selection.display, 1);
        assert!(s.selection.system_audio);
    }

    #[test]
    fn utf8_bom_is_accepted() {
        let path = temp_path("bom");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "\u{feff}{ \"selection\": { \"preset\": \"maximum\" } }").unwrap();
        assert_eq!(load(&path).selection.preset, QualityPreset::Maximum);
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Settings::default());
        assert!(path.with_extension("json.bak").exists());
        assert!(!path.exists());
    }

    #[test]
    fn validate_fixes_unplugged_hardware() {
        let mut sel = Selection { display: 3, microphone_id: Some("gone".into()), ..Selection::default() };
        validate(&mut sel, 2, &["present".to_string()]);
        assert_eq!(sel.display, 1);
        assert_eq!(sel.microphone_id, None);

        let mut ok = Selection { display: 2, microphone_id: Some("present".into()), ..Selection::default() };
        validate(&mut ok, 2, &["present".to_string()]);
        assert_eq!(ok.display, 2);
        assert_eq!(ok.microphone_id.as_deref(), Some("present"));
    }
}
