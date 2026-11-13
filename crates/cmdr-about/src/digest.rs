//! What a folder holds, and what an app leaves in the Library.
//!
//! Everything here comes from the drive index, never from a walk the card
//! started, so each part carries the index's coverage.

use serde::Serialize;

use crate::fact::Fact;
use crate::owner::OwnerPlace;
use crate::payload::RecursiveSize;
use crate::text::ItemText;

/// A folder's contents at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderDigest {
    /// How many entries sit directly in the folder. Zero is a real answer: the
    /// folder is empty.
    pub direct_children: u64,
    /// The biggest direct children, biggest first.
    pub biggest_children: Vec<SizedChild>,
    /// The largest files anywhere under the folder, largest first. Unknown when
    /// the search arena isn't loaded.
    pub largest_files: Fact<Vec<SizedChild>>,
    /// The kinds of the direct children, most common first.
    pub kind_mix: Vec<KindCount>,
    /// The share of the folder's bytes that haven't changed in a year.
    pub unchanged_for_a_year: Fact<ByteShare>,
    /// What kind of folder its name and place make it.
    pub class: FolderClass,
}

/// One child with its size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SizedChild {
    /// Its name, for showing.
    pub name: ItemText,
    /// Its full path, exactly as the volume has it: for navigating there, ❌ never
    /// for showing.
    pub path: String,
    /// Whether it's a folder.
    pub is_folder: bool,
    /// Its size: its own for a file, the whole subtree for a folder.
    pub size: Fact<RecursiveSize>,
}

/// How many direct children are of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KindCount {
    /// The kind, by extension class.
    pub kind: ChildKind,
    /// How many children are of it.
    pub count: u64,
}

/// A direct child's kind, read from its extension alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChildKind {
    /// A folder.
    Folder,
    /// A photo or other image.
    Image,
    /// A video.
    Video,
    /// A sound file.
    Audio,
    /// A text, office, or PDF document.
    Document,
    /// An archive or a disk image.
    Archive,
    /// Source code or a script.
    Code,
    /// An app or an executable.
    App,
    /// Anything else.
    Other,
}

/// A share of a folder's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ByteShare {
    /// The bytes in the share.
    pub bytes: u64,
    /// The bytes it's a share of.
    pub of: u64,
}

/// What kind of folder its name and place make it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FolderClass {
    /// Nothing special.
    Plain,
    /// A cache: whatever wrote it writes it again.
    Cache,
    /// Installed dependencies (`node_modules`): a package manager fetches them
    /// again.
    Dependencies,
    /// Build output (`target`, `dist`): a build tool makes it again.
    BuildOutput,
    /// Version-control data (`.git`).
    VersionControl,
}

/// The Library folders named after an app's bundle id, which removing the app
/// leaves behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    /// Each folder found, biggest first. Empty is a real answer: the index holds
    /// none.
    pub folders: Vec<LibraryFolder>,
}

/// One of an app's folders in the Library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolder {
    /// Its name, for showing.
    pub name: ItemText,
    /// Its full path, exactly as the volume has it: for navigating there, ❌ never
    /// for showing.
    pub path: String,
    /// Which app-data folder it sits in.
    pub place: OwnerPlace,
    /// Its whole size.
    pub size: Fact<RecursiveSize>,
}
