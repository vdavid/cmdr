//! Pure data definitions for search queries and results.

use serde::{Deserialize, Serialize};

// ── Query types ──────────────────────────────────────────────────────

/// How to order search results.
///
/// `Relevance` is what `ranking.rs` computes (match-quality band, then
/// importance-boosted recency). The other two are plain keys, and they REPLACE the
/// ranking rather than reordering its top-k, because a caller asking for the
/// biggest matches means the biggest ones that exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchSort {
    Relevance,
    Size,
    Modified,
}

#[derive(Debug, Clone, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub name_pattern: Option<String>,
    #[serde(default)]
    pub pattern_type: PatternType,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<u64>,
    pub modified_before: Option<u64>,
    pub is_directory: Option<bool>,
    #[serde(default)]
    pub include_paths: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_dir_names: Option<Vec<String>>,
    /// Never sent from the frontend or MCP, and never read by a search: the scope
    /// reaches the engine as a resolved `engine::ResolvedScope`. Only the pure-engine
    /// test wrapper (`engine::search`) reads it, as a fixture's shorthand.
    #[serde(skip)]
    pub include_path_ids: Option<Vec<i64>>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Per-query case sensitivity override.
    /// `None` = platform default (false on macOS, true on Linux).
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    /// How to order the results. `None` / `Relevance` is the ranked default;
    /// anything else replaces the ranking wholesale, so "the biggest matches"
    /// means the biggest on the drive rather than the best-ranked few reordered.
    #[serde(default)]
    pub sort_by: Option<SearchSort>,
    /// Whether to exclude common system/build/cache directories.
    /// `None` or `Some(true)` = exclude, `Some(false)` = include everything.
    #[serde(default)]
    pub exclude_system_dirs: Option<bool>,
    /// Count-only mode: return just `total_count`, with an empty `entries` vec.
    /// Skips ranking, truncation, and path materialization (the expensive parts).
    /// See `engine::search` for the exact-count contract with directory size filters.
    #[serde(default)]
    pub count_only: bool,
}

/// The most rows ONE search will ever return: the ceiling a caller's
/// [`SearchQuery::limit`] is clamped to, whichever path serves it.
///
/// It deliberately matches the frontend's `SNAPSHOT_ENTRIES_CAP`
/// (`src/lib/search/snapshot-store.svelte.ts`), because a promoted snapshot is what
/// consumes a whole result set: "Show all in main window" hands every row the search
/// returned to a pane. A backend ceiling below that one would silently decide how many
/// hits a person gets to see.
///
/// It's a CEILING, not a default — [`default_limit`] stays 30, and a caller asking for
/// 30 still gets 30. `total_count` keeps counting past it either way.
pub const MAX_RESULT_ROWS: u32 = 10_000;

pub(crate) fn default_limit() -> u32 {
    30
}

