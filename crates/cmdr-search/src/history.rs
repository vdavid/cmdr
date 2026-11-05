//! A recent search: the entry the dialog records, and the canonical key that
//! decides when two searches ask the same question.
//!
//! The list rules (newest first, dedupe on add, cap from the tail) are
//! `cmdr-recents`'; keeping the list is the host's (the desktop persists it to a
//! file, the browser demo keeps it for the session). [`HistoryMode`] and
//! [`HistoryFilters`] are shared with the desktop's Selection dialog, which builds
//! its own key from the same helpers ([`normalize_query`], [`filters_fingerprint`],
//! [`flag`]), so a new filter field can't teach one key about itself and not the
//! other.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use cmdr_recents::RecentItem;

/// The cap when the user hasn't tuned `search.recentSearches.maxCount`.
pub const DEFAULT_MAX_COUNT: usize = 1000;

/// Search modes recorded in history. Mirrors the frontend `SearchMode` union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum HistoryMode {
    Ai,
    Filename,
    Regex,
}

impl HistoryMode {
    /// The lowercase wire form, which is also what the canonical key carries.
    pub fn as_str(self) -> &'static str {
        match self {
            HistoryMode::Ai => "ai",
            HistoryMode::Filename => "filename",
            HistoryMode::Regex => "regex",
        }
    }
}

/// Filter slice of a history entry. Mirrors what the dialog carries on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
// Field docs are `//` on purpose: specta copies a `///` into `bindings.ts`.
pub struct HistoryFilters {
    #[serde(default)]
    pub size_min: Option<u64>,
    #[serde(default)]
    pub size_max: Option<u64>,
    #[serde(default)]
    pub modified_after: Option<String>,
    #[serde(default)]
    pub modified_before: Option<String>,
    /// Type filter, round-tripping the frontend `typeFilter` toggle:
    /// `Some(true) = folder`, `Some(false) = file`, `None = both`. Additive with
    /// `#[serde(default)]`, so older history files (no `isDirectory` key) load as `None`
    /// without a schema bump.
    #[serde(default)]
    pub is_directory: Option<bool>,
}

/// A single recent-search entry, persisted verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    /// Unix epoch milliseconds.
    pub timestamp: i64,
    pub mode: HistoryMode,
    pub query: String,
    #[serde(default)]
    pub filters: HistoryFilters,
    pub scope: String,
    pub case_sensitive: bool,
    pub exclude_system_dirs: bool,
    pub result_count: u32,
}

impl RecentItem for HistoryEntry {
    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Six segments: two entries with the same one asked the same question, so the
    /// newest copy wins and the older one goes. Selection's key is the same minus
    /// `scope` and `exclude_system_dirs`, which it has no concept of.
    fn dedupe_key(&self) -> String {
        let mode = self.mode.as_str();
        let query = normalize_query(&self.query);
        let filters = filters_fingerprint(&self.filters);
        // Scope: trim + lowercase so "/Users" and " /users " collapse.
        let scope = self.scope.trim().to_lowercase();
        let cs = flag(self.case_sensitive);
        let es = flag(self.exclude_system_dirs);
        format!("{mode}|{query}|{filters}|{scope}|{cs}|{es}")
    }
}

// The canonical-key pieces, shared with the desktop's Selection history. The key
// itself is never persisted; it only exists at compare time.

/// Collapses a query to its comparable form: trimmed, internal whitespace runs
/// squeezed to single spaces, lowercased.
pub fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Renders the filter slice into the key: keys sorted alphabetically, undefined
/// fields skipped entirely. `BTreeMap` gives the sort and keeps the key set
/// explicit, and sharing this is what keeps a newly-added filter field from
/// reaching one consumer's key and not the other's.
pub fn filters_fingerprint(filters: &HistoryFilters) -> String {
    let mut filter_kv: BTreeMap<&str, String> = BTreeMap::new();
    if let Some(v) = filters.size_min {
        filter_kv.insert("sizeMin", v.to_string());
    }
    if let Some(v) = filters.size_max {
        filter_kv.insert("sizeMax", v.to_string());
    }
    if let Some(ref v) = filters.modified_after {
        filter_kv.insert("modifiedAfter", v.clone());
    }
    if let Some(ref v) = filters.modified_before {
        filter_kv.insert("modifiedBefore", v.clone());
    }
    if let Some(v) = filters.is_directory {
        filter_kv.insert("isDirectory", v.to_string());
    }
    filter_kv
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// A bool as the key's single-char form.
pub fn flag(value: bool) -> &'static str {
    if value { "t" } else { "f" }
}

#[cfg(test)]
mod tests;
