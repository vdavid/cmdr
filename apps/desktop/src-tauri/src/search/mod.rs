//! In-memory search index and search execution.
//!
//! Loads all entries from the index DB into a `Vec<SearchEntry>` for fast
//! parallel scanning with rayon. The index is loaded lazily when the search
//! dialog opens and dropped after an idle timeout.

pub mod ai;
pub(crate) mod engine;
pub(crate) mod index;
pub(crate) mod query;
pub(crate) mod types;

// Flat re-exports so consumers can `use crate::search::{SearchQuery, search, ...}`

// types.rs
pub use types::{ParsedScope, PatternType, SearchQuery, SearchResult, SearchResultEntry};

// index.rs
pub(crate) use index::{
    DIALOG_OPEN, SEARCH_INDEX, SearchIndexState, drop_search_index, load_search_index, start_backstop_timer,
    start_idle_timer, touch_activity,
};
pub use index::{SearchEntry, SearchIndex};

// engine.rs
pub(crate) use engine::search;

// query.rs
pub use query::SYSTEM_DIR_EXCLUDES;
pub(crate) use query::{
    fill_directory_sizes, format_size, format_timestamp, parse_scope, resolve_include_paths, summarize_query,
};
