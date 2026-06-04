//! The `cmdr://logs` resource builder.
//!
//! Tails the live `cmdr.log` file, applying `since` / `filter` / `limit`
//! selection and mandatory per-line PII redaction. See the resource entry in
//! [`super::get_all_resources`] for the query syntax.

use super::parse_query;

/// Options parsed from a `cmdr://logs?...` URI.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    /// Drop lines whose ISO-8601 timestamp is `<=` this value. Lines without a
    /// recognizable timestamp prefix are kept (better to surface noise than to
    /// silently drop a panic line that didn't fit the usual prefix).
    pub since_iso: Option<String>,
    /// Case-sensitive substring filter.
    pub filter: Option<String>,
    /// Max lines to return. Defaults to 100, clamped to 1000.
    pub limit: usize,
}

pub const LOG_DEFAULT_LIMIT: usize = 100;
pub const LOG_MAX_LIMIT: usize = 1000;
/// How far back from end-of-file to read. 5 MB easily covers the most recent
/// few thousand lines on a busy session, without slurping the whole rotated
/// log (up to 50 MB per file).
pub const LOG_TAIL_WINDOW_BYTES: u64 = 5 * 1024 * 1024;

pub fn parse_log_options(query: Option<&str>) -> LogOptions {
    let q = parse_query(query);
    let since_iso = q.get("since").cloned().filter(|s| !s.is_empty());
    let filter = q.get("filter").cloned().filter(|s| !s.is_empty());
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(LOG_DEFAULT_LIMIT)
        .clamp(1, LOG_MAX_LIMIT);
    LogOptions {
        since_iso,
        filter,
        limit,
    }
}

/// Read the tail of the live `cmdr.log`, respecting `since` / `filter` / `limit`.
///
/// Reads up to [`LOG_TAIL_WINDOW_BYTES`] from the end of the file (5 MB), which
/// fits several thousand lines on a normal session — way more than the default
/// limit of 100. If the user passes a `since` older than the start of the
/// window, lines beyond the window are silently dropped; we keep the read
/// bounded so a 50 MB rotated log doesn't blow up MCP memory.
pub fn read_log_tail(opts: &LogOptions) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};

    let log_dir = crate::logging::log_dir().ok_or("Log directory is not configured yet")?;
    let log_path = log_dir.join("cmdr.log");

    let mut file = std::fs::File::open(&log_path).map_err(|e| format!("Can't open {}: {}", log_path.display(), e))?;
    let file_size = file
        .metadata()
        .map_err(|e| format!("Can't stat {}: {}", log_path.display(), e))?
        .len();

    let window = LOG_TAIL_WINDOW_BYTES.min(file_size);
    let start_pos = file_size.saturating_sub(window);
    file.seek(SeekFrom::Start(start_pos))
        .map_err(|e| format!("Can't seek log file: {}", e))?;
    let mut buf = Vec::with_capacity(window as usize);
    file.read_to_end(&mut buf)
        .map_err(|e| format!("Can't read log file: {}", e))?;
    // Drop a possibly-partial leading line so the first surviving line is
    // structurally intact. Only do this if we didn't read from byte 0.
    let text = String::from_utf8_lossy(&buf);
    Ok(select_log_lines(&text, start_pos == 0, opts))
}

/// Apply the `since` / `filter` / `limit` selection and per-line redaction to a
/// raw log-tail chunk, returning the joined, oldest-first result.
///
/// `skip_partial_first` is `false` when the chunk starts at byte 0 (the whole
/// file fit in the window, so the first line is intact) and `true` otherwise
/// (we read mid-file, so the leading line may be truncated).
///
/// **Redaction is mandatory.** The MCP logs resource is a third consumer of the
/// same log data the crash + error reporters scrub, so it must honor the same
/// contract: a loopback caller without filesystem read shouldn't be able to
/// exfiltrate home paths, SMB URIs, emails, or device names through
/// `cmdr://logs`. `redact_line` is a per-line `Cow` hot path (zero alloc on the
/// no-PII case), built for exactly this. Pure (no I/O), so it's unit-testable.
pub fn select_log_lines(text: &str, skip_partial_first: bool, opts: &LogOptions) -> String {
    let mut lines: Vec<&str> = if skip_partial_first {
        text.lines().skip(1).collect()
    } else {
        text.lines().collect()
    };

    if let Some(since) = opts.since_iso.as_deref() {
        lines.retain(|line| line_timestamp_passes_since(line, since));
    }
    if let Some(filter) = opts.filter.as_deref() {
        lines.retain(|line| line.contains(filter));
    }

    let take = opts.limit.min(lines.len());
    let start = lines.len() - take;
    let redacted: Vec<String> = lines[start..]
        .iter()
        .map(|line| crate::redact::redact_line(line).into_owned())
        .collect();
    redacted.join("\n")
}

/// Returns true when `line`'s leading ISO-8601 timestamp is strictly greater
/// than `since`. Lexicographic comparison works because both sides are
/// ISO-8601 with the same precision (millisecond) and a constant zone suffix
/// for the live log. Lines without a recognizable timestamp prefix are kept
/// (we'd rather over-include a panic line than silently drop one).
pub fn line_timestamp_passes_since(line: &str, since: &str) -> bool {
    // The fern logger writes lines like `2026-05-19T08:30:02.000+02:00 INFO ...`.
    // The timestamp is everything up to the first space.
    let Some(ts) = line.split_whitespace().next() else {
        return true;
    };
    if !ts.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    ts.as_bytes() > since.as_bytes()
}
