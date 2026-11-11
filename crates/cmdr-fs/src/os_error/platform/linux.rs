//! Linux: the POSIX errnos and nothing more. Its `EOPNOTSUPP` is `ENOTSUP`, and a
//! missing xattr is `ENODATA`, which no caller tells apart.

use super::super::OsErrorClass;
use super::unix::POSIX;

pub(crate) const TABLES: &[&[(i32, OsErrorClass)]] = &[&POSIX];

pub(crate) fn class_of(code: i32) -> OsErrorClass {
    super::class_in(TABLES, code)
}

pub(crate) fn code_of(class: OsErrorClass) -> Option<i32> {
    super::code_in(TABLES, class)
}
