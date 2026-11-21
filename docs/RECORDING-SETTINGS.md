# Recording Settings

## Development machine

| Part | Model | Hardware encoding |
|---|---|---|
| CPU | Intel Core i5-10210U (4 cores, 10th gen) | Too slow for software encoding at 1080p60. Avoid x264 |
| Integrated GPU | Intel UHD Graphics | **Intel QuickSync: H.264, HEVC 8-bit and 10-bit.** No AV1 |
| Dedicated GPU | NVIDIA GeForce MX230 (2 GB) | **None.** The MX230 has no NVENC encoder |

**Default encoder on this machine: Intel QuickSync via Media Foundation.** The Intel GPU also drives the display on this laptop, so capturing and encoding on it keeps every frame on one GPU with no cross-adapter copies.

Realistic targets:

| Use | Setting |
|---|---|
| Daily recording | 1080p, 60 fps, H.264 via QuickSync |
| Smaller files | 1080p, 60 fps, HEVC via QuickSync |
| Upper limit | 4K at 30 fps (HEVC) |

Before recording on this laptop:
- Keep the **Intel graphics driver up to date**. The installed driver (27.20.100.9079) is from about 2021.
- **Plug in the charger.** On battery, Windows lowers GPU power and frames can drop.

The app must still support NVENC and AMF for other machines, so it detects encoders at startup rather than assuming one vendor.

## Quality presets

| Setting | Standard | High (default) | Maximum |
|---|---|---|---|
| Resolution | Native | Native | Native |
| Frame rate | 30 fps | 60 fps | 60 fps |
| Codec | H.264 High | H.264 High | HEVC Main10 (or AV1 where supported; not on the dev laptop) |
| Rate control | CQ/VBR | CQ/VBR | CQ/VBR |
| Quality value (CQ) | 23 | 19 | 16 |
| Encoder preset | QSV `medium` / NVENC `p4` / AMF `balanced` | QSV `slow` / NVENC `p5` / AMF `quality` | QSV `veryslow` / NVENC `p7` / AMF `quality` |
| Keyframe interval | 2 s | 2 s | 2 s |
| B-frames | 2 | 2 | 3 |
| Audio | AAC 128 kbps | AAC 192 kbps | Opus 160 kbps or PCM |
| Use for | Long meetings, small files | Tutorials, demos | Archive, editing source |

**Why constant quality (CQ/CRF) instead of fixed bitrate?** Screen content is often static: the encoder spends almost nothing on still frames and more on fast changes. At the same file size, CQ gives sharper text than CBR.

## Bitrate reference

Use these only when a fixed bitrate is required (for example, streaming).

| Resolution | 30 fps | 60 fps |
|---|---|---|
| 1920×1080 | 8–12 Mbps | 12–20 Mbps |
| 2560×1440 | 14–20 Mbps | 20–30 Mbps |
| 3840×2160 | 25–40 Mbps | 40–60 Mbps |

Set max bitrate to 1.5× the target to absorb bursts.

## Codec guide

| Codec | Compatibility | Size at equal quality | Hardware support |
|---|---|---|---|
| **H.264** | Plays everywhere (browsers, phones, Slack, Teams) | 100% | All GPUs since about 2012 |
| **HEVC (H.265)** | Most players. Windows may need the HEVC extension | ~55–60% | NVIDIA GTX 900+, AMD RX 400+, Intel 6th gen+ |
| **AV1** | Modern browsers and players | ~45–50% | NVIDIA RTX 40+, AMD RX 7000+, Intel Arc |

Default to H.264 for sharing and HEVC/AV1 for archiving.

## Text clarity

- **Never downscale** screen recordings that contain text. Record at native resolution.
- Chroma subsampling 4:2:0 (the default) can make thin colored text slightly fuzzy. For maximum text sharpness, use **HEVC 4:4:4** (NVENC supports it; files grow about 30%).
- 30 fps is enough for code and documents. Use 60 fps when there is scrolling, animation or video.

## Colour accuracy

Recordings are encoded as **BT.709, limited range** (black = 16, white = 235) and **labelled as such** in the video stream (`color_range=tv`, `color_space/primaries/transfer=bt709`). Measured on the dev laptop, every grey level lands exactly on the standard value:

| Screen value | 0 | 16 | 64 | 128 | 192 | 255 |
|---|---|---|---|---|---|---|
| Encoded luma (Y) | 16 | 30 | 71 | 126 | 181 | 235 |

**Why the labels matter.** Recordings made before November 21, 2026 had *no* colour labels. Players then have to guess, and a player that guesses "full range" shows black (16) as dark grey and white (235) as off-white: the whole picture looks washed out and too bright. The encoding itself was always correct; only the label was missing.

Fix an older recording without re-encoding (instant, no quality loss):

```powershell
ffmpeg -i old.mp4 -c copy -bsf:v "h264_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1" fixed.mp4
```

Why not full range (0–255)? Many players and editors ignore the full-range flag and then crush shadows and highlights. Limited range with correct labels is what OBS, cameras and streaming sites use, so it plays correctly everywhere.

Check a recording: `ffprobe -v error -select_streams v -show_entries stream=color_range,color_space -of compact file.mp4` should print `color_range=tv|color_space=bt709`.

## Container

| Phase | Format | Reason |
|---|---|---|
| While recording | MKV (or fragmented MP4) | Survives crashes and power loss |
| After stop | MP4 (`+faststart`) | Universal playback; streams in browsers before fully downloaded |

Remuxing is a stream copy: no quality loss, and it takes seconds.

## Example FFmpeg equivalents

**High preset, Intel QuickSync (dev laptop default):**
```
-c:v h264_qsv -preset slow -global_quality 21 -look_ahead 0 -g 120 -profile:v high
```

**Smaller files, Intel QuickSync HEVC:**
```
-c:v hevc_qsv -preset slow -global_quality 23 -g 120
```

High preset, NVIDIA:
```
-c:v h264_nvenc -preset p5 -tune hq -rc vbr -cq 19 -b:v 0 -maxrate 40M -bf 2 -g 120 -profile:v high
```

Maximum preset, NVIDIA HEVC 10-bit:
```
-c:v hevc_nvenc -preset p7 -tune hq -rc vbr -cq 16 -b:v 0 -profile:v main10 -pix_fmt p010le -g 120
```

AMD (H.264, constant QP; AMF has no NVENC-style `-cq`):
```
-c:v h264_amf -quality quality -rc cqp -qp_i 19 -qp_p 21 -qp_b 23 -g 120
```

Encoder options vary between FFmpeg versions and drivers. Check them with `ffmpeg -h encoder=<name>`.
