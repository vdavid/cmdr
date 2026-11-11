//! The browser build: no OS hands it a code, so every code is `Other` and no class
//! has one.

use super::super::OsErrorClass;

pub(crate) const TABLES: &[&[(i32, OsErrorClass)]] = &[];

pub(crate) fn class_of(code: i32) -> OsErrorClass {
    super::class_in(TABLES, code)
}

pub(crate) fn code_of(class: OsErrorClass) -> Option<i32> {
    super::code_in(TABLES, class)
}
