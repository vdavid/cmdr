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

/// The outcome of resolving a volume's scope include paths against its index.
pub(crate) struct ScopeResolution {
    /// Entry IDs for the engine's ancestor-walk include filter. EMPTY means "no
    /// restriction — search the whole volume" (the caller sets `include_path_ids` to
    /// `None`); `[i64::MIN]` (an impossible id) means "restrict to nothing" so a
    /// scope that resolved to zero real folders returns no results rather than
    /// silently searching everything.
    pub include_ids: Vec<i64>,
    /// Original scope paths that routed to this volume but weren't found in its index
    /// (a typo, a since-deleted folder, or a path outside the mount root). Surfaced
    /// to the caller as an honest signal instead of a silent empty result.
    pub unresolved: Vec<String>,
}

/// Resolve a volume's scope include paths to entry IDs against that volume's index.
///
/// Canonicalizes each path (symlink resolution, off the hot loop), maps it into the
/// volume's index path space (mount-relative for a NAS/MTP volume via
/// [`to_index_relative`]), and looks it up via the volume's pool. Three outcomes per
/// path, and the whole result collapses accordingly:
///
/// - **The mount root itself** (`/Volumes/naspi`, stripped to `/`) means the WHOLE
///   VOLUME — routing already scoped to this volume, so there's no sub-restriction.
///   Any such path makes `include_ids` empty (no filter), regardless of the others.
/// - **A resolvable subpath** contributes its entry id.
/// - **An unresolvable path** (outside the mount root, or a folder not in the index)
///   goes to `unresolved` for honest reporting. If NONE of the non-whole-volume
///   paths resolve, `include_ids` is `[i64::MIN]` so the engine matches nothing.
pub(crate) fn resolve_include_scope(paths: &[String], pool: &ReadPool, mount_root: Option<&str>) -> ScopeResolution {
    let mut whole_volume = false;
    let mut unresolved: Vec<String> = Vec::new();
    // (original path, index-relative path) for the subpaths that need a DB lookup.
    let mut to_resolve: Vec<(String, String)> = Vec::new();

    for original in paths {
        // Canonicalize ONCE (resolve symlinks like /tmp -> /private/tmp) so the prefix
        // walk matches the index's stored real paths. Off the hot scan loop.
        match to_index_relative(&canonicalize_scope_path(original), mount_root) {
            Some(index_path) if index_path == "/" => whole_volume = true,
            Some(index_path) => to_resolve.push((original.clone(), index_path)),
            None => unresolved.push(original.clone()), // outside this volume's mount root
        }
    }

    if whole_volume {
        // The whole volume is in scope, so every subpath is already covered - no
        // include restriction and nothing to report as unresolved.
        return ScopeResolution {
            include_ids: Vec::new(),
            unresolved: Vec::new(),
        };
    }

    let mut include_ids: Vec<i64> = Vec::new();
    let _ = pool.with_conn(|conn| {
        for (original, index_path) in &to_resolve {
            match store::resolve_path(conn, index_path) {
                Ok(Some(id)) => include_ids.push(id),
                Ok(None) => {
                    log::debug!("search: include path not found in index: {index_path}");
                    unresolved.push(original.clone());
                }
                Err(e) => {
                    log::warn!("search: failed to resolve include path {index_path}: {e}");
                    unresolved.push(original.clone());
                }
            }
        }
    });

    if include_ids.is_empty() {
        // Nothing resolved (all typos / not in this index): match nothing.
        include_ids.push(i64::MIN);
    }
    ScopeResolution {
        include_ids,
        unresolved,
    }
}

#[cfg(test)]
mod tests;
