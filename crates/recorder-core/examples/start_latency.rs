//! Measures how long `Recorder::start` takes, several times in one process.
//!
//! ```text
//! cargo run --release -p recorder-core --example start_latency
//! ```

use std::time::{Duration, Instant};

use recorder_core::{RecordOptions, Recorder};

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--warm") {
        let took = recorder_core::warm_up_encoder(recorder_core::Codec::H264)?;
        println!("warm-up: {} ms", took.as_millis());
    }
    let dir = std::env::temp_dir().join("sr-start-latency");
    for i in 1..=3 {
        let t = Instant::now();
        let recorder = Recorder::start(RecordOptions {
            output_dir: dir.clone(),
            file_name: Some(format!("latency-{i}.mp4")),
            ..Default::default()
        })?;
        let started = t.elapsed();
        std::thread::sleep(Duration::from_millis(500));
        recorder.stop()?;
        println!("start #{i}: {} ms", started.as_millis());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
