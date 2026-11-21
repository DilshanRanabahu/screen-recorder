use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Video codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    /// Plays everywhere. The default.
    H264,
    /// About 40% smaller files at the same quality.
    Hevc,
}

/// Quality presets from `docs/RECORDING-SETTINGS.md`.
///
/// Media Foundation's transcoder takes a target bitrate rather than a
/// constant-quality value, so each preset maps to a bitrate that scales with
/// resolution and frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityPreset {
    /// 30 fps H.264. Long meetings, small files.
    Standard,
    /// 60 fps H.264. Tutorials and demos.
    #[default]
    High,
    /// 60 fps HEVC at a higher bitrate. Archive and editing source.
    Maximum,
}

impl QualityPreset {
    pub fn fps(self) -> u32 {
        match self {
            Self::Standard => 30,
            Self::High | Self::Maximum => 60,
        }
    }

    pub fn codec(self) -> Codec {
        match self {
            Self::Standard | Self::High => Codec::H264,
            Self::Maximum => Codec::Hevc,
        }
    }

    /// Bits per pixel per frame. Gives about 10 Mbps for 1080p30 (Standard),
    /// 16 Mbps for 1080p60 (High) and 21 Mbps for 1080p60 HEVC (Maximum),
    /// matching the bitrate table in the docs.
    fn bits_per_pixel(self) -> f64 {
        match self {
            Self::Standard => 0.16,
            Self::High => 0.13,
            Self::Maximum => 0.17,
        }
    }

    /// Target bitrate in bits per second for the given frame size.
    pub fn bitrate(self, width: u32, height: u32) -> u32 {
        let bps = f64::from(width) * f64::from(height) * f64::from(self.fps()) * self.bits_per_pixel();
        // Keep within sensible bounds for very small regions and 4K+.
        bps.clamp(2_000_000.0, 80_000_000.0) as u32
    }
}

/// What to record and where to save it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordOptions {
    /// One-based monitor index (see [`crate::list_displays`]).
    pub display: usize,
    /// Record this window (an id from [`crate::list_windows`]) instead of the
    /// display. The video size is the window size when recording starts.
    pub window: Option<u64>,
    pub preset: QualityPreset,
    /// Include the mouse pointer.
    pub capture_cursor: bool,
    /// Record what the computer plays (WASAPI loopback).
    pub system_audio: bool,
    /// Record a microphone.
    pub microphone: bool,
    /// Microphone device id from [`crate::list_microphones`]; `None` = Windows default.
    pub microphone_id: Option<String>,
    /// Folder for the finished file. Created if missing.
    pub output_dir: PathBuf,
    /// File name without folder. `None` means `Recording_<date>_<time>.mp4`.
    pub file_name: Option<String>,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self {
            display: 1,
            window: None,
            preset: QualityPreset::default(),
            capture_cursor: true,
            system_audio: true,
            microphone: false,
            microphone_id: None,
            output_dir: default_output_dir(),
            file_name: None,
        }
    }
}

/// `Videos\Recordings` in the user's profile.
pub fn default_output_dir() -> PathBuf {
    dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Recordings")
}

/// `Recording_2026-11-20_14-32-05.mp4`
pub fn timestamped_file_name() -> String {
    chrono::Local::now().format("Recording_%Y-%m-%d_%H-%M-%S.mp4").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrates_match_docs() {
        let mbps = |p: QualityPreset| p.bitrate(1920, 1080) / 1_000_000;
        assert_eq!(mbps(QualityPreset::Standard), 9);
        assert_eq!(mbps(QualityPreset::High), 16);
        assert_eq!(mbps(QualityPreset::Maximum), 21);
    }

    #[test]
    fn bitrate_is_clamped() {
        assert_eq!(QualityPreset::High.bitrate(16, 16), 2_000_000);
        assert_eq!(QualityPreset::Maximum.bitrate(7680, 4320), 80_000_000);
    }

    #[test]
    fn file_name_format() {
        let name = timestamped_file_name();
        assert!(name.starts_with("Recording_") && name.ends_with(".mp4"));
        assert_eq!(name.len(), "Recording_2026-11-20_14-32-05.mp4".len());
    }
}
