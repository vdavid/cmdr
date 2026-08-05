//! In-memory filename search, one volume per search.
//!
//! Each volume's index DB loads into a per-volume `Vec<SearchEntry>` arena for fast
//! parallel scanning with rayon. Arenas load lazily (root when the dialog opens, a
//! scoped volume on first query) and all drop together after an idle timeout. A
//! scope routes to the ONE volume that owns it; an unscoped query means the boot
//! volume.

pub mod ai;
#[cfg(test)]
mod bench;
pub(crate) mod engine;
pub(crate) mod excludes;
pub(crate) mod execute;
pub mod history;
pub(crate) mod index;
pub(crate) mod live;
pub(crate) mod matcher;
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
    DIALOG_OPEN, VolumeLoad, cancel_active_loads, cancel_idle_timer, ensure_volume, get_loaded, has_searchable_index,
    reset_backstop_timer, start_idle_timer, start_importance_weight_subscriber, touch_activity,
};

// execute.rs (single-volume orchestration)
pub(crate) use execute::{
    AGENT_WAIT_DEFAULT, AGENT_WAIT_MAX, LiveSearchStart, run_blocking, run_live_collected, start_live,
};

// live.rs (a search that walks what the index can't answer for)
pub(crate) use live::{
    AnswerEnding, LiveAnswer, SearchRunError, WalkEnding, cancel_all_live_runs, cancel_dialog_runs_except,
    cancel_live_run,
};

// query.rs
pub use query::SYSTEM_DIR_EXCLUDES;
pub(crate) use query::{format_size, format_timestamp, parse_scope, summarize_query};
