//! LLM enum → value conversion functions.
//!
//! Each function maps one parsed field from `ParsedLlmResponse` into the
//! corresponding search filter: type → regex, time → timestamps, size → bytes,
//! scope → paths, keywords → pattern. These are pure mapping functions with no
//! assembly logic.

mod keyword_mapping;
mod size_scope_mapping;
mod time_mapping;
mod type_mapping;

// ── Constants ────────────────────────────────────────────────────────

/// The `size` enum buckets (`tiny` … `huge`) are vague words, so their edges are just round binary amounts.
pub(crate) const KIB: u64 = 1_024;
pub(crate) const MIB: u64 = 1_024 * KIB;
pub(crate) const GIB: u64 = 1_024 * MIB;

/// Known file extensions for exact filename detection in `keywords_to_pattern`.
const KNOWN_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "rs", "py", "js", "ts", "go", "java", "json", "yml", "yaml", "toml", "html", "css",
    "md", "xml", "csv", "sql", "sh", "rb", "swift", "c", "cpp", "h", "hpp", "env", "log", "conf", "cfg", "ini", "lock",
    "png", "jpg", "jpeg", "gif", "svg", "mp3", "mp4", "mov", "zip", "tar", "gz",
];

// Re-exports
pub(super) use keyword_mapping::parse_exclude_list;
pub use keyword_mapping::{keywords_to_pattern, merge_keyword_and_type};
pub use size_scope_mapping::{scope_to_paths, size_to_filter};
pub use time_mapping::time_to_range;
pub use type_mapping::type_to_filter;
