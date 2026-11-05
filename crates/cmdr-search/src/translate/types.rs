//! Serialization DTOs for the AI translation result.
//!
//! Pure data, no logic. `query_builder` fills them; `commands::search` re-exports
//! them so the IPC surface is unchanged. specta names a type by its struct
//! identity, not its module, so `bindings.ts` is unaffected by where they live.

use serde::Serialize;

/// The structured query with unix timestamps, ready for `search_files`.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslatedQuery {
    pub name_pattern: Option<String>,
    pub pattern_type: String,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<u64>,
    pub modified_before: Option<u64>,
    pub is_directory: Option<bool>,
    pub include_paths: Option<Vec<String>>,
    pub exclude_dir_names: Option<Vec<String>>,
    pub case_sensitive: Option<bool>,
    pub exclude_system_dirs: Option<bool>,
}

/// Human-readable values so the frontend can populate filter UI.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslateDisplay {
    pub name_pattern: Option<String>,
    pub pattern_type: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub is_directory: Option<bool>,
    pub include_paths: Option<Vec<String>>,
    pub exclude_dir_names: Option<Vec<String>>,
    pub case_sensitive: Option<bool>,
}
