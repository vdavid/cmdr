//! Operations on search types: scope parsing and scope paths, formatting, query
//! summarization, and system directory exclusions.

use std::borrow::Cow;

use super::types::{PatternType, SearchQuery};

// ── System directory exclusions ──────────────────────────────────────

// Machine-output directory names (`node_modules`, `.git`, caches, build output): one
// list in `cmdr-fs`, so search and the importance scorer can't disagree about what
// counts as machine output.
pub use cmdr_fs::system_dirs::SYSTEM_DIR_EXCLUDES;
// ── Query summary ────────────────────────────────────────────────────

/// Build a dense, human-readable summary of a `SearchQuery` for functional display.
///
/// Examples: `"tes"`, `"*.pdf", dirs only`, `size >= 2 MB, last mod before 2026-03-01`
pub fn summarize_query(query: &SearchQuery) -> String {
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

/// Describe a query for the two engine log lines: its shape, plus where it was allowed to match.
///
/// The pattern shows only its mode and length: a literal query is the user's own words, and no
/// redactor can tell which of them are private. The scope roots and `!name` exclusions log
/// whole, as `path={:?}` / `dir={:?}` fields that a report tokenizes (`$HOME/Downloads` keeps its
/// role). `summarize_query` stays literal because MCP returns it as functional user data.
///
/// Why the scope is there: a 0-match line is uninterpretable without it. An empty scope box in
/// the dialog is NOT "everywhere" (it resolves to the focused pane's folder, see
/// `src/lib/search/search-runners.ts`), so a search that found nothing on a drive full of hits
/// looks identical in the log to a search of a genuinely empty drive. `ERR-FCAXU` was exactly
/// that, and cost an afternoon.
pub fn summarize_query_for_diagnostics(query: &SearchQuery) -> String {
    let pattern = match query.name_pattern.as_deref().filter(|pattern| !pattern.is_empty()) {
        Some(pattern) => {
            let mode = match query.pattern_type {
                PatternType::Glob => "glob",
                PatternType::Regex => "regex",
            };
            let chars = pattern.chars().count();
            if chars == pattern.len() {
                format!("{mode}({})", cmdr_fs::pluralize::pluralize(chars as u64, "char"))
            } else {
                format!(
                    "{mode}({}/{})",
                    cmdr_fs::pluralize::pluralize(chars as u64, "char"),
                    cmdr_fs::pluralize::pluralize(pattern.len() as u64, "byte")
                )
            }
        }
        None => "none".to_string(),
    };
    let size = match (query.min_size, query.max_size) {
        (Some(_), Some(_)) => "min+max",
        (Some(_), None) => "min",
        (None, Some(_)) => "max",
        (None, None) => "none",
    };
    let modified = match (query.modified_after, query.modified_before) {
        (Some(_), Some(_)) => "after+before",
        (Some(_), None) => "after",
        (None, Some(_)) => "before",
        (None, None) => "none",
    };
    let entry_type = match query.is_directory {
        Some(true) => "dirs",
        Some(false) => "files",
        None => "any",
    };
    let case = match query.case_sensitive {
        Some(true) => "sensitive",
        Some(false) => "insensitive",
        None => "platform-default",
    };
    let scope = match query.include_paths.as_deref() {
        None | Some([]) => "whole-volume".to_string(),
        Some(paths) => format!("roots({}) {}", paths.len(), keyed_list("path", paths)),
    };
    // The user's own `!name` exclusions only. The `SYSTEM_DIR_EXCLUDES` baseline is added later,
    // in `ExcludeRules::from_query`, so it never reaches the query and would be noise on every line.
    let exclusions = match query.exclude_dir_names.as_deref() {
        None | Some([]) => "0".to_string(),
        Some(names) => format!("{} {}", names.len(), keyed_list("dir", names)),
    };
    let system_exclusions = if query.exclude_system_dirs == Some(false) {
        "off"
    } else {
        "on"
    };

    format!(
        "pattern={pattern}, size={size}, modified={modified}, type={entry_type}, case={case}, count-only={}, scope={scope}, exclusions={exclusions}, system-exclusions={system_exclusions}",
        query.count_only
    )
}

/// `[key="a", key="b", key="c", +2 more]`: enough values to recognize the scope, without letting
/// a 40-path scope own the line.
fn keyed_list(key: &str, values: &[String]) -> String {
    const MAX_SHOWN: usize = 3;
    let mut items: Vec<String> = values.iter().take(MAX_SHOWN).map(|v| format!("{key}={v:?}")).collect();
    let rest = values.len().saturating_sub(MAX_SHOWN);
    if rest > 0 {
        items.push(format!("+{rest} more"));
    }
    format!("[{}]", items.join(", "))
}

/// A byte count for a model or a log to read, the inverse of [`parse_size`]. Re-exported for the model-facing
/// callers that reach it through `search::`.
pub use cmdr_fs::byte_size::format_size;

/// Parse a size a person or a model wrote ("50 MB", "1.5GiB", "500kb", "4096") into bytes, case-insensitively.
/// The symbol decides the base, matching what Cmdr's own sizes mean: IEC `KiB`/`MiB`/`GiB`/`TiB` are base 1024,
/// SI `kB`/`MB`/`GB`/`TB` base 1000. Uppercase `KB` is read leniently as the SI kilobyte. A bare number is bytes.
pub fn parse_size(text: &str) -> Option<u64> {
    const KIB: u64 = 1_024;
    // IEC suffixes first, so "mib" isn't read as a "b" suffix after "mi".
    const UNITS: &[(&str, u64)] = &[
        ("tib", KIB * KIB * KIB * KIB),
        ("gib", KIB * KIB * KIB),
        ("mib", KIB * KIB),
        ("kib", KIB),
        ("tb", 1_000_000_000_000),
        ("gb", 1_000_000_000),
        ("mb", 1_000_000),
        ("kb", 1_000),
        ("b", 1),
    ];
    let lower = text.trim().to_lowercase();
    let Some((number, multiplier)) = UNITS
        .iter()
        .find_map(|&(suffix, multiplier)| lower.strip_suffix(suffix).map(|number| (number, multiplier)))
    else {
        return lower.parse().ok();
    };
    let value: f64 = number.trim().parse().ok()?;
    (value.is_finite() && value >= 0.0).then_some((value * multiplier as f64) as u64)
}

pub fn format_timestamp(ts: u64) -> String {
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
///
/// The output is prefixed with `(?s)` so `.` matches a newline: `*` means "any
/// characters" and `?` means "one character", and a filename may legally contain a
/// newline, so both have to reach one. It lives HERE rather than as a
/// `RegexBuilder::dot_matches_new_line` at each call site because it's a property of
/// this translation, not of any one caller — two call sites disagreeing about it is
/// how filenames with newlines went unfindable in the first place. It composes with
/// `RegexBuilder::case_insensitive`: an inline `(?s)` sets that one flag and leaves
/// the builder's alone (pinned by
/// `matcher::tests::a_case_insensitive_accented_glob_still_crosses_a_newline`).
///
/// ❌ This does NOT apply to a user-typed REGEX pattern, which never comes through
/// here: a regex author expects `.` to stop at a newline and `(?s)` to be their own
/// call to make.
pub(crate) fn glob_to_regex(glob: &str) -> String {
    // `+ 6`: the flag prefix and the two anchors.
    let mut regex = String::with_capacity(glob.len() * 2 + 6);
    regex.push_str("(?s)^");
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
///
/// `home` is the absolute home folder `~` expands to; with `None` a `~` stays literal.
pub fn parse_scope(input: &str, home: Option<&str>) -> super::types::ParsedScope {
    let segments = split_scope_segments(input);

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
            if let Some(h) = home {
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

// ── Scope paths ──────────────────────────────────────────────────────

/// Expand a leading `~` (`~` alone or `~/…`) against `home`, the first step of a
/// scope path's pipeline (`~`, then the host's `realpath`, then the mount strip).
///
/// A pane sitting in the home folder reports its path as `~`, and since the default
/// scope became "the current folder" that string is the include path a plain search
/// runs with. `realpath` doesn't expand it (it's a shell convention), so unexpanded
/// it resolves to nothing in the index and to `/~` in a walk. ❌ Don't move the
/// expansion downstream. A `~` anywhere else is an ordinary character, and with no
/// home the path comes back as it was.
pub fn expand_tilde<'a>(path: &'a str, home: Option<&str>) -> Cow<'a, str> {
    match home {
        Some(home) if path == "~" || path.starts_with("~/") => Cow::Owned(path.replacen('~', home, 1)),
        _ => Cow::Borrowed(path),
    }
}

/// Map an absolute scope path into a volume's index path space (mount-relative),
/// so a source's path lookup, which walks component by component from the index
/// root, hits. A `/`-rooted volume (`mount_root` `None`) passes the path through.
/// A mount-rooted volume's index root is its mount root, so the mount prefix is
/// stripped: `/Volumes/naspi/sub` → `/sub`, the mount root itself → `/`. A path
/// outside the mount root yields `None` (don't mis-root it).
pub(crate) fn to_index_relative(path: &str, mount_root: Option<&str>) -> Option<String> {
    let Some(root) = mount_root else {
        return Some(path.to_string());
    };
    if path == root {
        return Some("/".to_string());
    }
    let rest = path.strip_prefix(root)?;
    rest.starts_with('/').then(|| rest.to_string())
}

#[cfg(test)]
mod tests;
