//! Maps parsed LLM classification output into a `SearchQuery`.
//!
//! Each function converts one enum/value from `ParsedLlmResponse` into the
//! corresponding field(s) on `SearchQuery`. The `build_search_query` function
//! assembles everything into a complete query.

use time::OffsetDateTime;

use crate::commands::search::{TranslateDisplay, TranslatedQuery};
use crate::indexing::search::{PatternType, SearchQuery, format_timestamp};

use super::ai_response_parser::{ParsedLlmResponse, is_range, is_year};
use super::file_system::expand_tilde;

// ── Constants ────────────────────────────────────────────────────────

const KB: u64 = 1_024;
const MB: u64 = 1_024 * KB;
const GB: u64 = 1_024 * MB;

/// Known file extensions for exact filename detection in `keywords_to_pattern`.
const KNOWN_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "rs", "py", "js", "ts", "go", "java", "json", "yml", "yaml", "toml", "html", "css",
    "md", "xml", "csv", "sql", "sh", "rb", "swift", "c", "cpp", "h", "hpp", "env", "log", "conf", "cfg", "ini", "lock",
    "png", "jpg", "jpeg", "gif", "svg", "mp3", "mp4", "mov", "zip", "tar", "gz",
];

// ── Type mapping ─────────────────────────────────────────────────────

/// A file type filter: regex pattern to match filenames, plus optional flags.
pub struct TypeFilter {
    pub pattern: &'static str,
    pub include_system_dirs: bool,
}

fn filter(pattern: &'static str) -> TypeFilter {
    TypeFilter {
        pattern,
        include_system_dirs: false,
    }
}

fn with_system_dirs(pattern: &'static str) -> TypeFilter {
    TypeFilter {
        pattern,
        include_system_dirs: true,
    }
}

/// Map a `type` enum value to its filename regex pattern and flags.
pub fn type_to_filter(t: &str) -> Option<TypeFilter> {
    Some(match t {
        "photos" => filter(r"\.(jpg|jpeg|png|heic|webp|gif)$"),
        "screenshots" => filter(r"^Screenshot.*\.(png|jpg|heic)$"),
        "videos" => filter(r"\.(mp4|mov|avi|mkv|webm)$"),
        "documents" => filter(r"\.(pdf|doc|docx|txt|odt|xls|xlsx)$"),
        "presentations" => filter(r"\.(ppt|pptx|odp)$"),
        "archives" => filter(r"\.(zip|tar|gz|tgz|bz2|xz|7z|rar)$"),
        "music" => filter(r"\.(mp3|m4a|flac|wav|ogg|aac)$"),
        "code" => filter(r"\.(rs|py|js|ts|go|java|c|cpp|h|rb|swift|svelte|vue)$"),
        "rust" => filter(r"\.rs$"),
        "python" => filter(r"\.py$"),
        "javascript" => filter(r"\.(js|jsx|mjs|cjs)$"),
        "typescript" => filter(r"\.(ts|tsx|mts|cts)$"),
        "go" => filter(r"\.go$"),
        "java" => filter(r"\.java$"),
        "config" => filter(r"\.(json|ya?ml|toml|ini|conf|cfg)$"),
        "logs" => with_system_dirs(r"\.(log|out|err)$"),
        "fonts" => filter(r"\.(ttf|otf|ttc|woff|woff2)$"),
        "databases" => filter(r"\.(sqlite|sqlite3|db)$"),
        "xcode" => filter(r"\.(xcodeproj|xcworkspace|pbxproj)$"),
        "ssh-keys" => filter(r"^(id_(rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)(\.pub)?$"),
        "shell-scripts" => filter(r"\.(sh|bash|zsh)$"),
        "docker-compose" => filter(r"^(docker-compose|compose)\.(yml|yaml)$"),
        "env-files" => filter(r"^\.env(\..+)?$"),
        "none" => return None,
        _ => return None,
    })
}

// ── Time mapping ─────────────────────────────────────────────────────

