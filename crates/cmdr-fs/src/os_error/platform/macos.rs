//! macOS: the POSIX errnos, then the BSD ones Linux doesn't have.

use super::super::OsErrorClass;
use super::unix::POSIX;

/// macOS's own errnos. `EOPNOTSUPP` is its own number here, apart from `ENOTSUP`.
const BSD: [(i32, OsErrorClass); 6] = [
    (libc::EOPNOTSUPP, OsErrorClass::OperationNotSupported),
    (libc::EAUTH, OsErrorClass::AuthFailed),
    (libc::ENEEDAUTH, OsErrorClass::AuthNeeded),
    (libc::EPWROFF, OsErrorClass::DevicePoweredOff),
    (libc::EDEVERR, OsErrorClass::DeviceError),
    (libc::ENOATTR, OsErrorClass::NoAttribute),
];

pub(crate) const TABLES: &[&[(i32, OsErrorClass)]] = &[&POSIX, &BSD];

pub(crate) fn class_of(code: i32) -> OsErrorClass {
    super::class_in(TABLES, code)
}

pub(crate) fn code_of(class: OsErrorClass) -> Option<i32> {
    super::code_in(TABLES, class)
}
