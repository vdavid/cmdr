//! The compiled query: one matcher, two evaluators.
//!
//! A [`CompiledQuery`] is a `SearchQuery` with its name pattern compiled and its
//! type, size, and date predicates resolved, ready to be asked about one candidate
//! at a time. Two evaluators ask it:
//!
//! - the **arena scan** ([`super::engine::search_ranked`]), over rows already in a
//!   volume's index, and
//! - a **live walk**, over the entries `Index::cover` discovers while it runs.
//!
//! (`search/execute.rs` drives the second one; `search/live.rs` feeds it.)
//!
//! They MUST NOT fork. A drive being indexed or not is allowed to be a speed
//! difference, never a behavioral one, so both go through [`CompiledQuery::matches`]
//! and neither gets to re-derive a rule. Two rules in particular are silent when
//! they break, so they live here and nowhere else:
//!
//! - **Case folding.** The platform default (insensitive on macOS, sensitive on
//!   Linux), overridable per query. The scope filter and the ranker read it back off
//!   the compiled query rather than recomputing it.
//! - **NFD normalization.** APFS stores filenames decomposed, so the PATTERN is
//!   NFD-normalized at compile time and candidate names are matched raw. Normalizing
//!   a candidate instead would make live results differ from indexed ones on every
//!   accented filename, and would cost an allocation per entry besides
//!   (`search/CLAUDE.md`: no stored `name_folded`).
//!
//! It lives app-side because `cmdr-index` cannot depend on the app
//! (`index-crate-isolation`), which is why the walk hands back batches of entries
//! rather than taking a callback (`docs/specs/unindexed-search-plan.md` Decision 3).
//!
//! ## What is deliberately NOT here
//!
//! - **Directory size filters.** A directory's size lives in `dir_stats`, not in the
//!   entries table, so it's written over the ranked results afterwards
//!   (`execute.rs::fill_dir_sizes`, then `filter_dirs_by_size`). The size predicate
//!   here applies to FILES only, exactly as the engine always has. One matcher does
//!   not cover directory sizes, and over live-walked ground `dir_stats` is absent or
//!   a lower bound by construction (Accepted difference 5).
//! - **The scope filter** (include roots, `excludeDirNames`, `excludeSystemDirs`).
//!   It's an ancestor walk over arena entry ids, so it has no meaning for a candidate
//!   that isn't in the arena; the live path applies the same policy against a walked
//!   entry's own path instead.

use std::borrow::Cow;
use std::path::Path;

use regex::{Regex, RegexBuilder};

use cmdr_index::CoveredEntry;

use super::query::glob_to_regex;
use super::types::{PatternType, SearchQuery};

/// Arena rows above which a query with no narrowing predicate is refused. Below it
/// the scan is cheap enough that "show me everything, by recency" is a fair ask.
const ARENA_BROAD_QUERY_CEILING: usize = 100_000;

/// What a compiled query will be evaluated against. It decides exactly one thing:
/// how broad a query is allowed to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Evaluator {
    /// An arena of `entries` rows, scanned in memory. The cost is known up front and
    /// bounded, so a query with no narrowing predicate is refused only once the arena
    /// is big enough for the scan to be felt (~60 s at 5M entries).
    Arena { entries: usize },
    /// A live walk over ground the index doesn't cover yet. There is no size to
    /// weigh: the scan reads a filesystem of unknown extent, over a network in the
    /// worst case, so a query with no narrowing predicate is refused outright.
    ///
    /// ❌ Don't key this on an entry count the way [`Self::Arena`] does. An unindexed
    /// volume's arena holds zero rows, so a count-based ceiling is exactly the guard
    /// that never fires on the path that needs it most.
    LiveWalk,
}