/// Map a `time` enum value to a (modified_after, modified_before) timestamp range.
///
/// Returns `(None, None)` for unrecognized values.
pub fn time_to_range(t: &str) -> (Option<u64>, Option<u64>) {
    let now = OffsetDateTime::now_utc();
    match t {
        "today" => (Some(start_of_today(now)), None),
        "yesterday" => (Some(start_of_yesterday(now)), Some(start_of_today(now))),
        "this_week" => (Some(monday_of_this_week(now)), None),
        "last_week" => (Some(monday_of_last_week(now)), Some(monday_of_this_week(now))),
        "this_month" => (Some(first_of_this_month(now)), None),
        "last_month" => (Some(first_of_last_month(now)), Some(first_of_this_month(now))),
        "this_quarter" => (Some(first_of_this_quarter(now)), None),
        "last_quarter" => (Some(first_of_last_quarter(now)), Some(first_of_this_quarter(now))),
        "this_year" => (Some(jan1(now, 0)), None),
        "last_year" => (Some(jan1(now, -1)), Some(jan1(now, 0))),
        "recent" => (Some(n_months_ago(now, 3)), None),
        "last_3_months" => (Some(n_months_ago(now, 3)), None),
        "last_6_months" => (Some(n_months_ago(now, 6)), None),
        "old" => (None, Some(one_year_ago(now))),
        _ => {
            if is_year(t) {
                year_range(t)
            } else if is_range(t) {
                parse_date_range(t)
            } else {
                (None, None)
            }
        }
    }
}

fn to_timestamp(dt: OffsetDateTime) -> u64 {
    dt.unix_timestamp().max(0) as u64
}

