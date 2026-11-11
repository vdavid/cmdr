//! What a memory-watchdog trip can say beyond the footprint, per OS: macOS reads the full
//! allocator breakdown (`memory_snapshot.rs`); elsewhere the footprint alone is reported.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

use super::breakdown::Breakdown;

const _: fn() -> Option<Breakdown> = breakdown;

/// The breakdown right now, or `None` where this OS gives none (or its read failed).
pub(super) use imp::breakdown;
