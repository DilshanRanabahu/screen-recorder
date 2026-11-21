# UI Design

This document sets out the user interface for the screen recorder: what existing apps do well, the design direction taken from them, each screen as built, the visual system, and the implementation.

**Target feel:** a modern, dark **studio** in the spirit of OBS Studio (live preview, audio mixer, big record control), but simpler: one screen, sensible defaults, no scenes to configure. While recording it collapses into a small pill that stays out of the way.

> **Design history.** The first version followed the "tray-first, tiny launcher" pattern (section 1). Feedback was that it felt dated and boxy, and that a studio layout like OBS was preferred. Version 2 (this document) is a studio window with a live preview; the tiny pill is kept for the recording phase.

---

## 1. Research: existing screen recorders

### What each app does well

| App | Platform | UI pattern | Worth borrowing | Worth avoiding |
|---|---|---|---|---|
| **Windows Snipping Tool** | Windows | One floating toolbar for screenshots *and* recording. A camera icon switches modes; `Win + Shift + R` opens region select directly. | Native Windows 11 look; one shortcut straight into region selection; the toolbar *is* the app | Few recording options; no overlays or presets |
| **ShareX** | Windows | Small floating bar with a timer and stop button while recording. Large settings tree. | The tiny recording bar; hotkeys for everything; custom file names | Overwhelming settings; dated UI; audio needs manual setup |
| **OBS Studio** | Win/Mac/Linux | Full studio: scenes, sources, mixer, docks | Live preview; multi-source capture; audio mixer meters | Steep learning curve. Reviewers call it overkill for a quick bug recording |
| **Xbox Game Bar** | Windows | Full-screen overlay with widgets | Always available, no install | Records only the focused app; game-oriented |
| **Loom** | Win/Mac/Web | Compact launcher → round webcam bubble → small control pill → instant share link | The webcam bubble; the 3-step flow; the "done → link copied" ending | Cloud-first. Not needed for a local daily tool |
| **CleanShot X** | macOS | Menu-bar app; post-capture thumbnail floats in a corner with quick actions | **Quick-access actions after recording**; menu-bar/tray-first design | Mac only, but the patterns transfer |
| **Screen Studio** | macOS | Records first, then a polished editor adds automatic zoom, smooth cursor and backgrounds | Automatic zoom on clicks; smoothed cursor; beautiful defaults | The editor is heavy for daily quick recordings |
| **Cap** (open source) | Win/Mac | Loom-style: floating bar to pause, resume and stop; resizable webcam bubble; camera-only, split and floating layouts | Open-source reference (Rust + Tauri); the floating bar | Still young; sharing-focused |

### Common patterns in the best apps

1. **The tray or menu bar is the home.** The best tools have no big main window. A tray icon or hotkey opens a small launcher.
2. **Selection happens on the screen itself.** You drag a region directly on a dimmed screen instead of typing coordinates.
3. **One tiny control pill while recording**: timer, pause, stop. Nothing else is visible.
4. **A floating thumbnail after recording** with quick actions (open, copy, trim, delete). This is CleanShot X's best idea.
5. **Good defaults, hidden depth.** Loom and CleanShot need no setup. OBS and ShareX expose everything at once, which is the main complaint about them.
6. **Polish comes from motion.** Screen Studio's auto-zoom and smoothed cursor make ordinary recordings look professional.

---

## 2. Design principles

| # | Principle | What it means in practice |
|---|---|---|
| 1 | **See what you'll get** | A live preview of the selected display, with the recording frame drawn on it. |
| 2 | **Studio when setting up, pill when recording** | The full window is for choosing; once recording starts it shrinks to an always-on-top pill. |
| 3 | **Never in the recording** | All app UI is excluded from capture (`WDA_EXCLUDEFROMCAPTURE`), so it never appears in the video or the preview. |
| 4 | **Hear what you'll get** | Live dB meters for each audio source, *before* and during recording. |
| 5 | **Defaults that just work** | *High* preset, desktop audio on, mic off, saved to `Videos\Recordings`. |
| 6 | **Numbers you can trust** | Resolution, fps, codec, bitrate, GB per hour and hours of disk left are always visible. |
| 7 | **Keyboard first** | `Ctrl+Alt+R` start/stop, `Ctrl+Alt+P` pause, `Esc` hides to tray. |
| 8 | **Clear status, never colour alone** | Title-bar chip, preview badge, tray icon and pill all show Ready / REC / Paused with text and colour. |
| 9 | **Smooth but light** | GPU-scaled preview at a steady 30 fps (20 while recording); nothing when the screen is still or the window is minimized. |

