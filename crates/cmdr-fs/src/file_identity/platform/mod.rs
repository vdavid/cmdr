//! Picks this OS's reader for [`FileId`], [`DeviceId`], and the link count.

use std::fs::Metadata;

use super::{DeviceId, FileId};

#[cfg(unix)]
mod unix;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

const _: fn(&Metadata) -> Option<FileId> = file_id;
const _: fn(&Metadata) -> Option<DeviceId> = device_of;
const _: fn(&Metadata) -> Option<u64> = link_count;

/// The node `metadata` describes, or `None` where this OS's metadata carries no id.
pub(super) use imp::file_id;

/// The device `metadata`'s node lives on, or `None` where this OS's metadata doesn't say.
pub(super) use imp::device_of;

/// The node's name count, or `None` where this OS's metadata doesn't say.
pub(super) use imp::link_count;
