//! Screen recording engine.
//!
//! Captures a monitor with Windows.Graphics.Capture and encodes it with the
//! GPU's hardware encoder through Media Foundation (Intel QuickSync, NVENC or
//! AMF, whichever the system provides). System audio and the microphone are
//! captured with WASAPI, each as its own AAC track, into fragmented MP4.
//! No UI code lives here, so the engine can be driven from the Tauri app,
//! a CLI or tests.
//!
//! ```no_run
//! use recorder_core::{RecordOptions, Recorder};
//!
//! let recorder = Recorder::start(RecordOptions::default())?;
//! std::thread::sleep(std::time::Duration::from_secs(5));
//! let summary = recorder.stop()?;
//! println!("saved {}", summary.path.display());
//! # Ok::<(), recorder_core::RecorderError>(())
//! ```

mod audio;
mod clock;
mod display;
mod error;
mod options;
mod session;
mod video;
mod writer;

pub use audio::{AudioDevice, AudioTrack, list_microphones};
pub use display::{Display, list_displays};
pub use error::RecorderError;
pub use options::{Codec, QualityPreset, RecordOptions, default_output_dir, timestamped_file_name};
pub use session::{Recorder, RecordingSummary};
