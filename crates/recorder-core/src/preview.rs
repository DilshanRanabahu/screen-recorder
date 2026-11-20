//! Live preview: a lightweight Windows.Graphics.Capture session that
//! downscales on the GPU and hands out small JPEG frames.
//!
//! Two threads, so a slow JPEG never holds up capture:
//!
//! - **capture thread** (windows-capture): copy the frame into a mipmapped
//!   texture → `GenerateMips` (GPU) → copy one small level (e.g. 960×540) to a
//!   staging texture → read it back → put it in a one-frame "latest" slot,
//!   replacing any frame the encoder hasn't picked up yet.
//! - **encoder thread**: take the newest frame → JPEG (Windows Imaging
//!   Component, or the pure-Rust encoder) → callback.
//!
//! The full-size frame never touches the CPU. The preview runs independently
//! of recording and can run alongside it. Windows marked
//! `WDA_EXCLUDEFROMCAPTURE` (the app's own window) are left out.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::{debug, warn};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_STAGING, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_SAMPLE_DESC};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
    SecondaryWindowSettings, Settings,
};

use crate::RecorderError;

type HandlerError = Box<dyn std::error::Error + Send + Sync>;

/// How preview frames are delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewEncoding {
    /// JPEG: small (≈80 KB at 960×540), costs a few ms of CPU to encode.
    Jpeg,
    /// Raw top-down RGBA: large (2 MB at 960×540) but no encoding. Measured
    /// to cost *more* overall, because of moving 2 MB per frame into the UI.
    Raw,
}

pub struct PreviewFrame<'a> {
    pub width: u32,
    pub height: u32,
    pub encoding: PreviewEncoding,
    /// JPEG bytes, or RGBA pixels for [`PreviewEncoding::Raw`].
    pub data: &'a [u8],
}

