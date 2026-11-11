//! Picks this OS's mount layout.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> &'static [&'static str] = user_mount_prefixes;

/// This OS's user-mount prefixes.
pub(super) use imp::user_mount_prefixes;
