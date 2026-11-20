# Architecture

## Principle: keep frames on the GPU

Most laggy recorders copy each frame from GPU memory into system memory and then compress it on the CPU. At 1440p60 that means moving about 900 MB/s and saturating several CPU cores.

This design keeps every frame on the GPU from capture until it has been compressed. Only the small compressed bitstream (a few MB/s) reaches the CPU.

## Pipeline

```
┌────────────────────────── GPU ──────────────────────────┐
│                                                         │
│  Windows.Graphics.Capture ──► D3D11 Video Processor ──► Hardware encoder
│  (BGRA texture per frame)     (BGRA → NV12/P010,        (NVENC / AMF / QSV)
│                                optional scale/crop)            │
└────────────────────────────────────────────────────────────────┼───┘
                                                                 │ H.264/HEVC/AV1 packets
                                                                 ▼
 WASAPI loopback (system audio) ──► AAC/Opus encoder ──►   Muxer (MKV / fragmented MP4)
 WASAPI capture  (microphone)   ──► AAC/Opus encoder ──►        │
                                                                ▼
                                                        Async file writer → SSD
```

## Components

### 1. Video capture

| API | Use | Notes |
|---|---|---|
| **Windows.Graphics.Capture (WGC)** | Primary | Captures monitors *and* individual windows, including windows that are occluded. Hardware-accelerated. Windows 10 1903+; turning off the yellow border needs Windows 11 or Windows 10 2104+. |
| DXGI Desktop Duplication (DDA) | Fallback | Monitors only. Very low latency, and provides dirty rects. Fails in some RDP/elevated scenarios. |
| GDI BitBlt | Avoid | CPU-bound and slow at high resolutions. |

Capture settings:
- `GraphicsCaptureSession.IsCursorCaptureEnabled`: on, or draw the cursor yourself to add highlights.
- `IsBorderRequired = false` (Windows 11) to hide the yellow capture border.
- Frame pool: `Direct3D11CaptureFramePool.CreateFreeThreaded` with 2–3 buffers.

### Choosing the GPU (laptops with two GPUs)

Laptops like the dev machine (Intel UHD + NVIDIA MX230) have two GPUs. The Intel GPU drives the built-in display, and the NVIDIA GPU only renders apps that ask for it.

- Create the D3D11 device on the **adapter that owns the monitor being captured**. Find it by matching the monitor's `HMONITOR` against each adapter's outputs (`IDXGIAdapter::EnumOutputs`).
- Use the hardware encoder on **that same adapter**. On the dev laptop that is Intel QuickSync, and frames never cross between GPUs.
- Only fall back to a different adapter's encoder if the display adapter has none. That requires a cross-adapter copy, which costs performance.
- At startup, list each adapter with its available encoders (H.264 / HEVC / AV1) in the log, so encoder problems are easy to diagnose.

### 2. Color conversion and scaling

The capture returns 8-bit BGRA. Encoders want NV12 (8-bit 4:2:0) or P010 (10-bit). Convert with `ID3D11VideoProcessor`, which runs on the GPU's fixed-function hardware at almost no cost. Do the region crop and any scaling in the same pass.

For HDR monitors, capture in `R16G16B16A16Float` and tone-map to SDR in a pixel shader. Without that step, colors look washed out.

### 3. Video encoding

Use the GPU's hardware encoder through one of these:
- **Media Foundation** `IMFSinkWriter` / `IMFTransform` (built into Windows, works with all three vendors). Called from Rust through the `windows` crate. **Used for the MVP.**
- **FFmpeg libavcodec** via the `ffmpeg-next` crate (`h264_nvenc`, `hevc_amf`, `h264_qsv`, `av1_nvenc`, …) using `AV_HWDEVICE_TYPE_D3D11VA` hardware frames, so textures go in without a copy. **Added later** for AV1, 10-bit and finer rate control.

Both sit behind one Rust trait, so the rest of the pipeline doesn't change when the backend does:

```rust
pub trait VideoEncoder: Send {
    fn encode(&mut self, frame: &GpuFrame) -> Result<(), EncodeError>; // NV12/P010 texture + PTS
    fn drain(&mut self) -> Result<Vec<EncodedPacket>, EncodeError>;    // compressed packets ready for the muxer
    fn finish(&mut self) -> Result<Vec<EncodedPacket>, EncodeError>;   // flush on stop
}
```

Fall back to software x264 only when no hardware encoder exists.

### 4. Audio capture

- **System audio**: WASAPI loopback (`AUDCLNT_STREAMFLAGS_LOOPBACK`) on the default render device.
- **Microphone**: WASAPI shared-mode capture.
- Keep them as **separate tracks** in the container, so levels can be adjusted or the mic removed later.
- Resample everything to 48 kHz.
- When nothing is playing, loopback delivers no packets. Insert silence so the timeline has no gaps.

### 5. Muxing and file writing

- Record into **MKV** or **fragmented MP4**. Both stay playable if the app crashes.
- When recording stops, remux to a standard MP4 (stream copy, no re-encode, takes seconds).
- Write on a dedicated thread with a bounded queue, so slow disk I/O never stalls the encoder.

## Threading model

