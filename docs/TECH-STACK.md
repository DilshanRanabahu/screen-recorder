# Technology Stack

**Decision: Rust core + Tauri 2 UI.**

## Why Rust

- **Native speed with no garbage-collection pauses.** Frame pacing stays steady, which matters for smooth 60 fps capture.
- **Memory safety** in a long-running, multi-threaded app (capture, encode, audio and writer threads) without the crash risks of C++.
- **Direct Windows API access** through Microsoft's official [`windows`](https://crates.io/crates/windows) crate: Windows.Graphics.Capture, Direct3D 11, Media Foundation and WASAPI.
- **A proven path.** [Cap](https://github.com/CapSoftware/Cap), an open-source recorder, uses Rust + Tauri, so there is real-world code to learn from.
- **Small output.** A Tauri app installs at a few MB, compared with 100+ MB for Electron.

## Options considered

| Stack | Performance | Effort | Notes |
|---|---|---|---|
| **Rust core + Tauri 2 UI** | Excellent | Medium | **Chosen.** Safe native core, modern web-styled UI, small binary |
| Rust core + Slint / egui UI | Excellent | Medium | All-Rust UI, but harder to reach the Fluent look and polished animations |
| C# .NET 8 + WPF + ScreenRecorderLib | Excellent | Low | Fastest to build, but less control over the pipeline |
| C++ core + Qt / WinUI 3 | Best | High | Full control, but more risk of memory bugs |
| Electron + FFmpeg | Good | Low | Heavy, and limited control over pacing and sync |

## Stack overview

```
┌──────────────────────── Tauri 2 app ────────────────────────┐
│  Frontend (WebView2)                                        │
│  Svelte or React + TypeScript + Tailwind CSS                │
│  Launcher · Region overlay · Control pill · Thumbnail ·     │
│  Settings · Trim                                            │
│              ▲ events                 │ commands            │
│              │                        ▼                     │
│  Rust backend                                               │
│  ├─ recorder   (state machine, session control)            │
│  ├─ capture    (Windows.Graphics.Capture → D3D11)          │
│  ├─ encoder    (Media Foundation or FFmpeg, hardware)      │
│  ├─ audio      (WASAPI loopback + microphone)              │
│  ├─ writer     (MF sink writer → fragmented MP4)           │
│  └─ settings   (serde + JSON in %APPDATA%)                 │
└─────────────────────────────────────────────────────────────┘
```

## Crates and tools

### Recording core

| Purpose | Crate / API | Notes |
|---|---|---|
| Windows APIs | [`windows`](https://crates.io/crates/windows) | Official Microsoft bindings: WGC, D3D11, DXGI, Media Foundation, WASAPI |
| Screen capture | [`windows-capture`](https://crates.io/crates/windows-capture) | High-level WGC wrapper: monitor/window capture, cursor and border options. Only its capture is used: its built-in encoder drifts out of sync on static screens |
| GPU conversion | `windows` → `ID3D11VideoProcessor` | BGRA → NV12/P010 on the GPU, plus crop and scale |
| Video encoding (MVP) | Media Foundation **sink writer** via `windows` (`writer.rs`) | Hardware H.264/HEVC built into Windows; no extra DLLs. Picks Intel QuickSync on the dev laptop. Writes fragmented MP4 with one AAC track per audio source |
| Video encoding (later) | [`ffmpeg-next`](https://crates.io/crates/ffmpeg-next) | NVENC/AMF/QSV, AV1, 10-bit and fine-grained rate control. Needs FFmpeg DLLs shipped with the app |
| Audio capture | [`wasapi`](https://crates.io/crates/wasapi) | Direct WASAPI, including loopback for system audio. [`cpal`](https://crates.io/crates/cpal) is an alternative |
| Audio resampling | [`rubato`](https://crates.io/crates/rubato) | Resampling to 48 kHz and drift correction |
| Audio encoding | Media Foundation AAC, or `ffmpeg-next` | AAC 192 kbps |
| Remux / trim | Bundled `ffmpeg.exe` as a Tauri sidecar | Stream copy, so no quality loss |

### Concurrency and infrastructure

| Purpose | Crate |
|---|---|
| Thread channels (bounded queues) | [`crossbeam-channel`](https://crates.io/crates/crossbeam-channel) |
| Async runtime (UI commands, file I/O) | [`tokio`](https://crates.io/crates/tokio) (comes with Tauri) |
| Error handling | [`thiserror`](https://crates.io/crates/thiserror) (library code), [`anyhow`](https://crates.io/crates/anyhow) (app code) |
| Logging | [`tracing`](https://crates.io/crates/tracing) + `tracing-appender` (rotating log files) |
| Settings | [`serde`](https://crates.io/crates/serde) + `serde_json` |

Keep the capture and encode paths on dedicated OS threads (`std::thread`) with MMCSS priority. Use tokio only for UI commands and file work, never for frame timing.

### UI (Tauri 2)

| Purpose | Tool |
|---|---|
| App shell | [Tauri 2](https://tauri.app) (WebView2 on Windows) |
| Frontend | Svelte 5 (light and fast) or React, TypeScript, Vite |
| Styling | Tailwind CSS with the color tokens from [UI-DESIGN.md](UI-DESIGN.md) |
| Icons | Fluent UI System Icons (SVG) |
| Mica / Acrylic backdrop | [`window-vibrancy`](https://crates.io/crates/window-vibrancy) |
| Global hotkeys | `tauri-plugin-global-shortcut` |
| Tray icon | Tauri built-in tray API |
| Start with Windows | `tauri-plugin-autostart` |
| Single instance | `tauri-plugin-single-instance` |
| Auto-update | `tauri-plugin-updater` |
| Hide app UI from recordings | `WebviewWindow::set_content_protected(true)`, or call `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` directly via `windows`. Check that the window disappears from the capture rather than showing up black |

### Tooling

| Purpose | Tool |
|---|---|
| Toolchain | Rust stable (MSVC target `x86_64-pc-windows-msvc`), Node.js LTS |
| Build | `cargo`, `pnpm` / `npm`, `tauri-cli` |
| Lint / format | `clippy`, `rustfmt`, ESLint, Prettier |
| Tests | `cargo test`; integration tests that record 5 seconds and check the output with `ffprobe` |
| Installer | Tauri bundler (MSI or NSIS) |

## Project layout

```
screen-recorder/
├─ Cargo.toml                 # Cargo workspace
├─ crates/
│  └─ recorder-core/          # Pure Rust recording engine, no UI code
│     └─ src/
│        ├─ lib.rs
│        ├─ session.rs        # State machine: Idle → Recording ⇄ Paused → Finishing
│        ├─ capture/          # WGC, D3D11, frame pacing
│        ├─ encode/           # Media Foundation / FFmpeg backends behind one trait
│        ├─ audio/            # WASAPI loopback + mic, resampling
│        ├─ mux/              # Container writing, crash recovery
│        └─ clock.rs          # QPC timebase shared by audio and video
├─ src-tauri/                 # Tauri app: commands, tray, hotkeys, windows
│  ├─ src/main.rs
│  └─ tauri.conf.json
├─ src/                       # Frontend (Svelte/React)
│  ├─ windows/                # launcher, overlay, pill, thumbnail, settings
│  └─ lib/
└─ docs/
```

Keeping the engine in its own crate (`recorder-core`) means it can be tested without the UI, and also reused in a CLI tool.

## Design notes

- **Encoder trait.** Define `trait VideoEncoder` with a Media Foundation implementation first, then add an FFmpeg one later without touching the rest of the pipeline.
- **One D3D11 device.** Create it once and share it between capture, conversion and the encoder, so frames never get copied between devices.
- **Session events.** The Rust core emits state changes (`recording-started`, `tick`, `paused`, `finished { path }`) as Tauri events. Every window listens to the same events, so the tray, pill and launcher always agree.
- **`unsafe` boundaries.** Keep raw COM/Win32 calls inside small, documented wrapper modules, and expose safe Rust APIs to the rest of the code.

## Learning resources

- Tauri 2 docs: https://tauri.app
- `windows` crate docs and samples: https://github.com/microsoft/windows-rs
- `windows-capture` examples: https://github.com/NiiightmareXD/windows-capture
- Cap source, a real Rust + Tauri recorder: https://github.com/CapSoftware/Cap
