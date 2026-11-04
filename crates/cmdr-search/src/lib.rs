//! The drive search: Cmdr's ⌘F over one volume's file names.
//!
//! The query model, the one matcher and exclusion set, ranking, and the in-memory
//! arena a search scans, fed through a narrow seam ([`seam`]): a host hands over a
//! built arena and answers, a few times per query, which entries a scope names and
//! how big a folder is. No database, no runtime, and no ambient platform, home, or
//! clock: the name rules and the home folder arrive as a [`SearchContext`]. Hosted by
//! the desktop app (`apps/desktop/src-tauri/src/search/`, answering from the drive
//! index). `CLAUDE.md` has the must-knows.
#![warn(unused_crate_dependencies)]

pub mod arena;
pub mod context;
pub mod engine;
pub mod excludes;
pub mod matcher;
pub mod query;
pub mod ranking;
pub mod run;
pub mod seam;
pub mod types;

#[cfg(test)]
mod test_alloc;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use arena::{ARENA_ROOT_ID, ArenaBuilder, OptU64, SearchEntry, SearchIndex};
pub use context::{NameFold, NameRules, SearchContext};
pub use seam::{
    CoveredHalf, DirSizeSource, ResolvedScope, ScopeResolution, SearchError, SearchRows, SourceError, SourceRead,
    search_covered,
};
pub use types::{ParsedScope, PatternType, SearchQuery, SearchResult, SearchResultEntry, SearchSort};

// `serde_json` and `proptest` are the tests' own; the lint can't see a dev-only use.
#[cfg(test)]
use {proptest as _, serde_json as _};
