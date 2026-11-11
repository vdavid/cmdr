//! The values the class modules and the per-OS seam both speak: an xattr, the item a staged
//! apply writes to, and the dates it sets.
//!
//! A leaf so `platform/` can take them without importing the class modules that call it.
//! ❌ Don't move them back beside their callers (`carried.rs`, `staged.rs`): each would then
//! import the seam that imports it.

use std::ffi::OsString;
use std::fs::File;
use std::path::Path;
use std::time::SystemTime;

/// One extended attribute: its name and its whole value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xattr {
    /// The full name, namespace included (`com.apple.metadata:_kMDItemUserTags`,
    /// `user.app`).
    pub name: OsString,
    /// The value, opaque bytes.
    pub value: Vec<u8>,
}

/// What [`apply_staged`](super::apply_staged) writes to.
#[derive(Debug, Clone, Copy)]
pub enum StagedItem<'a> {
    /// A file the copy holds open: every step goes through its fd, so there's
    /// no path lookup, and an fd write on a `0444` file isn't refused the way a
    /// path write is.
    File(&'a File),
    /// An item with no fd to hand (a folder). Follows nothing: a folder isn't a
    /// link.
    Path(&'a Path),
    /// A symlink's OWN metadata (`XATTR_NOFOLLOW`, `FSOPT_NOFOLLOW`), never its
    /// target's. Takes no mode. Its dates are macOS-only: Linux would need
    /// `utimensat(AT_SYMLINK_NOFOLLOW)`, and they answer `NotApplicable` there.
    Link(&'a Path),
}

/// The dates [`apply_staged`](super::apply_staged) sets beside
/// [`CarriedMetadata::created`](super::CarriedMetadata::created), in the
/// same call. `None` leaves that date as the filesystem stamped it.
// DEFAULT-OK: "set neither", which leaves the item's own dates alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StagedTimes {
    /// The modification date: a stream's `modified_at`, a local source's mtime.
    pub modified: Option<SystemTime>,
    /// The access date, carried only where it costs nothing extra (a local
    /// source's stat). Never reported.
    pub accessed: Option<SystemTime>,
}

impl StagedTimes {
    /// The mtime and atime as one `FileTimes` (the birth date, where an OS can set one, is
    /// its arm's to add).
    pub(super) fn file_times(self) -> std::fs::FileTimes {
        let mut file_times = std::fs::FileTimes::new();
        if let Some(modified) = self.modified {
            file_times = file_times.set_modified(modified);
        }
        if let Some(accessed) = self.accessed {
            file_times = file_times.set_accessed(accessed);
        }
        file_times
    }
}