type FrameCallback = Box<dyn FnMut(PreviewFrame<'_>) + Send>;

/// A read-back frame waiting for the encoder.
struct Pending {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// One-frame mailbox between the capture and encoder threads.
#[derive(Default)]
struct Slot {
    frame: Mutex<Option<Pending>>,
    ready: Condvar,
    stop: AtomicBool,
    /// Frames replaced before the encoder got to them (encoder too slow).
    replaced: AtomicU64,
}

struct Config {
    max_width: u32,
    /// Minimum time between frames, in microseconds; changeable while running.
    min_interval_us: Arc<AtomicU64>,
    slot: Arc<Slot>,
}

/// GPU resources sized for the current capture size.
struct Scaler {
    src_size: (u32, u32),
    mips: ID3D11Texture2D,
    mips_view: ID3D11ShaderResourceView,
    level: u32,
    out_size: (u32, u32),
    staging: ID3D11Texture2D,
}

struct Handler {
    config: Config,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    scaler: Option<Scaler>,
    last_frame: Option<Instant>,
    /// Timing totals for the debug log: (frames, gpu + readback).
    timing: (u32, Duration),
}

// SAFETY: the D3D11 objects are only used on the capture thread that owns this
// handler; windows-capture requires the handler to be `Send` to move it there.
unsafe impl Send for Handler {}

impl Handler {
    fn scaler(&mut self, frame: &Frame) -> Result<&Scaler, HandlerError> {
        let size = (frame.width(), frame.height());
        if self.scaler.as_ref().is_none_or(|s| s.src_size != size) {
            self.scaler = Some(self.build_scaler(size, frame.desc().Format)?);
        }
        Ok(self.scaler.as_ref().expect("just built"))
    }

    fn build_scaler(&self, (w, h): (u32, u32), format: DXGI_FORMAT) -> Result<Scaler, HandlerError> {
        // First mip level at or below max_width (each level halves the size).
        let mut level = 0;
        while (w >> level) > self.config.max_width && (w >> (level + 1)) >= 16 {
            level += 1;
        }
        let out_size = ((w >> level).max(1), (h >> level).max(1));

        let mips_desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: level + 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: out_size.0,
            Height: out_size.1,
            MipLevels: 1,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..mips_desc
        };

        unsafe {
            let mut mips = None;
            self.device.CreateTexture2D(&mips_desc, None, Some(&mut mips))?;
            let mips = mips.ok_or("no mip texture")?;
            let mut mips_view = None;
            self.device.CreateShaderResourceView(&mips, None, Some(&mut mips_view))?;
            let mut staging = None;
            self.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
            debug!(src = ?(w, h), out = ?out_size, level, "preview scaler ready");
            Ok(Scaler {
                src_size: (w, h),
                mips,
                mips_view: mips_view.ok_or("no shader view")?,
                level,
                out_size,
                staging: staging.ok_or("no staging texture")?,
            })
        }
    }
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Config;
    type Error = HandlerError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            config: ctx.flags,
            device: ctx.device,
            context: ctx.device_context,
            scaler: None,
            last_frame: None,
            timing: (0, Duration::ZERO),
        })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _control: InternalCaptureControl) -> Result<(), Self::Error> {
        // Rate limit before doing any GPU work.
        let min = Duration::from_micros(self.config.min_interval_us.load(Ordering::Relaxed));
        if self.last_frame.is_some_and(|t| t.elapsed() < min) {
            return Ok(());
        }
        self.last_frame = Some(Instant::now());

        let t_start = Instant::now();
        let context = self.context.clone();
        let src = frame.as_raw_texture().clone();
        let s = self.scaler(frame)?;
        let (mips, view, staging, level, (ow, oh)) =
            (s.mips.clone(), s.mips_view.clone(), s.staging.clone(), s.level, s.out_size);

        let row = (ow * 4) as usize;
        let mut rgba = vec![0u8; row * oh as usize];
        unsafe {
            // GPU: full-size frame → mip 0, build the chain, copy the small level out.
            context.CopySubresourceRegion(&mips, 0, 0, 0, 0, &src, 0, None);
            if level > 0 {
                context.GenerateMips(&view);
            }
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &mips, level, None);

            // CPU: read back the small image (tightly packed RGBA).
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            for y in 0..oh as usize {
                let src_row = std::slice::from_raw_parts((mapped.pData as *const u8).add(y * mapped.RowPitch as usize), row);
                rgba[y * row..(y + 1) * row].copy_from_slice(src_row);
            }
            context.Unmap(&staging, 0);
        }

        // Hand over to the encoder, replacing a frame it hasn't taken yet.
        let slot = &self.config.slot;
        if slot.frame.lock().unwrap().replace(Pending { width: ow, height: oh, rgba }).is_some() {
            slot.replaced.fetch_add(1, Ordering::Relaxed);
        }
        slot.ready.notify_one();

        self.timing.0 += 1;
        self.timing.1 += t_start.elapsed();
        if self.timing.0 == 150 {
            debug!(
                gpu_readback_ms = format!("{:.2}", self.timing.1.as_secs_f64() * 1e3 / 150.0),
                "preview capture cost"
            );
            self.timing = (0, Duration::ZERO);
        }
        Ok(())
    }
}

/// JPEG encoder backends.
enum Jpeg {
    /// Windows Imaging Component (built into Windows, SIMD-optimized).
    Wic(wic::Encoder),
    /// Pure Rust (`jpeg-encoder`), used if WIC is unavailable.
    Rust,
}

impl Jpeg {
    fn new() -> Self {
        if std::env::var("SR_PREVIEW_JPEG").is_ok_and(|v| v == "rust") {
            return Self::Rust;
        }
        match wic::Encoder::new() {
            Ok(e) => Self::Wic(e),
            Err(e) => {
                warn!(error = %e, "WIC JPEG encoder unavailable; using the Rust encoder");
                Self::Rust
            }
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Wic(_) => "wic",
            Self::Rust => "rust",
        }
    }

