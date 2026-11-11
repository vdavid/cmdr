//! Each OS's own startup installers: the observers, watchers, and native hooks the setup hook
//! starts once and that no other OS has. A platform seam (`docs/guides/platform-seams.md`).
//!
//! Only order-free work goes here. A step whose position in the setup hook is part of its
//! contract stays on the spine in `lib.rs`, where its ordering comment can be read at the call.

use tauri::AppHandle;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn(&AppHandle) = install;

/// Starts this OS's installers. Call once, from the setup hook, on the main thread.
///
/// Its place in the hook is the strictest of what it starts: after `volume_broadcast::init`
/// (the volume watcher emits), before the MTP hotplug watcher (an earlier session may have left
/// `ptpcamerad` suppressed), before the first volume list goes out, and before the menu bar is
/// built (macOS decides the Services submenu from the send types registered by then).
pub(crate) use imp::install;
