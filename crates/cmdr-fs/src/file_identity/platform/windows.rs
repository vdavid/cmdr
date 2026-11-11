//! Windows keeps a node's id (volume serial plus a 128-bit file id) behind a handle, not
//! in `std::fs::Metadata`, so nothing here can read it yet: every answer is "don't know",
//! which callers take as the cautious branch. The reader is
//! `GetFileInformationByHandleEx(FileIdInfo)` on an open handle.

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