    fn encode(&self, f: &Pending, quality: u8, out: &mut Vec<u8>) -> Result<(), HandlerError> {
        out.clear();
        match self {
            Self::Wic(e) => e.encode(&f.rgba, f.width, f.height, quality, out)?,
            Self::Rust => jpeg_encoder::Encoder::new(&mut *out, quality).encode(
                &f.rgba,
                f.width as u16,
                f.height as u16,
                jpeg_encoder::ColorType::Rgba,
            )?,
        }
        Ok(())
    }
}

/// Encoder thread: waits for the newest frame, encodes it, calls back.
fn encode_loop(slot: &Slot, encoding: PreviewEncoding, quality: u8, mut on_frame: FrameCallback) {
    let jpeg = (encoding == PreviewEncoding::Jpeg).then(Jpeg::new);
    let mut out = Vec::with_capacity(256 * 1024);
    let (mut n, mut total) = (0u32, Duration::ZERO);

    loop {
        let frame = {
            let mut guard = slot.frame.lock().unwrap();
            loop {
                if slot.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(f) = guard.take() {
                    break f;
                }
                guard = slot.ready.wait(guard).unwrap();
            }
        };

        let t = Instant::now();
        let data = match &jpeg {
            Some(j) => match j.encode(&frame, quality, &mut out) {
                Ok(()) => &out[..],
                Err(e) => {
                    warn!(error = %e, "preview JPEG encode failed");
                    continue;
                }
            },
            None => &frame.rgba[..],
        };
        n += 1;
        total += t.elapsed();
        on_frame(PreviewFrame { width: frame.width, height: frame.height, encoding, data });

        if n == 150 {
            debug!(
                encoder = jpeg.as_ref().map_or("raw", Jpeg::name),
                encode_ms = format!("{:.2}", total.as_secs_f64() * 1e3 / 150.0),
                replaced = slot.replaced.swap(0, Ordering::Relaxed),
                "preview encode cost"
            );
            (n, total) = (0, Duration::ZERO);
        }
    }
}

/// A running preview. Dropping it stops the capture.
pub struct PreviewSession {
    control: Option<CaptureControl<Handler, HandlerError>>,
    encoder: Option<JoinHandle<()>>,
    slot: Arc<Slot>,
    min_interval_us: Arc<AtomicU64>,
}

impl PreviewSession {
    /// Starts previewing `display` (one-based) at up to `fps`, scaled to at
    /// most `max_width` pixels wide (by halving: 1920 → 960 for 960).
    /// `on_frame` runs on the encoder thread for every new frame. Windows only
    /// delivers frames when the screen changes, so a still screen sends none.
    pub fn start(
        display: usize,
        max_width: u32,
        fps: u32,
        encoding: PreviewEncoding,
        on_frame: impl FnMut(PreviewFrame<'_>) + Send + 'static,
    ) -> Result<Self, RecorderError> {
        let monitor = Monitor::from_index(display).map_err(|_| RecorderError::DisplayNotFound(display))?;
        let min_interval_us = Arc::new(AtomicU64::new(interval_us(fps)));
        let slot = Arc::new(Slot::default());

        let encoder_slot = Arc::clone(&slot);
        let on_frame: FrameCallback = Box::new(on_frame);
        let encoder = std::thread::Builder::new()
            .name("preview-encoder".into())
            .spawn(move || encode_loop(&encoder_slot, encoding, 72, on_frame))
            .map_err(|e| RecorderError::Capture(format!("preview encoder: {e}")))?;

        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            // Windows may deliver up to 60 fps; the handler applies the current limit.
            MinimumUpdateIntervalSettings::Custom(Duration::from_secs(1) / 60),
            DirtyRegionSettings::Default,
            // RGBA, so raw frames go straight into an ImageData with no swizzle.
            ColorFormat::Rgba8,
            Config { max_width: max_width.max(64), min_interval_us: Arc::clone(&min_interval_us), slot: Arc::clone(&slot) },
        );
        let mut session = Self { control: None, encoder: Some(encoder), slot, min_interval_us };
        session.control = Some(
            Handler::start_free_threaded(settings).map_err(|e| RecorderError::Capture(format!("preview: {e:?}")))?,
        );
        let display_index = display;
        debug!(display_index, max_width, fps, "preview started");
        Ok(session)
    }

