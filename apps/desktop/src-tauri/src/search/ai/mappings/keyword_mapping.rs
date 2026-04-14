use crate::search::PatternType;

use super::KNOWN_EXTENSIONS;
use super::type_mapping::TypeFilter;

// ── Keyword pattern ──────────────────────────────────────────────────

/// Convert keywords into a (pattern, PatternType) pair.
///
/// - Single keyword → `*keyword*` (glob)
/// - Multiple keywords → `(kw1|kw2)` (regex, unanchored)
/// - Exact filename (has known extension) → `^name\.ext$` (regex, anchored)
pub fn keywords_to_pattern(keywords: &str) -> Option<(String, PatternType)> {
    let keywords = keywords.trim();
    if keywords.is_empty() {
        return None;
    }

    let words: Vec<&str> = keywords.split_whitespace().collect();

    // Check for exact filename: single token with a known extension
    if words.len() == 1 {
        if let Some(ext) = extract_known_extension(words[0]) {
            let name_part = &words[0][..words[0].len() - ext.len() - 1]; // strip ".ext"
            let escaped_name = regex_escape(name_part);
            let escaped_ext = regex_escape(ext);
            return Some((format!("^{escaped_name}\\.{escaped_ext}$"), PatternType::Regex));
        }
        // Single keyword, not a filename → glob
        return Some((format!("*{}*", words[0]), PatternType::Glob));
    }

    // Multiple keywords → regex alternation
    let alts: Vec<String> = words.iter().map(|w| regex_escape(w)).collect();
    Some((format!("({})", alts.join("|")), PatternType::Regex))
}

/// If the string has a `.ext` suffix where ext is 2-5 alpha chars matching a known extension,
/// return the extension (without the dot). Otherwise `None`.
fn extract_known_extension(s: &str) -> Option<&str> {
    if let Some(dot_pos) = s.rfind('.') {
        let ext = &s[dot_pos + 1..];
        if ext.len() >= 2
            && ext.len() <= 5
            && ext.chars().all(|c| c.is_ascii_alphabetic())
            && KNOWN_EXTENSIONS.contains(&ext.to_lowercase().as_str())
        {
            return Some(ext);
        }
    }
    None
}

