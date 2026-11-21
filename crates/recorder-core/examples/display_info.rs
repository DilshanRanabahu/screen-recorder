//! Prints each display's colour space and whether Windows HDR is on.
//!
//! ```text
//! cargo run --release -p recorder-core --example display_info
//! ```

use windows::Win32::Graphics::Dxgi::Common::{DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6};
use windows::core::Interface;

fn main() -> windows::core::Result<()> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            let desc = adapter.GetDesc1()?;
            let name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                let out6: IDXGIOutput6 = output.cast()?;
                let d = out6.GetDesc1()?;
                let device = String::from_utf16_lossy(&d.DeviceName).trim_end_matches('\0').to_string();
                let space = match d.ColorSpace {
                    DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 => "HDR (BT.2100 PQ)".to_string(),
                    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 => "SDR (sRGB / BT.709)".to_string(),
                    other => format!("other ({})", other.0),
                };
                let r = d.DesktopCoordinates;
                println!(
                    "{name} | {device} | {}x{} at ({},{}) | {space} | {} bpc | max {:.0} nits",
                    r.right - r.left,
                    r.bottom - r.top,
                    r.left,
                    r.top,
                    d.BitsPerColor,
                    d.MaxLuminance
                );
                o += 1;
            }
            a += 1;
        }
    }
    Ok(())
}
