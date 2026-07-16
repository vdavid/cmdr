//! Operations on search types: scope parsing, formatting, query summarization,
//! directory size enrichment, and system directory exclusions.

use crate::indexing::ReadPool;
use crate::indexing::store;

use super::types::{PatternType, SearchQuery};

// ── System directory exclusions ──────────────────────────────────────

/// Common system, build, and cache directory names excluded by default.
/// Applied automatically when `SearchQuery::exclude_system_dirs` is not `Some(false)`.
pub const SYSTEM_DIR_EXCLUDES: &[&str] = &[
    // Package managers & build tools
    "node_modules",
    ".pnpm-store",
    ".npm",
    ".yarn",
    ".cargo",
    ".m2",
    ".gradle",
    // VCS
    ".git",
    ".svn",
    ".hg",
    // Python
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    // JS/TS build output
    "build",
    "dist",
    ".next",
    ".nuxt",
    ".cache",
    ".parcel-cache",
    "target",
    // macOS system & caches
    "Caches",
    "CacheStorage",
    "Cache",
    "GPUCache",
    "ScriptCache",
    "GrShaderCache",
    "ShaderCache",
    "Logs",
    "Cookies",
    "WebKit",
    "Saved Application State",
    ".Trash",
    ".Spotlight-V100",
    ".fseventsd",
    ".DocumentRevisions-V100",
    // IDE workspace caches
    "workspaceStorage",
    "DerivedData",
];

// ── Query summary ────────────────────────────────────────────────────

/// Build a dense, human-readable summary of a `SearchQuery` for logging and display.
///
/// Examples: `"tes"`, `"*.pdf", dirs only`, `size >= 2 MB, last mod before 2026-03-01`
pub(crate) fn summarize_query(query: &SearchQuery) -> String {
    let mut parts = Vec::new();

    // Name pattern
    if let Some(ref pattern) = query.name_pattern
        && !pattern.is_empty()
    {
        let suffix = if query.pattern_type == PatternType::Regex {
            " (regex)"
        } else {
            ""
        };
        parts.push(format!("\"{pattern}\"{suffix}"));
    }

    // Size filters
    match (query.min_size, query.max_size) {
        (Some(min), Some(max)) => parts.push(format!("size {}–{}", format_size(min), format_size(max))),
        (Some(min), None) => parts.push(format!("size >= {}", format_size(min))),
        (None, Some(max)) => parts.push(format!("size <= {}", format_size(max))),
        (None, None) => {}
    }

    // Date filters
    match (query.modified_after, query.modified_before) {
        (Some(after), Some(before)) => {
            parts.push(format!(
                "last mod {}–{}",
                format_timestamp(after),
                format_timestamp(before)
            ));
        }
        (Some(after), None) => parts.push(format!("last mod after {}", format_timestamp(after))),
        (None, Some(before)) => parts.push(format!("last mod before {}", format_timestamp(before))),
        (None, None) => {}
    }

    // Directory filter
    match query.is_directory {
        Some(true) => parts.push("dirs only".to_string()),
        Some(false) => parts.push("files only".to_string()),
        None => {}
    }

    // Case sensitivity (only show when explicitly set)
    match query.case_sensitive {
        Some(true) => parts.push("case-sensitive".to_string()),
        Some(false) => parts.push("case-insensitive".to_string()),
        None => {}
    }

    if parts.is_empty() {
        "(all entries)".to_string()
    } else {
        parts.join(", ")
    }
}

pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    const TB: u64 = 1_024 * GB;
    const UNITS: &[(u64, &str)] = &[(TB, "TB"), (GB, "GB"), (MB, "MB"), (KB, "KB")];

    for &(threshold, unit) in UNITS {
        if bytes >= threshold {
            let val = bytes as f64 / threshold as f64;
            return if val.fract() == 0.0 {
                format!("{} {unit}", val as u64)
            } else {
                format!("{val:.1} {unit}")
            };
        }
    }
    format!("{bytes} B")
}

