//! The OS half of the test fixtures: links, privilege, and special files, so a test asks
//! for a capability (`can_create_symlinks()`, `is_root()`) instead of naming an OS.

use std::io;
use std::path::Path;

#[cfg(unix)]
mod unix;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&'static Path, &'static Path) -> io::Result<()> = symlink::<&'static Path, &'static Path>;
const _: fn() -> bool = is_root;
const _: fn() -> bool = can_create_symlinks;

/// Creates a symlink at `link` pointing at `target`, which may be relative (to `link`'s
/// folder) and may not exist. Windows keeps file and folder links apart, so there the
/// kind follows the target, and a dangling one gets a file link.
pub use imp::symlink;

/// Whether the test runs with superuser rights, which bypass the permission refusals a
/// test means to provoke, so such a test skips itself.
pub use imp::is_root;

/// Whether this process may create symlinks: always on Unix; on Windows only with
/// Developer Mode or an elevated process, so a link test skips itself without them.
pub use imp::can_create_symlinks;

/// A named pipe (FIFO) at `path`, the special file a walk must neither read nor copy.
/// Unix-only: Windows has no FIFO on disk.
#[cfg(unix)]
pub use unix::make_fifo;
