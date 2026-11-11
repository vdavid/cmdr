//! Who this process is to an NFS server: the machine name and the uid and gid `AUTH_SYS`
//! carries, per OS.

#[cfg(unix)]
mod unix;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> String = machine_name;
const _: fn() -> (u32, u32) = process_ids;

/// This machine's host name, or empty when the OS won't say.
pub(super) use imp::machine_name;

/// The uid and gid this process runs as, which is who `EXPORT` asks as.
pub(super) use imp::process_ids;
