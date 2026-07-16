//! Multi-volume in-memory filename search.
//!
//! Each volume's index DB loads into a per-volume `Vec<SearchEntry>` arena for fast
//! parallel scanning with rayon. Arenas load lazily (root when the dialog opens, a
//! scoped volume on first query) and all drop together after an idle timeout. A
//! scope routes to the owning volume(s); an unscoped query fans out across every
//! volume with a persisted index and merges the ranked results.

pub mod ai;
pub(crate) mod engine;
pub(crate) mod execute;
pub mod history;
pub(crate) mod index;
pub(crate) mod query;
pub(crate) mod ranking;
pub(crate) mod types;
pub(crate) mod volumes;

// Flat re-exports so consumers can `use crate::search::{SearchQuery, ...}`

// types.rs
pub use types::{ParsedScope, PatternType, SearchQuery, SearchResult, SearchResultEntry};

// index.rs
pub use index::{SearchEntry, SearchIndex};

// volumes.rs (per-volume registry + dialog lifecycle)
pub(crate) use volumes::{
    DIALOG_OPEN, VolumeLoad, cancel_active_loads, cancel_idle_timer, ensure_volume, get_loaded, reset_backstop_timer,
    start_idle_timer, start_importance_weight_subscriber, touch_activity,
};

// execute.rs (multi-volume orchestration)
pub(crate) use execute::run_blocking;

// query.rs
pub use query::SYSTEM_DIR_EXCLUDES;
pub(crate) use query::{format_size, format_timestamp, parse_scope, summarize_query};
