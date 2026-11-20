# Screen Recorder

A high-quality, low-lag screen recorder for Windows 10/11, built for daily work: tutorials, bug reports, demos and meetings.

## Goals

- **Sharp output**: native resolution, crisp text, no scaling blur.
- **Smooth capture**: steady 30/60 fps with no dropped frames.
- **Low overhead**: the CPU stays mostly idle while recording (about 1–5%), because capture and encoding run on the GPU.
- **Reliable**: a crash or power loss never destroys a recording.

## Documentation

| Document | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | The capture → encode → mux pipeline, threading model, A/V sync |
| [docs/TECH-STACK.md](docs/TECH-STACK.md) | Technology options compared, and the recommended stack |
| [docs/UI-DESIGN.md](docs/UI-DESIGN.md) | UI research on existing recorders, design principles, wireframes, visual system |
| [docs/FEATURES.md](docs/FEATURES.md) | Feature list and the phased roadmap (MVP → v2 → v3) |
| [docs/RECORDING-SETTINGS.md](docs/RECORDING-SETTINGS.md) | Recommended codecs, bitrates, presets and quality profiles |
| [docs/OPTIMIZATION.md](docs/OPTIMIZATION.md) | Performance and quality optimization checklist |
| [docs/PROTOTYPE.md](docs/PROTOTYPE.md) | Testing the pipeline with FFmpeg before writing app code |

## Tech stack

Rust recording core (Windows.Graphics.Capture, Direct3D 11, Media Foundation/FFmpeg hardware encoding, WASAPI) with a Tauri 2 UI. Details in [docs/TECH-STACK.md](docs/TECH-STACK.md).

## Requirements (target)

- Windows 10 version 2004 or newer, or Windows 11
- A GPU with a hardware video encoder: NVIDIA (NVENC), AMD (AMF/VCN) or Intel (QuickSync)
- An SSD for the output folder (recommended)

### Development machine

Intel Core i5-10210U laptop with Intel UHD Graphics and an NVIDIA GeForce MX230. The MX230 has no hardware encoder, so recording uses **Intel QuickSync** (H.264/HEVC) on the integrated GPU. Target: 1080p at 60 fps. See [docs/RECORDING-SETTINGS.md](docs/RECORDING-SETTINGS.md).

## Status

**Phase 0 done** (except CI). **Phase 1 in progress:** audio, pause and crash-safe output are done.

What works:
- Full-screen recording of a chosen display, constant 60 fps (or 30), hardware H.264/HEVC via Media Foundation (Intel QuickSync on the dev laptop)
- **System audio and microphone** (device picker), each as its own AAC track, in sync with video (≈40 ms measured)
- **Pause/resume** (button or **Ctrl+Alt+P**) with no gap in the file
- **Crash-safe** fragmented MP4: a killed recording still plays
- Launcher window, compact timer/pause/stop window with level meters while recording; app windows are excluded from recordings
- Tray icon (grey idle, red recording, amber paused) and **Ctrl+Alt+R** start/stop
- Logs in `%APPDATA%\ScreenRecorder\logs`; recordings in `Videos\Recordings`

Still to do in Phase 1 (see [docs/FEATURES.md](docs/FEATURES.md)): window and region capture, post-recording thumbnail, settings window and saved preferences, countdown, mic test meter before recording. Known issues are listed in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#known-issues).

## Project layout

```
crates/recorder-core/   Recording engine (no UI). examples/record.rs = CLI smoke test
src-tauri/              Tauri app: commands, state, tray, hotkey, logging
src/                    Svelte frontend (App.svelte, lib/api.ts, app.css)
docs/                   Design docs
```

## Development

```powershell
# Prerequisites: Rust (MSVC toolchain), VS Build Tools (C++ workload), Node.js LTS
npm install
npm run tauri dev        # run the app with hot reload
npm run tauri build      # build the installer (NSIS)
cargo test --workspace   # test the recording engine

# Record 5 s from the command line (engine only):
cargo run --release -p recorder-core --example record -- 5 high
```

The first build takes about 15–20 minutes on the dev laptop; later builds are incremental.
