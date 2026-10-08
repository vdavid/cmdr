//! A `ListObjectsV2` page as a folder's children. Pure: no request, no path
//! spelling, so the rules about what S3 hands back are testable on their own.
//!
//! S3 has no folders, only keys. With `delimiter=/`, a listing of `photos/`
//! answers `CommonPrefixes` (each a "folder") and `Contents` (the objects
//! directly inside). This module is where those become what a pane shows.

use std::collections::HashSet;
use std::time::SystemTime;

use crate::xml::ObjectPage;

/// One child of the folder a page lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Child {
    /// A common prefix, or a folder marker: something with keys under it.
    Folder { name: String },
    /// An object directly in the folder.
    Object {
        name: String,
        size: u64,
        /// `LastModified`: the upload time. ❗ A listing carries no user
        /// metadata, so `x-amz-meta-mtime` is only on `get_metadata`'s HEAD.
        modified: Option<SystemTime>,
        /// Glacier Flexible Retrieval or Deep Archive: reads fail until
        /// restored.
        archived: bool,
        /// A folder of the object's own name sits beside it, so `name` is the
        /// key's last segment plus [`FILE_SUFFIX`] ([`settle`]).
        beside_folder: bool,
    },
}

/// What a file listed beside a folder of its own name shows as after that
/// name. A path segment, so it's the same in every language.
pub(super) const FILE_SUFFIX: &str = " (file)";

/// A whole listing's children, every page's in order, made into what a pane
/// can show: folders first, each folder once, and a file whose name a folder
/// also holds renamed with [`FILE_SUFFIX`].
///
/// ❗ A renamed file whose new name is taken (a real `notes (file)` beside it)
/// stays out, as it did before files beside folders were shown: two rows on
/// one path break everything keyed on the path.
pub(super) fn settle(children: Vec<Child>) -> Vec<Child> {
    let mut folders: Vec<String> = Vec::new();
    let mut objects = Vec::new();
    for child in children {
        match child {
            Child::Folder { name } if !folders.contains(&name) => folders.push(name),
            Child::Folder { .. } => {}
            object @ Child::Object { .. } => objects.push(object),
        }
    }
    let taken: HashSet<String> = objects
        .iter()
        .map(|object| object.name().to_string())
        .chain(folders.iter().cloned())
        .collect();
    let objects = objects.into_iter().filter_map(|object| match object {
        Child::Object {
            name,
            size,
            modified,
            archived,
            ..
        } if folders.contains(&name) => {
            let shown = format!("{name}{FILE_SUFFIX}");
            (!taken.contains(&shown)).then_some(Child::Object {
                name: shown,
                size,
                modified,
                archived,
                beside_folder: true,
            })
        }
        other => Some(other),
    });
    folders
        .iter()
        .map(|name| Child::Folder { name: name.clone() })
        .chain(objects)
        .collect()
}

impl Child {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Folder { name } | Self::Object { name, .. } => name,
        }
    }

    #[cfg(test)]
    pub(super) fn is_folder(&self) -> bool {
        matches!(self, Self::Folder { .. })
    }
}

/// The children `page` names under `prefix` (`""` for a bucket's top, else
/// ending in `/`), folders first, each in the page's order.
///
/// - **The folder's own marker** (the key `prefix` itself) is left out.
/// - **A key with a `/` past the prefix** names the folder it sits under: a
///   child's marker (`photos/empty/`), or a deeper key from a server that
///   ignored the delimiter. Each folder appears once.
/// - **A name nothing can address** is left out: empty (from `a//b`), `.`, and
///   `..`, which every URL parser resolves away (`encoding::KeyError`).
/// - An object and a folder of one name both come back; [`settle`] tells them
///   apart once every page is in.
pub(super) fn children_of(page: &ObjectPage, prefix: &str) -> Vec<Child> {
    let mut folders: Vec<String> = Vec::new();
    let mut add_folder = |name: &str| {
        if addressable(name) && !folders.iter().any(|known| known == name) {
            folders.push(name.to_string());
        }
    };
    for common in &page.prefixes {
        if let Some(rest) = common.strip_prefix(prefix) {
            add_folder(rest.split('/').next().unwrap_or_default());
        }
    }
    let mut objects = Vec::new();
    for object in &page.objects {
        let Some(rest) = object.key.strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        if let Some((folder, _)) = rest.split_once('/') {
            add_folder(folder);
            continue;
        }
        if addressable(rest) {
            objects.push(Child::Object {
                name: rest.to_string(),
                size: object.size,
                modified: object.last_modified,
                archived: object.storage_class.is_archived(),
                beside_folder: false,
            });
        }
    }
    folders
        .into_iter()
        .map(|name| Child::Folder { name })
        .chain(objects)
        .collect()
}

/// Whether a name can be a path segment Cmdr sends.
fn addressable(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}

/// What a folder holds, read off one listing of `prefix` (ending in `/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FolderContents {
    /// No key at all under the prefix: no folder by that name.
    Nothing,
    /// Only the folder's own zero-byte marker (`prefix` itself).
    MarkerOnly,
    /// Something besides the marker: a file, or a folder below.
    Holds,
}

/// Judges a page listing `prefix` with `delimiter=/` and `max-keys` of at
/// least two, which is enough to tell a marker alone from a marker and a child.
pub(super) fn folder_contents(page: &ObjectPage, prefix: &str) -> FolderContents {
    let others = page.objects.iter().filter(|object| object.key != prefix).count() + page.prefixes.len();
    if others > 0 {
        FolderContents::Holds
    } else if page.objects.is_empty() {
        FolderContents::Nothing
    } else {
        FolderContents::MarkerOnly
    }
}

/// S3's key ceiling, in UTF-8 bytes.
const MAX_KEY_BYTES: usize = 1024;

/// Whether any key can start with `wire_prefix` (as it goes out, NFC on R2).
/// ❗ Every "what's under this?" listing asks for `<key>/`, one byte past the
/// ceiling for a key at it, and B2 refuses such a prefix with `400
/// InvalidRequest` where others answer an empty page, which made a 1,024-byte
/// object impossible to delete there. So a prefix past the ceiling is answered
/// as empty without a request.
pub(super) fn can_hold_keys(wire_prefix: &str) -> bool {
    wire_prefix.len() <= MAX_KEY_BYTES
}

#[cfg(test)]
#[path = "listing_test.rs"]
mod listing_test;
