# UI Design

This document sets out the user interface for the screen recorder: what existing apps do well, the design principles taken from them, each screen as a wireframe, and the visual system and implementation approach.

**Target feel:** smart, minimalistic and modern. The app should feel like part of Windows 11, stay out of the way while recording, and need no learning.

---

## 1. Research: existing screen recorders

### What each app does well

| App | Platform | UI pattern | Worth borrowing | Worth avoiding |
|---|---|---|---|---|
| **Windows Snipping Tool** | Windows | One floating toolbar for screenshots *and* recording. A camera icon switches modes; `Win + Shift + R` opens region select directly. | Native Windows 11 look; one shortcut straight into region selection; the toolbar *is* the app | Few recording options; no overlays or presets |
| **ShareX** | Windows | Small floating bar with a timer and stop button while recording. Large settings tree. | The tiny recording bar; hotkeys for everything; custom file names | Overwhelming settings; dated UI; audio needs manual setup |
| **OBS Studio** | Win/Mac/Linux | Full studio: scenes, sources, mixer, docks | Multi-source capture; audio mixer meters | Steep learning curve. Reviewers call it overkill for a quick bug recording |
| **Xbox Game Bar** | Windows | Full-screen overlay with widgets | Always available, no install | Records only the focused app; game-oriented |
| **Loom** | Win/Mac/Web | Compact launcher → round webcam bubble → small control pill → instant share link | The webcam bubble; the 3-step flow; the "done → link copied" ending | Cloud-first. Not needed for a local daily tool |
| **CleanShot X** | macOS | Menu-bar app; post-capture thumbnail floats in a corner with quick actions | **Quick-access thumbnail after recording**; menu-bar/tray-first design | Mac only, but the patterns transfer |
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
| 1 | **Invisible until needed** | Lives in the tray. No window opens at startup. |
| 2 | **Three clicks to record** | Hotkey or tray → choose region → Record. |
| 3 | **Never in the recording** | All app UI is excluded from capture (`WDA_EXCLUDEFROMCAPTURE`). |
| 4 | **Defaults that just work** | The *High* preset, system audio on, mic off, saved to `Videos\Recordings`. |
| 5 | **Progressive disclosure** | Common choices on the launcher. Everything else in Settings. |
| 6 | **Native Windows 11 feel** | Fluent Design, Mica backdrop, rounded corners, Segoe UI Variable, light/dark that follows the system. |
| 7 | **Keyboard first** | Every action has a shortcut. Esc always cancels. |
| 8 | **Clear status** | It is always obvious whether the app is recording: red dot, timer, tray icon change. |

---

## 3. User flow

```
 Ctrl+Alt+R / tray click
          │
          ▼
   ┌──────────────┐     Esc      ┌─────────┐
   │   Launcher   │ ───────────► │  Idle   │
   └──────┬───────┘              └─────────┘
          │ choose mode
          ▼
   ┌──────────────┐
   │ Region select│ (screen dims, drag or click a window)
   └──────┬───────┘
          │ Record / Enter
          ▼
   ┌──────────────┐
   │ Countdown 3s │ (optional, skip with Space)
   └──────┬───────┘
          ▼
   ┌──────────────┐  pause/resume
   │  Recording   │ ◄──────────┐
   │ (control pill)│ ──────────┘
   └──────┬───────┘
          │ Stop (hotkey or pill)
          ▼
   ┌──────────────┐
   │  Thumbnail   │ → Open · Copy · Trim · Show in folder · Delete
   └──────────────┘ (auto-hides after 8 s)
```

---

## 4. Screens

### 4.1 Launcher (main entry)

A compact floating panel near the tray or at the top centre of the screen. It is the only "main window".

