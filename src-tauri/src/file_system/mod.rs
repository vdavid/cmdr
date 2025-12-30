//! File system module - operations, watchers, and providers.

#[cfg(target_os = "macos")]
mod macos_metadata;
#[cfg(test)]
mod mock_provider;
mod operations;
#[cfg(test)]
mod provider;
#[cfg(test)]
mod real_provider;
#[cfg(target_os = "macos")]
pub mod sync_status;
mod watcher;

// Re-export public types
#[cfg(test)]
pub use mock_provider::MockFileSystemProvider;
pub use operations::{
    FileEntry, ListingStartResult, find_file_index, get_file_at, get_file_range, get_total_count, list_directory_end,
    list_directory_start,
};
// FileEntry also re-exported for internal test modules
#[cfg(test)]
pub use provider::FileSystemProvider;
// Watcher management - init_watcher_manager must be called from lib.rs
pub use watcher::init_watcher_manager;

#[cfg(test)]
mod operations_test;

#[cfg(test)]
mod watcher_test;

#[cfg(test)]
mod mock_provider_test;
