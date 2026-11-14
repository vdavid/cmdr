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
#[cfg(test)]
mod bench_card;
#[cfg(test)]
mod diagnostics_tests;
pub(crate) mod execute;
pub mod history;
pub(crate) mod index;
pub(crate) mod index_source;
pub(crate) mod live;
pub(crate) mod volumes;
pub(crate) mod zone;

// The core lives in `cmdr-search`; its modules keep their old paths here
// (`crate::search::matcher`, …) so the host reads the same either way.
pub(crate) use cmdr_search::{context, engine, excludes, matcher, query, ranking, seam, types};

// Flat re-exports so consumers can `use crate::search::{SearchQuery, ...}`

// The name rules (a private leaf in the crate, promised at its root)
pub(crate) use cmdr_search::NameRules;

// types.rs
pub use types::{ParsedScope, PatternType, SearchQuery, SearchResult, SearchResultEntry, SearchSort};

// index.rs
pub use index::{OptU64, SearchEntry, SearchIndex};

// volumes.rs (per-volume registry + dialog lifecycle)
pub(crate) use volumes::{
    DIALOG_OPEN, VolumeLoad, cancel_active_loads, cancel_idle_timer, drop_all_indices, ensure_volume, forget_volume,
    get_loaded, has_searchable_index, reset_backstop_timer, set_drive_index_dir, start_idle_timer,
    start_importance_weight_subscriber, touch_activity,
};

// execute.rs (single-volume orchestration)
pub(crate) use execute::{
    AGENT_WAIT_DEFAULT, AGENT_WAIT_MAX, LiveSearchStart, run_blocking, run_if_loaded, run_live_collected, start_live,
};

// live.rs (a search that walks what the index can't answer for)
pub(crate) use live::{
    AnswerEnding, LiveAnswer, SearchRunError, WalkEnding, cancel_all_live_runs, cancel_dialog_runs_except,
    cancel_live_run, is_live_run,
};

// query.rs
pub use query::SYSTEM_DIR_EXCLUDES;
pub(crate) use query::{format_size, parse_size, summarize_query_for_diagnostics};
pub(crate) use zone::{format_local_date, format_local_datetime, summarize_query};

/// This machine's home folder, as the search core takes it: a value, so the core
/// never reads it ambiently and a test can pass its own.
pub(crate) fn host_home_dir() -> Option<String> {
    dirs::home_dir().map(|home| home.to_string_lossy().into_owned())
}

/// Parse a scope string (`~/projects, !node_modules`) with `~` expanded against
/// this machine's home folder.
pub(crate) fn parse_scope(input: &str) -> ParsedScope {
    query::parse_scope(input, host_home_dir().as_deref())
}
