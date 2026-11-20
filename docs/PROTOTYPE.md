# Prototype with FFmpeg

Before writing app code, use FFmpeg to check that your GPU can record smoothly at the quality you want. FFmpeg's `ddagrab` source uses Desktop Duplication and keeps frames on the GPU, so these commands follow the same zero-copy path the app will use.

## 1. Install FFmpeg

```powershell
winget install Gyan.FFmpeg
```

Open a new terminal, then list the hardware encoders FFmpeg was built with:

```powershell
ffmpeg -hide_banner -encoders | findstr /i "nvenc amf qsv"
```

> This list shows what FFmpeg *supports*, not what your hardware has. On the dev laptop, `h264_nvenc` appears in the list but fails when used, because the GeForce MX230 has no NVENC encoder. Use the **Intel** commands below.

## 2. Record the screen (video only)

**Intel QuickSync (use this on the dev laptop)**
```powershell
ffmpeg -init_hw_device d3d11va -filter_hw_device d3d11va `
  -f lavfi -i ddagrab=output_idx=0:framerate=60 `
  -vf "hwmap=derive_device=qsv,format=qsv" `
  -c:v h264_qsv -preset slow -global_quality 21 -g 120 `
  test_qsv.mkv
```

For smaller files, try `-c:v hevc_qsv -global_quality 23` instead.

**NVIDIA (GTX/RTX cards only; not the MX230)**
```powershell
ffmpeg -f lavfi -i ddagrab=output_idx=0:framerate=60 `
  -c:v h264_nvenc -preset p5 -tune hq -rc vbr -cq 19 -b:v 0 -g 120 `
  test_nvenc.mkv
```

**AMD**
```powershell
ffmpeg -f lavfi -i ddagrab=output_idx=0:framerate=60 `
  -c:v h264_amf -quality quality -rc cqp -qp_i 19 -qp_p 21 -g 120 `
  test_amf.mkv
```

Press `q` to stop.

If the Intel command fails with a device error, update the Intel graphics driver first, then try again.

Useful `ddagrab` options:
- `output_idx=1`: second monitor
- `offset_x=100:offset_y=100:video_size=1280x720`: capture a region
- `draw_mouse=0`: hide the cursor

## 3. Add audio

List devices:
```powershell
ffmpeg -hide_banner -list_devices true -f dshow -i dummy
```

Record the microphone alongside the screen (replace the device name):
```powershell
ffmpeg -f lavfi -i ddagrab=framerate=60 `
  -f dshow -i audio="Microphone (Realtek(R) Audio)" `
  -map 0:v -map 1:a `
  -c:v h264_mf -hw_encoding 1 -rate_control quality -quality 70 `
  -c:a aac -b:a 192k `
  test_av.mkv
```

This example uses `h264_mf`, FFmpeg's Media Foundation encoder. It picks the hardware encoder automatically (QuickSync on the dev laptop), which is the same route the Rust app takes for the MVP.

> FFmpeg cannot capture system audio (WASAPI loopback) directly on Windows. Use a virtual device such as "Stereo Mix" if your sound card has one. The real app will use WASAPI loopback instead.

## 4. Convert to MP4 (no quality loss)

```powershell
ffmpeg -i test_av.mkv -c copy -movflags +faststart test_av.mp4
```

## 5. What to check

- [ ] Text is sharp when you play the file back at 100% zoom
- [ ] Scrolling and window dragging look smooth
- [ ] FFmpeg's `speed=` stays at **1.0x** or above, and `drop=` stays near 0
- [ ] Task Manager shows low CPU use, with activity under **GPU 0 (Intel UHD Graphics)** → "Video Encode"
- [ ] The laptop is plugged in during the test (battery mode lowers GPU power)
- [ ] Audio stays in sync after a 10-minute test

Once these pass, the pipeline works on your hardware, and you can build the app around it.

## 6. First Rust prototype

After the FFmpeg test, check the same pipeline from Rust before building any UI.

Install the toolchain:
```powershell
winget install Rustlang.Rustup
rustup default stable-x86_64-pc-windows-msvc
```
Rust on Windows also needs the **Visual Studio Build Tools** with the "Desktop development with C++" workload.

Create a test project:
```powershell
cargo new capture-test
cd capture-test
cargo add windows-capture
```

Copy the screen-recording example from the [`windows-capture` README](https://github.com/NiiightmareXD/windows-capture). It captures the primary monitor through Windows.Graphics.Capture and writes an MP4 with its built-in Media Foundation encoder. Then run it in release mode:
```powershell
cargo run --release
```

Check the result against the same list as in step 5: sharp text, smooth motion, low CPU, and activity under "Video Encode" in Task Manager. The crate's API changes between versions, so follow the README for the version that `cargo add` installed.