---

## 3. User flow

```
 Launch / tray click / second launch
          │
          ▼
   ┌──────────────────────────┐  Esc / ✕    ┌──────────┐
   │ Studio: preview, sources │ ──────────► │ Tray     │
   │ mixer, quality, recent   │ ◄────────── │ (hidden) │
   └────────────┬─────────────┘  tray click └──────────┘
                │ Start recording / Ctrl+Alt+R
                ▼
   ┌──────────────────────────┐
   │ Starting… (encoder ~1–2s)│
   └────────────┬─────────────┘
                ▼
   ┌──────────────────────────┐  ⤢ expand / ⤡ compact
   │ Pill (always on top)     │ ◄──────────────────► Studio (recording)
   │ ● 02:41  ▮▮  ⏸  Stop     │
   └────────────┬─────────────┘
                │ Stop / Ctrl+Alt+R          Pause ⇄ Resume: Ctrl+Alt+P
                ▼
   Studio + toast "Recording saved · 2:41 · 48 MB"  [Play] [Show in folder]
```

---

## 4. Screens (as built)

### 4.1 Studio window

Default 1120×700 (logical px), resizable, minimum 880×580. Custom title bar (no Windows frame).

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ● Screen Recorder             ( ● Ready )                    📁  —  ☐  ✕     │
├──────────────────────────────────────────────────┬───────────────────────────┤
│ ┌──────────────────────────────────────────────┐ │ CAPTURE                   │
│ │ PREVIEW                Display 1·1920×1080·60Hz│ │ [▣ Screen][▢ Window][⌗ Region]│
│ │                                              │ │            SOON     SOON  │
│ │        live preview (up to 30 fps)           │ │ Display [Display 1 ▾]     │
│ │                                              │ ├───────────────────────────┤
│ │                                              │ │ QUALITY                   │
│ └──────────────────────────────────────────────┘ │ ○ Standard  30fps · H.264 │
│ ┌──────────────────────────────────────────────┐ │ ◉ High      60fps · H.264 │
│ │ AUDIO MIXER       Each source is its own track│ │ ○ Maximum   60fps · HEVC  │
│ │ 🔊 Desktop audio   -23 dB [on]  🎙 Microphone off [ ]│├───────────────────────────┤
│ │ ███████████▌░░░░░░░░░         ░░░░░░░░░░░░░░ │ │ RECENT RECORDINGS  Open ▸ │
│ │ -60 -45 -30 -20 -9 -3 0                       │ │ 🎞 Today 06:07 PM  4.9 MB │
│ └──────────────────────────────────────────────┘ │ 🎞 Today 05:50 PM 131 MB  │
├──────────────────────────────────────────────────┴───────────────────────────┤
│ (● Start recording)  (⏸)  00:00:00     [1920×1080][60 fps][H.264][16 Mbps] 🛢 242 GB free │
│                        Ctrl+Alt+R …                            ≈7.4 GB/h · 33 h left │
└──────────────────────────────────────────────────────────────────────────────┘
```

| Area | Component | Behaviour |
|---|---|---|
| Title bar | `TitleBar.svelte` | Drag to move, double-click to maximize. Status chip: *Ready* (green), *REC 00:12* (red, pulsing), *Paused* (amber), *Starting…/Saving…*. Buttons: recordings folder, minimize, maximize, hide to tray. |
| Preview | `Preview.svelte` | Live GPU-scaled preview of the selected display: 30 fps (20 while recording), stopped when minimized. Badge top-left (PREVIEW / REC / PAUSED), display info top-right, measured preview fps bottom-right, red or amber frame while recording. |
| Audio mixer | `Mixer.svelte`, `Meter.svelte` | One strip per source: icon, name, device (mic picker), live dB readout, on/off switch, horizontal meter on a −60…0 dBFS scale with green/yellow/red zones (−20 / −9 dB) and a 1 s peak-hold tick. Works before recording (mic check); sources lock while recording because each is a track in the file. |
| Capture | `SidePanel.svelte` | Mode cards: Screen, Window, Region (*Soon*). Screen shows the display picker; Window shows the window picker (see *Window capture*). Locked while recording. |
| Quality | `SidePanel.svelte` | Three radio cards with purpose hint and fps/codec. |
| Recent recordings | `SidePanel.svelte` | Up to 50 newest files in the output folder (relative date, size), with a count badge. Hover: Play, Show in folder. The list has its **own scroll area**: a slim scrollbar appears only when the list overflows, and scrolling stays inside the box (`overscroll-behavior: contain`) while the heading, Capture and Quality stay fixed. The box takes the remaining panel height (minimum 112 px), so the side panel itself does not scroll at the default window size. **Updates in real time** when files are added, renamed, moved or deleted, including in Explorer (`watcher.rs`); also re-read when the window regains focus. Opening a file that no longer exists shows an error toast and refreshes the list. |
| Control bar | `ControlBar.svelte` | Big record button (circle → square while recording, spinner while starting/saving), pause/resume, large monospace timer, stat chips, free disk space and estimated GB/hour and hours left. While recording, a *compact* button switches to the pill. |
| Toasts | `Toast.svelte` | Bottom-right. *Recording saved* (with Play / Show in folder), warnings (e.g. mic unavailable), errors. Auto-dismiss after 7 s (errors 12 s). |

### 4.2 Recording pill

340×56, always on top, top-centre of the current monitor, draggable, excluded from capture.

```
╭──────────────────────────────────────────╮
│ ● 02:41  ▮▮        ⏸   [■ Stop]   ⤢      │
╰──────────────────────────────────────────╯
```

| Element | Behaviour |
|---|---|
| ● + timer | Red pulsing dot; amber and static while paused. Timer excludes paused time. |
| ▮▮ | One mini level bar per recorded audio track. |
| ⏸ / ▶ | Pause / resume (`Ctrl+Alt+P`). |
| Stop | Stop and save (`Ctrl+Alt+R`). |
| ⤢ | Expand to the studio (keeps recording). The studio's ⤡ button returns to the pill. |

The studio's size and position are remembered and restored when leaving the pill.

### 4.3 Tray icon

| State | Icon | Left click | Menu |
|---|---|---|---|
| Idle | Grey dot | Opens the studio | Start recording · Pause (disabled) · Show window · Open recordings folder · Quit |
| Recording | Red dot | Stops | Stop recording · Pause · … |
| Paused | Amber dot | Stops | Stop recording · Resume · … |

### 4.4 Planned screens

| Screen | Plan |
|---|---|
| Region selection overlay | Full-screen dimmed overlay (≈40% black); drag a rectangle; handles to resize; size readout and aspect presets (Free, 16:9, 9:16, 1:1); Enter records, Esc cancels. Window mode: hover highlights a window, click selects it. |
| Settings | Dialog with sections: General (start with Windows, countdown), Video (advanced encoder options), Audio, Hotkeys, Storage (output folder, file name pattern). |
| Quick trim | Small player with in/out handles and a lossless cut. |
| Webcam bubble | Round, resizable overlay that *is* recorded. |

---

## 5. Visual design system

### Style

- **Dark studio theme** so the preview and meters stand out (OBS-like, but softer surfaces and more spacing).
- Layered surfaces: background → panels (12 px radius, 1 px hairline border) → raised controls.
- One accent colour (periwinkle) for selection; **red only for recording**, amber only for paused, green for "ready" and healthy audio levels.
- Custom 24×24 line icons (1.75 px stroke) in `src/lib/icons.ts`.

### Colour tokens (`src/app.css`)

| Token | Value | Use |
|---|---|---|
| `--bg` | `#0b0d12` | Window background, preview letterbox |
| `--panel` | `#12151c` | Cards, title bar, control bar |
| `--raised` | `#191d26` | Inputs, chips, toasts |
| `--hover` | `#222734` | Hover state |
| `--border` / `--border-strong` | white 7% / 12% | Hairlines |
| `--fg` / `--muted` / `--faint` | `#e9ecf2` / `#8a93a6` / `#5b6376` | Text levels |
| `--accent` | `#7c8cff` | Selected mode/preset, switches, links |
| `--record` | `#ff4d5e` | Record button, REC status, recording frame |
| `--paused` | `#f5b544` | Paused status |
| `--ok` | `#3ddc97` | Ready dot, meter green zone |
| Meter zones | green `#3ddc97` < −20 dB, yellow `#f5d24a` −20…−9 dB, red `#ff4d5e` > −9 dB | Level meters |

