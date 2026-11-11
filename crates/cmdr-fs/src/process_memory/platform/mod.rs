//! Picks this OS's footprint reader.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

const _: fn() -> Option<u64> = footprint;

/// The memory this process alone holds, in bytes.
pub(super) use imp::footprint;