/// Why a query couldn't be compiled. Typed so callers branch on the variant rather
/// than the sentence; [`std::fmt::Display`]
/// is what reaches the user, through the bare-message IPC contract search results
/// already have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompileError {
    /// The name pattern isn't a valid regex (either as typed, or after glob
    /// conversion). Carries the regex crate's own explanation.
    InvalidPattern(String),
    /// Nothing in the query narrows the candidate set — no name pattern, no size
    /// bound, no date bound, no type filter — and the evaluator can't afford that.
    TooBroad,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPattern(reason) => write!(f, "Invalid pattern: {reason}"),
            Self::TooBroad => {
                f.write_str("Query too broad. Add a filename pattern, size, date, or type filter to narrow results.")
            }
        }
    }
}

/// One entry offered to the matcher, in the shape both evaluators can produce: a
/// borrowed name plus the three fields the predicates read. The arena builds one from
/// a `SearchEntry` and its slice of the name arena; a walk builds one from a
/// `CoveredEntry`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Candidate<'a> {
    /// The entry's own name, not its path.
    pub name: &'a str,
    /// Whether it's a directory.
    pub is_directory: bool,
    /// Apparent size in bytes, or `None` when it isn't known.
    pub size: Option<u64>,
    /// Last-modified time, seconds since the Unix epoch.
    pub modified_at: Option<u64>,
}

/// A `SearchQuery` compiled once, then asked about many candidates.
#[derive(Debug)]
pub(crate) struct CompiledQuery {
    /// The name pattern, or `None` when the query doesn't filter by name.
    pattern: Option<Regex>,
    /// The case-folding rule this query resolved to, shared with the scope filter
    /// and the ranker so all three fold alike.
    case_insensitive: bool,
    is_directory: Option<bool>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    modified_after: Option<u64>,
    modified_before: Option<u64>,
}

impl CompiledQuery {
    /// Compile `query` for `evaluator`, refusing a pattern that won't build and a
    /// query too broad for what it's about to be run against.
    pub(crate) fn compile(query: &SearchQuery, evaluator: Evaluator) -> Result<Self, CompileError> {
        if !narrows(query) && refuses_broad_queries(evaluator) {
            return Err(CompileError::TooBroad);
        }

        let case_insensitive = case_insensitive_for(query);
        let pattern = match &query.name_pattern {
            Some(pattern) if !pattern.is_empty() => {
                Some(compile_pattern(pattern, &query.pattern_type, case_insensitive)?)
            }
            _ => None,
        };

        Ok(Self {
            pattern,
            case_insensitive,
            is_directory: query.is_directory,
            min_size: query.min_size,
            max_size: query.max_size,
            modified_after: query.modified_after,
            modified_before: query.modified_before,
        })
    }

    /// The case-folding rule this query resolved to. Read it rather than deriving it
    /// again: the pattern was compiled with it, so a second derivation that disagreed
    /// would rank and exclude against a different alphabet than it matched with.
    pub(crate) fn case_insensitive(&self) -> bool {
        self.case_insensitive
    }

    /// Whether one candidate satisfies every predicate.
    ///
    /// Runs per entry over millions of rows, so it borrows its name and does no
    /// allocation. Size bounds apply to FILES only; a directory's size arrives later,
    /// from `dir_stats` (see the module doc).
    #[inline]
    pub(crate) fn matches(&self, candidate: &Candidate<'_>) -> bool {
        if let Some(ref re) = self.pattern
            && !re.is_match(candidate.name)
        {
            return false;
        }

        if let Some(is_dir) = self.is_directory
            && candidate.is_directory != is_dir
        {
            return false;
        }

        if !candidate.is_directory {
            if let Some(min) = self.min_size {
                match candidate.size {
                    Some(s) if s >= min => {}
                    _ => return false,
                }
            }
            if let Some(max) = self.max_size {
                match candidate.size {
                    Some(s) if s <= max => {}
                    _ => return false,
                }
            }
        }

        if let Some(after) = self.modified_after {
            match candidate.modified_at {
                Some(t) if t >= after => {}
                _ => return false,
            }
        }
        if let Some(before) = self.modified_before {
            match candidate.modified_at {
                Some(t) if t <= before => {}
                _ => return false,
            }
        }

        true
    }
}

