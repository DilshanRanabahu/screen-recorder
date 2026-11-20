//! Media Foundation sink writer: hardware H.264/HEVC + AAC into fragmented MP4.
//!
//! - Video arrives as BGRA D3D11 textures. With a DXGI device manager and
//!   hardware transforms enabled, Media Foundation converts BGRA → NV12 on the
//!   GPU and encodes with the GPU's encoder (QuickSync / NVENC / AMF).
//! - Each audio source gets its own AAC track, so levels can be fixed later.
//! - Fragmented MP4 is written progressively, so the file stays playable up
//!   to the last fragment if the app crashes or the power goes out.

use std::path::Path;
use std::sync::OnceLock;

use windows::Win32::Graphics::Direct3D10::ID3D10Multithread;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::{HSTRING, Interface};

use crate::RecorderError;
use crate::audio::{self, AudioTrack};
use crate::options::Codec;

fn err(context: &'static str) -> impl Fn(windows::core::Error) -> RecorderError {
    move |e| RecorderError::Encoder(format!("{context}: {e}"))
}

fn mf_startup() -> Result<(), RecorderError> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|e| e.to_string()))
        .clone()
        .map_err(RecorderError::Encoder)
}

fn pack(hi: u32, lo: u32) -> u64 {
    (u64::from(hi) << 32) | u64::from(lo)
}

pub struct VideoFormat {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
    pub codec: Codec,
}

/// Owns the sink writer. All methods are called with the writer's mutex held.
pub struct Writer {
    writer: IMFSinkWriter,
    video_stream: u32,
    audio_streams: Vec<(AudioTrack, u32)>,
    // Kept alive for the writer's lifetime.
    _device_manager: IMFDXGIDeviceManager,
}

// SAFETY: Media Foundation objects created here are free-threaded (MTA), and
// every call goes through a Mutex, so only one thread uses the writer at a time.
unsafe impl Send for Writer {}

