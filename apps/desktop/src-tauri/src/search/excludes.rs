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

use regex::{Regex, RegexBuilder};

use cmdr_index::store;

use super::query::{SYSTEM_DIR_EXCLUDES, glob_to_regex};
use super::types::SearchQuery;

/// The directory names and path prefixes a query excludes, compiled once.
#[derive(Debug)]
pub(crate) struct ExcludeRules {
    /// Exact directory names (system tier + plain user excludes), stored
    /// normalized when [`case_insensitive`](Self::case_insensitive) is set so a
    /// lookup is one hash.
    exact_names: HashSet<String>,
    /// User excludes containing a wildcard, as regexes over a directory name.
    name_patterns: Vec<Regex>,
    /// User excludes containing a `/`, matched as absolute path prefixes.
    path_prefixes: Vec<String>,
    /// Whether name matching folds case, taken from the compiled query.
    case_insensitive: bool,
}

impl ExcludeRules {
    /// Compile a query's exclusions. `case_insensitive` comes from the compiled
    /// query; ❌ don't derive it again here.
    pub(crate) fn from_query(query: &SearchQuery, case_insensitive: bool) -> Self {
        let mut rules = Self {
            exact_names: HashSet::new(),
            name_patterns: Vec::new(),
            path_prefixes: Vec::new(),
            case_insensitive,
        };

        // User excludes: a `/` means a path prefix, a wildcard means a glob, and
        // anything else is a plain directory name.
        if let Some(patterns) = query.exclude_dir_names.as_ref() {
            for pattern in patterns {
                if pattern.contains('/') {
                    rules.path_prefixes.push(pattern.clone());
                } else if pattern.contains('*') || pattern.contains('?') {
                    let regex_str = glob_to_regex(pattern);
                    if let Ok(re) = RegexBuilder::new(&regex_str).case_insensitive(case_insensitive).build() {
                        rules.name_patterns.push(re);
                    }
                } else {
                    rules.exact_names.insert(rules.fold(pattern));
                }
            }
        }

        // The system/build/cache tier, on unless the query turns it off.
        if query.exclude_system_dirs != Some(false) {
            for &name in SYSTEM_DIR_EXCLUDES {
                let key = rules.fold(name);
                rules.exact_names.insert(key);
            }
        }

        rules
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
    /// Runs per ancestor per candidate on the arena's hot path, so the
    /// case-sensitive branch borrows rather than folding a copy of every name it
    /// is asked about.
    pub(crate) fn excludes_dir_name(&self, name: &str) -> bool {
        if !self.exact_names.is_empty() {
            let excluded = if self.case_insensitive {
                self.exact_names.contains(&store::normalize_for_comparison(name))
            } else {
                self.exact_names.contains(name)
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

    /// One name in the alphabet the exact-name set is keyed on.
    fn fold(&self, name: &str) -> String {
        if self.case_insensitive {
            store::normalize_for_comparison(name)
        } else {
            name.to_string()
        }
    }
}

#[cfg(test)]
mod tests;
