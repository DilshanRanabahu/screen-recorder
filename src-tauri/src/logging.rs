use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Logs to `%APPDATA%\ScreenRecorder\logs\screen-recorder.log.<date>` (one file per day),
/// and to the console in debug builds. Keep the returned guard alive for the
/// whole run so buffered lines are flushed on exit.
pub fn init() -> Option<WorkerGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,screen_recorder=debug,recorder_core=debug"));

    let console = cfg!(debug_assertions).then(|| fmt::layer().with_target(false));

    let (file, guard) = match dirs::config_dir().map(|d| d.join("ScreenRecorder").join("logs")) {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            let appender = tracing_appender::rolling::Builder::new()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("screen-recorder")
                .filename_suffix("log")
                .max_log_files(14)
                .build(&dir);
            match appender {
                Ok(appender) => {
                    let (writer, guard) = tracing_appender::non_blocking(appender);
                    (Some(fmt::layer().with_ansi(false).with_writer(writer)), Some(guard))
                }
                Err(_) => (None, None),
            }
        }
        None => (None, None),
    };

    tracing_subscriber::registry().with(filter).with(console).with(file).init();
    guard
}