impl Writer {
    pub fn create(
        path: &Path,
        device: &ID3D11Device,
        video: &VideoFormat,
        audio_tracks: &[AudioTrack],
    ) -> Result<Self, RecorderError> {
        mf_startup()?;
        unsafe {
            // The capture thread and Media Foundation share this device.
            let _ = device
                .cast::<ID3D10Multithread>()
                .map_err(err("multithread"))?
                .SetMultithreadProtected(true);

            let mut token = 0u32;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager).map_err(err("device manager"))?;
            let manager = manager.ok_or_else(|| RecorderError::Encoder("no device manager".into()))?;
            manager.ResetDevice(device, token).map_err(err("reset device"))?;

            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 4).map_err(err("attributes"))?;
            let attrs = attrs.ok_or_else(|| RecorderError::Encoder("no attributes".into()))?;
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1).map_err(err("hw transforms"))?;
            attrs.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager).map_err(err("d3d manager"))?;
            // Never block the capture/mixer threads; the hardware encoder keeps up.
            attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).map_err(err("throttling"))?;
            attrs
                .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)
                .map_err(err("container"))?;

            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attrs)
                .map_err(err("create sink writer"))?;

            let video_stream = Self::add_video(&writer, video)?;
            let mut audio_streams = Vec::new();
            for &track in audio_tracks {
                audio_streams.push((track, Self::add_audio(&writer)?));
            }

            writer.BeginWriting().map_err(err("begin writing"))?;
            Ok(Self { writer, video_stream, audio_streams, _device_manager: manager })
        }
    }

    unsafe fn add_video(writer: &IMFSinkWriter, v: &VideoFormat) -> Result<u32, RecorderError> {
        unsafe {
            let out = MFCreateMediaType().map_err(err("video type"))?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err("video major"))?;
            let subtype = match v.codec {
                Codec::H264 => MFVideoFormat_H264,
                Codec::Hevc => MFVideoFormat_HEVC,
            };
            out.SetGUID(&MF_MT_SUBTYPE, &subtype).map_err(err("video subtype"))?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, v.bitrate).map_err(err("bitrate"))?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(err("interlace"))?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, pack(v.width, v.height)).map_err(err("frame size"))?;
            out.SetUINT64(&MF_MT_FRAME_RATE, pack(v.fps, 1)).map_err(err("frame rate"))?;
            out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(err("aspect"))?;
            if v.codec == Codec::H264 {
                out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32).map_err(err("profile"))?;
            }
            let stream = writer.AddStream(&out).map_err(err("add video stream"))?;

            let input = MFCreateMediaType().map_err(err("video input type"))?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(err("input major"))?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32).map_err(err("input subtype"))?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(err("input interlace"))?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(v.width, v.height)).map_err(err("input size"))?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(v.fps, 1)).map_err(err("input rate"))?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(err("input aspect"))?;

            // Keyframe every 2 s, for quick seeking and small fragments.
            let mut params = None;
            MFCreateAttributes(&mut params, 1).map_err(err("encoder params"))?;
            let params = params.ok_or_else(|| RecorderError::Encoder("no params".into()))?;
            params.SetUINT32(&CODECAPI_AVEncMPVGOPSize, v.fps * 2).map_err(err("gop"))?;

            writer.SetInputMediaType(stream, &input, &params).map_err(err("video input (is a hardware encoder available?)"))?;
            Ok(stream)
        }
    }

    unsafe fn add_audio(writer: &IMFSinkWriter) -> Result<u32, RecorderError> {
        let channels = audio::CHANNELS as u32;
        unsafe {
            let out = MFCreateMediaType().map_err(err("audio type"))?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(err("audio major"))?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(err("audio subtype"))?;
            out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, audio::SAMPLE_RATE).map_err(err("aac rate"))?;
            out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels).map_err(err("aac channels"))?;
            out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(err("aac bits"))?;
            out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000 / 8).map_err(err("aac bitrate"))?;
            out.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0).map_err(err("aac payload"))?;
            let stream = writer.AddStream(&out).map_err(err("add audio stream"))?;

            let input = MFCreateMediaType().map_err(err("pcm type"))?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(err("pcm major"))?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(err("pcm subtype"))?;
            input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, audio::SAMPLE_RATE).map_err(err("pcm rate"))?;
            input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels).map_err(err("pcm channels"))?;
            input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(err("pcm bits"))?;
            input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2).map_err(err("pcm align"))?;
            input
                .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, audio::SAMPLE_RATE * channels * 2)
                .map_err(err("pcm bytes"))?;
            writer.SetInputMediaType(stream, &input, None).map_err(err("audio input"))?;
            Ok(stream)
        }
    }

    /// Queues one video frame. `time` and `duration` are in 100 ns units.
    pub fn write_video(&mut self, texture: &ID3D11Texture2D, time: i64, duration: i64) -> Result<(), RecorderError> {
        unsafe {
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false)
                .map_err(err("surface buffer"))?;
            let len = buffer.cast::<IMF2DBuffer>().and_then(|b| b.GetContiguousLength()).map_err(err("buffer length"))?;
            buffer.SetCurrentLength(len).map_err(err("set length"))?;
            let sample = MFCreateSample().map_err(err("sample"))?;
            sample.AddBuffer(&buffer).map_err(err("add buffer"))?;
            sample.SetSampleTime(time).map_err(err("sample time"))?;
            sample.SetSampleDuration(duration).map_err(err("sample duration"))?;
            self.writer.WriteSample(self.video_stream, &sample).map_err(err("write video"))
        }
    }

    /// Queues 16-bit stereo PCM for one track. `time` is in 100 ns units.
    pub fn write_audio(&mut self, track: AudioTrack, pcm: &[u8], time: i64) -> Result<(), RecorderError> {
        let Some(&(_, stream)) = self.audio_streams.iter().find(|(t, _)| *t == track) else {
            return Ok(());
        };
        let frames = pcm.len() / (audio::CHANNELS * 2);
        let duration = frames as i64 * 10_000_000 / i64::from(audio::SAMPLE_RATE);
        unsafe {
            let buffer = MFCreateMemoryBuffer(pcm.len() as u32).map_err(err("audio buffer"))?;
            let mut ptr = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None).map_err(err("lock"))?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr(), ptr, pcm.len());
            buffer.Unlock().map_err(err("unlock"))?;
            buffer.SetCurrentLength(pcm.len() as u32).map_err(err("audio length"))?;
            let sample = MFCreateSample().map_err(err("audio sample"))?;
            sample.AddBuffer(&buffer).map_err(err("audio add"))?;
            sample.SetSampleTime(time).map_err(err("audio time"))?;
            sample.SetSampleDuration(duration).map_err(err("audio duration"))?;
            self.writer.WriteSample(stream, &sample).map_err(err("write audio"))
        }
    }

    /// Flushes the encoders and closes the file.
    pub fn finalize(self) -> Result<(), RecorderError> {
        unsafe { self.writer.Finalize() }.map_err(err("finalize"))
    }
}
