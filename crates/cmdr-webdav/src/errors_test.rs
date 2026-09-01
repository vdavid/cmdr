//! The status table, one cell per row.

use cmdr_fs::volume::VolumeError;
use reqwest::StatusCode;

use super::{Attempted, EBUSY, map_status};

const PATH: &str = "/Photos/a.jpg";

#[test]
fn a_refusal_carries_the_path() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        assert!(matches!(map_status(status, PATH, Attempted::Reaching), VolumeError::PermissionDenied(p) if p == PATH));
    }
}

#[test]
fn a_404_and_a_409_are_both_not_found() {
    assert!(
        matches!(map_status(StatusCode::NOT_FOUND, PATH, Attempted::Reaching), VolumeError::NotFound(p) if p == PATH)
    );
    assert!(
        matches!(map_status(StatusCode::CONFLICT, PATH, Attempted::TakingAName), VolumeError::NotFound(p) if p == PATH)
    );
}

#[test]
fn a_405_means_already_exists_only_when_taking_a_name() {
    assert!(matches!(
        map_status(StatusCode::METHOD_NOT_ALLOWED, PATH, Attempted::TakingAName),
        VolumeError::AlreadyExists(p) if p == PATH
    ));
    assert!(matches!(
        map_status(StatusCode::METHOD_NOT_ALLOWED, PATH, Attempted::Reaching),
        VolumeError::NotSupported
    ));
}

#[test]
fn a_412_means_already_exists_only_when_taking_a_name() {
    assert!(matches!(
        map_status(StatusCode::PRECONDITION_FAILED, PATH, Attempted::TakingAName),
        VolumeError::AlreadyExists(p) if p == PATH
    ));
    assert!(matches!(
        map_status(StatusCode::PRECONDITION_FAILED, PATH, Attempted::Reaching),
        VolumeError::IoError { .. }
    ));
}

#[test]
fn a_lock_is_ebusy() {
    assert!(matches!(
        map_status(StatusCode::LOCKED, PATH, Attempted::Reaching),
        VolumeError::IoError {
            raw_os_error: Some(EBUSY),
            ..
        }
    ));
}

#[test]
fn a_507_is_storage_full_and_a_501_is_not_supported() {
    assert!(matches!(
        map_status(StatusCode::INSUFFICIENT_STORAGE, PATH, Attempted::Reaching),
        VolumeError::StorageFull { .. }
    ));
    assert!(matches!(
        map_status(StatusCode::NOT_IMPLEMENTED, PATH, Attempted::Reaching),
        VolumeError::NotSupported
    ));
}

#[test]
fn anything_else_keeps_the_status_number() {
    assert!(matches!(
        map_status(StatusCode::BAD_GATEWAY, PATH, Attempted::Reaching),
        VolumeError::IoError { message, raw_os_error: None } if message.contains("502")
    ));
}