```
╭──────────────────────────────────────────────────────────╮
│  ▢ Screen   ⧉ Window   ⬚ Region         ⚙   ✕            │
├──────────────────────────────────────────────────────────┤
│  🖥  Display 1 · 2560×1440                       ▾       │
│  🔊 System audio                              ●━━ On     │
│  🎙  Microphone · Realtek Mic                  ━━○ Off    │
│  📷 Camera                                     ━━○ Off    │
│  ✦  Quality · High (60 fps)                      ▾       │
├──────────────────────────────────────────────────────────┤
│              ╭──────────────────────────╮                │
│              │   ●  Start recording     │     Ctrl+Alt+R │
│              ╰──────────────────────────╯                │
╰──────────────────────────────────────────────────────────╯
```

- About 380 px wide, Mica backdrop, 8 px corner radius.
- The mode buttons are a segmented control. The selected one is filled with the accent color.
- Live audio level bars appear next to the audio toggles, so you can confirm the mic works *before* recording.
- It remembers the last-used choices.

### 4.2 Region selection overlay

```
┌──────────────────────────────────────────────────────────────┐
│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░┃                               ┃░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░┃      (clear, undimmed)        ┃░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░┃                               ┃░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛░░░░░░░░░░░░░░░░░░░░│
│░░░░░░░░░   1280 × 720   [16:9 ▾]  [● Record]  [✕]  ░░░░░░░░░░│
└──────────────────────────────────────────────────────────────┘
```

- The screen dims to about 40% black; the selected area stays clear.
- In window mode, hovering highlights a window and a click selects it.
- Handles on corners and edges resize the area. Arrow keys nudge it by 1 px, Shift+arrow by 10 px.
- Size presets: Free, 16:9, 9:16 (vertical/shorts), 1:1, 1080p, 720p.
- A magnifier near the cursor helps with pixel-exact edges.
- Enter starts recording; Esc cancels.

### 4.3 Recording control pill

The only UI visible during recording. It is excluded from the capture and can be dragged anywhere.

```
╭─────────────────────────────────────╮
│ ● 02:41   ⏸   ⏹   🎙   ✎   ⋯        │
╰─────────────────────────────────────╯
```

| Element | Behavior |
|---|---|
| ● red dot + timer | Pulses gently. Turns amber and shows "Paused" while paused |
| ⏸ Pause / ▶ Resume | `Ctrl+Alt+P` |
| ⏹ Stop | `Ctrl+Alt+R` (same as start) |
| 🎙 Mic mute | Instant mute without stopping |
| ✎ Draw | Opens annotation tools (Phase 3) |
| ⋯ More | Restart, discard, hide pill |

- It fades to 40% opacity when the mouse is away and back to full on hover.
- An option collapses it to just the dot and timer.
- A thin dashed border around the recorded region (also excluded from capture) shows what is being recorded.

### 4.4 Post-recording thumbnail

Inspired by CleanShot X. It slides in at the bottom-right corner.

```
╭──────────────────────────────╮
│ ┌──────────────────────────┐ │
│ │      ▶  (preview)        │ │
│ └──────────────────────────┘ │
│ Recording_2026-11-20 · 2:41  │
│ 48 MB · 1440p60              │
│ [Open] [Copy] [Trim] [📁] [🗑]│
╰──────────────────────────────╯
```

- It can be dragged straight into Slack, Teams, email or a browser upload field.
- It auto-hides after 8 seconds, but stays while the mouse is over it.
- Copy puts the file on the clipboard, ready to paste.

### 4.5 Quick trim editor

A small window, not a full editor.

```
╭──────────────────────────────────────────────────────────╮
│                                                          │
│                    (video preview)                       │
│                                                          │
├──────────────────────────────────────────────────────────┤
│  ▶  00:12 / 02:41                                        │
│  ┃▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓┃  │
│  ◀ start                                       end ▶     │
├──────────────────────────────────────────────────────────┤
│                          [Cancel]  [Save copy]  [Save]   │
╰──────────────────────────────────────────────────────────╯
```

- The timeline shows thumbnails and the audio waveform.
- `I` and `O` set the in and out points; `Space` plays and pauses.
- Saving uses a lossless cut (stream copy), so it is instant and there is no quality loss.

### 4.6 Settings

A standard Windows 11 settings window with navigation on the left.

