//! What a transfer says when one of its two drives leaves mid-flight.
//!
//! The mount table decides, not the errno: a drive pulled mid-write answers
//! whatever the call in flight happened to hit. These drive
//! `name_the_vanished_drive` with the table answered by
//! [`test_hook`](super::test_hook), so no test needs a real drive.

use super::*;
use crate::error_classification::classify_io_error;

fn sides() -> TransferSides {
    TransferSides::new(
        TransferSide::new("vol-mac".to_string(), "Macintosh HD".to_string(), "/".into()),
        TransferSide::new(
            "vol-stick".to_string(),
            "Fältkamera".to_string(),
            "/Volumes/Stick".into(),
        ),
    )
}

/// Every errno a drive pulled mid-write can answer with, as `std::io::Error`
/// values: a lookup that finds nothing, a read that faults, a device node whose
/// hardware left, and a handle the kernel invalidated.
fn errnos_a_pulled_drive_answers() -> [(&'static str, std::io::Error); 4] {
    [
        ("ENOENT", std::io::Error::from_raw_os_error(libc::ENOENT)),
        ("EIO", std::io::Error::from_raw_os_error(libc::EIO)),
        ("ENXIO", std::io::Error::from_raw_os_error(libc::ENXIO)),
        ("EBADF", std::io::Error::from_raw_os_error(libc::EBADF)),
    ]
}

#[test]
fn a_destination_that_left_the_mount_table_names_itself_whatever_the_errno() {
    let sides = sides();
    let _hook = test_hook::pull(&sides.destination.root);

    for (name, raw) in errnos_a_pulled_drive_answers() {
        let error = classify_io_error(&raw, "/Volumes/Stick/trip/DSC1.arw".to_string());
        let named = name_the_vanished_drive(error, Some(&sides));
        match named {
            WriteOperationError::DeviceDisconnected { path, side } => {
                let side = side.expect("a vanished side is named");
                assert_eq!(side.role, TransferRole::Destination, "{name}");
                assert_eq!(side.volume_name, "Fältkamera", "{name}: the name captured at start");
                assert_eq!(side.counterpart_name, "Macintosh HD", "{name}");
                assert_eq!(path, "/Volumes/Stick/trip/DSC1.arw", "{name}: keeps the failing path");
            }
            other => panic!("{name} on a drive that left must read as a disconnect, got {other:?}"),
        }
    }
}

#[test]
fn a_source_that_left_names_itself_and_points_at_the_destination() {
    let sides = sides();
    let _hook = test_hook::pull(&sides.source.root);

    let error = classify_io_error(
        &std::io::Error::from_raw_os_error(libc::ENOENT),
        "/photos/DSC1.arw".to_string(),
    );
    let named = name_the_vanished_drive(error, Some(&sides));
    let WriteOperationError::DeviceDisconnected { side, .. } = named else {
        panic!("a source that left must read as a disconnect");
    };
    let side = side.expect("a vanished side is named");
    assert_eq!(side.role, TransferRole::Source);
    assert_eq!(side.volume_name, "Macintosh HD");
    assert_eq!(side.counterpart_name, "Fältkamera");
}

/// The whole point of asking the mount table: with both drives still listed,
/// a missing file is a missing file and a read fault is a read fault. Reading
/// either as a disconnect would send someone hunting for a drive that's right
/// there.
#[test]
fn errors_on_two_mounted_drives_keep_their_own_meaning() {
    let sides = sides();
    let _hook = test_hook::answer_each(vec![
        (sides.source.root.clone(), Some(true)),
        (sides.destination.root.clone(), Some(true)),
    ]);

    let missing = classify_io_error(
        &std::io::Error::from_raw_os_error(libc::ENOENT),
        "/Volumes/Stick/gone.txt".to_string(),
    );
    assert!(
        matches!(
            name_the_vanished_drive(missing, Some(&sides)),
            WriteOperationError::SourceNotFound { .. }
        ),
        "a missing file on a mounted drive stays a missing file"
    );

    let faulted = classify_io_error(
        &std::io::Error::from_raw_os_error(libc::EIO),
        "/Volumes/Stick/bad.txt".to_string(),
    );
    assert!(
        matches!(
            name_the_vanished_drive(faulted, Some(&sides)),
            WriteOperationError::IoError { .. }
        ),
        "a read fault on a mounted drive stays a read fault"
    );
}

/// `ENODEV` and `ENXIO` are the device itself answering, so they stay a
/// disconnect even before the mount table catches up — and the side is still
/// named, from the path the failure was on.
#[test]
fn typed_device_evidence_stays_a_disconnect_and_still_names_its_side() {
    let sides = sides();
    let _hook = test_hook::answer_each(vec![
        (sides.source.root.clone(), Some(true)),
        (sides.destination.root.clone(), Some(true)),
    ]);

    for errno in [libc::ENODEV, libc::ENXIO] {
        let error = classify_io_error(
            &std::io::Error::from_raw_os_error(errno),
            "/Volumes/Stick/trip/DSC1.arw".to_string(),
        );
        let WriteOperationError::DeviceDisconnected { side, .. } = name_the_vanished_drive(error, Some(&sides)) else {
            panic!("errno {errno} is the device saying it's gone");
        };
        assert_eq!(
            side.expect("named from the path it failed on").role,
            TransferRole::Destination
        );
    }
}

/// An unreadable mount table is not evidence. Claiming a disconnect from it
/// would tell someone their drive left when nothing observed it leaving.
#[test]
fn an_unreadable_mount_table_names_nobody() {
    let sides = sides();
    let _hook = test_hook::answer_each(vec![
        (sides.source.root.clone(), None),
        (sides.destination.root.clone(), None),
    ]);

    let error = classify_io_error(
        &std::io::Error::from_raw_os_error(libc::EIO),
        "/Volumes/Stick/bad.txt".to_string(),
    );
    assert!(matches!(
        name_the_vanished_drive(error, Some(&sides)),
        WriteOperationError::IoError { .. }
    ));
}

/// The user's own click. Rewriting it into a disconnect would blame their drive
/// for something they did.
#[test]
fn a_cancel_is_never_rewritten_into_a_disconnect() {
    let sides = sides();
    let _hook = test_hook::pull(&sides.destination.root);

    let cancelled = WriteOperationError::Cancelled {
        message: "Operation cancelled by user".into(),
    };
    assert!(matches!(
        name_the_vanished_drive(cancelled, Some(&sides)),
        WriteOperationError::Cancelled { .. }
    ));
}

/// Both drives on one pulled hub. The failing path is the only thing that says
/// which of them the operation was actually touching.
#[test]
fn with_both_drives_gone_the_failing_path_picks_the_side() {
    let sides = sides();
    let _hook = test_hook::answer_each(vec![
        (sides.source.root.clone(), Some(false)),
        (sides.destination.root.clone(), Some(false)),
    ]);

    let on_the_stick = classify_io_error(
        &std::io::Error::from_raw_os_error(libc::EIO),
        "/Volumes/Stick/trip/DSC1.arw".to_string(),
    );
    let WriteOperationError::DeviceDisconnected { side, .. } = name_the_vanished_drive(on_the_stick, Some(&sides))
    else {
        panic!("both drives are gone");
    };
    assert_eq!(side.expect("a named side").role, TransferRole::Destination);
}

/// An operation with no typed sides (a delete, a trash, an MTP or SMB transfer)
/// is left exactly as its backend worded it.
#[test]
fn an_operation_without_sides_is_left_alone() {
    let error = classify_io_error(
        &std::io::Error::from_raw_os_error(libc::ENOENT),
        "/Volumes/Stick/gone.txt".to_string(),
    );
    assert!(matches!(
        name_the_vanished_drive(error, None),
        WriteOperationError::SourceNotFound { .. }
    ));
}