pub(crate) fn format_timestamp(ts: u64) -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::OffsetDateTime::from_unix_timestamp(ts as i64)
        .map(|dt| dt.format(&format).unwrap_or_else(|_| ts.to_string()))
        .unwrap_or_else(|_| ts.to_string())
}

// ── Glob to regex conversion ─────────────────────────────────────────

/// Convert a glob pattern to a regex pattern.
///
/// Escapes regex metacharacters, converts `*` to `.*` and `?` to `.`,
/// wraps in `^...$` for full-match semantics.
pub(crate) fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::with_capacity(glob.len() * 2 + 2);
    regex.push('^');
    for c in glob.chars() {
        match c {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '.' | '+' | '(' | ')' | '{' | '}' | '[' | ']' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(c);
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    regex
}

// ── Scope parsing ────────────────────────────────────────────────────

/// Parse a comma-separated scope string into include paths and exclude patterns.
///
/// Syntax: `~/projects, !node_modules, !.git`
/// - `~` expands to the user's home directory
/// - `!` prefix means exclude
/// - Quoted segments (single or double quotes) and backslash-escaped commas are supported
pub(crate) fn parse_scope(input: &str) -> super::types::ParsedScope {
    let segments = split_scope_segments(input);
    let home = dirs::home_dir().map(|p| p.to_string_lossy().to_string());

    let mut include_paths = Vec::new();
    let mut exclude_patterns = Vec::new();

    for seg in segments {
        let trimmed = seg.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (is_exclude, value) = if let Some(rest) = trimmed.strip_prefix('!') {
            (true, rest.trim())
        } else {
            (false, trimmed)
        };

        // Expand ~ prefix
        let expanded = if let Some(rest) = value.strip_prefix('~') {
            if let Some(ref h) = home {
                format!("{h}{rest}")
            } else {
                value.to_string()
            }
        } else {
            value.to_string()
        };

        if is_exclude {
            exclude_patterns.push(expanded);
        } else {
            include_paths.push(expanded);
        }
    }

    super::types::ParsedScope {
        include_paths,
        exclude_patterns,
    }
}

/// Split a scope string on commas, respecting quoting and backslash escapes.
pub(crate) fn split_scope_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quote: Option<char> = None;

    while let Some(c) = chars.next() {
        match c {
            '\\' if in_quote.is_none() => {
                // Backslash-escaped character: consume next char literally
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '"' | '\'' if in_quote.is_none() => {
                in_quote = Some(c);
            }
            q if in_quote == Some(q) => {
                in_quote = None;
            }
            ',' if in_quote.is_none() => {
                segments.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    segments.push(current);
    segments
}

// ── Include path resolution ──────────────────────────────────────────

/// Canonicalize a scope include path (resolve symlinks) so it matches the index's
/// stored REAL paths, without wedging on a hung mount.
///
/// The scanner walks the real filesystem, so the index stores canonical paths (on
/// macOS `/tmp` is a symlink, recorded as `/private/tmp`), while panes and agents
/// report the symlinked form (`scope:/tmp/x`). A literal prefix match then resolves
/// nothing → silent empty results. We canonicalize each include path ONCE here (a
/// handful of paths, off the hot per-entry scan loop) before the DB walk.
///
/// `fs::canonicalize` issues `realpath`, which blocks indefinitely on a dead network
/// mount, so it runs on a detached worker thread under a 2 s deadline (the sync
/// analog of `blocking_with_timeout`; `resolve_include_paths` is sync). On timeout,
/// an error, or a non-existent path we keep the literal — today's best-effort
/// behavior, so an offline/unmounted-index scope still gets its literal match.
fn canonicalize_scope_path(path: &str) -> String {
    let owned = path.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::fs::canonicalize(&owned));
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(canonical)) => canonical.to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

/// Map an absolute scope path into a volume's index path space (mount-relative),
/// so `store::resolve_path` — which walks component-by-component from the index
/// `ROOT_ID` — hits. `root` (mount_root `None`) is already index-rooted, so the
/// path passes through. A mount-rooted volume's index `ROOT_ID` is its mount root,
/// so the mount prefix is stripped: `/Volumes/naspi/sub` → `/sub`, the mount root
/// itself → `/`. A path outside the mount root yields `None` (don't mis-root it).
fn to_index_relative(path: &str, mount_root: Option<&str>) -> Option<String> {
    let Some(root) = mount_root else {
        return Some(path.to_string());
    };
    if path == root {
        return Some("/".to_string());
    }
    let rest = path.strip_prefix(root)?;
    rest.starts_with('/').then(|| rest.to_string())
}

/// Resolve a volume's scope include paths to entry IDs against that volume's index,
/// for the engine's ancestor-walk scope filter. Canonicalizes each path (symlink
/// resolution, off the hot loop), maps it into the volume's index path space
/// (mount-relative for a NAS/MTP volume), and looks it up via the volume's pool.
///
/// Returns the resolved IDs, or `[i64::MIN]` (an impossible id) when NONE resolve,
/// so the engine's include filter rejects every entry — a scope that matched
/// nothing in this volume yields no results rather than silently ignoring the
/// scope. Callers set the result on a per-volume `SearchQuery::include_path_ids`.
pub(crate) fn resolve_include_path_ids(paths: &[String], pool: &ReadPool, mount_root: Option<&str>) -> Vec<i64> {
    // Canonicalize each include path ONCE (resolve symlinks like /tmp → /private/tmp)
    // so the prefix walk matches the index's stored real paths. Off the hot scan loop.
    let index_paths: Vec<String> = paths
        .iter()
        .filter_map(|p| to_index_relative(&canonicalize_scope_path(p), mount_root))
        .collect();
    pool.with_conn(|conn| {
        let mut resolved = Vec::with_capacity(index_paths.len());
        for path in &index_paths {
            match store::resolve_path(conn, path) {
                Ok(Some(id)) => resolved.push(id),
                Ok(None) => log::debug!("search: include path not found in index: {path}"),
                Err(e) => log::warn!("search: failed to resolve include path {path}: {e}"),
            }
        }
        if resolved.is_empty() {
            resolved.push(i64::MIN);
        }
        resolved
    })
    .unwrap_or_else(|e| {
        log::warn!("search: ReadPool error resolving include paths: {e}");
        vec![i64::MIN]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── glob_to_regex ────────────────────────────────────────────────

    #[test]
    fn glob_to_regex_star() {
        assert_eq!(glob_to_regex("*.pdf"), r"^.*\.pdf$");
    }

    #[test]
    fn glob_to_regex_question() {
        assert_eq!(glob_to_regex("file?.txt"), r"^file.\.txt$");
    }

    #[test]
    fn glob_to_regex_escapes_metacharacters() {
        assert_eq!(glob_to_regex("a+b(c)"), r"^a\+b\(c\)$");
    }

    #[test]
    fn glob_to_regex_literal() {
        assert_eq!(glob_to_regex("readme"), "^readme$");
    }

    // ── glob_to_regex (property-based) ───────────────────────────────
    //
    // The output of `glob_to_regex` is fed directly into `regex::Regex::new`
    // by the search engine. A glob that escapes incorrectly would either
    // panic the regex parser or silently match more than the user intended.
    // These properties pin (a) the output is always a syntactically valid
    // regex and (b) it matches the user's literal intent when no glob
    // metacharacters are present.

    mod glob_proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// For any glob string, the produced regex compiles successfully
            /// and is anchored end-to-end.
            #[test]
            fn output_is_valid_anchored_regex(glob in ".*") {
                let pattern = glob_to_regex(&glob);
                prop_assert!(pattern.starts_with('^'), "regex must start with ^: {}", pattern);
                prop_assert!(pattern.ends_with('$'), "regex must end with $: {}", pattern);
                let compiled = regex::Regex::new(&pattern);
                prop_assert!(
                    compiled.is_ok(),
                    "regex must compile, got error for glob {:?}: {:?}",
                    glob,
                    compiled.err()
                );
            }

            /// For globs with no `*` or `?` (after the regex metachar set is
            /// taken into account), the compiled regex matches the original
            /// string literally and nothing else of different content.
            #[test]
            fn literal_globs_match_themselves(
                glob in "[A-Za-z0-9 ._+(){}\\[\\]^$|\\\\]{0,30}"
                    .prop_filter("no glob metacharacters", |s: &String| {
                        !s.contains('*') && !s.contains('?')
                    })
            ) {
                let pattern = glob_to_regex(&glob);
                let compiled = regex::Regex::new(&pattern).expect("must compile");
                prop_assert!(
                    compiled.is_match(&glob),
                    "regex {:?} must match its own literal glob {:?}",
                    pattern, glob
                );
                // It must not match a string with a different last character.
                // Skip strings ending in `]` or other edge codepoints because
                // appending arbitrary content might collide with grapheme
                // clusters in surprising ways, so prepend instead.
                let modified = format!("X{glob}Y");
                prop_assert!(
                    !compiled.is_match(&modified) || modified == glob,
                    "regex for literal glob {:?} must not match longer {:?}",
                    glob, modified
                );
            }

            /// For globs containing only `*` wildcards interleaved with
            /// literal segments, the compiled regex matches any string
            /// produced by replacing each `*` with the empty string OR an
            /// arbitrary literal segment.
            #[test]
            fn star_matches_arbitrary_content(
                prefix in "[A-Za-z0-9_]{0,5}",
                middle in "[A-Za-z0-9_]{0,10}",
                suffix in "[A-Za-z0-9_]{0,5}"
            ) {
                let glob = format!("{prefix}*{suffix}");
                let pattern = glob_to_regex(&glob);
                let compiled = regex::Regex::new(&pattern).expect("must compile");
                let candidate = format!("{prefix}{middle}{suffix}");
                prop_assert!(
                    compiled.is_match(&candidate),
                    "regex {:?} for glob {:?} must match {:?}",
                    pattern, glob, candidate
                );
            }
        }
    }

    // ── summarize_query ──────────────────────────────────────────────

    fn make_query(
        name_pattern: Option<&str>,
        pattern_type: PatternType,
        min_size: Option<u64>,
        max_size: Option<u64>,
        modified_after: Option<u64>,
        modified_before: Option<u64>,
        is_directory: Option<bool>,
    ) -> SearchQuery {
        SearchQuery {
            name_pattern: name_pattern.map(|s| s.to_string()),
            pattern_type,
            min_size,
            max_size,
            modified_after,
            modified_before,
            is_directory,
            include_paths: None,
            exclude_dir_names: None,
            include_path_ids: None,
            count_only: false,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        }
    }

    #[test]
    fn summarize_empty_query() {
        let q = make_query(None, PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "(all entries)");
    }

    #[test]
    fn summarize_name_only() {
        let q = make_query(Some("tes"), PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "\"tes\"");
    }

    #[test]
    fn summarize_glob_pattern() {
        let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "\"*.pdf\"");
    }

    #[test]
    fn summarize_regex_pattern() {
        let q = make_query(Some("Q[1-4].*"), PatternType::Regex, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "\"Q[1-4].*\" (regex)");
    }

    #[test]
    fn summarize_size_min() {
        let q = make_query(None, PatternType::Glob, Some(2 * 1024 * 1024), None, None, None, None);
        assert_eq!(summarize_query(&q), "size >= 2 MB");
    }

    #[test]
    fn summarize_size_max() {
        let q = make_query(None, PatternType::Glob, None, Some(500 * 1024), None, None, None);
        assert_eq!(summarize_query(&q), "size <= 500 KB");
    }

    #[test]
    fn summarize_size_range() {
        let q = make_query(
            None,
            PatternType::Glob,
            Some(1024 * 1024),
            Some(5 * 1024 * 1024 * 1024),
            None,
            None,
            None,
        );
        assert_eq!(summarize_query(&q), "size 1 MB\u{2013}5 GB");
    }

    #[test]
    fn summarize_date_after() {
        // 2025-01-01 00:00:00 UTC = 1735689600
        let q = make_query(None, PatternType::Glob, None, None, Some(1_735_689_600), None, None);
        assert_eq!(summarize_query(&q), "last mod after 2025-01-01");
    }

    #[test]
    fn summarize_date_before() {
        // 2026-03-01 00:00:00 UTC = 1772265600
        let q = make_query(None, PatternType::Glob, None, None, None, Some(1_772_323_200), None);
        assert_eq!(summarize_query(&q), "last mod before 2026-03-01");
    }

    #[test]
    fn summarize_date_range() {
        let q = make_query(
            None,
            PatternType::Glob,
            None,
            None,
            Some(1_735_689_600),
            Some(1_772_323_200),
            None,
        );
        assert_eq!(summarize_query(&q), "last mod 2025-01-01\u{2013}2026-03-01");
    }

    #[test]
    fn summarize_dirs_only() {
        let q = make_query(Some("*.pdf"), PatternType::Glob, None, None, None, None, Some(true));
        assert_eq!(summarize_query(&q), "\"*.pdf\", dirs only");
    }

    #[test]
    fn summarize_files_only() {
        let q = make_query(None, PatternType::Glob, None, None, None, None, Some(false));
        assert_eq!(summarize_query(&q), "files only");
    }

    #[test]
    fn summarize_combined() {
        let q = make_query(
            Some("tes"),
            PatternType::Glob,
            Some(2 * 1024 * 1024),
            None,
            None,
            Some(1_772_323_200),
            None,
        );
        assert_eq!(summarize_query(&q), "\"tes\", size >= 2 MB, last mod before 2026-03-01");
    }

    #[test]
    fn summarize_size_bytes() {
        let q = make_query(None, PatternType::Glob, Some(500), None, None, None, None);
        assert_eq!(summarize_query(&q), "size >= 500 B");
    }

    #[test]
    fn summarize_size_gb() {
        let q = make_query(
            None,
            PatternType::Glob,
            Some(1024 * 1024 * 1024),
            None,
            None,
            None,
            None,
        );
        assert_eq!(summarize_query(&q), "size >= 1 GB");
    }

    #[test]
    fn summarize_empty_name_pattern() {
        let q = make_query(Some(""), PatternType::Glob, None, None, None, None, None);
        assert_eq!(summarize_query(&q), "(all entries)");
    }

    // ── canonicalize_scope_path ─────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn canonicalize_scope_path_resolves_symlinks() {
        use std::os::unix::fs::symlink;

        // A real dir with a child, plus a symlink pointing at the real dir — the
        // /tmp → /private/tmp shape the index stores canonically but scopes report
        // symlinked.
        let base = std::env::temp_dir().join(format!("cmdr-scope-canon-{}", std::process::id()));
        let real = base.join("real");
        std::fs::create_dir_all(real.join("child")).expect("create real dir");
        let link = base.join("link");
        let _ = std::fs::remove_file(&link);
        symlink(&real, &link).expect("create symlink");

        // A path THROUGH the symlink canonicalizes to the real path (both fully
        // symlink-resolved), so a scope typed against the symlink now matches the
        // index's stored real path.
        let through_link = link.join("child").to_string_lossy().into_owned();
        let want = std::fs::canonicalize(real.join("child"))
            .expect("canonicalize real")
            .to_string_lossy()
            .into_owned();
        assert_eq!(canonicalize_scope_path(&through_link), want);

        // A non-existent path keeps the literal (best-effort: the offline-index case).
        let missing = link.join("nope").to_string_lossy().into_owned();
        assert_eq!(canonicalize_scope_path(&missing), missing);

        std::fs::remove_dir_all(&base).ok();
    }

    // ── parse_scope ─────────────────────────────────────────────────

    #[test]
    fn parse_scope_basic_include() {
        let scope = parse_scope("~/projects");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert!(scope.exclude_patterns.is_empty());
    }

    #[test]
    fn parse_scope_basic_exclude() {
        let scope = parse_scope("!node_modules");
        assert!(scope.include_paths.is_empty());
        assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
    }

    #[test]
    fn parse_scope_mixed() {
        let scope = parse_scope("~/projects, !node_modules, !.git");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert_eq!(scope.exclude_patterns, vec!["node_modules", ".git"]);
    }

    #[test]
    fn parse_scope_multiple_includes() {
        let scope = parse_scope("~/projects, ~/Documents");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(
            scope.include_paths,
            vec![format!("{home}/projects"), format!("{home}/Documents")]
        );
    }

    #[test]
    fn parse_scope_quoted_commas_double() {
        let scope = parse_scope("\"path,with,commas\"");
        assert_eq!(scope.include_paths, vec!["path,with,commas"]);
    }

    #[test]
    fn parse_scope_quoted_commas_single() {
        let scope = parse_scope("'path,with,commas'");
        assert_eq!(scope.include_paths, vec!["path,with,commas"]);
    }

    #[test]
    fn parse_scope_backslash_escaped_commas() {
        let scope = parse_scope("path\\,with\\,commas");
        assert_eq!(scope.include_paths, vec!["path,with,commas"]);
    }

    #[test]
    fn parse_scope_empty_segments() {
        let scope = parse_scope("~/projects, , !.git");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert_eq!(scope.exclude_patterns, vec![".git"]);
    }

    #[test]
    fn parse_scope_bare_exclude_wildcard() {
        let scope = parse_scope("!.*");
        assert_eq!(scope.exclude_patterns, vec![".*"]);
    }

    #[test]
    fn parse_scope_absolute_exclude_path() {
        let scope = parse_scope("!/Users/alice/Downloads");
        assert_eq!(scope.exclude_patterns, vec!["/Users/alice/Downloads"]);
    }

    #[test]
    fn parse_scope_empty_input() {
        let scope = parse_scope("");
        assert!(scope.include_paths.is_empty());
        assert!(scope.exclude_patterns.is_empty());
    }

    #[test]
    fn parse_scope_whitespace_trimming() {
        let scope = parse_scope("  ~/projects  ,  !node_modules  ");
        let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
        assert_eq!(scope.include_paths, vec![format!("{home}/projects")]);
        assert_eq!(scope.exclude_patterns, vec!["node_modules"]);
    }

    // ── split_scope_segments + parse_scope (property-based) ──────────
    //
    // The scope parser has nested escape/quote rules. Property tests probe
    // the round-trip and count invariants that don't require asserting a
    // specific canonical form.

    mod scope_proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// For inputs containing no special characters (no comma, no
            /// quotes, no backslash), the result is exactly `[input]` and
            /// the round-trip `segments.join(",") == input` holds.
            #[test]
            fn plain_input_round_trips(input in "[^,\"'\\\\]*") {
                let segments = split_scope_segments(&input);
                prop_assert_eq!(segments.len(), 1, "no commas → exactly 1 segment");
                prop_assert_eq!(&segments[0], &input, "the only segment must equal input");
                prop_assert_eq!(segments.join(","), input.clone(), "round-trip via join must match");
            }

            /// For inputs containing only safe characters and unquoted commas
            /// (no quotes, no backslashes), the segment count equals the
            /// comma count + 1.
            #[test]
            fn comma_count_matches_segment_count(input in "[^\"'\\\\]*") {
                let segments = split_scope_segments(&input);
                let comma_count = input.chars().filter(|&c| c == ',').count();
                prop_assert_eq!(
                    segments.len(),
                    comma_count + 1,
                    "expected {} segments for input {:?}, got {:?}",
                    comma_count + 1, input, segments
                );
                // And the join round-trips for this character class.
                prop_assert_eq!(segments.join(","), input);
            }

            /// `parse_scope` never panics, and the count of resolved
            /// include/exclude entries is bounded by the segment count.
            #[test]
            fn parse_scope_never_overcounts(input in ".*") {
                let scope = parse_scope(&input);
                let segments = split_scope_segments(&input);
                prop_assert!(
                    scope.include_paths.len() + scope.exclude_patterns.len() <= segments.len(),
                    "parse_scope produced more entries than segments: input={:?}, scope={:?}, segments={:?}",
                    input, scope, segments
                );
            }
        }
    }
}