### Typography

| Role | Font | Size / weight |
|---|---|---|
| UI text | Segoe UI Variable Text | 13 px regular / medium |
| Section labels | Segoe UI Variable Text | 11 px semibold, uppercase, +0.08em tracking |
| App name, buttons | Segoe UI Variable Display | 13–14 px semibold |
| Timer, stats, dB | Cascadia Mono (fallback Consolas) | 26 px timer; 11 px chips; tabular numbers |

### Spacing and sizing

- 12 px gaps between panels, 16 px panel padding; 4 px base grid.
- Click targets at least 28 px (36–48 px for primary controls).

### Motion

| Interaction | Animation |
|---|---|
| REC dot | Opacity pulse, 1.4 s loop |
| Spinner (starting/saving) | 0.9 s rotation |
| Toast in | 180 ms fade + 8 px rise |
| Switches | 150 ms knob slide |

All animation is disabled under Windows' *reduced motion* setting. Meters deliberately have **no** width transition: they update 10× per second and animating them costs CPU for no visible gain.

---

## 6. Key features by UI surface

| Feature | Where it appears | Status |
|---|---|---|
| Full-screen capture, display picker | Side panel → Capture | ✅ |
| Live preview | Studio | ✅ |
| Quality presets | Side panel → Quality | ✅ |
| Desktop audio + microphone, device picker | Audio mixer | ✅ |
| Level meters before and during recording | Audio mixer, pill | ✅ |
| Separate audio tracks | (file) | ✅ |
| Start/stop, pause/resume + hotkeys | Control bar, pill, tray | ✅ |
| Disk space and size estimate | Control bar | ✅ |
| Recent recordings | Side panel | ✅ |
| Saved/warning/error toasts | Studio | ✅ |
| Window and region capture | Capture cards (marked *Soon*) | Planned |
| Settings, saved preferences, custom hotkeys | Settings dialog | Planned |
| Countdown | Overlay | Planned |
| Webcam bubble, click/keystroke overlays | Overlays | v2 |
| Quick trim | Trim window | v2 |

