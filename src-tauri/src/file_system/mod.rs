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
mod watcher;

// Re-export public types
#[cfg(test)]
pub use mock_provider::MockFileSystemProvider;
pub use operations::{
    ChunkNextResult, SessionStartResult, list_directory_end, list_directory_next, list_directory_start,
};
// Re-export FileEntry for internal submodules (provider, mock_provider, real_provider)
#[cfg(test)]
pub(crate) use operations::FileEntry;
#[cfg(test)]
pub use provider::FileSystemProvider;

#[cfg(test)]
mod operations_test;

#[cfg(test)]
mod watcher_test;

#[cfg(test)]
mod mock_provider_test;
