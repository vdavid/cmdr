//! What each OS can say about the machine for a diagnostic snapshot. A platform seam
//! (`docs/guides/platform-seams.md`): an OS with no answer for a field says `None`, and the
//! snapshot records it as unknown.

use std::path::Path;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

// The POSIX half macOS and Linux share.
#[cfg(unix)]
mod unix;

const _: fn() -> Option<String> = os_build;
const _: fn() -> Option<String> = hardware_model;
const _: fn() -> Option<u32> = cpu_physical;
const _: fn() -> Option<u32> = cpu_logical;
const _: fn() -> Option<String> = thermal_state;
const _: fn(&Path) -> (Option<u64>, Option<u64>) = volume_space;

/// The OS build number, which pins the exact OS more tightly than the product version (macOS:
/// `25F80`).
pub(super) use imp::os_build;

/// The hardware model identifier (macOS: `Mac15,9`).
pub(super) use imp::hardware_model;

/// Physical and logical CPU core counts, the logical one including SMT.
pub(super) use imp::{cpu_logical, cpu_physical};

/// Thermal pressure: `nominal` / `fair` / `serious` / `critical` (`unknown` for an unrecognized
/// raw value).
pub(super) use imp::thermal_state;

/// Free (available to the user) and total bytes of the volume holding `path`. `path` is the local
/// app data dir, so this never hits a slow network mount.
pub(super) use imp::volume_space;
