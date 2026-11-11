//! The local backend's path flavor, per OS: one `/` root on macOS, Linux, and in the browser
//! build (whose demo tree is POSIX), drives and UNC shares on Windows.

#[cfg_attr(any(unix, target_family = "wasm"), path = "posix.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: super::PathFlavor = HOST_FLAVOR;

/// This OS's flavor.
pub(super) use imp::HOST_FLAVOR;
