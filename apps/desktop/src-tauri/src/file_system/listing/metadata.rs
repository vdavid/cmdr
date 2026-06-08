//! File entry metadata types and helper functions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, RwLock};
use uzers::{get_group_by_gid, get_user_by_uid};

/// Cache for uid→username resolution.
pub(crate) static OWNER_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
/// Cache for gid→groupname resolution.
pub(crate) static GROUP_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Resolves a uid to a username, with caching.
pub(crate) fn get_owner_name(uid: u32) -> String {
    // Try read lock first
    if let Ok(cache) = OWNER_CACHE.read()
        && let Some(name) = cache.get(&uid)
    {
        return name.clone();
    }
    // Cache miss, resolve and store
    let name = get_user_by_uid(uid)
        .map(|u| u.name().to_string_lossy().into_owned())
        .unwrap_or_else(|| uid.to_string());
    if let Ok(mut cache) = OWNER_CACHE.write() {
        cache.insert(uid, name.clone());
    }
    name
}

/// Resolves a gid to a group name, with caching.
pub(crate) fn get_group_name(gid: u32) -> String {
    if let Ok(cache) = GROUP_CACHE.read()
        && let Some(name) = cache.get(&gid)
    {
        return name.clone();
    }
    let name = get_group_by_gid(gid)
        .map(|g| g.name().to_string_lossy().into_owned())
        .unwrap_or_else(|| gid.to_string());
    if let Ok(mut cache) = GROUP_CACHE.write() {
        cache.insert(gid, name.clone());
    }
    name
}

/// Generates icon ID based on file type, extension, and (for directories) the
/// folder's well-known path and package extension.
///
/// `path` is the entry's full path. It's used to detect the finite set of
/// special system folders (Downloads, Applications, the home folder, …) by
/// canonical path — NOT by name, since any folder can be named "Downloads". A
/// real special folder gets a bounded `special:{name}` key (Tier B); every other
/// directory keeps the shared `dir` icon (Tier A). Detection is a cheap path
/// comparison (no NSWorkspace, no TCC), so it's safe to run per entry.
///
/// Package/bundle directories (`Safari.app`, `Foo.bundle`, …) route to a
/// `pkg:{path}` key (Tier C) by a pure suffix check on the name — also no I/O, so
/// safe per entry. Custom-icon folders (the `kHasCustomIcon` xattr) are NOT
/// detected here: that check is a `getxattr` syscall, too costly to run for every
/// entry in a 100k-directory listing, so it's deferred to the visible-range fetch
/// (`icons::custom_folder_icon_ids`), which the frontend drives only for visible
/// directory rows.
///
/// Symlinks (even to a special location) keep their `symlink-dir` icon: the link
/// badge is the salient signal, and following a symlink to classify it would
/// cost a syscall per entry.
pub(crate) fn get_icon_id(is_dir: bool, is_symlink: bool, name: &str, path: &str) -> String {
    if is_symlink {
        // Distinguish symlinks to directories vs files
        return if is_dir {
            "symlink-dir".to_string()
        } else {
            "symlink-file".to_string()
        };
    }
    if is_dir {
        if let Some(special_id) = crate::icons::special_folders::icon_id_for_path(Path::new(path)) {
            return special_id;
        }
        if let Some(pkg_id) = crate::icons::per_path::package_icon_id(name, path) {
            return pkg_id;
        }
        return "dir".to_string();
    }
    // Extract extension
    if let Some(ext) = Path::new(name).extension() {
        return format!("ext:{}", ext.to_string_lossy().to_lowercase());
    }
    "file".to_string()
}

/// Represents a file or directory entry with extended metadata.
///
/// Carries `Deserialize` because it nests in the typed `directory-diff` event
/// payload (`DiffChange.entry`), which must round-trip; it's otherwise a
/// Rust → frontend return type.
/// `None`/empty fields serialize as explicit `null` (no `skip_serializing_if`) so
/// specta's `validate_exported_command` accepts the type in Unified mode.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: Option<u64>,
    /// Physical size on disk in bytes (st_blocks * 512 on Unix, same as size on other platforms)
    pub physical_size: Option<u64>,
    /// Inode number, when known. Populated by `LocalPosixVolume` from
    /// `MetadataExt::ino()`; left `None` by MTP, SMB, and InMemory backends
    /// because their protocols have no inode concept. Consumed by the
    /// volume-aware delete / copy walkers to dedupe hardlinks the same way
    /// the local-FS walker does (see `seen_inodes` in
    /// `write_operations/scan.rs`). Non-local backends never produce
    /// hardlinks, so `None` is the safe default — the dedup loop just treats
    /// every entry as a unique inode.
    pub inode: Option<u64>,
    pub modified_at: Option<u64>,
    pub created_at: Option<u64>,
    /// When the file was added to its current directory (macOS only)
    pub added_at: Option<u64>,
    /// When the file was last opened (macOS only)
    pub opened_at: Option<u64>,
    pub permissions: u32,
    pub owner: String,
    pub group: String,
    pub icon_id: String,
    /// Whether extended metadata (addedAt, openedAt) has been loaded
    /// Always true for legacy list_directory(), false for list_directory_core()
    pub extended_metadata_loaded: bool,
    /// Recursive size in bytes (from drive index, None if not indexed)
    pub recursive_size: Option<u64>,
    /// Recursive physical size on disk in bytes (from drive index, None if not indexed)
    pub recursive_physical_size: Option<u64>,
    /// Recursive file count (from drive index, None if not indexed)
    pub recursive_file_count: Option<u64>,
    /// Recursive dir count (from drive index, None if not indexed)
    pub recursive_dir_count: Option<u64>,
    /// True when the subtree contains symlinks (whose content is omitted from the
    /// recursive size). Drives the "size omits symlinked content" hint in the UI.
    /// `None` when the directory isn't indexed yet.
    pub recursive_has_symlinks: Option<bool>,
    /// When set on a virtual entry, the frontend navigates to this path instead
    /// of treating the entry as a normal directory listing. Currently set on
    /// `worktrees/` and `submodules/` entries inside the git portal so they
    /// open their working dir directly. The field lives on the base
    /// `FileEntry` schema so every consumer (frontend list views, MCP
    /// `cmdr://state`, drag-drop, copy preview, Brief/Full renderers) carries
    /// it for free.
    pub redirect_to_path: Option<String>,
    /// Loose Size-column override for virtual git entries: rendered verbatim
    /// in the Full mode Size column instead of formatted bytes from `size`.
    /// Examples: `+12 / -3`, `5 files`, `12 items`, `on main`, short SHA.
    /// `size` keeps the within-category numeric sort key (ahead-count for
    /// branches, files-changed for commits, item count for category roots).
    /// Cross-category Size sorting is meaningless and that's an honest
    /// tradeoff. Each cell is self-explaining via tooltip + aria-label.
    pub display_size: Option<String>,
    /// Optional rich tooltip string for the Size cell, used when
    /// `display_size` is set. Example: "12 commits ahead, 3 commits behind
    /// `origin/main`". Doubles as the aria-label for screen readers.
    pub display_size_tooltip: Option<String>,
}

