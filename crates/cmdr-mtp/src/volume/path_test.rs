//! `MtpVolume`'s identity and its path conversions.
//!
//! Crate-side because every cell reaches the backend's own internals: the
//! `device_id` / `storage_id` fields, and `to_mtp_path`, which is the one place
//! the three shapes a caller can hand a path in (an `mtp://` URL, an absolute
//! path, a relative one) are reduced to the storage-relative string the PTP
//! layer takes. A wrong reduction is a read from the wrong object, which is why
//! this is asserted here rather than through the pipeline above it.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use super::MtpVolume;
use crate::connection::testing::test_connection_manager as connection_manager;

#[test]
fn test_new_creates_volume() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-20-5", 65537, "Internal storage");
    assert_eq!(vol.name(), "Internal storage");
    assert_eq!(vol.device_id, "mtp-20-5");
    assert_eq!(vol.storage_id, 65537);
}

#[test]
fn test_root_path() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-20-5", 65537, "Internal storage");
    assert_eq!(vol.root().to_string_lossy(), "mtp://mtp-20-5/65537");
}

#[test]
fn test_to_mtp_path_empty() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-20-5", 65537, "Test");
    assert_eq!(vol.to_mtp_path(Path::new("")), "");
    assert_eq!(vol.to_mtp_path(Path::new("/")), "");
    assert_eq!(vol.to_mtp_path(Path::new(".")), "");
}

#[test]
fn test_to_mtp_path_relative() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-20-5", 65537, "Test");
    assert_eq!(vol.to_mtp_path(Path::new("DCIM")), "DCIM");
    assert_eq!(vol.to_mtp_path(Path::new("DCIM/Camera")), "DCIM/Camera");
}

#[test]
fn test_to_mtp_path_absolute() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-20-5", 65537, "Test");
    assert_eq!(vol.to_mtp_path(Path::new("/DCIM")), "DCIM");
    assert_eq!(vol.to_mtp_path(Path::new("/DCIM/Camera")), "DCIM/Camera");
}

#[test]
fn test_to_mtp_path_mtp_url_root() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-0-1", 65537, "Test");
    // MTP URL for storage root
    assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537")), "");
}

#[test]
fn test_to_mtp_path_mtp_url_with_path() {
    let vol = MtpVolume::new(Arc::clone(connection_manager()), "mtp-0-1", 65537, "Test");
    // MTP URL with nested path
    assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM")), "DCIM");
    assert_eq!(
        vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM/Camera")),
        "DCIM/Camera"
    );
}
