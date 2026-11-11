//! Dates through `std` alone, which Linux and the browser build share: no settable birth date,
//! the mtime and atime in one `FileTimes`, and no OS code of its own for "can't hold this".

use std::fs::File;
use std::time::SystemTime;

use super::super::items::{StagedItem, StagedTimes};

pub(crate) const CREATED_SETTABLE: bool = false;

/// `std`'s path setter is unstable, so a folder is opened for its fd; a link's own dates
/// aren't settable through `std` at all.
pub(crate) fn set_dates(item: StagedItem<'_>, _created: Option<SystemTime>, times: StagedTimes) -> std::io::Result<()> {
    let file_times = times.file_times();
    match item {
        StagedItem::File(file) => file.set_times(file_times),
        StagedItem::Path(path) => File::open(path)?.set_times(file_times),
        StagedItem::Link(_) => Err(std::io::ErrorKind::Unsupported.into()),
    }
}

/// Only `Unsupported` says the destination can't hold the class.
pub(crate) fn is_unsupported(_e: &std::io::Error) -> bool {
    false
}