```
╭───────────────┬──────────────────────────────────────────╮
│  General      │  Video                                   │
│ ▸ Video       │                                          │
│  Audio        │  Quality preset      [ High          ▾ ] │
│  Hotkeys      │  Frame rate          [ 60 fps        ▾ ] │
│  Overlays     │  Codec               [ H.264         ▾ ] │
│  Storage      │  Encoder             [ NVIDIA NVENC  ▾ ] │
│  About        │                                          │
│               │  ▸ Advanced (CQ value, keyframes, …)     │
╰───────────────┴──────────────────────────────────────────╯
```

| Section | Contents |
|---|---|
| General | Start with Windows, countdown, theme, language |
| Video | Preset, fps, codec, encoder, *Advanced* (collapsed) |
| Audio | Devices, separate tracks, noise suppression, test meters |
| Hotkeys | Click-to-record shortcut fields; conflicts are flagged |
| Overlays | Cursor highlight, click ripple, keystrokes, webcam shape and size |
| Storage | Output folder, file name pattern, auto-delete older than N days, disk space warning |

### 4.7 Tray icon

| State | Icon | Left click | Right-click menu |
|---|---|---|---|
| Idle | Monochrome circle | Opens the launcher | Record screen · Record region · Open folder · Settings · Quit |
| Recording | Red filled circle | Stops | Pause · Stop · Discard |
| Paused | Amber circle | Resumes | Resume · Stop · Discard |

---

## 5. Visual design system

### Style

- **Fluent Design (Windows 11)**: Mica and Acrylic backdrops, 8 px corner radius on windows and 4 px on controls, soft elevation shadows.
- Flat, monochrome line icons (**Fluent UI System Icons**, regular weight; filled only for the active state).
- Little color: neutral surfaces, the system accent color for primary actions, and **red only for recording**.

### Color tokens

| Token | Light | Dark | Use |
|---|---|---|---|
| `surface` | `#F3F3F3` (Mica) | `#202020` (Mica) | Window background |
| `surface-raised` | `#FFFFFF` | `#2B2B2B` | Cards, pill |
| `text-primary` | `#1B1B1B` | `#FFFFFF` | Main text |
| `text-secondary` | `#5F5F5F` | `#C5C5C5` | Labels, hints |
| `accent` | System accent | System accent | Primary buttons, toggles |
| `record` | `#E5484D` | `#FF6369` | Record button, recording dot |
| `paused` | `#F5A524` | `#FFB224` | Paused state |
| `overlay-dim` | `rgba(0,0,0,0.4)` | `rgba(0,0,0,0.5)` | Region selection backdrop |

### Typography

| Role | Font | Size / weight |
|---|---|---|
| Title | Segoe UI Variable Display | 20 px Semibold |
| Body | Segoe UI Variable Text | 14 px Regular |
| Caption | Segoe UI Variable Text | 12 px Regular |
| Timer | Segoe UI Variable, tabular numbers | 14 px Semibold |

### Spacing and sizing

- 4 px base grid. Use spacing steps 4 / 8 / 12 / 16 / 24.
- Minimum click target 32 × 32 px (40 px on the control pill).
- The launcher is about 380 px wide; the control pill is about 44 px tall.

### Motion

| Interaction | Animation | Duration |
|---|---|---|
| Panel open/close | Fade + 8 px slide | 150 ms ease-out |
| Overlay dim | Fade | 120 ms |
| Recording dot | Opacity pulse 100% → 50% | 1.6 s loop |
| Thumbnail in/out | Slide from the screen edge | 200 ms |
| Countdown | Scale 1.2 → 1.0 + fade per number | 1 s per step |

Respect Windows' *Animation effects* setting (reduced motion): replace slides with plain fades.

---

## 6. Key features by UI surface

