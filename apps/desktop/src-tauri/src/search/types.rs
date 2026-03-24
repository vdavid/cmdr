//! Pure data definitions for search queries and results.

use serde::{Deserialize, Serialize};

// ── Query types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// Pre-resolved entry IDs for `include_paths`. Populated server-side
    /// before calling `search()` — not sent from the frontend/MCP.
    #[serde(skip)]
    pub include_path_ids: Option<Vec<i64>>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Per-query case sensitivity override.
    /// `None` = platform default (false on macOS, true on Linux).
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    /// Whether to exclude common system/build/cache directories.
    /// `None` or `Some(true)` = exclude, `Some(false)` = include everything.
    #[serde(default)]
    pub exclude_system_dirs: Option<bool>,
}

pub(crate) fn default_limit() -> u32 {
    30
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PatternType {
    #[default]
    Glob,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub entries: Vec<SearchResultEntry>,
    pub total_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedScope {
    pub include_paths: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
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
                path: "/Users/alice/test.pdf".to_string(),
                parent_path: "~/alice".to_string(),
                is_directory: false,
                size: Some(1024),
                modified_at: Some(1000),
                icon_id: "ext:pdf".to_string(),
                entry_id: 42,
            }],
            total_count: 1,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("totalCount"));
        assert!(json.contains("isDirectory"));
        assert!(json.contains("modifiedAt"));
        assert!(json.contains("iconId"));
        assert!(json.contains("parentPath"));
        // entry_id is #[serde(skip)] — must not appear in JSON
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
        let json = r#"{"namePattern":"*.rs","patternType":"glob","includePaths":["/Users/alice/projects"],"excludeDirNames":["node_modules",".git"],"limit":30}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.include_paths, Some(vec!["/Users/alice/projects".to_string()]));
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
            include_paths: vec!["/Users/alice/projects".to_string()],
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