/// Escape regex metacharacters in a string.
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if matches!(
            c,
            '.' | '+' | '*' | '?' | '(' | ')' | '{' | '}' | '[' | ']' | '^' | '$' | '|' | '\\'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// ── Pattern merging ──────────────────────────────────────────────────

/// Known file extensions that, when used as keywords alongside a matching type,
/// are redundant — the type pattern already covers them.
const EXTENSION_KEYWORDS: &[&str] = &[
    "heic", "jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "tiff", "mp4", "mov", "avi", "mkv", "webm", "pdf",
    "doc", "docx", "txt", "odt", "xls", "xlsx", "ppt", "pptx", "odp", "zip", "tar", "gz", "tgz", "bz2", "xz", "7z",
    "rar", "mp3", "m4a", "flac", "wav", "ogg", "aac", "rs", "py", "js", "ts", "go", "java", "c", "cpp", "rb", "swift",
    "json", "yml", "yaml", "toml", "ini", "conf", "cfg", "log", "out", "err", "ttf", "otf", "woff", "woff2", "sqlite",
    "sqlite3", "db", "sh", "bash", "zsh",
];

/// Check if a keyword is redundant with the type filter.
///
/// Returns `true` when the keyword (case-insensitive, with optional leading dot)
/// is a known file extension OR matches the type name itself (e.g., keyword "fonts"
/// with type "fonts"). In these cases the type pattern alone is sufficient.
fn keyword_redundant_with_type(kw_pattern: &str, type_pattern: &str) -> bool {
    let core = extract_keyword_core(kw_pattern);
    let core_lower = core.to_lowercase();
    // Strip leading dot and regex escapes for comparison
    let clean = core_lower
        .strip_prefix("\\.")
        .or(core_lower.strip_prefix('.'))
        .unwrap_or(&core_lower);
    let clean = clean.strip_prefix("\\").unwrap_or(clean);

    // Check if keyword is a known extension that appears in the type's pattern
    if EXTENSION_KEYWORDS.contains(&clean) && type_pattern.to_lowercase().contains(clean) {
        return true;
    }
    // Check if keyword matches the type concept itself (e.g., "fonts" keyword with fonts type)
    // by checking if the keyword text appears as a substring in common type names
    let type_names = [
        "photos",
        "screenshots",
        "videos",
        "documents",
        "presentations",
        "archives",
        "music",
        "code",
        "config",
        "logs",
        "fonts",
        "databases",
        "shell-scripts",
    ];
    for name in type_names {
        if clean == name || clean == name.strip_suffix('s').unwrap_or(name) {
            return true;
        }
    }
    false
}

/// Merge keyword pattern and type filter into a single (name_pattern, PatternType).
///
/// When both are present, they're combined into a regex that requires both
/// conditions. When only one is present, it's used directly.
/// If the keyword is redundant with the type (e.g., an extension name that the
/// type pattern already covers), the keyword is dropped.
pub fn merge_keyword_and_type(
    kw: Option<(String, PatternType)>,
    type_filter: Option<&TypeFilter>,
) -> (Option<String>, PatternType) {
    match (kw, type_filter) {
        // Both keyword and type → check for redundancy, then merge
        (Some((kw_pattern, _kw_type)), Some(tf)) => {
            // If keyword is redundant with the type (e.g., "heic" + photos, "sqlite" + databases),
            // just use the type pattern — the keyword adds no value.
            if keyword_redundant_with_type(&kw_pattern, tf.pattern) {
                return (Some(format!("(?i){}", tf.pattern)), PatternType::Regex);
            }
            // Extract the core keyword from the pattern
            let keyword_core = extract_keyword_core(&kw_pattern);
            // Combine: keyword must appear, then type extension must match
            let merged = format!("(?i){keyword_core}.*{}", strip_anchors(tf.pattern));
            (Some(merged), PatternType::Regex)
        }
        // Keywords only → use keyword pattern as-is
        (Some((pattern, pt)), None) => (Some(pattern), pt),
        // Type only → use type pattern as regex
        (None, Some(tf)) => (Some(format!("(?i){}", tf.pattern)), PatternType::Regex),
        // Neither → no pattern
        (None, None) => (None, PatternType::Glob),
    }
}

/// Extract the core keyword text from a pattern, stripping glob/regex wrappers.
///
/// `*keyword*` → `keyword`, `^name\.ext$` → `name\.ext`, `(a|b)` → `(a|b)`
fn extract_keyword_core(pattern: &str) -> String {
    let p = pattern.trim();
    // Glob pattern: *keyword*
    if p.starts_with('*') && p.ends_with('*') && p.len() > 2 {
        return regex_escape(&p[1..p.len() - 1]);
    }
    // Anchored regex: ^...$
    if p.starts_with('^') && p.ends_with('$') {
        // Remove anchors but keep the regex body (already escaped)
        return p[1..p.len() - 1].to_string();
    }
    // Already a regex (like alternation) → use as-is
    p.to_string()
}

/// Strip leading `^` and trailing `$` from a regex pattern.
fn strip_anchors(pattern: &str) -> &str {
    let p = pattern.strip_prefix('^').unwrap_or(pattern);
    p.strip_suffix('$').unwrap_or(p)
}

// ── Exclude parsing ──────────────────────────────────────────────────

/// Parse space-separated exclude directory names.
pub(crate) fn parse_exclude_list(exclude: &str) -> Vec<String> {
    exclude
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::type_mapping::type_to_filter;
    use super::*;

    #[test]
    fn keywords_single_word_glob() {
        let (pattern, pt) = keywords_to_pattern("rymd").unwrap();
        assert_eq!(pattern, "*rymd*");
        assert_eq!(pt, PatternType::Glob);
    }

    #[test]
    fn keywords_multiple_words_regex() {
        let (pattern, pt) = keywords_to_pattern("contract agreement").unwrap();
        assert_eq!(pt, PatternType::Regex);
        assert!(pattern.contains("contract"));
        assert!(pattern.contains("agreement"));
        assert!(pattern.contains('|'));
    }

    #[test]
    fn keywords_exact_filename() {
        let (pattern, pt) = keywords_to_pattern("package.json").unwrap();
        assert_eq!(pt, PatternType::Regex);
        assert_eq!(pattern, r"^package\.json$");
    }

    #[test]
    fn keywords_not_filename_if_unknown_ext() {
        // "v2.0" → not a filename (0 is not a known extension)
        let (pattern, pt) = keywords_to_pattern("v2.0").unwrap();
        assert_eq!(pt, PatternType::Glob);
        assert_eq!(pattern, "*v2.0*");
    }

    #[test]
    fn keywords_not_filename_if_ext_too_long() {
        let (pattern, pt) = keywords_to_pattern("file.something").unwrap();
        assert_eq!(pt, PatternType::Glob);
        assert_eq!(pattern, "*file.something*");
    }

    #[test]
    fn keywords_empty_returns_none() {
        assert!(keywords_to_pattern("").is_none());
        assert!(keywords_to_pattern("   ").is_none());
    }

    // ── Pattern merge ────────────────────────────────────────────────

    #[test]
    fn merge_keywords_and_type() {
        let kw = keywords_to_pattern("rymd");
        let tf = type_to_filter("documents");
        let (pattern, pt) = merge_keyword_and_type(kw, tf.as_ref());
        assert_eq!(pt, PatternType::Regex);
        let pattern = pattern.unwrap();
        // Should contain keyword and extension pattern
        assert!(pattern.contains("rymd"));
        assert!(pattern.contains("pdf"));
        // Verify it compiles
        let re = regex::RegexBuilder::new(&pattern)
            .case_insensitive(false) // (?i) is in the pattern
            .build();
        assert!(re.is_ok(), "merged pattern should compile: {pattern}");
        // Test matching
        let re = re.unwrap();
        assert!(re.is_match("rymd.pdf"));
        assert!(re.is_match("rymd_invoice.docx"));
        assert!(!re.is_match("rymd.rs")); // wrong extension
    }

    #[test]
    fn merge_type_only() {
        let tf = type_to_filter("rust");
        let (pattern, pt) = merge_keyword_and_type(None, tf.as_ref());
        assert_eq!(pt, PatternType::Regex);
        let pattern = pattern.unwrap();
        assert!(pattern.contains("rs"));
    }

    #[test]
    fn merge_keywords_only() {
        let kw = keywords_to_pattern("node_modules");
        let (pattern, pt) = merge_keyword_and_type(kw, None);
        assert_eq!(pt, PatternType::Glob);
        assert_eq!(pattern.unwrap(), "*node_modules*");
    }

    #[test]
    fn merge_neither() {
        let (pattern, pt) = merge_keyword_and_type(None, None);
        assert!(pattern.is_none());
        assert_eq!(pt, PatternType::Glob);
    }

    #[test]
    fn merge_screenshots_with_keyword() {
        let kw = keywords_to_pattern("meeting");
        let tf = type_to_filter("screenshots");
        let (pattern, pt) = merge_keyword_and_type(kw, tf.as_ref());
        assert_eq!(pt, PatternType::Regex);
        let pattern = pattern.unwrap();
        // Should compile and contain both parts
        let re = regex::Regex::new(&pattern);
        assert!(re.is_ok(), "pattern should compile: {pattern}");
    }

    #[test]
    fn merge_exact_filename_with_type() {
        let kw = keywords_to_pattern("package.json");
        let tf = type_to_filter("config");
        let (pattern, pt) = merge_keyword_and_type(kw, tf.as_ref());
        assert_eq!(pt, PatternType::Regex);
        let pattern = pattern.unwrap();
        let re = regex::Regex::new(&pattern);
        assert!(re.is_ok(), "pattern should compile: {pattern}");
    }

    #[test]
    fn merge_redundant_extension_keyword_dropped() {
        // "heic" keyword + photos type → just photos type pattern (keyword is redundant)
        let kw = keywords_to_pattern("heic");
        let tf = type_to_filter("photos");
        let (pattern, pt) = merge_keyword_and_type(kw, tf.as_ref());
        assert_eq!(pt, PatternType::Regex);
        let pattern = pattern.unwrap();
        // Should NOT contain "heic.*" prefix — just the photos pattern
        assert!(
            !pattern.contains("heic.*"),
            "redundant keyword should be dropped: {pattern}"
        );
        assert!(pattern.contains("jpg"));
    }

    #[test]
    fn merge_redundant_sqlite_keyword_dropped() {
        // "sqlite" keyword + databases type → just databases type pattern
        let kw = keywords_to_pattern("sqlite");
        let tf = type_to_filter("databases");
        let (pattern, pt) = merge_keyword_and_type(kw, tf.as_ref());
        assert_eq!(pt, PatternType::Regex);
        let pattern = pattern.unwrap();
        assert!(
            !pattern.contains("sqlite.*"),
            "redundant keyword should be dropped: {pattern}"
        );
        assert!(pattern.contains("db"));
    }

    #[test]
    fn merge_nonredundant_keyword_preserved() {
        // "rymd" keyword + documents type → merged pattern
        let kw = keywords_to_pattern("rymd");
        let tf = type_to_filter("documents");
        let (pattern, _) = merge_keyword_and_type(kw, tf.as_ref());
        let pattern = pattern.unwrap();
        assert!(
            pattern.contains("rymd"),
            "non-redundant keyword should be preserved: {pattern}"
        );
    }
}
