//! Directory listing module - reading, operations, caching, metadata, sorting, streaming.

pub(crate) mod caching;
pub(crate) mod metadata;
pub(crate) mod operations;
pub(crate) mod reading;
pub(crate) mod sorting;
pub(crate) mod streaming;

// Re-export types for backwards compatibility (they were originally defined in operations.rs)
// These re-exports make the types available both externally and locally in this module
pub use metadata::{ExtendedMetadata, FileEntry};
pub use operations::{
    ListingStartResult, ListingStats, ResortResult, find_file_index, get_file_at, get_file_range, get_listing_stats,
    get_max_filename_width, get_total_count, list_directory_end, list_directory_start_with_volume, resort_listing,
};
pub use reading::{get_single_entry, list_directory_core};
pub use sorting::{SortColumn, SortOrder};
pub use streaming::{StreamingListingStartResult, cancel_listing, list_directory_start_streaming};

// macOS-only exports (used by drag operations)
#[cfg(target_os = "macos")]
pub use operations::get_paths_at_indices;

// Internal re-exports for file_system module internals (pub(crate) for crate-internal use)
pub(crate) use operations::{get_listing_entries, get_listings_by_volume_prefix, update_listing_entries};

#[cfg(test)]
mod hidden_files_test;
#[cfg(test)]
mod operations_test;
#[cfg(test)]
mod sorting_test;