impl SearchQuery {
    /// How many rows this query may actually receive: its `limit`, clamped to
    /// [`MAX_RESULT_ROWS`]. The one place the ceiling is applied, so the index answer
    /// and the live walk can't cap differently.
    pub fn effective_limit(&self) -> usize {
        self.limit.min(MAX_RESULT_ROWS) as usize
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PatternType {
    #[default]
    Glob,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub entries: Vec<SearchResultEntry>,
    pub total_count: u32,
    /// Scope paths the search couldn't cover because they resolve to a volume with
    /// no search index on disk (an unindexed NAS share, an ejected drive). A TYPED
    /// honesty signal (not a message the caller string-matches): when it's non-empty
    /// AND `entries` is empty, the query returned nothing because the target isn't
    /// searchable — distinct from a genuine "no matches" — so the UI and MCP render
    /// "not indexed yet" rather than "no files found". Empty on a fully-covered
    /// search. Absent from a request body; populated server-side.
    #[serde(default)]
    pub uncovered_scopes: Vec<String>,
    /// Scope paths that routed to an INDEXED volume but weren't found in its index (a
    /// typo, a since-deleted folder, or a path outside a volume's mount root). The
    /// sibling of `uncovered_scopes`: there the whole volume is unindexed, here the
    /// volume is searchable but the specific path isn't in it. Also typed (branch on
    /// emptiness), so the UI/MCP say "couldn't find that path" rather than a silent
    /// "no files found". Empty when every scope path resolved.
    #[serde(default)]
    pub unresolved_scopes: Vec<String>,
    /// The ONE volume this search covered, as routing resolved it. Sent so a caller
    /// acting on a coverage gap acts on the right drive: a scope typed into the box
    /// can point at a volume the focused pane isn't on, and re-deriving that from the
    /// path frontend-side would fork routing (an SMB id keys on the ADDRESS, cloud
    /// drives route to `root`). Empty only on a result nothing routed (the test-only
    /// pure-engine wrapper).
    #[serde(default)]
    pub target_volume_id: String,
    /// How many matches an exclusion rule kept out of `total_count`: the
    /// system/build/cache tier (on unless `exclude_system_dirs: Some(false)`) plus
    /// any `!` excludes in the scope.
    ///
    /// Typed, and reported rather than swallowed. The default exclusions are right
    /// for "find my invoice" and exactly wrong for "where is my disk space going",
    /// where the caches ARE the answer — so a caller that doesn't know it's seeing
    /// a filtered count would state a wrong conclusion confidently. Zero when
    /// nothing was hidden.
    #[serde(default)]
    pub hidden_by_excludes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultEntry {
    pub name: String,
    pub path: String,
    pub parent_path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
    pub icon_id: String,
    /// Internal entry ID for batch dir_stats lookup. Not sent to frontend.
    #[serde(skip)]
    pub entry_id: i64,
}

/// Parsed search scope: which subtrees to include and which directory names/paths to exclude.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ParsedScope {
    pub include_paths: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── The row ceiling ──────────────────────────────────────────────

    /// A query with everything defaulted except `limit`.
    fn query_with_limit(limit: u32) -> SearchQuery {
        SearchQuery {
            name_pattern: None,
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            include_path_ids: None,
            limit,
            case_sensitive: None,
            sort_by: None,
            exclude_system_dirs: None,
            count_only: false,
        }
    }

    #[test]
    fn limit_below_the_ceiling_is_untouched() {
        assert_eq!(query_with_limit(default_limit()).effective_limit(), 30);
        assert_eq!(query_with_limit(1).effective_limit(), 1);
    }

    #[test]
    fn limit_above_the_ceiling_gets_the_ceiling() {
        assert_eq!(
            query_with_limit(MAX_RESULT_ROWS + 1).effective_limit(),
            MAX_RESULT_ROWS as usize
        );
        assert_eq!(query_with_limit(u32::MAX).effective_limit(), MAX_RESULT_ROWS as usize);
    }

    #[test]
    fn the_ceiling_matches_the_frontend_snapshot_cap() {
        // `SNAPSHOT_ENTRIES_CAP` in `src/lib/search/snapshot-store.svelte.ts`. A promoted
        // snapshot consumes a whole result set, so a lower backend ceiling would decide
        // how many hits reach the pane.
        assert_eq!(MAX_RESULT_ROWS, 10_000);
    }

    // ── Serde round-trip ─────────────────────────────────────────────

    #[test]
    fn serde_roundtrip_search_query() {
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: Some(1024),
            max_size: None,
            modified_after: Some(1000),
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            include_path_ids: None,
            count_only: false,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
            sort_by: None,
        };
        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("minSize"));
        assert!(json.contains("modifiedAfter"));

        let deserialized: SearchQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name_pattern, Some("*.pdf".to_string()));
        assert_eq!(deserialized.pattern_type, PatternType::Glob);
        assert_eq!(deserialized.min_size, Some(1024));
        assert_eq!(deserialized.max_size, None);
    }

    #[test]
    fn serde_roundtrip_search_result() {
        let result = SearchResult {
            entries: vec![SearchResultEntry {
                name: "test.pdf".to_string(),
                path: "/Users/test_user_4/test.pdf".to_string(),
                parent_path: "~/test_user_4".to_string(),
                is_directory: false,
                size: Some(1024),
                modified_at: Some(1000),
                icon_id: "ext:pdf".to_string(),
                entry_id: 42,
            }],
            total_count: 1,
            uncovered_scopes: Vec::new(),
            unresolved_scopes: Vec::new(),
            target_volume_id: "root".to_string(),
            hidden_by_excludes: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("totalCount"));
        assert!(json.contains("isDirectory"));
        assert!(json.contains("modifiedAt"));
        assert!(json.contains("iconId"));
        assert!(json.contains("parentPath"));
        // entry_id is #[serde(skip)]: must not appear in JSON
        assert!(!json.contains("entryId"));

        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_count, 1);
        assert_eq!(deserialized.entries[0].name, "test.pdf");
    }

    #[test]
    fn serde_query_optional_fields_null() {
        let json = r#"{"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.name_pattern, None);
        assert_eq!(query.pattern_type, PatternType::Glob);
        assert_eq!(query.min_size, None);
        assert_eq!(query.limit, 30);
    }

    #[test]
    fn serde_query_camel_case_from_frontend() {
        let json = r#"{"namePattern":"*.pdf","patternType":"glob","minSize":1024,"maxSize":null,"modifiedAfter":null,"modifiedBefore":null,"isDirectory":false,"limit":50}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.name_pattern, Some("*.pdf".to_string()));
        assert_eq!(query.min_size, Some(1024));
        assert_eq!(query.is_directory, Some(false));
        assert_eq!(query.limit, 50);
    }

    // ── Serde: new scope fields ─────────────────────────────────────

    #[test]
    fn serde_query_scope_fields_camel_case() {
        let json = r#"{"namePattern":"*.rs","patternType":"glob","includePaths":["/Users/test_user_4/projects"],"excludeDirNames":["node_modules",".git"],"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(
            query.include_paths,
            Some(vec!["/Users/test_user_4/projects".to_string()])
        );
        assert_eq!(
            query.exclude_dir_names,
            Some(vec!["node_modules".to_string(), ".git".to_string()])
        );
    }

    #[test]
    fn serde_query_scope_fields_omitted() {
        let json = r#"{"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include_paths, None);
        assert_eq!(query.exclude_dir_names, None);
    }

    #[test]
    fn serde_parsed_scope_roundtrip() {
        let scope = ParsedScope {
            include_paths: vec!["/Users/test_user_4/projects".to_string()],
            exclude_patterns: vec!["node_modules".to_string(), ".git".to_string()],
        };
        let json = serde_json::to_string(&scope).unwrap();
        assert!(json.contains("includePaths"));
        assert!(json.contains("excludePatterns"));
        let deserialized: ParsedScope = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.include_paths, scope.include_paths);
        assert_eq!(deserialized.exclude_patterns, scope.exclude_patterns);
    }
}