| Feature | Where it appears | Phase |
|---|---|---|
| Screen / Window / Region capture | Launcher, region overlay | MVP |
| System audio + mic with live meters | Launcher | MVP |
| Global hotkeys | Settings → Hotkeys | MVP |
| Countdown | Full-screen overlay | MVP |
| Control pill (timer, pause, stop, mute) | Floating while recording | MVP |
| Tray states | Tray | MVP |
| Post-recording thumbnail + quick actions | Bottom-right corner | MVP |
| Quality presets | Launcher dropdown, Settings | MVP |
| Webcam bubble (round, resizable, draggable) | On screen, recorded | v2 |
| Cursor highlight and click ripples | Settings → Overlays | v2 |
| Keystroke overlay | Settings → Overlays | v2 |
| Quick trim | Trim window | v2 |
| Recording history (grid of thumbnails) | Library window | v2 |
| Instant replay ("save last 5 min") | Hotkey + tray | v2 |
| On-screen drawing | Control pill → ✎ | v3 |
| Auto-zoom on clicks, smoothed cursor | Settings → Overlays / export | v3 |
| Blur regions | Region overlay | v3 |

---

## 7. Accessibility

- Every control is reachable by keyboard, with visible focus rings.
- Screen reader labels on all icon-only buttons (UI Automation `Name` properties).
- Contrast of at least 4.5:1 for text in both themes.
- Status is never shown by color alone: the paused state also shows a "Paused" label and a different icon.
- High-contrast themes are supported.
- The UI scales with Windows text-size settings.

---

## 8. Implementation approach

### UI technology

**Chosen: Tauri 2 + Svelte (or React) + TypeScript + Tailwind CSS**, on top of the Rust recording core. See [TECH-STACK.md](TECH-STACK.md).

| Need | How it's done in Tauri |
|---|---|
| Fluent look | Tailwind using the color tokens in section 5; Fluent UI System Icons (SVG); Segoe UI Variable font |
| Mica / Acrylic backdrop | `window-vibrancy` crate (`apply_mica`) |
| Follow system light/dark | `prefers-color-scheme` in CSS, plus Tauri's theme-changed event |
| Accent color | Read on the Rust side (`UISettings::GetColorValue(Accent)`) and pass to the frontend as a CSS variable |
| Animations | CSS transitions; Svelte transitions; turned off when `prefers-reduced-motion` is set |
| Hotkeys / tray | `tauri-plugin-global-shortcut`, Tauri tray API |

### Window types needed

Each is a separate Tauri `WebviewWindow`, created at startup and hidden until needed, so it opens instantly.

| Window | Tauri settings |
|---|---|
| Launcher | `decorations: false`, `always_on_top`, `skip_taskbar`, Mica; hides on blur |
| Region overlay | `transparent: true`, `always_on_top`, covers the whole virtual screen; per-monitor DPI aware |
| Control pill | `decorations: false`, `always_on_top`, `skip_taskbar`, `content_protected` (excluded from capture) |
| Recording border | `transparent`, `set_ignore_cursor_events(true)` (click-through), `content_protected` |
| Thumbnail | `decorations: false`, `always_on_top`, `skip_taskbar`; slides in |
| Settings / Trim | Normal window with Mica |

Overlay and pill windows need a quick test on multi-monitor setups with mixed scaling (100% + 150%).

### Build order

1. **Tray icon + launcher**, wired to the recording engine with fixed defaults.
2. **Control pill + hotkeys**, which together give a complete record/stop loop.
3. **Region overlay** for window and region modes.
4. **Post-recording thumbnail.**
5. **Settings window**: presets, devices, hotkeys, storage.
6. **Polish**: animations, light/dark themes, accessibility pass.
7. **v2 surfaces**: webcam bubble, overlays, trim, history.

### Architecture pattern

- **The Rust core owns all state.** A single session state machine (`Idle → Selecting → Countdown → Recording ⇄ Paused → Finishing`) lives in `recorder-core`.
- **The frontend only displays state.** Windows call Tauri commands (`start_recording`, `pause`, `stop`) and listen to events (`state-changed`, `tick`, `finished`). A small Svelte store mirrors the state, so the tray, pill and launcher never disagree.
- **Settings** are serde structs saved as JSON in `%APPDATA%`, with typed commands to read and update them.

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
