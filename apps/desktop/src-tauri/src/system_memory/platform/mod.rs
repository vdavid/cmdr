//! Where each OS reads its memory breakdown. A platform seam: `docs/guides/platform-seams.md`.

use super::SystemMemoryInfo;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> SystemMemoryInfo = system_memory_info;

/// This OS's memory breakdown, in non-overlapping categories that sum to the total.
pub(super) use imp::system_memory_info;
