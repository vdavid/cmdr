//! `VolumeError::from_io_at`: a kind with a typed home lands in it, the same way
//! a remote backend would say it, and the errnos a classifier reads stay an
//! `IoError` carrying the number.

use std::io;

use super::VolumeError;

const AT: &str = "/Volumes/Installer/.cmdr-tmp-1";

fn at(kind: io::ErrorKind) -> VolumeError {
    VolumeError::from_io_at(&io::Error::from(kind), AT)
}

#[test]
fn the_three_path_carrying_variants_carry_the_path() {
    assert!(matches!(at(io::ErrorKind::NotFound), VolumeError::NotFound(path) if path == AT));
    assert!(matches!(at(io::ErrorKind::PermissionDenied), VolumeError::PermissionDenied { path, .. } if path == AT));
    assert!(matches!(at(io::ErrorKind::AlreadyExists), VolumeError::AlreadyExists(path) if path == AT));
}

/// The ERR-P7XKX case: a copy onto a mounted installer image, whose staged
/// `File::create` the OS refused with `EROFS`. As an `IoError` it reached the
/// user as a generic failure with a Retry that could only fail again.
#[test]
fn a_read_only_filesystem_is_read_only_and_names_the_path() {
    assert!(matches!(
        at(io::ErrorKind::ReadOnlyFilesystem),
        VolumeError::ReadOnly(path) if path == AT
    ));
}

/// The errno itself, so the test also pins that the OS's number decodes to the
/// kind the mapping keys on.
#[cfg(target_os = "macos")]
#[test]
fn the_os_errno_for_a_read_only_filesystem_is_read_only() {
    let err = VolumeError::from_io_at(&io::Error::from_raw_os_error(libc::EROFS), AT);
    assert!(matches!(err, VolumeError::ReadOnly(path) if path == AT));
}

#[test]
fn a_full_disk_and_a_spent_quota_are_storage_full() {
    for kind in [io::ErrorKind::StorageFull, io::ErrorKind::QuotaExceeded] {
        let err = at(kind);
        assert!(
            matches!(err, VolumeError::StorageFull { .. }),
            "{kind:?} should be StorageFull, got {err:?}"
        );
    }
}

#[test]
fn a_directory_where_a_file_was_expected_names_the_path() {
    assert!(matches!(
        at(io::ErrorKind::IsADirectory),
        VolumeError::IsADirectory(path) if path == AT
    ));
}

/// A full disk and an unusable name carry what the OS said, ❌ never the path: their variants
/// hold a message, which logs as plain text (so the path would ship through the report's regex
/// fallback only), and every caller already names the item from its own context.
#[test]
fn a_full_disk_or_an_unusable_name_keeps_the_path_out_of_its_message() {
    for kind in [
        io::ErrorKind::StorageFull,
        io::ErrorKind::QuotaExceeded,
        io::ErrorKind::InvalidFilename,
    ] {
        let err = at(kind);
        let message = match &err {
            VolumeError::StorageFull { message } | VolumeError::InvalidName(message) => message,
            other => panic!("{kind:?} landed in {other:?}"),
        };
        assert_eq!(message, &io::Error::from(kind).to_string(), "{kind:?}");
    }
}

/// `ENAMETOOLONG`: retrying the same name can only fail the same way, so it's
/// `InvalidName`, the variant that offers no Retry.
#[test]
fn a_name_the_filesystem_cant_hold_is_an_invalid_name() {
    assert!(matches!(
        at(io::ErrorKind::InvalidFilename),
        VolumeError::InvalidName(_)
    ));
}

/// ❗ `note_root_failure` promotes a mount on these errnos and transfer retry
/// treats them as a blip, and both read them off `IoError::raw_os_error`. A
/// typed variant here would silently switch both off.
#[cfg(target_os = "macos")]
#[test]
fn a_wedged_mount_stays_an_io_error_carrying_its_errno() {
    for errno in [
        libc::ETIMEDOUT,
        libc::ENOTCONN,
        libc::ESTALE,
        libc::EHOSTDOWN,
        libc::EHOSTUNREACH,
        libc::ENETDOWN,
        libc::ENETUNREACH,
    ] {
        let err = VolumeError::from_io_at(&io::Error::from_raw_os_error(errno), AT);
        assert!(
            matches!(err, VolumeError::IoError { raw_os_error: Some(code), .. } if code == errno),
            "errno {errno} must stay an IoError carrying it, got {err:?}"
        );
    }
}

/// No typed home: the classifiers dispatch on the number (`ENOTEMPTY` is how
/// every backend spells a non-empty folder), so it has to survive.
#[cfg(target_os = "macos")]
#[test]
fn kinds_without_a_typed_home_keep_their_errno() {
    for errno in [libc::ENOTEMPTY, libc::EXDEV, libc::ENOTDIR, libc::EFBIG, libc::EIO] {
        let err = VolumeError::from_io_at(&io::Error::from_raw_os_error(errno), AT);
        assert!(
            matches!(err, VolumeError::IoError { raw_os_error: Some(code), .. } if code == errno),
            "errno {errno} must stay an IoError carrying it, got {err:?}"
        );
    }
}

/// A listing failure logs `errno=` from this, and EACCES vs EPERM is what separates a POSIX
/// or ACL refusal from a TCC one.
#[cfg(target_os = "macos")]
#[test]
fn raw_os_error_reads_the_errno_a_variant_carries() {
    let denied = VolumeError::from_io_at(&io::Error::from_raw_os_error(libc::EPERM), AT);
    assert_eq!(denied.raw_os_error(), Some(libc::EPERM));
    let io_error = VolumeError::from_io_without_path(&io::Error::from_raw_os_error(libc::EIO));
    assert_eq!(io_error.raw_os_error(), Some(libc::EIO));
    assert_eq!(VolumeError::NotFound(AT.to_string()).raw_os_error(), None);
}

/// Log lines append `errno=13` when the variant carries one, and nothing when it doesn't.
#[cfg(target_os = "macos")]
#[test]
fn the_errno_field_renders_bare_and_only_when_present() {
    let denied = VolumeError::from_io_at(&io::Error::from_raw_os_error(libc::EACCES), AT);
    assert_eq!(
        format!("x{}", denied.errno_field()),
        format!("x, errno={}", libc::EACCES)
    );
    assert_eq!(format!("x{}", VolumeError::NotFound(AT.to_string()).errno_field()), "x");
}
