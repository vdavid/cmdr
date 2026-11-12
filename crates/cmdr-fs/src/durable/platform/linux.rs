//! Linux: `fdatasync` and `fsync` flush the drive's cache themselves (`one_sync.rs`).

pub(crate) use super::one_sync::{
    NEEDS_DEVICE_BARRIER, device_barrier, sync_dir_to_drive, sync_file_data, sync_to_drive,
};
