# Optimization Checklist

## Image quality

- [ ] **Per-Monitor DPI Awareness V2** for the process. Tauri sets this by default; confirm it at startup with `GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext())`. Without it, Windows can hand the app a scaled capture on 125%/150% displays and the text comes out blurry. This is the most common cause of fuzzy recordings.
- [ ] Capture at native resolution, and crop on the GPU rather than scaling.
- [ ] Use a high-quality scaler (bicubic/Lanczos) only when output scaling is requested.
- [ ] Tag the color space correctly (BT.709, full range) so players don't shift colors.
- [ ] Tone-map HDR displays to SDR. Otherwise the recording looks washed out.

## Performance (low lag)

- [ ] **Zero-copy GPU pipeline**: capture texture → GPU convert → hardware encoder. Never use `Map()` on frames to read them on the CPU.
- [ ] Share one `ID3D11Device` between capture, conversion and encoding, so no cross-device copies happen.
- [ ] On two-GPU laptops, create that device on the adapter that drives the captured monitor (the Intel GPU on the dev laptop), and encode there too. See "Choosing the GPU" in [ARCHITECTURE.md](ARCHITECTURE.md).
- [ ] Laptops: record on mains power, and keep Windows on the *Balanced* or *Best performance* power mode. Battery saver throttles the GPU.
- [ ] Use a free-threaded WGC frame pool with 2–3 buffers. Process frames immediately so the pool never runs dry.
- [ ] Register the capture and encode threads with **MMCSS** (`AvSetMmThreadCharacteristics(L"Capture")`) and the audio threads with `"Pro Audio"`.
- [ ] Set process priority to *Above Normal* while recording.
- [ ] Use bounded queues between threads. When overloaded, drop video frames (and log them), never audio.
- [ ] Use an async, buffered file writer. Record to an **SSD**.
- [ ] Turn on **Hardware-accelerated GPU scheduling** in Windows Settings → Display → Graphics. It reduces latency on many systems.
- [ ] Keep the recorder's own UI out of the capture: Tauri's `set_content_protected(true)`, or `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` through the `windows` crate.
- [ ] Throttle the UI and preview while recording. A live preview should be small and low-fps.

## Rust-specific

- [ ] **No allocations per frame.** Reuse texture pools, packet buffers and `Vec`s; allocate once when the session starts.
- [ ] **No locks on the hot path.** Pass frames through `crossbeam-channel` instead of sharing them behind a `Mutex`. Use `parking_lot` if a lock is unavoidable.
- [ ] **Never block a capture callback.** Hand the frame to a channel with `try_send` and return immediately.
- [ ] **Release profile** in the workspace `Cargo.toml`:
  ```toml
  [profile.release]
  opt-level = 3
  lto = "fat"
  codegen-units = 1
  panic = "abort"   # optional; only if no code relies on catching panics
  strip = true
  ```
- [ ] Use `profile.dev.package."*"` with `opt-level = 2`, so dependencies stay fast even in debug builds.
- [ ] Keep `unsafe` COM/Win32 code in small wrapper modules, and release COM objects in `Drop`. The `windows` crate does this automatically for its interface types.
- [ ] Profile with **Tracy** (`tracing-tracy`) or **Superluminal** to find frame-time spikes; check the GPU with **PIX** or **GPUView**.
- [ ] Log per-frame timing with `tracing` at `trace` level only. Logging every frame at `info` costs real time.

## Smoothness (frame pacing)

- [ ] Timestamp every frame with QPC at the time it was captured, not when it gets processed.
- [ ] Output a constant frame rate: duplicate the last frame when the screen is static, and skip extras when frames arrive faster than the target.
- [ ] Use a high-resolution waitable timer (`CreateWaitableTimerExW` with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`) for pacing, not `std::thread::sleep`, which can oversleep by several milliseconds on Windows.
- [ ] Match the recording fps to a divisor of the monitor refresh rate (60 on 60/120/240 Hz, 72 on 144 Hz if smoothness matters most).

## Audio and video sync

- [ ] Use a single QPC clock for audio and video.
- [ ] Fill gaps with silence when loopback reports no data.
- [ ] Correct clock drift on long recordings with tiny resampling adjustments.
- [ ] Keep pause/resume timestamps continuous by subtracting the paused time.

## Reliability

- [ ] Write crash-safe MKV or fragmented MP4, flushed every few seconds.
- [ ] Recover leftover `.recording.mkv` files on startup.
- [ ] Check free disk space before and during recording. Warn below 5 GB and stop safely at 1 GB.
- [ ] Handle display changes (resolution change, monitor unplugged, lock screen) without crashing. End the segment cleanly.
- [ ] Handle the audio device being unplugged by switching to the new default device.
- [ ] Log dropped frames, encoder lag and disk queue depth to a log file for troubleshooting.

## Measuring

Track these during test recordings:

| Metric | Target |
|---|---|
| Dropped frames | 0 (< 0.1% acceptable) |
| Encode time per frame | < 50% of the frame interval (< 8 ms at 60 fps) |
| CPU usage | < 5% |
| A/V offset after 60 min | < 40 ms |
| Start-recording latency | < 300 ms after hotkey |