    /// Changes the frame-rate limit without restarting the capture.
    pub fn set_fps(&self, fps: u32) {
        self.min_interval_us.store(interval_us(fps), Ordering::Relaxed);
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(control) = self.control.take()
            && let Err(e) = control.stop()
        {
            warn!(error = ?e, "preview capture ended with an error");
        }
        self.slot.stop.store(true, Ordering::Relaxed);
        self.slot.ready.notify_all();
        if let Some(encoder) = self.encoder.take() {
            let _ = encoder.join();
        }
    }
}

impl Drop for PreviewSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Minimum frame interval for an fps limit, with 10% slack so frames that
/// arrive slightly early (Windows paces them at the display's refresh) aren't
/// skipped and the real rate doesn't halve.
fn interval_us(fps: u32) -> u64 {
    1_000_000 * 9 / 10 / u64::from(fps.clamp(1, 60))
}

/// JPEG encoding with the Windows Imaging Component.
mod wic {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory,
        WICBitmapEncoderNoCache,
    };
    use windows::Win32::System::Com::StructuredStorage::{IPropertyBag2, PROPBAG2};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STREAM_SEEK_CUR};
    use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_R4};
    use windows::core::{PWSTR, w};

    pub struct Encoder {
        factory: IWICImagingFactory,
        /// Output buffer the WIC stream writes into (reused).
        buffer: std::cell::RefCell<Vec<u8>>,
    }

    impl Encoder {
        pub fn new() -> windows::core::Result<Self> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
                Ok(Self { factory, buffer: std::cell::RefCell::new(Vec::new()) })
            }
        }

        pub fn encode(&self, rgba: &[u8], width: u32, height: u32, quality: u8, out: &mut Vec<u8>) -> windows::core::Result<()> {
            unsafe {
                // A JPEG of an RGBA frame is far smaller than the frame; size the
                // memory stream generously so it never runs out.
                let mut buffer = self.buffer.borrow_mut();
                buffer.resize(rgba.len() / 2 + 64 * 1024, 0);
                let stream = self.factory.CreateStream()?;
                stream.InitializeFromMemory(&buffer[..])?;

                let encoder = self.factory.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
                encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
                let mut frame = None;
                let mut props: Option<IPropertyBag2> = None;
                encoder.CreateNewFrame(&mut frame, &mut props)?;
                let frame = frame.ok_or_else(windows::core::Error::empty)?;
                if let Some(props) = &props {
                    let name = w!("ImageQuality");
                    let bag = PROPBAG2 { pstrName: PWSTR(name.as_ptr().cast_mut()), ..Default::default() };
                    let value = VARIANT {
                        Anonymous: VARIANT_0 {
                            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                                vt: VT_R4,
                                wReserved1: 0,
                                wReserved2: 0,
                                wReserved3: 0,
                                Anonymous: VARIANT_0_0_0 { fltVal: f32::from(quality) / 100.0 },
                            }),
                        },
                    };
                    props.Write(1, &bag, &value)?;
                }
                frame.Initialize(props.as_ref())?;
                frame.SetSize(width, height)?;
                // The encoder converts RGBA to its own format (24-bit) itself.
                let bitmap = self.factory.CreateBitmapFromMemory(width, height, &GUID_WICPixelFormat32bppRGBA, width * 4, rgba)?;
                frame.WriteSource(&bitmap, std::ptr::null())?;
                frame.Commit()?;
                encoder.Commit()?;

                let mut written = 0u64;
                stream.Seek(0, STREAM_SEEK_CUR, Some(&mut written))?;
                out.extend_from_slice(&buffer[..written as usize]);
                Ok(())
            }
        }
    }
}