---

## 7. Accessibility

- Icon-only buttons have `aria-label`s; switches use `role="switch"`, mode and quality cards use `role="radio"`.
- Status is never shown by colour alone: *Ready / REC / Paused / Starting / Saving* are always written out.
- Text contrast on panels is at least 4.5:1 for primary and muted text (`--faint` is used only for hints).
- Visible focus ring (2 px accent) on every control.
- Reduced-motion setting disables pulses, spinners and transitions.
- To do: full keyboard navigation of the cards with arrow keys; high-contrast theme.

---

## 8. Implementation

### Stack

**Tauri 2 + Svelte 5 + TypeScript + Tailwind CSS 4** on top of the Rust engine. See [TECH-STACK.md](TECH-STACK.md).

```
src/
├─ App.svelte              state, polling, view switching, toasts
├─ app.css                 theme tokens, select styling, animations
├─ lib/api.ts              typed Tauri commands/events, preset maths, formatting
├─ lib/icons.ts            icon paths
└─ components/
   ├─ TitleBar.svelte  Preview.svelte  Mixer.svelte  Meter.svelte
   ├─ SidePanel.svelte  ControlBar.svelte  Pill.svelte  Toast.svelte
   └─ Icon.svelte  Switch.svelte
```

### One window, two views

The app uses a single `main` window. The Rust side switches it between **studio** (resizable, normal z-order, min 880×580) and **pill** (340×56, fixed, always on top, top-centre) with `set_view`, and remembers the studio bounds. The window is `contentProtected`, so it is excluded from recordings and from the preview.

