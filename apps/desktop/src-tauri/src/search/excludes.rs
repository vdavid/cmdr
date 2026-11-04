//! Which directories a search refuses to look inside, however it's reading them.
//!
//! The sibling of [`matcher.rs`](super::matcher): that one owns the per-entry
//! predicates, this one owns the SCOPE exclusions (`excludeDirNames` and the
//! `excludeSystemDirs` tier), and both exist for the same reason — two evaluators
//! ask the same question and must not answer it differently.
//!
//! - the **arena scan** ([`super::engine`]) walks an entry's ancestor ids through
//!   the in-memory index and asks about each ancestor's name, and
//! - a **live walk** has no ids at all, so it asks about the components of the
//!   entry's own path.
//!
//! The rules themselves — what counts as an exact name, what's a glob, what's a
//! path prefix, and how case folding applies to each — live here once. A fork
//! there is silent: the same query would exclude a folder on an indexed drive and
//! search it on an unindexed one.
//!
//! ❌ Case folding isn't derived here either. It arrives from the compiled query
//! (`CompiledQuery::case_insensitive`), so the pattern, the ranker, and these
//! exclusions all fold under one alphabet.

use std::collections::HashSet;
use std::sync::Arc;

use regex::{Regex, RegexBuilder};

use super::context::NameFold;
use super::matcher::CompileError;
use super::query::{SYSTEM_DIR_EXCLUDES, glob_to_regex};
use super::types::SearchQuery;

/// Longest name the exact-name lookup folds on the stack: `NAME_MAX` on APFS and
/// most local filesystems. A longer name (possible over SMB) takes the general fold.
const STACK_FOLD_MAX: usize = 255;

/// The directory names and path prefixes a query excludes, compiled once.
///
/// Cheap to clone, and cloned once per scan chunk (`engine.rs`): the sets are
/// shared, and each clone's regexes get a cache pool of their own, which keeps the
/// arena scan's workers off each other's caches.
#[derive(Debug, Clone)]
pub(crate) struct ExcludeRules {
    /// Exact directory names (system tier + plain user excludes), stored
    /// normalized when [`case_insensitive`](Self::case_insensitive) is set so a
    /// lookup is one hash.
    exact_names: Arc<HashSet<String>>,
    /// User excludes containing a wildcard, as regexes over a directory name.
    name_patterns: Vec<Regex>,
    /// User excludes containing a `/`, matched as absolute path prefixes.
    path_prefixes: Arc<[String]>,
    /// Whether name matching folds case, taken from the compiled query.
    case_insensitive: bool,
    /// The volume's name fold, under which a case-insensitive name is keyed.
    name_fold: NameFold,
}

impl ExcludeRules {
    /// Compile a query's exclusions. `case_insensitive` comes from the compiled
    /// query; ❌ don't derive it again here.
    ///
    /// A glob that won't compile is an error, ❌ never skipped: skipping it answers with
    /// the folders the user asked to hide.
    pub(crate) fn from_query(
        query: &SearchQuery,
        case_insensitive: bool,
        name_fold: NameFold,
    ) -> Result<Self, CompileError> {
        let mut exact_names = HashSet::new();
        let mut name_patterns = Vec::new();
        let mut path_prefixes = Vec::new();

        // User excludes: a `/` means a path prefix, a wildcard means a glob, and
        // anything else is a plain directory name.
        if let Some(patterns) = query.exclude_dir_names.as_ref() {
            for pattern in patterns {
                if pattern.contains('/') {
                    path_prefixes.push(pattern.clone());
                } else if pattern.contains('*') || pattern.contains('?') {
                    let regex_str = glob_to_regex(pattern);
                    let re = RegexBuilder::new(&regex_str)
                        .case_insensitive(case_insensitive)
                        .build()
                        .map_err(|e| CompileError::InvalidExclude {
                            pattern: pattern.clone(),
                            reason: e.to_string(),
                        })?;
                    name_patterns.push(re);
                } else {
                    exact_names.insert(fold(pattern, case_insensitive, name_fold));
                }
            }
        }

        // The system/build/cache tier, on unless the query turns it off.
        if query.exclude_system_dirs != Some(false) {
            for &name in SYSTEM_DIR_EXCLUDES {
                exact_names.insert(fold(name, case_insensitive, name_fold));
            }
        }

        Ok(Self {
            exact_names: Arc::new(exact_names),
            name_patterns,
            path_prefixes: path_prefixes.into(),
            case_insensitive,
            name_fold,
        })
    }

    /// Whether anything is excluded at all, so a caller can skip the walk.
    pub(crate) fn is_empty(&self) -> bool {
        self.exact_names.is_empty() && self.name_patterns.is_empty() && self.path_prefixes.is_empty()
    }

    /// Whether any path prefix is set, so a caller can skip materializing a path
    /// it would only compare.
    pub(crate) fn has_path_prefixes(&self) -> bool {
        !self.path_prefixes.is_empty()
    }

