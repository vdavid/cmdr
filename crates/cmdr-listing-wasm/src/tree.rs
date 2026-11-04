//! The demo's file tree, as the page hands it over, and the `InMemoryVolume` it
//! becomes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryContent, InMemoryVolume, Volume};
use serde::Deserialize;

/// One volume's whole tree. Every parent folder is listed explicitly.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoTree {
    /// The id panes list it under (`"root"` is the app's main volume).
    pub volume_id: String,
    /// The name people read it by (`"Macintosh HD"`), as a transfer's summary words
    /// its ends; the id when the page gives none.
    #[serde(default)]
    pub volume_name: Option<String>,
    /// The visitor's home folder (`"/Users/you"`): the volume then stands in for
    /// the computer's own disk, so a transfer's summary words its ends as the
    /// desktop's (`~/Documents`). `None` reads them as `<volume name>/…`.
    #[serde(default)]
    pub home: Option<String>,
    /// How much room the disk has left beyond what `entries` hold, when the
    /// page sets it (the `?free=` URL parameter, how a visitor or a test sees a
    /// full disk). `None` makes the disk [`DEVICE_CAPACITY_BYTES`] big.
    ///
    /// [`DEVICE_CAPACITY_BYTES`]: crate::commands::DEVICE_CAPACITY_BYTES
    #[serde(default)]
    pub free_bytes: Option<u64>,
    /// Where the volume is mounted (`"/"`).
    pub root: String,
    /// Every file, folder, and symlink, by absolute path.
    pub entries: Vec<DemoEntry>,
}

/// One item in a [`DemoTree`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoEntry {
    /// Absolute, `/`-separated, without a trailing slash.
    pub path: String,
    /// File, folder, or symlink.
    pub kind: DemoKind,
    /// Logical size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
    /// Last modified, in milliseconds since the Unix epoch (JS `Date.now()`).
    #[serde(default)]
    pub modified_ms: Option<u64>,
    /// Created, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub created_ms: Option<u64>,
    /// Where a symlink points. A target that's a folder in this tree makes the
    /// link list as a folder, the way a real disk's does.
    #[serde(default)]
    pub symlink_target: Option<String>,
}

/// What a [`DemoEntry`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DemoKind {
    /// A regular file.
    File,
    /// A folder.
    Dir,
    /// A symbolic link.
    Symlink,
}

/// Why a tree couldn't become a volume. Developer-facing: the page's own data
/// is wrong, so these never reach a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    /// A path that isn't absolute, or ends in `/`.
    BadPath(String),
    /// The same path twice.
    Duplicate(String),
    /// A path whose parent folder isn't in the tree.
    MissingParent(String),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadPath(path) => write!(f, "demo tree: `{path}` isn't an absolute path without a trailing slash"),
            Self::Duplicate(path) => write!(f, "demo tree: `{path}` is listed twice"),
            Self::MissingParent(path) => write!(f, "demo tree: the parent of `{path}` isn't listed"),
        }
    }
}

/// Seconds from milliseconds, the unit `FileEntry` dates are in.
fn secs(ms: Option<u64>) -> Option<u64> {
    ms.map(|ms| ms / 1000)
}

impl DemoTree {
    /// The disk's size under [`Self::free_bytes`]: what the files hold plus the
    /// room left. `None` when the page didn't set it.
    pub fn capacity_bytes(&self) -> Option<u64> {
        let free = self.free_bytes?;
        let stored: u64 = self
            .entries
            .iter()
            .filter(|e| e.kind == DemoKind::File)
            .filter_map(|e| e.size)
            .sum();
        Some(stored.saturating_add(free))
    }

    /// The tree as an in-memory volume, after checking every path has its parent.
    pub fn into_volume(self) -> Result<Arc<dyn Volume>, TreeError> {
        Ok(Arc::new(self.into_in_memory()?))
    }

    /// [`Self::into_volume`], before it's shared, so a host can add the device
    /// options (bandwidth, yielding, capacity, pane patches).
    pub fn into_in_memory(self) -> Result<InMemoryVolume, TreeError> {
        let root = PathBuf::from(&self.root);
        let dirs: HashSet<&str> = self
            .entries
            .iter()
            .filter(|e| e.kind == DemoKind::Dir)
            .map(|e| e.path.as_str())
            .collect();
        let mut seen = HashSet::new();
        let mut entries = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let path = Path::new(&entry.path);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            // `starts_with('/')`, ❌ not `Path::is_absolute`: that's `false` for every
            // path on `wasm32-unknown-unknown`, which has no notion of a filesystem.
            let absolute = entry.path.starts_with('/');
            let Some(name) = name.filter(|_| absolute && !entry.path.ends_with('/')) else {
                return Err(TreeError::BadPath(entry.path.clone()));
            };
            if !seen.insert(entry.path.as_str()) {
                return Err(TreeError::Duplicate(entry.path.clone()));
            }
            let parent = path.parent().unwrap_or(&root);
            if parent != root && !dirs.contains(parent.to_string_lossy().as_ref()) {
                return Err(TreeError::MissingParent(entry.path.clone()));
            }
            entries.push(file_entry(entry, name, &dirs));
        }
        let name = self.volume_name.unwrap_or(self.volume_id);
        let mut volume = InMemoryVolume::with_entries(name, entries)
            .with_root(root)
            .with_held_folders_only();
        if let Some(home) = self.home {
            volume = volume.standing_in_for_own_disk(home);
        }
        // Every file reads as many bytes as its listing says: generated ones,
        // seeded from its path, so a copy moves real-sized data without the page
        // holding gigabytes (`crates/cmdr-fs/src/volume/in_memory/device.rs`).
        for entry in self.entries.iter().filter(|e| e.kind == DemoKind::File) {
            if let Some(size) = entry.size {
                volume = volume.with_content(&entry.path, InMemoryContent::synthetic(seed_of(&entry.path), size));
            }
        }
        Ok(volume)
    }
}

/// A file's content seed: FNV-1a of its path, so the same tree always generates
/// the same bytes.
fn seed_of(path: &str) -> u64 {
    path.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The row a listing shows for `entry`: the dates and size it gave, and the
/// mode a fresh file or folder gets under the usual `022` umask.
fn file_entry(entry: &DemoEntry, name: String, dirs: &HashSet<&str>) -> FileEntry {
    let is_symlink = entry.kind == DemoKind::Symlink;
    let is_dir = match entry.kind {
        DemoKind::Dir => true,
        DemoKind::File => false,
        DemoKind::Symlink => entry.symlink_target.as_deref().is_some_and(|t| dirs.contains(t)),
    };
    FileEntry {
        size: if is_dir { None } else { entry.size },
        modified_at: secs(entry.modified_ms),
        created_at: secs(entry.created_ms),
        permissions: if is_dir { 0o755 } else { 0o644 },
        ..FileEntry::new(name, entry.path.clone(), is_dir, is_symlink)
    }
}