### Live preview

The preview is its own lightweight Windows.Graphics.Capture session (`crates/recorder-core/src/preview.rs`), independent of the recording pipeline. It uses two threads so a slow JPEG never holds up capture:

```
capture thread:  WGC frame (GPU, 1920×1080 RGBA)
                   → copy into a mipmapped texture → GenerateMips         (GPU)
                   → copy mip level 1 (960×540) to a staging texture     (GPU)
                   → Map / read back 960×540                              (≈4 ms)
                   → one-frame "latest" slot (an unclaimed frame is replaced)
encoder thread:  newest frame → JPEG q72 via Windows Imaging Component  (≈3.5 ms, ≈85 KB)
                   → Tauri channel (raw bytes) → createImageBitmap → canvas
```

- **Rate:** 30 fps whenever the studio is visible (focused or not), 20 fps while recording, stopped when minimized or in pill view. Changed live with `set_preview_fps` (no restart). Windows only delivers frames when the screen changes, so a still screen costs nothing.
- **Flow control:** the UI calls `preview_ack` after drawing; at most two frames are in flight and extra frames are dropped in Rust, so a busy UI never builds a backlog. The encoder always takes the newest frame.
- **Encoder:** WIC (built into Windows) by default; the pure-Rust `jpeg-encoder` is the fallback (`SR_PREVIEW_JPEG=rust` forces it).
- **Message format:** `[encoding u8: 0 = JPEG, 1 = RGBA][width u32 LE][height u32 LE][session id u32 LE][data]`.
- **Sessions have ids.** `start_preview` returns an id; `stop_preview_session(id)` and `preview_ack(id)` name the session they mean, and anything for an older session is ignored. Without this, switching displays raced: the old session's "stop" often arrived *after* the new session started and killed it, freezing the preview on the previous display until the user switched again. Measured after the fix: 10 switches down to 300 ms apart, the late stop was ignored every time, and the new display's first frame arrived in 101–118 ms.
- The app's own window is `WDA_EXCLUDEFROMCAPTURE`, so it never appears in the preview.
- **CSP must allow Tauri IPC** (`connect-src 'self' ipc: http://ipc.localhost` in `tauri.conf.json`). Without it, installed builds silently fall back to JSON IPC and binary frames arrive as number arrays. The first installer hit this: the preview stayed on "Starting preview…" while the dev build (which has no CSP) worked. The preview now accepts both shapes and shows an error instead of waiting forever.

Measured on the dev laptop (whole app incl. WebView2, screen changing constantly):

| Version | Rate | JPEG time | App CPU |
|---|---|---|---|
| v2.0: GDI snapshots, raw RGBA | 3 fps | n/a | 3.5% |
| v2.1: raw RGBA over IPC (2 MB/frame) | 27 fps | none | 20.0% |
| v2.1: Rust JPEG on the capture thread | 30 fps focused / 15 fps background | ≈10 ms (up to 28 ms under load) | 13.7% / 7.8% |
| **v2.2: WIC JPEG on its own thread (current)** | **30 fps, focused or not** | **≈3.5 ms** | **12.4%** |
| v2.2, **release build** (installed app) | 29 fps | ≈2.7 ms | **7.8%** |

Why v2.1 still felt laggy: it dropped to 15 fps whenever another window had focus, and the 10–28 ms JPEG encode ran on the capture thread, so the frame rate wobbled. v2.2 fixes both.

The remaining cost is mostly WebView2 receiving and drawing frames (≈8%). Ideas if it needs to go lower: pick the mip level from the preview's on-screen size so small windows get 480 px frames, or encode on the GPU.

### Level meters