| Thread | Responsibility | Priority |
|---|---|---|
| UI | Window, tray, hotkeys | Normal |
| Capture | Receives WGC frames, timestamps them, submits them to the GPU convert step | MMCSS "Capture" |
| Video encode | Feeds the hardware encoder and drains packets | MMCSS "Capture" |
| Audio capture ×2 | WASAPI event-driven loops | MMCSS "Pro Audio" |
| Mux/writer | Interleaves packets and writes to disk | Above normal |

Threads communicate through lock-free or bounded queues. If a queue fills up, drop *video* frames (and log them). Never drop audio, because gaps in audio are far more noticeable.

### In Rust

- Capture, encode, audio and writer threads are plain `std::thread`s, not async tasks. Each one sets its MMCSS priority as soon as it starts.
- Threads are connected by `crossbeam_channel::bounded` channels. The capture side uses `try_send`: a full queue means "drop this video frame and count it", never block.
- Tokio (from Tauri) is used only for UI commands, settings and file housekeeping.
- A `Session` struct owns every thread handle. `stop()` sends a shutdown message, waits for each thread in order (capture → encode → mux), and returns the final file path. Dropping a `Session` stops it cleanly, so a panic cannot leave threads recording in the background.
- COM is initialized per thread (`CoInitializeEx` with `COINIT_MULTITHREADED`) on the capture, encode and audio threads.

## As built (Phase 1)

The engine lives in `crates/recorder-core/src`:

| Module | Role | Main APIs / crates |
|---|---|---|
| `session.rs` | `Recorder`: start → pause ⇄ resume → stop. Owns every thread; dropping it finalizes the file | |
| `clock.rs` | `MediaClock`: one media time for video and audio. Starts when the encoder is ready; frozen while paused | `std::time::Instant` |
| `video.rs` | `FrameRing` (8 GPU textures) + video pacer thread: submits the latest frame every 1/fps, repeating it while the screen is static → constant frame rate | `windows` (D3D11) |
| `writer.rs` | Media Foundation **sink writer**: BGRA textures in, GPU colour conversion + hardware H.264/HEVC, one AAC track per audio source, **fragmented MP4** | `windows` (Media Foundation) |
| `audio.rs` | WASAPI capture per source + audio pacer thread: emits exactly the samples due on the media clock, silence-filled | `wasapi` |
| `preview.rs` | Live preview: separate WGC session → GPU mipmap downscale → small readback; JPEG (WIC) on its own encoder thread → callback | `windows` (D3D11, WIC), `jpeg-encoder` (fallback) |
| `display.rs`, `options.rs` | Monitor list, presets, file naming | `windows-capture` |

Screen capture uses `windows-capture` (Windows.Graphics.Capture) only to receive frames; its built-in encoder is **not** used. That encoder ignores the gaps between frames, so video drifted ahead of audio whenever the screen was static.

Threads during a recording:

| Thread | Does |
|---|---|
| capture (windows-capture) | GPU-copies each new screen frame into the ring |
| `video-pacer` | Writes one frame per tick to the sink writer |
| `audio-system`, `audio-microphone` | Poll WASAPI every 5 ms |
| `audio-pacer` | Every 10 ms, writes due samples for each track |
| Media Foundation (internal) | Colour conversion, encoding, muxing |

The D3D11 device is shared between capture and Media Foundation, with multithread protection on. Sink-writer throttling is off so writes never block the pacers. One mutex guards the writer.

Measured on the dev laptop (i5-10210U, Intel UHD, QuickSync), 1080p60 H.264:

| Check | Result |
|---|---|
| Frame rate | Constant 60 fps, 0 dropped frames |
| A/V offset | 37–48 ms, including Windows playback-start latency |
| Pause | 2 s pause → no gap; file = recorded time only |
| Crash (process killed) | File plays up to the last fragment, decodes without errors |

The Tauri app (`src-tauri`) only calls into `recorder-core` and forwards its events to the UI. Recording code never depends on Tauri.

## Timing and A/V sync

- Use **one clock** for everything: `QueryPerformanceCounter` (QPC). WGC frame `SystemRelativeTime` and WASAPI `qpcPosition` are both on QPC.
- PTS = (sample QPC − recording start QPC), converted to the stream timebase.
- **Constant frame rate output**: run a pacing timer at the target fps. If WGC delivers no new frame because the screen is static, re-submit the previous texture. Editors (Premiere, DaVinci Resolve) handle CFR much better than VFR.
- **Pause/resume**: accumulate paused duration and subtract it from all later timestamps.
- **Drift correction**: on recordings longer than about 30 minutes, the audio device clock can drift from QPC. Resample slightly (±0.1%) to keep audio aligned.

## Crash safety

**As built:** the sink writer writes **fragmented MP4** directly to the final `.mp4` file. Each fragment is self-contained, so a crash or power loss keeps everything up to the last fragment, and no remux step is needed. Keyframes every 2 s keep fragments short.

Possible later improvements:
- On stop, remux to a regular (non-fragmented) MP4 with `+faststart`, for the widest editor compatibility.
- On startup, detect recordings that were not finalized (log entry without "recording saved") and tell the user.

## Known issues

- **Rare colour glitch:** one keyframe in one test had wrong colours (correct brightness) for a single frame. It was not reproduced in about 2,500 frames over 5 further runs. Suspected cause: concurrent use of the shared D3D11 device. Scan test recordings for colour outliers to catch it if it returns.
- **WGC capture delivers frames at up to the target rate only;** the pacer repeats frames, so very fast content above the frame rate is sampled, not blended.
