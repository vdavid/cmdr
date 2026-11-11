//! Reading an OS error code, per OS: errno numbers on macOS and Linux, Win32 codes
//! on Windows, and none in the browser build.

use super::OsErrorClass;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

#[cfg(unix)]
mod unix;

const _: fn(i32) -> OsErrorClass = class_of;
const _: fn(OsErrorClass) -> Option<i32> = code_of;
const _: &[&[(i32, OsErrorClass)]] = TABLES;

/// The class of a code this OS produced; [`OsErrorClass::Other`] for one no caller
/// tells apart.
pub(super) use imp::class_of;
/// This OS's code for a class, the first one when several share it; `None` when the
/// OS has none.
pub(super) use imp::code_of;

/// Every code this OS names, with its class, in lookup order.
pub(super) use imp::TABLES;

/// The arms' one lookup: `code`'s class in `tables`, or `Other`.
fn class_in(tables: &[&[(i32, OsErrorClass)]], code: i32) -> OsErrorClass {
    tables
        .iter()
        .flat_map(|table| table.iter())
        .find_map(|&(named, class)| (named == code).then_some(class))
        .unwrap_or(OsErrorClass::Other)
}

/// The first code `tables` give `class`.
fn code_in(tables: &[&[(i32, OsErrorClass)]], class: OsErrorClass) -> Option<i32> {
    tables
        .iter()
        .flat_map(|table| table.iter())
        .find_map(|&(code, named)| (named == class).then_some(code))
}