/// Whether anything in the query narrows the candidate set. An empty name pattern
/// narrows nothing, so it doesn't count.
fn narrows(query: &SearchQuery) -> bool {
    query.name_pattern.as_ref().is_some_and(|p| !p.is_empty())
        || query.min_size.is_some()
        || query.max_size.is_some()
        || query.modified_after.is_some()
        || query.modified_before.is_some()
        || query.is_directory.is_some()
}

/// Whether `evaluator` refuses a query that narrows nothing.
fn refuses_broad_queries(evaluator: Evaluator) -> bool {
    match evaluator {
        Evaluator::Arena { entries } => entries > ARENA_BROAD_QUERY_CEILING,
        Evaluator::LiveWalk => true,
    }
}

/// The platform case-folding default (insensitive on macOS, sensitive on Linux),
/// with the query's own override winning.
fn case_insensitive_for(query: &SearchQuery) -> bool {
    match query.case_sensitive {
        Some(true) => false,
        Some(false) => true,
        None => cfg!(target_os = "macos"),
    }
}

/// Build the name-matching regex.
///
/// A glob with no wildcard is wrapped in `*…*` so typing `tes` finds `test.rs`, the
/// UX every file-search dialog has. On macOS the pattern is NFD-normalized first, so
/// it matches the decomposed names APFS stores without folding every candidate.
///
/// The two pattern types get DIFFERENT `.` semantics, on purpose. `glob_to_regex`
/// emits its own `(?s)`, because a glob's `*` and `?` mean "any characters" and "one
/// character" and a newline in a filename is one of them. A user-supplied regex goes
/// to the builder untouched and keeps the standard rule, where `.` stops at a newline
/// and `(?s)` is how an author asks for more; overriding that would make Cmdr's regex
/// mode a dialect of its own.
fn compile_pattern(pattern: &str, pattern_type: &PatternType, case_insensitive: bool) -> Result<Regex, CompileError> {
    #[cfg(target_os = "macos")]
    let pattern = {
        use unicode_normalization::UnicodeNormalization;
        pattern.nfd().collect::<String>()
    };
    let regex_str = match pattern_type {
        PatternType::Glob => {
            let glob = if !pattern.contains('*') && !pattern.contains('?') {
                format!("*{pattern}*")
            } else {
                pattern.to_string()
            };
            glob_to_regex(&glob)
        }
        PatternType::Regex => pattern.to_string(),
    };
    RegexBuilder::new(&regex_str)
        .case_insensitive(case_insensitive)
        .build()
        .map_err(|e| CompileError::InvalidPattern(e.to_string()))
}

// ── The live-walk evaluator ──────────────────────────────────────────

impl CompiledQuery {
    /// Whether one entry a walk discovered satisfies every predicate.
    ///
    /// The size is the entry's OWN, before hardlink dedup, because that's what a
    /// listing shows. The index stores the deduplicated size instead, so a 2nd+
    /// hardlink to one file is sizeless there: a size bound keeps it in a live result
    /// and drops it from an indexed one. Bounded, and the live answer is the truthful
    /// one, so it stays.
    pub(crate) fn matches_covered(&self, entry: &CoveredEntry) -> bool {
        self.matches(&Candidate {
            name: &covered_name(&entry.path),
            is_directory: entry.is_directory,
            size: entry.logical_size,
            modified_at: entry.modified_at,
        })
    }
}

/// The name a walked entry matches under.
///
/// Byte-identical to the name the index would have stored for the same entry, lossy
/// conversion and the nameless-path fallback included: the local walker derives its
/// row name from the same path the same way (`indexing/scanner/insert_visitor.rs`),
/// and the trait walk's row name is the listing's `name`, which is that path's last
/// component. ❌ Don't "improve" this alone — a name derived two ways is the fork
/// this module exists to prevent, and the RESULT ROW takes its name from here too
/// (`live.rs::live_result_entry`), so a row can't be named differently from the
/// name it matched under.
pub(crate) fn covered_name(path: &Path) -> Cow<'_, str> {
    path.file_name()
        .map_or(Cow::Borrowed(""), |name| name.to_string_lossy())
}

#[cfg(test)]
mod tests;
