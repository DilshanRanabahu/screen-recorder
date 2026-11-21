use std::path::PathBuf;

/// Errors returned by the recording engine.
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("display {0} was not found")]
    DisplayNotFound(usize),

    #[error("the selected window is no longer open")]
    WindowNotFound,

    #[error("could not list displays: {0}")]
    Displays(String),

    #[error("could not create the output folder {path}: {source}")]
    OutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("screen capture failed: {0}")]
    Capture(String),

    #[error("video encoder failed: {0}")]
    Encoder(String),

    #[error("audio: {0}")]
    Audio(String),

    #[error("no frames were captured, so no file was written")]
    NoFrames,
}