fn start_of_today(now: OffsetDateTime) -> u64 {
    let date = now.date();
    to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn start_of_yesterday(now: OffsetDateTime) -> u64 {
    let date = now.date().previous_day().unwrap_or(now.date());
    to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn monday_of_this_week(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let weekday = date.weekday().number_from_monday(); // Monday=1, Sunday=7
    let days_since_monday = weekday - 1;
    let monday = date
        .checked_sub(time::Duration::days(days_since_monday as i64))
        .unwrap_or(date);
    to_timestamp(monday.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn monday_of_last_week(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let weekday = date.weekday().number_from_monday();
    let days_since_monday = weekday - 1;
    let this_monday = date
        .checked_sub(time::Duration::days(days_since_monday as i64))
        .unwrap_or(date);
    let last_monday = this_monday.checked_sub(time::Duration::days(7)).unwrap_or(this_monday);
    to_timestamp(last_monday.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn first_of_this_month(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let first = date.replace_day(1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn first_of_last_month(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let (year, month) = if date.month() == time::Month::January {
        (date.year() - 1, time::Month::December)
    } else {
        (date.year(), date.month().previous())
    };
    let first = time::Date::from_calendar_date(year, month, 1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn quarter_start_month(month: time::Month) -> time::Month {
    match month {
        time::Month::January | time::Month::February | time::Month::March => time::Month::January,
        time::Month::April | time::Month::May | time::Month::June => time::Month::April,
        time::Month::July | time::Month::August | time::Month::September => time::Month::July,
        time::Month::October | time::Month::November | time::Month::December => time::Month::October,
    }
}

fn first_of_this_quarter(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let qm = quarter_start_month(date.month());
    let first = time::Date::from_calendar_date(date.year(), qm, 1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn first_of_last_quarter(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let qm = quarter_start_month(date.month());
    let (year, month) = match qm {
        time::Month::January => (date.year() - 1, time::Month::October),
        time::Month::April => (date.year(), time::Month::January),
        time::Month::July => (date.year(), time::Month::April),
        time::Month::October => (date.year(), time::Month::July),
        _ => unreachable!(),
    };
    let first = time::Date::from_calendar_date(year, month, 1).unwrap_or(date);
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn jan1(now: OffsetDateTime, year_offset: i32) -> u64 {
    let first = time::Date::from_calendar_date(now.year() + year_offset, time::Month::January, 1).unwrap_or(now.date());
    to_timestamp(first.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn n_months_ago(now: OffsetDateTime, n: u8) -> u64 {
    let date = now.date();
    let mut year = date.year();
    let mut month_num = date.month() as u8;
    if month_num <= n {
        year -= 1;
        month_num += 12 - n;
    } else {
        month_num -= n;
    }
    let month = time::Month::try_from(month_num).unwrap_or(time::Month::January);
    let day = date.day().min(days_in_month(year, month));
    let target = time::Date::from_calendar_date(year, month, day).unwrap_or(date);
    to_timestamp(target.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn one_year_ago(now: OffsetDateTime) -> u64 {
    let date = now.date();
    let target = date.replace_year(date.year() - 1).unwrap_or(date);
    to_timestamp(target.with_hms(0, 0, 0).expect("valid").assume_utc())
}

fn days_in_month(year: i32, month: time::Month) -> u8 {
    // Use the first of the next month minus one day trick
    let next_month = month.next();
    let next_year = if next_month == time::Month::January {
        year + 1
    } else {
        year
    };
    let next_first = time::Date::from_calendar_date(next_year, next_month, 1)
        .unwrap_or(time::Date::from_calendar_date(year, month, 28).expect("valid"));
    let last_day = next_first
        .previous_day()
        .unwrap_or(time::Date::from_calendar_date(year, month, 28).expect("valid"));
    last_day.day()
}

fn year_range(y: &str) -> (Option<u64>, Option<u64>) {
    let year: i32 = match y.parse() {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let start = match time::Date::from_calendar_date(year, time::Month::January, 1) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    let end = match time::Date::from_calendar_date(year + 1, time::Month::January, 1) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };
    (
        Some(to_timestamp(start.with_hms(0, 0, 0).expect("valid").assume_utc())),
        Some(to_timestamp(end.with_hms(0, 0, 0).expect("valid").assume_utc())),
    )
}

fn parse_date_range(r: &str) -> (Option<u64>, Option<u64>) {
    // Try separators: "..", " to ", "–" (en-dash), "-"
    let parts: Option<(&str, &str)> = ["..", " to ", "\u{2013}"]
        .iter()
        .find_map(|sep| r.split_once(sep))
        .or_else(|| {
            // Single hyphen: only for YYYY-YYYY
            r.split_once('-')
                .filter(|(l, r)| is_year(l.trim()) && is_year(r.trim()))
        });

    let (left, right) = match parts {
        Some((l, r)) => (l.trim(), r.trim()),
        None => return (None, None),
    };

    let start = parse_date_start(left);
    let end = parse_date_end(right);
    (start, end)
}

/// Parse a date string as the start of a range. "2024" → Jan 1 2024, "2024-06" → Jun 1 2024.
fn parse_date_start(s: &str) -> Option<u64> {
    if is_year(s) {
        let year: i32 = s.parse().ok()?;
        let date = time::Date::from_calendar_date(year, time::Month::January, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    // YYYY-MM
    if let Some((y, m)) = s.split_once('-') {
        let year: i32 = y.parse().ok()?;
        let month: u8 = m.parse().ok()?;
        let month = time::Month::try_from(month).ok()?;
        let date = time::Date::from_calendar_date(year, month, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    None
}

/// Parse a date string as the end of a range. "2025" → Jan 1 2026, "2024-06" → Jul 1 2024.
fn parse_date_end(s: &str) -> Option<u64> {
    if is_year(s) {
        let year: i32 = s.parse().ok()?;
        let date = time::Date::from_calendar_date(year + 1, time::Month::January, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    // YYYY-MM → first of next month
    if let Some((y, m)) = s.split_once('-') {
        let year: i32 = y.parse().ok()?;
        let month_num: u8 = m.parse().ok()?;
        let (next_year, next_month) = if month_num >= 12 {
            (year + 1, time::Month::January)
        } else {
            (year, time::Month::try_from(month_num + 1).ok()?)
        };
        let date = time::Date::from_calendar_date(next_year, next_month, 1).ok()?;
        return Some(to_timestamp(date.with_hms(0, 0, 0).expect("valid").assume_utc()));
    }
    None
}

// ── Size mapping ─────────────────────────────────────────────────────

/// Map a `size` enum value to a (min_size, max_size) byte range.
pub fn size_to_filter(s: &str) -> (Option<u64>, Option<u64>) {
    match s {
        "empty" => (None, Some(0)),
        "tiny" => (None, Some(100 * KB)),
        "small" => (None, Some(MB)),
        "large" => (Some(100 * MB), None),
        "huge" => (Some(GB), None),
        _ if s.starts_with('>') => parse_size_value(&s[1..]).map_or((None, None), |v| (Some(v), None)),
        _ if s.starts_with('<') => parse_size_value(&s[1..]).map_or((None, None), |v| (None, Some(v))),
        _ => (None, None),
    }
}

/// Parse a size string like "50mb", "1gb", "500kb" into bytes.
fn parse_size_value(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if let Some(num_str) = s.strip_suffix("gb") {
        let num: f64 = num_str.parse().ok()?;
        Some((num * GB as f64) as u64)
    } else if let Some(num_str) = s.strip_suffix("mb") {
        let num: f64 = num_str.parse().ok()?;
        Some((num * MB as f64) as u64)
    } else if let Some(num_str) = s.strip_suffix("kb") {
        let num: f64 = num_str.parse().ok()?;
        Some((num * KB as f64) as u64)
    } else {
        // Try parsing as plain bytes
        s.parse().ok()
    }
}

// ── Scope mapping ────────────────────────────────────────────────────

/// Result of scope resolution: paths to search in, plus an optional name prefix filter.
pub struct ScopeResult {
    pub paths: Vec<String>,
    pub name_prefix: Option<&'static str>,
}

/// Map a `scope` enum value to search paths and optional name prefix.
pub fn scope_to_paths(s: &str) -> ScopeResult {
    let home = dirs::home_dir().unwrap_or_default();
    match s {
        "downloads" => ScopeResult {
            paths: vec![home.join("Downloads").to_string_lossy().into_owned()],
            name_prefix: None,
        },
        "documents" => ScopeResult {
            paths: vec![home.join("Documents").to_string_lossy().into_owned()],
            name_prefix: None,
        },
        "desktop" => ScopeResult {
            paths: vec![home.join("Desktop").to_string_lossy().into_owned()],
            name_prefix: None,
        },
        "dotfiles" => ScopeResult {
            paths: vec![home.to_string_lossy().into_owned()],
            name_prefix: Some("."),
        },
        path => ScopeResult {
            paths: vec![expand_tilde(path)],
            name_prefix: None,
        },
    }
}

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
fn parse_exclude_list(exclude: &str) -> Vec<String> {
    exclude
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

// ── Assembly ─────────────────────────────────────────────────────────

/// Build a `SearchQuery` from a parsed LLM response.
pub fn build_search_query(parsed: &ParsedLlmResponse) -> SearchQuery {
    let type_filter = parsed.type_field.as_deref().and_then(type_to_filter);
    let (time_after, time_before) = parsed.time.as_deref().map(time_to_range).unwrap_or_default();
    let (size_min, size_max) = parsed.size.as_deref().map(size_to_filter).unwrap_or_default();
    let scope = parsed.scope.as_deref().map(scope_to_paths);
    let exclude = parsed.exclude.as_deref().map(parse_exclude_list);
    let is_dir = parsed.folders.as_deref().map(|f| f == "yes");

    // Build keyword pattern
    let kw = parsed.keywords.as_deref().and_then(keywords_to_pattern);

    // Handle dotfiles scope name_prefix: if no keywords, add `.*` glob for dotfile prefix
    let kw = match (&kw, &scope) {
        (None, Some(s)) if s.name_prefix == Some(".") => Some((".*".to_string(), PatternType::Glob)),
        _ => kw,
    };

    // Merge keywords + type into a single name_pattern + pattern_type
    let (name_pattern, pattern_type) = merge_keyword_and_type(kw, type_filter.as_ref());

    let include_system_dirs = type_filter.as_ref().is_some_and(|f| f.include_system_dirs);

    SearchQuery {
        name_pattern,
        pattern_type,
        min_size: size_min,
        max_size: size_max,
        modified_after: time_after,
        modified_before: time_before,
        is_directory: is_dir,
        include_paths: scope.as_ref().map(|s| s.paths.clone()),
        exclude_dir_names: exclude,
        include_path_ids: None,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: if include_system_dirs { Some(false) } else { None },
    }
}

// ── Caveat generation ────────────────────────────────────────────────

/// Generate a caveat string based on the parsed LLM response and built query.
///
/// Priority: LLM-provided note (truncated 200 chars) > Rust-inferred caveats.
pub fn generate_caveat(parsed: &ParsedLlmResponse, query: &SearchQuery) -> Option<String> {
    // LLM provided a note → use it (sanitized: max 200 chars, no HTML)
    if let Some(note) = &parsed.note {
        let sanitized: String = note.chars().filter(|c| *c != '<' && *c != '>').take(200).collect();
        return Some(sanitized);
    }
    // No name pattern → very broad search
    if query.name_pattern.is_none() {
        return Some(
            "No filename filter \u{2014} results may be very broad. Add a name or file type to narrow.".into(),
        );
    }
    None
}

// ── Display value generation ─────────────────────────────────────────

/// Build human-readable display values for the frontend filter UI.
pub fn build_translate_display(parsed: &ParsedLlmResponse, query: &SearchQuery) -> TranslateDisplay {
    TranslateDisplay {
        name_pattern: query.name_pattern.clone(),
        pattern_type: Some(match query.pattern_type {
            PatternType::Glob => "glob".to_string(),
            PatternType::Regex => "regex".to_string(),
        }),
        min_size: query.min_size,
        max_size: query.max_size,
        modified_after: query.modified_after.map(format_timestamp),
        modified_before: query.modified_before.map(format_timestamp),
        is_directory: query.is_directory,
        include_paths: query.include_paths.clone(),
        exclude_dir_names: query.exclude_dir_names.clone(),
        case_sensitive: parsed.type_field.as_deref().and(query.case_sensitive),
    }
}

/// Build a `TranslatedQuery` from a `SearchQuery` (for IPC serialization).
pub fn build_translated_query(query: &SearchQuery) -> TranslatedQuery {
    TranslatedQuery {
        name_pattern: query.name_pattern.clone(),
        pattern_type: match query.pattern_type {
            PatternType::Glob => "glob".to_string(),
            PatternType::Regex => "regex".to_string(),
        },
        min_size: query.min_size,
        max_size: query.max_size,
        modified_after: query.modified_after,
        modified_before: query.modified_before,
        is_directory: query.is_directory,
        include_paths: query.include_paths.clone(),
        exclude_dir_names: query.exclude_dir_names.clone(),
        case_sensitive: query.case_sensitive,
        exclude_system_dirs: query.exclude_system_dirs,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Type mapping ─────────────────────────────────────────────────

    #[test]
    fn type_to_filter_all_enum_values() {
        let types = [
            "photos",
            "screenshots",
            "videos",
            "documents",
            "presentations",
            "archives",
            "music",
            "code",
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "java",
            "config",
            "logs",
            "fonts",
            "databases",
            "xcode",
            "shell-scripts",
            "ssh-keys",
            "docker-compose",
            "env-files",
        ];
        for t in types {
            let f = type_to_filter(t);
            assert!(f.is_some(), "type '{t}' should produce a filter");
            // Verify the pattern compiles as regex
            let f = f.unwrap();
            let re = regex::RegexBuilder::new(f.pattern).case_insensitive(true).build();
            assert!(re.is_ok(), "type '{t}' pattern should compile: {}", f.pattern);
        }
    }

    #[test]
    fn type_logs_includes_system_dirs() {
        let f = type_to_filter("logs").unwrap();
        assert!(f.include_system_dirs);
    }

    #[test]
    fn type_photos_no_system_dirs() {
        let f = type_to_filter("photos").unwrap();
        assert!(!f.include_system_dirs);
    }

    #[test]
    fn type_unknown_returns_none() {
        assert!(type_to_filter("bananas").is_none());
    }

    #[test]
    fn type_none_returns_none() {
        assert!(type_to_filter("none").is_none());
    }

    #[test]
    fn type_screenshots_anchored() {
        let f = type_to_filter("screenshots").unwrap();
        assert!(f.pattern.starts_with('^'));
    }

    #[test]
    fn type_documents_matches_expected_files() {
        let f = type_to_filter("documents").unwrap();
        let re = regex::RegexBuilder::new(f.pattern)
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(re.is_match("report.pdf"));
        assert!(re.is_match("notes.txt"));
        assert!(re.is_match("budget.xlsx"));
        assert!(!re.is_match("photo.jpg"));
        assert!(!re.is_match("code.rs"));
    }

    // ── Time mapping ─────────────────────────────────────────────────

    #[test]
    fn time_today_has_start_no_end() {
        let (after, before) = time_to_range("today");
        assert!(after.is_some());
        assert!(before.is_none());
    }

    #[test]
    fn time_yesterday_has_both_bounds() {
        let (after, before) = time_to_range("yesterday");
        assert!(after.is_some());
        assert!(before.is_some());
        assert!(after.unwrap() < before.unwrap());
    }

    #[test]
    fn time_all_enum_values_produce_timestamps() {
        let enums = [
            "today",
            "yesterday",
            "this_week",
            "last_week",
            "this_month",
            "last_month",
            "this_quarter",
            "last_quarter",
            "this_year",
            "last_year",
            "recent",
            "last_3_months",
            "last_6_months",
            "old",
        ];
        for t in enums {
            let (after, before) = time_to_range(t);
            assert!(
                after.is_some() || before.is_some(),
                "time '{t}' should produce at least one bound"
            );
        }
    }

    #[test]
    fn time_year_range() {
        let (after, before) = time_to_range("2024");
        assert!(after.is_some());
        assert!(before.is_some());
        // 2024 → Jan 1 2024 to Jan 1 2025
        assert!(after.unwrap() < before.unwrap());
    }

    #[test]
    fn time_year_range_dotdot() {
        let (after, before) = time_to_range("2024..2025");
        assert!(after.is_some());
        assert!(before.is_some());
    }

    #[test]
    fn time_invalid_returns_none() {
        let (after, before) = time_to_range("next millennium");
        assert!(after.is_none());
        assert!(before.is_none());
    }

    #[test]
    fn time_recent_is_about_three_months_ago() {
        let (after, _) = time_to_range("recent");
        let now = OffsetDateTime::now_utc();
        let ts = after.unwrap();
        // Should be roughly 90 days ago (±5 days for month length variation)
        let days_ago = (now.unix_timestamp() as u64 - ts) / 86400;
        assert!(
            (85..=95).contains(&days_ago),
            "recent should be ~90 days ago, got {days_ago}"
        );
    }

    #[test]
    fn time_old_has_only_upper_bound() {
        let (after, before) = time_to_range("old");
        assert!(after.is_none());
        assert!(before.is_some());
    }

    // ── Size mapping ─────────────────────────────────────────────────

    #[test]
    fn size_all_enum_values() {
        assert_eq!(size_to_filter("empty"), (None, Some(0)));
        assert_eq!(size_to_filter("tiny"), (None, Some(100 * KB)));
        assert_eq!(size_to_filter("small"), (None, Some(MB)));
        assert_eq!(size_to_filter("large"), (Some(100 * MB), None));
        assert_eq!(size_to_filter("huge"), (Some(GB), None));
    }

    #[test]
    fn size_greater_than() {
        let (min, max) = size_to_filter(">50mb");
        assert_eq!(min, Some(50 * MB));
        assert!(max.is_none());
    }

    #[test]
    fn size_less_than() {
        let (min, max) = size_to_filter("<1gb");
        assert!(min.is_none());
        assert_eq!(max, Some(GB));
    }

    #[test]
    fn size_invalid_returns_none() {
        let (min, max) = size_to_filter("medium");
        assert!(min.is_none());
        assert!(max.is_none());
    }

    // ── Scope mapping ────────────────────────────────────────────────

    #[test]
    fn scope_downloads() {
        let result = scope_to_paths("downloads");
        assert_eq!(result.paths.len(), 1);
        assert!(result.paths[0].ends_with("Downloads"));
        assert!(result.name_prefix.is_none());
    }

    #[test]
    fn scope_dotfiles() {
        let result = scope_to_paths("dotfiles");
        assert_eq!(result.name_prefix, Some("."));
    }

    #[test]
    fn scope_literal_path() {
        let result = scope_to_paths("~/projects");
        assert_eq!(result.paths.len(), 1);
        // Should have expanded tilde
        assert!(!result.paths[0].starts_with('~'));
    }

    // ── Keyword pattern ──────────────────────────────────────────────

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

    #[test]
    fn time_last_6_months() {
        let (after, before) = time_to_range("last_6_months");
        assert!(after.is_some());
        assert!(before.is_none());
        let now = OffsetDateTime::now_utc();
        let ts = after.unwrap();
        let days_ago = (now.unix_timestamp() as u64 - ts) / 86400;
        assert!(
            (175..=190).contains(&days_ago),
            "last_6_months should be ~180 days ago, got {days_ago}"
        );
    }

    #[test]
    fn type_shell_scripts_matches() {
        let f = type_to_filter("shell-scripts").unwrap();
        let re = regex::RegexBuilder::new(f.pattern)
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(re.is_match("deploy.sh"));
        assert!(re.is_match("init.bash"));
        assert!(re.is_match("setup.zsh"));
        assert!(!re.is_match("readme.md"));
    }

    #[test]
    fn type_presentations_no_key_extension() {
        let f = type_to_filter("presentations").unwrap();
        assert!(!f.pattern.contains("key"), "presentations should not match .key files");
    }

    // ── Assembly ─────────────────────────────────────────────────────

    #[test]
    fn build_full_query() {
        let parsed = ParsedLlmResponse {
            keywords: Some("rymd".to_string()),
            type_field: Some("documents".to_string()),
            time: Some("recent".to_string()),
            size: Some("large".to_string()),
            scope: Some("downloads".to_string()),
            exclude: Some("node_modules .git".to_string()),
            folders: Some("no".to_string()),
            note: None,
        };
        let query = build_search_query(&parsed);
        assert!(query.name_pattern.is_some());
        assert_eq!(query.pattern_type, PatternType::Regex);
        assert!(query.min_size.is_some());
        assert!(query.modified_after.is_some());
        assert_eq!(query.is_directory, Some(false));
        assert!(query.include_paths.is_some());
        assert!(query.exclude_dir_names.is_some());
        assert_eq!(query.exclude_dir_names.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn build_logs_type_includes_system_dirs() {
        let parsed = ParsedLlmResponse {
            type_field: Some("logs".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        assert_eq!(query.exclude_system_dirs, Some(false));
    }

    #[test]
    fn build_dotfiles_scope() {
        let parsed = ParsedLlmResponse {
            scope: Some("dotfiles".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        assert!(query.include_paths.is_some());
        // Should have a name pattern for dotfiles (starts with ".")
        assert!(query.name_pattern.is_some());
    }

    #[test]
    fn build_folders_yes() {
        let parsed = ParsedLlmResponse {
            keywords: Some("node_modules".to_string()),
            folders: Some("yes".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        assert_eq!(query.is_directory, Some(true));
    }

    // ── Caveat generation ────────────────────────────────────────────

    #[test]
    fn caveat_from_llm_note() {
        let parsed = ParsedLlmResponse {
            note: Some("can't filter by photo content".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query);
        assert_eq!(caveat.as_deref(), Some("can't filter by photo content"));
    }

    #[test]
    fn caveat_truncated_at_200_chars() {
        let long_note = "a".repeat(300);
        let parsed = ParsedLlmResponse {
            note: Some(long_note),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query).unwrap();
        assert_eq!(caveat.len(), 200);
    }

    #[test]
    fn caveat_no_name_pattern() {
        let parsed = ParsedLlmResponse {
            size: Some("large".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query);
        assert!(caveat.is_some());
        assert!(caveat.unwrap().contains("No filename filter"));
    }

    #[test]
    fn caveat_none_when_has_pattern() {
        let parsed = ParsedLlmResponse {
            keywords: Some("test".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let caveat = generate_caveat(&parsed, &query);
        assert!(caveat.is_none());
    }

    // ── Display values ───────────────────────────────────────────────

    #[test]
    fn display_values_populated() {
        let parsed = ParsedLlmResponse {
            keywords: Some("test".to_string()),
            type_field: Some("rust".to_string()),
            time: Some("today".to_string()),
            size: Some("large".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        let display = build_translate_display(&parsed, &query);
        assert!(display.name_pattern.is_some());
        assert_eq!(display.pattern_type.as_deref(), Some("regex"));
        assert!(display.min_size.is_some());
        assert!(display.modified_after.is_some());
    }

    // ── Fallback integration ─────────────────────────────────────────

    #[test]
    fn empty_llm_response_produces_empty_parsed() {
        let parsed = ParsedLlmResponse::default();
        assert!(parsed.is_empty());
    }

    #[test]
    fn partial_llm_response_builds_partial_query() {
        let parsed = ParsedLlmResponse {
            time: Some("recent".to_string()),
            ..Default::default()
        };
        let query = build_search_query(&parsed);
        // Only time filter set
        assert!(query.modified_after.is_some());
        assert!(query.name_pattern.is_none());
        assert!(query.min_size.is_none());
    }

    // ── Size parsing edge cases ──────────────────────────────────────

    #[test]
    fn size_greater_than_gb() {
        let (min, max) = size_to_filter(">2gb");
        assert_eq!(min, Some(2 * GB));
        assert!(max.is_none());
    }

    // ── Date range parsing ───────────────────────────────────────────

    #[test]
    fn date_range_with_to() {
        let (after, before) = time_to_range("2024 to 2025");
        assert!(after.is_some());
        assert!(before.is_some());
    }

    #[test]
    fn date_range_with_en_dash() {
        let (after, before) = time_to_range("2024\u{2013}2025");
        assert!(after.is_some());
        assert!(before.is_some());
    }

    #[test]
    fn date_range_with_hyphen() {
        // YYYY-YYYY
        let (after, before) = time_to_range("2023-2024");
        assert!(after.is_some());
        assert!(before.is_some());
    }
}
