//! Directory listing cache for on-demand virtual scrolling.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{SortColumn, SortOrder};

/// Cache for directory listings (on-demand virtual scrolling).
/// Key: listing_id, Value: cached listing with all entries.
#[cfg(not(test))]
pub(crate) static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
#[cfg(test)]
pub(crate) static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cached directory listing for on-demand virtual scrolling.
#[cfg(not(test))]
pub(crate) struct CachedListing {
    /// Volume ID this listing belongs to (like "root", "dropbox")
    pub volume_id: String,
    /// Path within the volume (absolute path for now)
    pub path: PathBuf,
    /// Cached file entries
    pub entries: Vec<FileEntry>,
    /// Current sort column
    pub sort_by: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
}

/// Cached directory listing for on-demand virtual scrolling.
#[cfg(test)]
pub(crate) struct CachedListing {
    /// Volume ID this listing belongs to (like "root", "dropbox")
    pub volume_id: String,
    /// Path within the volume (absolute path for now)
    pub path: PathBuf,
    /// Cached file entries
    pub entries: Vec<FileEntry>,
    /// Current sort column
    pub sort_by: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
}