impl FileEntry {
    /// Creates a `FileEntry` with the four essential fields set and everything else defaulted.
    pub(crate) fn new(name: String, path: String, is_dir: bool, is_symlink: bool) -> Self {
        Self {
            icon_id: get_icon_id(is_dir, is_symlink, &name, &path),
            name,
            path,
            is_directory: is_dir,
            is_symlink,
            size: None,
            physical_size: None,
            inode: None,
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0,
            owner: String::new(),
            group: String::new(),
            extended_metadata_loaded: false,
            recursive_size: None,
            recursive_physical_size: None,
            recursive_file_count: None,
            recursive_dir_count: None,
            recursive_has_symlinks: None,
            redirect_to_path: None,
            display_size: None,
            display_size_tooltip: None,
        }
    }
}

#[cfg(test)]
mod icon_id_tests {
    use super::*;

    // `special:*` routing is a macOS concept (Linux falls back to the XDG theme
    // path and never classifies special folders), and the standard-location
    // resolvers (`dirs::download_dir`, …) return `None` in a headless Linux CI
    // container with no XDG user-dirs configured. So the test that asserts a
    // `special:*` id is macOS-only; the OS-neutral routing tests use fixed paths
    // and never depend on a `dirs::*` resolver returning `Some`.

    #[test]
    fn plain_directory_gets_the_generic_dir_icon() {
        assert_eq!(get_icon_id(true, false, "foo", "/some/where/Projects/foo"), "dir");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn real_downloads_folder_gets_the_special_key() {
        let downloads = dirs::download_dir().expect("download_dir resolves");
        assert_eq!(
            get_icon_id(true, false, "Downloads", &downloads.to_string_lossy()),
            "special:downloads"
        );
    }

    #[test]
    fn a_folder_merely_named_downloads_elsewhere_stays_generic() {
        let fake = "/some/where/Projects/Downloads";
        assert_eq!(get_icon_id(true, false, "Downloads", fake), "dir");
    }

    #[test]
    fn a_symlink_to_a_special_path_keeps_the_symlink_dir_icon() {
        // Symlinks keep the link badge; we don't promote them to `special:*`. The
        // symlink branch short-circuits before any special-path lookup, so a fixed
        // path is enough and this holds on every platform.
        assert_eq!(
            get_icon_id(true, true, "Downloads", "/some/where/Downloads"),
            "symlink-dir"
        );
    }

    #[test]
    fn an_app_bundle_gets_a_pkg_key() {
        assert_eq!(
            get_icon_id(true, false, "Safari.app", "/Applications/Safari.app"),
            "pkg:/Applications/Safari.app"
        );
    }

    #[test]
    fn a_plain_folder_with_a_dot_is_not_a_package() {
        assert_eq!(get_icon_id(true, false, "my.project", "/Users/x/my.project"), "dir");
    }

    #[test]
    fn a_symlink_to_an_app_bundle_keeps_the_symlink_dir_icon() {
        assert_eq!(
            get_icon_id(true, true, "Safari.app", "/Applications/Safari.app"),
            "symlink-dir"
        );
    }

    #[test]
    fn a_file_named_like_a_bundle_routes_by_extension_not_pkg() {
        // A regular file "thing.app" (not a directory) is just an `ext:app` file.
        assert_eq!(get_icon_id(false, false, "thing.app", "/tmp/thing.app"), "ext:app");
    }

    #[test]
    fn files_are_unaffected_by_special_folder_detection() {
        // A file (not a directory) always routes by extension, never to `special:*`
        // — the file branch runs after the dir branch, so even a file sitting
        // inside a special folder gets `ext:*`. A fixed path keeps this OS-neutral.
        assert_eq!(
            get_icon_id(false, false, "notes.txt", "/some/where/Downloads/notes.txt"),
            "ext:txt"
        );
    }
}

/// Extended metadata for a single file (macOS-specific fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedMetadata {
    /// Key for merging with FileEntry.
    pub path: String,
    /// macOS only.
    pub added_at: Option<u64>,
    /// macOS only.
    pub opened_at: Option<u64>,
}
