//! A walked entry, as the one matcher sees it.
//!
//! `Index::cover` hands back `CoveredEntry`s (a path and its metadata); the matcher
//! judges a [`Candidate`] (a name and the three predicate fields), the same shape an
//! arena row becomes. This adapter is the walk's half of that, kept beside
//! `WalkJudge` because the walk, and `cmdr-index` with it, stays host-side while the
//! matcher doesn't.

use std::borrow::Cow;
use std::path::Path;

use cmdr_index::CoveredEntry;

use crate::search::matcher::{Candidate, CompiledQuery};

/// Whether one entry a walk discovered satisfies every predicate.
///
/// The size is the entry's OWN, before hardlink dedup, because that's what a
/// listing shows. The index stores the deduplicated size instead, so a 2nd+
/// hardlink to one file is sizeless there: a size bound keeps it in a live result
/// and drops it from an indexed one. Bounded, and the live answer is the truthful
/// one, so it stays.
pub(crate) fn matches_walked(compiled: &CompiledQuery, entry: &CoveredEntry) -> bool {
    compiled.matches(&Candidate {
        name: &covered_name(&entry.path),
        is_directory: entry.is_directory,
        size: entry.logical_size,
        modified_at: entry.modified_at,
    })
}

/// The name a walked entry matches under.
///
/// Byte-identical to the name the index would have stored for the same entry, lossy
/// conversion and the nameless-path fallback included: the local walker derives its
/// row name from the same path the same way (`indexing/scanner/insert_visitor.rs`),
/// and the trait walk's row name is the listing's `name`, which is that path's last
/// component. ❌ Don't "improve" this alone — a name derived two ways is the fork
/// the matcher exists to prevent, and the RESULT ROW takes its name from here too
/// (`live.rs::live_result_entry`), so a row can't be named differently from the
/// name it matched under.
pub(crate) fn covered_name(path: &Path) -> Cow<'_, str> {
    path.file_name()
        .map_or(Cow::Borrowed(""), |name| name.to_string_lossy())
}

#[cfg(test)]
#[path = "walked_tests.rs"]
mod tests;
