//! Directory listing module - reading, operations, caching, metadata, sorting, streaming.

pub(crate) mod brief_columns;
pub(crate) mod caching;
pub(crate) mod diff_emitter;
pub(crate) mod fuzzy_jump;
pub(crate) mod metadata;
pub(crate) mod operations;
pub(crate) mod reading;
pub(crate) mod sorting;
pub(crate) mod streaming;

// Re-export types for backwards compatibility (they were originally defined in operations.rs)
// These re-exports make the types available both externally and locally in this module
pub use brief_columns::{BriefColumnsError, compute_brief_column_text_widths};
pub use fuzzy_jump::fuzzy_find_first_match_in_listing;
pub use metadata::{ExtendedMetadata, FileEntry};
pub use operations::{
    ListingStartResult, ListingStats, ResortResult, find_file_index, find_file_indices, get_file_at, get_file_range,
    get_listing_stats, get_total_count, list_directory_end, list_directory_start_with_volume,
    refresh_listing_index_sizes, resort_listing,
};
pub use reading::{get_single_entry, list_directory_core};
pub use sorting::{DirectorySortMode, SortColumn, SortOrder};
pub use streaming::{StreamingListingStartResult, cancel_listing, list_directory_start_streaming};

// Batch accessors (used by drag, clipboard, and transfer dialogs)
pub use operations::{get_files_at_indices, get_paths_at_indices};

// Internal re-exports for file_system module internals (pub(crate) for crate-internal use)
pub(crate) use caching::{
    ModifyResult, find_listings_for_path, get_listing_path, get_listing_volume_id_and_path, has_entry,
    increment_sequence, insert_entry_sorted, remove_entry_by_path, update_entry_sorted,
};
// Notification API for volume mutations
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use operations::get_listings_by_volume_prefix;
pub(crate) use operations::{get_listing_entries, update_listing_entries};

#[cfg(test)]
mod brief_columns_test;
#[cfg(test)]
mod caching_test;
#[cfg(test)]
mod diff_emitter_test;
#[cfg(test)]
mod hidden_files_test;
#[cfg(test)]
mod operations_test;
#[cfg(test)]
mod sorting_test;
#[cfg(test)]
mod stats_test;
#[cfg(test)]
mod streaming_test;
