//! How each OS catches a crash the panic hook can't see, and the process facts a crash report
//! records. A platform seam (`docs/guides/platform-seams.md`); the report, its files, and the
//! next-launch pass are portable.

use std::path::Path;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

#[cfg(unix)]
mod unix;

const _: fn(&Path, &Path) = install_native_crash_hooks;
const _: fn() -> u64 = current_image_base;
const _: fn() -> usize = current_thread_count;

/// Arms this OS's native-crash capture: the raw crash file at `raw_crash_path`, plus any
/// sidecar under `data_dir`. Call only after the next-launch pass read the previous session's
/// files, never before.
pub(super) use imp::install_native_crash_hooks;

/// The main image's load address in THIS process, or 0 when unknown. Only meaningful for a
/// report built in-process: a relaunched process has a different ASLR slide.
pub(super) use imp::current_image_base;

/// How many threads this process has right now, or 0 when the OS can't say.
pub(super) use imp::current_thread_count;
