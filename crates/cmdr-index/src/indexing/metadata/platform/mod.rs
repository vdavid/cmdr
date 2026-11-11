//! The two metadata facts the index reads differently per OS: the allocated size, and the
//! modified time in whole seconds.

use std::fs::Metadata;

#[cfg(unix)]
mod unix;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&Metadata) -> Option<u64> = allocated_bytes;
const _: fn(&Metadata) -> Option<u64> = modified_secs;

/// The bytes the file occupies on disk, or `None` where the metadata gives no block count.
pub(super) use imp::allocated_bytes;

/// The modified time in whole seconds since the epoch, `None` before it.
pub(super) use imp::modified_secs;
