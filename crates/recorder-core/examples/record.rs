//! Engine smoke test: records the primary display and prints a summary.
//!
//! ```text
//! cargo run --release -p recorder-core --example record            # 5 s, High preset
//! cargo run --release -p recorder-core --example record -- 10 max  # 10 s, Maximum preset
//! cargo run --release -p recorder-core --example record -- 5 high mic  # + default microphone
//! cargo run --release -p recorder-core --example record -- 5 high window  # front-most window
//! ```

use std::time::Duration;

use recorder_core::{QualityPreset, RecordOptions, Recorder, list_displays, list_microphones, list_windows};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(5);
    let preset = match args.next().as_deref() {
        Some("standard") => QualityPreset::Standard,
        Some("max" | "maximum") => QualityPreset::Maximum,
        _ => QualityPreset::High,
    };
    let rest: Vec<String> = args.collect();
    let microphone = rest.iter().any(|a| a == "mic");
    // "window": record the front-most window instead of display 1.
    let window = if rest.iter().any(|a| a == "window") {
        let windows = list_windows()?;
        for w in windows.iter().take(8) {
            println!("window: {} | {} | {}x{}{}", w.title, w.process, w.width, w.height, if w.minimized { " (minimized)" } else { "" });
        }
        let w = windows.iter().find(|w| !w.minimized).ok_or_else(|| anyhow::anyhow!("no window to record"))?;
        println!("recording window: {} ({}x{})", w.title, w.width, w.height);
        Some(w.id)
    } else {
        None
    };

    for d in list_displays()? {
        println!("display {}: {} {}x{} @ {} Hz", d.index, d.name, d.width, d.height, d.refresh_rate);
    }
    for m in list_microphones()? {
        println!("microphone: {}{}", m.name, if m.is_default { " (default)" } else { "" });
    }

    let recorder = Recorder::start(RecordOptions { preset, microphone, window, ..Default::default() })?;
    println!("recording {seconds} s to {} ...", recorder.path().display());
    std::thread::sleep(Duration::from_secs(seconds));

    let (sys, mic) = recorder.levels();
    println!("last audio peaks: system {sys:.2}, microphone {mic:.2}");
    let s = recorder.stop()?;
    for w in &s.warnings {
        println!("warning: {w}");
    }
    let fps = s.frames as f64 / s.duration.as_secs_f64();
    println!(
        "saved {}\n  {:.1} s, {} frames ({fps:.1} fps delivered), {:.1} MB",
        s.path.display(),
        s.duration.as_secs_f64(),
        s.frames,
        s.bytes as f64 / 1e6
    );
    Ok(())
}
