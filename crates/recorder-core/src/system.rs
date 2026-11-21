//! Small Windows helpers: thread scheduling priority and power state.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW};
use windows::core::HSTRING;

/// While alive, the current thread is scheduled by MMCSS (the Multimedia
/// Class Scheduler Service) as a multimedia task, so it keeps running on
/// time when the PC is busy (games, builds). Dropping it restores the
/// normal priority.
pub struct MmcssGuard(HANDLE);

impl MmcssGuard {
    /// `task` is an MMCSS task name, e.g. `"Capture"` or `"Pro Audio"`.
    /// Returns `None` if MMCSS is unavailable; the thread then just keeps
    /// its normal priority.
    pub fn register(task: &str) -> Option<Self> {
        let mut index = 0u32;
        match unsafe { AvSetMmThreadCharacteristicsW(&HSTRING::from(task), &mut index) } {
            Ok(handle) => Some(Self(handle)),
            Err(e) => {
                tracing::debug!(task, error = %e, "MMCSS registration failed");
                None
            }
        }
    }
}

impl Drop for MmcssGuard {
    fn drop(&mut self) {
        let _ = unsafe { AvRevertMmThreadCharacteristics(self.0) };
    }
}

/// True when the PC runs on battery (laptop unplugged).
pub fn on_battery() -> bool {
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status) }.is_ok() && status.ACLineStatus == 0
}
