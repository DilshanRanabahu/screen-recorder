# Features and Roadmap

## Phase 0: Project setup (Rust + Tauri)

- [x] Cargo workspace with `crates/recorder-core` (engine) and `src-tauri` (app)
- [x] Tauri 2 app with a Svelte + TypeScript frontend and Tailwind CSS
- [x] Tray icon and global hotkey (Ctrl+Alt+R), wired to real start/stop recording
- [x] `tracing` logging to `%APPDATA%\ScreenRecorder\logs`
- [x] Engine smoke test: record 5 seconds of the primary monitor to MP4 from a CLI example (`cargo run --example record`)
- [x] Release build and NSIS installer (`npm run tauri build`, 2.4 MB)
- [ ] CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, frontend build

## Phase 1: MVP

The minimum needed to replace other recorders for daily work.

### Capture
- [x] Full screen (choose a monitor)
- [x] Single window: records only that window, even when covered or moved; picker lists open windows (cloaked/untitled ones hidden); remembered by app and title across restarts; recording stops and saves automatically if the window is closed
- [ ] Custom region (drag to select, with the size shown while dragging)
- [x] Cursor included (option to hide it exists in the engine, not yet in the UI)

### Audio
- [x] System audio (WASAPI loopback)
- [x] Microphone, with device picker; external mics (USB, headsets, Bluetooth) appear and disappear live as they are plugged in or out, with a "Use this microphone" prompt; a saved mic that is unplugged is kept and recordings fall back to the Windows default with a warning
- [x] Separate tracks for system audio and microphone
- [x] Live level meters during recording
- [x] Level meters *before* recording (mic check)

### Controls
- [x] Studio window: live preview, audio mixer, quality cards, recent recordings, disk-space estimate
- [x] Compact always-on-top pill while recording (expand back to the studio any time)
- [x] "Recording saved" toast with Play / Show in folder
- [x] Global hotkeys: start/stop (Ctrl+Alt+R), pause/resume (Ctrl+Alt+P)
- [ ] Configurable hotkeys
- [x] System tray icon with a quick menu (grey idle, red recording, amber paused)
- [ ] 3-second countdown (optional)
- [x] Recording timer, shown in a small floating widget that is excluded from the capture
- [x] Pause / resume with no gap in the file

### Output
- [x] Hardware encoding via Media Foundation (uses QuickSync on the dev laptop)
- [x] Clear error at start if the encoder can't be created
- [ ] Software-encoding fallback at a lower frame rate
- [x] Quality presets: *Standard*, *High*, *Maximum* (see [RECORDING-SETTINGS.md](RECORDING-SETTINGS.md))
- [x] Constant frame rate output, in sync with audio
- [x] Crash-safe recording (fragmented MP4, written directly; no remux needed). H.264 only; HEVC uses regular MP4
- [x] Correct colours: BT.709 limited range, labelled in the stream (fixes washed-out playback)
- [x] Automatic file naming: `Recording_2026-11-20_14-32-05.mp4`
- [ ] Configurable output folder (fixed to `Videos\Recordings` for now; "Open folder" works)
- [x] Choices saved between app restarts (display, quality, audio, mic) in `%APPDATA%\ScreenRecorder\settings.json`

## Phase 2: Productivity

- [ ] **Click visualization**: a ripple effect on mouse clicks
- [ ] **Cursor highlight**: a soft circle around the pointer
- [ ] **Keystroke overlay**: shows shortcuts pressed, useful for tutorials
- [ ] **Webcam overlay**: picture-in-picture, movable, round or rectangular
- [ ] **Instant replay buffer**: always keeps the last N minutes in memory, saved with a hotkey
- [ ] **Quick trim editor**: cut start and end without re-encoding (no quality loss)
- [x] **Recording history**: recent recordings list in the studio, kept in sync with the folder in real time (no thumbnails yet)
- [ ] **Screenshot** hotkey (PNG, same region tools)

## Phase 3: Polish

- [ ] On-screen drawing and annotations while recording (pen, arrow, box, highlight)
- [ ] Zoom-to-cursor or smooth auto-zoom on clicks
- [ ] Microphone noise suppression (RNNoise) and auto gain
- [ ] Blur or redact regions (for passwords and private data)
- [ ] Scheduled or timed recordings (stop after N minutes)
- [ ] Auto-split long recordings by size or time
- [ ] Export presets: GIF, compressed "share" MP4, audio-only
- [ ] Optional upload or copy-to-clipboard after recording

## Settings screen (planned layout)

| Section | Options |
|---|---|
| General | Output folder, file name pattern, start with Windows, countdown |
| Video | Encoder, codec, quality preset, fps, resolution (native/scaled) |
| Audio | System device, mic device, separate tracks, noise suppression |
| Hotkeys | Start/stop, pause, screenshot, save replay |
| Overlays | Cursor, clicks, keystrokes, webcam |
