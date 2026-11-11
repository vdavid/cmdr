//! How each OS reads a file at an offset without a shared cursor. A platform seam
//! (`docs/guides/platform-seams.md`); `../source.rs` builds every local read on it.

use std::fs::File;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "browser.rs")]
mod imp;

#[cfg(unix)]
mod unix;

const _: fn(&File, &mut [u8], u64) -> std::io::Result<usize> = positioned_read;

/// One read of up to `buf.len()` bytes at `offset`, returning how many came back (0 at end of
/// file). It never depends on, or disturbs, another reader of the same handle, so concurrent
/// readers share one open file without a lock.
pub(crate) use imp::positioned_read;
