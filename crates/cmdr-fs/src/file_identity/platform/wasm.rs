//! The browser build has no local disk, so no metadata ever describes a real node.

use std::fs::Metadata;

use super::super::{DeviceId, FileId};

pub(crate) fn file_id(_metadata: &Metadata) -> Option<FileId> {
    None
}

pub(crate) fn device_of(_metadata: &Metadata) -> Option<DeviceId> {
    None
}

pub(crate) fn link_count(_metadata: &Metadata) -> Option<u64> {
    None
}