Peak levels are measured in the WASAPI capture threads. While idle, an `AudioMeter` opens the enabled devices only while the studio is visible (`set_monitoring`), and is closed before a recording opens them. The frontend polls `get_levels` every 100 ms, only when a source is enabled and the window isn't minimized.

### Window capture

- **Picker:** Capture → Window shows a dropdown of open windows (`list_windows`): visible, titled, not cloaked (suspended UWP apps), excluding this app. Front-most first; minimized windows are marked and record once restored. The list refreshes every 3 s while Window mode is shown, when the dropdown is focused, and via **Refresh**.
- **Remembered:** the choice is saved as id + app + title. Ids change when an app restarts, so a missing id is matched again by app + title, then by app.
- **Recording:** Windows.Graphics.Capture records only that window, even when it is covered or moved. The video size is the window's visible size (DWM extended frame bounds) when recording starts; if the window gets smaller, the rest of the frame is cleared to black (no ghosting), and if it gets larger it is cropped.
- **Window closed:** the recording stops and saves automatically within ≈0.3 s, with a "the recorded window was closed" notice. Starting with a window that is already closed shows an error.
- Preview, size chip and bitrate follow the window.

Tested: VS Code window (1920×1032), a test window recorded at 982×616 and closed mid-recording (stopped and saved in 0.26 s, file decodes cleanly), re-matching after the window id changed.

### Microphones (plug and play)

- `recorder_core::watch_audio_devices` registers a Windows `IMMNotificationClient` (via `wasapi`) and reports device added/removed/state/default changes, batched for 300 ms. The app emits `audio-devices-changed`; the UI re-reads the list and the idle meter reopens so it follows the chosen or default microphone.
- Dropdown: "Windows default · <name>", each microphone (the default one marked), and "Saved microphone (not connected)" while a remembered mic is unplugged.
- A newly connected microphone shows a toast with **Use this microphone**. Unplugging the chosen mic shows a warning; during a recording the mic track stays silent for the rest of that recording.
- A saved microphone is never forgotten because it is unplugged. If it is missing when recording starts, the Windows default mic is used and the recording shows a warning.

### State

- The Rust core owns all recording state; the frontend mirrors it from the `recorder-state` event (`idle` / `starting` / `recording` / `stopping`) and the `view` event.
- The timer is computed locally from `elapsed_ms` + time since the event, so it needs no polling of the engine.

### Developer aid

Set `SR_SHOW_IN_CAPTURE=1` before `npm run tauri dev` to make the window visible to screenshots and recordings (for UI screenshots). The preview will then show the app inside itself.

---

## 9. Sources

- [ShareX vs Windows Snipping Tool](https://getsharex.com/compare/sharex-vs-snipping-tool/) (published by ShareX)
- [Record Screen on Windows 11: 5 Methods (2026)](https://atomisystems.com/screencasting/record-screen-on-windows-11-5-methods-that-actually-work-in-2026/) (vendor-published)
- [Windows 11 deep dive: the new Snipping Tool (XDA)](https://www.xda-developers.com/windows-11-snipping-tool/)
- [The Snipping Tool quietly became the only screenshot app I keep installed (TweakTown)](https://www.tweaktown.com/guides/11466/the-snipping-tool-quietly-became-the-only-screenshot-app-i-keep-installed-on-windows-11/index.html)
- [Best Screen Recorders for Mac (Setapp)](https://setapp.com/app-reviews/best-screen-recorders-for-mac)
- [Best Screen Recording Software 2026 (Toolradar)](https://toolradar.com/blog/best-screen-recording-software)
- [Screen Studio vs CleanShot X (ToolQuestor)](https://toolquestor.com/vs/screen-studio-vs-cleanshot-x)
- [Cap: Open Source Screen Recorder](https://cap.so/open-source-screen-recorder) · [Cap on Windows](https://cap.so/screen-recorder-windows) · [Cap on GitHub](https://github.com/cap-so/cap)
- [Cap: The Open Source Loom Alternative](https://themenonlab.blog/blog/cap-open-source-loom-alternative-screen-recording)