    /// Whether any name rule is set (exact or glob).
    pub(crate) fn has_name_rules(&self) -> bool {
        !self.exact_names.is_empty() || !self.name_patterns.is_empty()
    }

    /// Whether one ANCESTOR directory's name is excluded.
    ///
    /// Runs per ancestor per match on the arena's hot path (a one-letter query
    /// asks it ~15 M times), so it doesn't allocate for any real-world name: the
    /// case-sensitive branch borrows, and the insensitive one folds an ASCII name
    /// on the stack. Only a non-ASCII or over-long name pays [`fold`]'s `String`s.
    ///
    /// ❌ The insensitive branch goes through [`fold`] (or its ASCII shortcut),
    /// never the volume's [`NameFold`] alone: the set is KEYED through `fold`, and
    /// off macOS that fold is a no-op, so a raw lookup would compare an
    /// unfolded name against a lowercased key and match nothing with a capital in it.
    pub(crate) fn excludes_dir_name(&self, name: &str) -> bool {
        if !self.exact_names.is_empty() {
            let excluded = if !self.case_insensitive {
                self.exact_names.contains(name)
            } else if let Some(excluded) = with_ascii_folded(name, |folded| self.exact_names.contains(folded)) {
                excluded
            } else {
                self.exact_names.contains(&fold(name, true, self.name_fold))
            };
            if excluded {
                return true;
            }
        }
        self.name_patterns.iter().any(|pattern| pattern.is_match(name))
    }

    /// Whether an absolute path sits under an excluded path prefix.
    pub(crate) fn excludes_path(&self, path: &str) -> bool {
        self.path_prefixes
            .iter()
            .any(|prefix| path.starts_with(prefix.as_str()))
    }

    /// Whether a walked entry is excluded, judged by its own path.
    ///
    /// The live evaluator. `path` is absolute in the space the walk reports (so
    /// mount-absolute on a share), and every component ABOVE the entry itself is
    /// checked as an ancestor directory name — the same set the arena's ancestor
    /// walk sees, which stops at the volume root. `volume_root` is where that
    /// walk would stop: the mount root for a mount-rooted volume, `None` for the
    /// boot volume, whose paths are already index-rooted.
    ///
    /// ❌ The entry's OWN name is never checked here. Excluding `node_modules`
    /// hides what's inside it, and the arena walk starts at `entry.parent_id` for
    /// exactly that reason; checking the leaf too would drop the folder itself
    /// from the results of a search that named it.
    pub(crate) fn excludes_walked(&self, path: &str, volume_root: Option<&str>) -> bool {
        if self.has_path_prefixes() && self.excludes_path(path) {
            return true;
        }
        if !self.has_name_rules() {
            return false;
        }
        let relative = match volume_root {
            Some(root) => path.strip_prefix(root).unwrap_or(path),
            None => path,
        };
        // Cut the leaf off first (it's the candidate, not an ancestor), then walk
        // what's left. Borrowed throughout: this runs per walked entry.
        let Some((ancestors, _leaf)) = relative.rsplit_once('/') else {
            return false;
        };
        ancestors
            .split('/')
            .filter(|component| !component.is_empty())
            .any(|component| self.excludes_dir_name(component))
    }
}

/// One name in the alphabet the exact-name set is keyed on.
///
/// ⚠️ The lowercase is EXPLICIT and not left to the volume's [`NameFold`], which is
/// deliberately [`NameFold::Exact`] on Linux: two names differing in case are two
/// different FILES there, so the index is right not to fold its keys. This is a
/// user's filter, not an index key — the query said case-insensitive, and
/// honoring that only on macOS is exactly the fork that makes a search exclude
/// under one alphabet and match under another. The normalization still comes
/// from the volume's fold, so macOS keeps folding NFD to the form APFS stores.
fn fold(name: &str, case_insensitive: bool, name_fold: NameFold) -> String {
    if case_insensitive {
        name_fold.apply(name).to_lowercase()
    } else {
        name.to_string()
    }
}

/// Run `f` on `name` case-folded, without allocating, if `name` is ASCII and fits
/// on the stack; `None` otherwise, for the caller to take the general [`fold`].
///
/// Byte-identical to `fold(name, true)` for every name it accepts: NFD leaves ASCII
/// untouched, and Unicode lowercasing of ASCII is ASCII lowercasing (pinned by
/// `the_ascii_fold_agrees_with_the_general_one`).
fn with_ascii_folded<R>(name: &str, f: impl FnOnce(&str) -> R) -> Option<R> {
    if name.len() > STACK_FOLD_MAX || !name.is_ascii() {
        return None;
    }
    let mut buf = [0u8; STACK_FOLD_MAX];
    let folded = &mut buf[..name.len()];
    folded.copy_from_slice(name.as_bytes());
    folded.make_ascii_lowercase();
    // ASCII in, ASCII out, so this is always valid UTF-8.
    std::str::from_utf8(folded).ok().map(f)
}

#[cfg(test)]
mod tests;
