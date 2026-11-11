//! The seam's own promises: each OS's table names a code once, every class it names
//! reads back, and the one Windows-only class never comes out of a Unix code.

use super::OsErrorClass;
use super::platform::TABLES;

#[test]
fn a_code_is_named_once() {
    let mut seen = std::collections::HashSet::new();
    for &(code, class) in TABLES.iter().flat_map(|table| table.iter()) {
        assert!(
            seen.insert(code),
            "code {code} is named twice (second time as {class:?})"
        );
    }
}

#[test]
fn every_named_code_reads_back_as_its_class_and_every_class_has_a_code_of_its_own() {
    for &(code, class) in TABLES.iter().flat_map(|table| table.iter()) {
        assert_eq!(OsErrorClass::of_code(code), class, "code {code}");
        let host = class.host_code().expect("a named class has a code");
        assert_eq!(OsErrorClass::of_code(host), class, "{class:?}'s code {host}");
    }
}

#[test]
fn a_code_nobody_names_is_other_and_other_has_no_code() {
    assert_eq!(OsErrorClass::of_code(-1), OsErrorClass::Other);
    assert_eq!(OsErrorClass::of_code(999_999), OsErrorClass::Other);
    assert_eq!(OsErrorClass::Other.host_code(), None);
}

#[test]
fn an_io_error_without_a_code_has_no_class() {
    assert_eq!(OsErrorClass::of_io(&std::io::Error::other("a backend's words")), None);
}

/// "In use by another app" is Windows' sharing and lock violations. No Unix errno
/// means it (`EBUSY` is a busy mount or device, `ETXTBSY` a running program), so no
/// Unix code may produce it, and it has no Unix code to report.
#[cfg(unix)]
#[test]
fn no_unix_code_is_in_use_by_another_app() {
    for code in 0..=4096 {
        assert_ne!(
            OsErrorClass::of_code(code),
            OsErrorClass::InUseByAnotherApp,
            "errno {code}"
        );
    }
    assert_eq!(OsErrorClass::InUseByAnotherApp.host_code(), None);
    assert_eq!(OsErrorClass::of_code(libc::EBUSY), OsErrorClass::Busy);
    assert_eq!(OsErrorClass::of_code(libc::ETXTBSY), OsErrorClass::Other);
}

/// The transport set is the one `std` reads as a dropped link, errno for errno:
/// the set this replaced was decided through `ErrorKind`.
#[cfg(unix)]
#[test]
fn the_transport_classes_are_the_errnos_std_reads_as_a_dropped_link() {
    use std::io::ErrorKind;

    for code in 0..=4096 {
        let by_kind = matches!(
            std::io::Error::from_raw_os_error(code).kind(),
            ErrorKind::BrokenPipe
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::NotConnected
                | ErrorKind::TimedOut
                | ErrorKind::NetworkDown
                | ErrorKind::NetworkUnreachable
                | ErrorKind::HostUnreachable
        );
        assert_eq!(OsErrorClass::of_code(code).is_transport(), by_kind, "errno {code}");
    }
}

/// The numbers that differ between the two Unixes land in one class each.
#[cfg(unix)]
#[test]
fn each_unix_names_its_own_numbers() {
    assert_eq!(OsErrorClass::of_code(libc::ENOTEMPTY), OsErrorClass::NotEmpty);
    assert_eq!(OsErrorClass::of_code(libc::EDQUOT), OsErrorClass::QuotaExceeded);
    assert_eq!(OsErrorClass::of_code(libc::ESTALE), OsErrorClass::Stale);
    assert_eq!(OsErrorClass::of_code(libc::ENOSYS), OsErrorClass::NotImplemented);
    #[cfg(target_os = "linux")]
    assert_eq!(OsErrorClass::of_code(libc::EOPNOTSUPP), OsErrorClass::Unsupported);
    #[cfg(target_os = "macos")]
    assert_eq!(
        OsErrorClass::of_code(libc::EOPNOTSUPP),
        OsErrorClass::OperationNotSupported
    );
}
