//! How a scratch file is NAMED: the one builder every temp and aside goes
//! through, so each fits wherever its final name fits.
//!
//! A scratch name is the original name, a marker, and a hyphenated UUID:
//! `photo.jpg.cmdr-tmp-<uuid>`, 46 bytes longer than the original. A name
//! already near the destination's limit (a downloader that byte-capped it, a
//! long name in a two-byte script) would otherwise be refused for OUR suffix,
//! and the user would be told to shorten a name that's fine (ERR-9JY7Q, a
//! Synology share answering `STATUS_OBJECT_NAME_INVALID`). So the original-name
//! part gives way: the marker and the UUID stay whole, since they're what makes
//! the file recognizable and unique, and the name in front keeps as many whole
//! grapheme clusters as fit.
//!
//! ## The limit
//!
//! [`MAX_SCRATCH_NAME_BYTES`] is 143 UTF-8 bytes, the strictest name limit Cmdr
//! is likely to meet: eCryptfs, behind a Synology encrypted share among others,
//! spends the rest of the usual 255 on its own encoding of the name. APFS and
//! ext4 or btrfs behind a NAS's Samba allow 255 bytes, so 143 fits there too.
//! SMB to Windows, NTFS, and exFAT count UTF-16 units instead (255), and no
//! extra check is needed for them: every character costs at least as many UTF-8
//! bytes as UTF-16 units (1:1, 2:1, 3:1, 4:2), so a name within the byte cap is
//! within the unit cap. The cost of the lower cap is only a shorter original
//! part in a scratch name, which is there for a person to recognize a leftover
//! by, and 97 bytes of it still do that.
//!
//! ## Recognizing a shortened one
//!
//! Every reader still recognizes a shortened scratch name, because none of them
//! parses the original name back out: the name tests look for the marker
//! anywhere in the name, and the in-flight registry keys on the exact name the
//! mint produced. The one reader that matches ONE file's leftovers, an archive
//! edit reaping its own crashed builds, asks [`temp_sibling_prefix`] for the
//! same fitted prefix the mint used.

use std::path::{Path, PathBuf};

use unicode_segmentation::UnicodeSegmentation;

use super::{STAGING_DELETING_MARKER, STAGING_TEMP_MARKER, UUID_TEXT_LEN};

/// The longest scratch file name Cmdr creates, in UTF-8 bytes: eCryptfs's
/// 143-byte name limit (Synology encrypted shares), the strictest a write meets.
/// See the module docs for why bytes alone also bound UTF-16 units.
pub const MAX_SCRATCH_NAME_BYTES: usize = 143;

/// The name of a staged write's temp next to a file called `file_name`, with a
/// fresh UUID: `photo.jpg.cmdr-tmp-<uuid>`, the original part shortened when the
/// whole wouldn't fit in [`MAX_SCRATCH_NAME_BYTES`].
///
/// For a writer that names its own staging file (a network backend's upload).
/// Code that holds a temp through an operation mints it with
/// [`StagingTemp::mint`](super::StagingTemp::mint), which names it the same way
/// and registers it.
pub fn temp_sibling_name(file_name: &str) -> String {
    scratch_name(file_name, STAGING_TEMP_MARKER, uuid::Uuid::new_v4())
}

/// The name a delete moves the folder `folder_name` aside to, with a fresh
/// UUID: `album.cmdr-deleting-<uuid>`, shortened the same way.
pub fn deleting_sibling_name(folder_name: &str) -> String {
    scratch_name(folder_name, STAGING_DELETING_MARKER, uuid::Uuid::new_v4())
}

/// [`temp_sibling_name`] as a path in `final_path`'s directory.
pub fn temp_sibling_path(final_path: &Path) -> PathBuf {
    let (parent, file_name) = split_final_path(final_path);
    parent.join(temp_sibling_name(&file_name))
}

/// What every temp minted for `file_name` starts with: its fitted original
/// part plus the marker. For a reaper that clears ONE file's leftovers and must
/// match its shortened temps too.
pub fn temp_sibling_prefix(file_name: &str) -> String {
    format!("{}{STAGING_TEMP_MARKER}", fitted(file_name, STAGING_TEMP_MARKER))
}

/// `file_name`, fitted, then `marker`, then `uuid`.
pub(super) fn scratch_name(file_name: &str, marker: &str, uuid: uuid::Uuid) -> String {
    format!("{}{marker}{uuid}", fitted(file_name, marker))
}

/// A path's parent and its file name as text, for the minting side.
pub(super) fn split_final_path(final_path: &Path) -> (&Path, String) {
    let parent = final_path.parent().unwrap_or(Path::new(""));
    let file_name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (parent, file_name)
}

/// The longest run of whole grapheme clusters from the start of `file_name`
/// that leaves room for `marker` and a UUID.
///
/// A cluster cut in half would strand an accent without its letter or leave a
/// broken emoji in the name a user reads. A single cluster longer than the whole
/// budget (a pile of hundreds of combining marks) can't be kept whole, so that
/// one falls back to the last character boundary.
fn fitted<'a>(file_name: &'a str, marker: &str) -> &'a str {
    let budget = MAX_SCRATCH_NAME_BYTES - marker.len() - UUID_TEXT_LEN;
    if file_name.len() <= budget {
        return file_name;
    }
    let end = file_name
        .grapheme_indices(true)
        .map(|(at, cluster)| at + cluster.len())
        .take_while(|&end| end <= budget)
        .last()
        .unwrap_or_else(|| file_name.floor_char_boundary(budget));
    &file_name[..end]
}
