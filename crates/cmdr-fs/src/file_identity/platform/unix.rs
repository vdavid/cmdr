//! Both Unixes answer from the `stat` the metadata already holds: no extra syscall.

use std::fs::Metadata;
use std::os::unix::fs::MetadataExt as _;

use super::super::{DeviceId, FileId};

pub(crate) fn file_id(metadata: &Metadata) -> Option<FileId> {
    Some(FileId::from_parts(
        DeviceId::from_raw(metadata.dev()),
        u128::from(metadata.ino()),
    ))
}

pub(crate) fn device_of(metadata: &Metadata) -> Option<DeviceId> {
    Some(DeviceId::from_raw(metadata.dev()))
}

pub(crate) fn link_count(metadata: &Metadata) -> Option<u64> {
    Some(metadata.nlink())
}
