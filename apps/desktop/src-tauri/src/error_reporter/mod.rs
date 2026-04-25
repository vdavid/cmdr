//! Error reporter: builds a privacy-redacted zip bundle of recent logs + manifest and
//! (optionally) uploads it to the ingestion server. Used by the user-initiated flow in
//! Phase 4 and, later, by the auto-send flow in Phase 5.
//!
//! The bundle layout is:
//!
//! ```text
//! manifest.json
//! logs/<filename>          # one entry per recent log file, redacted line-by-line
//! logs/<next-filename>
//! ...
//! ```
//!
//! Every log line passes through [`crate::redact::redact_line`] before it hits the zip.
//! The manifest mirrors `ActiveSettings` from the crash reporter so triage is consistent
//! across the two report types.

#[cfg(debug_assertions)]
use crate::config;
use crate::crash_reporter::ActiveSettings;
use crate::logging;
use crate::redact;
use chrono::{DateTime, Datelike, Timelike, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
#[cfg(debug_assertions)]
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::SystemTime;
use zip::DateTime as ZipDateTime;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

#[cfg(test)]
mod tests;

pub mod auto_dispatcher;

#[cfg(test)]
mod auto_dispatcher_tests;

/// Log an error and (if Flow B is opted in) feed it to the auto-dispatcher.
///
/// Drop-in replacement for [`log::error!`] at user-visible failure sites — anything that
/// already produces a user-facing toast or that an end user would consider "this didn't
/// work." Don't migrate noisy library-level errors (`smb2`, `nusb`, etc.); the goal is
/// signal, not coverage.
///
/// The macro evaluates its arguments exactly once. The `format!()` happens whether or not
/// the auto-dispatcher is enabled — same cost as the underlying `log::error!`. The
/// dispatcher's hot path bails out on a single atomic load when the opt-in flag is off.
///
/// ```ignore
/// use cmdr_lib::log_error;
/// log_error!("couldn't mount SMB share at {}: {}", host, err);
/// log_error!(target: "cmdr_lib::network", "DNS lookup failed: {err}");
/// ```
#[macro_export]
macro_rules! log_error {
    (target: $target:expr, $($arg:tt)+) => {{
        let __msg = format!($($arg)+);
        log::error!(target: $target, "{}", __msg);
        $crate::error_reporter::auto_dispatcher::on_error_logged($target, &__msg);
    }};
    ($($arg:tt)+) => {{
        let __msg = format!($($arg)+);
        log::error!("{}", __msg);
        $crate::error_reporter::auto_dispatcher::on_error_logged(module_path!(), &__msg);
    }};
}

/// Same unambiguous alphabet the server uses for license short codes and error report IDs.
/// Kept in sync with `apps/api-server/src/license.ts` — avoids `0`/`O`, `1`/`I`/`L`.
const SHORT_ID_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const SHORT_ID_LEN: usize = 5;
const SHORT_ID_PREFIX: &str = "ERR";

/// Max lines in the first-lines preview sample shown in the dialog.
const SAMPLE_FIRST_LINES: usize = 5;
/// Max lines in the last-lines preview sample.
const SAMPLE_LAST_LINES: usize = 20;

/// Flavor of the bundle — kept separate so Phase 5's auto-sender can share the same builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleKind {
    User,
    Auto,
}

/// Time filter applied when picking which log content to include.
///
/// - `Last24Hours`: include log files whose mtime is within the last 24 hours. Used by
///   Flow A — the user just clicked "send error report," they care about what they
///   saw recently.
/// - `Window { first_error_at }`: include only content whose timestamp falls inside
///   `[first_error_at - 30 min, now]`. Files entirely outside that window are dropped;
///   the file containing the lower bound is head-trimmed line-by-line by parsing the
///   leading ISO-8601 stamp the file chain writes (see
///   [`logging::dispatch::file_timestamp`]). Used by Flow B — the window is anchored on
///   the actual error, so we ship surrounding context without the noise.
#[derive(Debug, Clone, Copy)]
pub enum BundleScope {
    Last24Hours,
    Window { first_error_at: DateTime<Utc> },
}

/// 30 minutes of pre-error context for Flow B.
const FLOW_B_PRE_ERROR_WINDOW: chrono::Duration = chrono::Duration::minutes(30);

/// Last-24-hour cutoff for Flow A.
const FLOW_A_MAX_AGE: chrono::Duration = chrono::Duration::hours(24);

/// Hard cap for Flow A bundles.
pub const FLOW_A_BUNDLE_CAP_MB: usize = 10;
/// Hard cap for Flow B bundles. Smaller because Flow B fires without per-event consent.
pub const FLOW_B_BUNDLE_CAP_MB: usize = 1;

/// Always preserve at least this many lines of the most recent file, even if the cap
/// would otherwise force it down to nothing. `cap_bundle_to_mb` may exceed the cap by
/// up to ~10% to honor this — better to ship 1.1 MB of useful context than 0.
const MIN_TAIL_LINES_OF_NEWEST_FILE: usize = 50;

/// Metadata written into `manifest.json` at the root of the bundle.
/// Mirrors the shape expected by `apps/api-server/src/error-report.ts`'s `ErrorReportMeta`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub id: String,
    pub kind: BundleKind,
    pub app_version: String,
    pub os_version: String,
    pub arch: String,
    pub active_settings: ActiveSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
    pub generated_at: String,
}

/// In-memory bundle ready to upload (or save to disk in dev).
#[derive(Debug, Clone)]
pub struct BuiltBundle {
    pub id: String,
    pub zip_bytes: Vec<u8>,
    pub manifest: BundleManifest,
    pub total_redacted_lines: usize,
    pub sample_first: Vec<String>,
    pub sample_last: Vec<String>,
}

/// Server response shape from `POST /error-report`.
#[derive(Debug, Clone, Deserialize)]
pub struct UploadResult {
    pub id: String,
}

/// One log file selected for inclusion: redacted lines plus the source file's mtime
/// (used to date the zip entry — without an explicit mtime, `zip` writes 1980-01-01).
#[derive(Debug, Clone)]
struct PreparedFile {
    lines: Vec<String>,
    mtime: SystemTime,
}

/// Build an error report bundle in memory. No network. No disk writes (except reading logs).
///
/// `user_note` is trimmed and dropped if empty. Callers are expected to cap its length
/// (the commands layer enforces 100 000 chars) — we store it verbatim.
///
/// `scope` controls which log content makes it into the bundle. See [`BundleScope`].
pub fn build_bundle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: BundleKind,
    user_note: Option<String>,
    scope: BundleScope,
) -> Result<BuiltBundle, String> {
    let id = generate_short_id();

    let now_utc = Utc::now();
    let now_system = SystemTime::now();

    // BTreeMap so zip order is deterministic — same inputs, same bytes out. Matters for
    // the preview hash and for byte-level tests.
    let mut prepared: BTreeMap<String, PreparedFile> = BTreeMap::new();
    let mut total_redacted_lines: usize = 0;
    let mut live_file_name: Option<String> = None;

    if let Some(dir) = logging::log_dir() {
        let files = logging::list_recent_log_files(dir);
        if let Some(first) = files.first()
            && let Some(name) = first.file_name().and_then(|n| n.to_str())
        {
            live_file_name = Some(name.to_string());
        }
        for path in files {
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((lines, mtime)) = load_and_filter_log_file(&path, scope, now_utc, now_system) else {
                continue;
            };
            if lines.is_empty() {
                continue;
            }
            total_redacted_lines += lines.len();
            prepared.insert(file_name.to_string(), PreparedFile { lines, mtime });
        }
    }

    // Derive samples from the most recent log file (the live one in normal operation).
    let (sample_first, sample_last) = match live_file_name.as_ref().and_then(|name| prepared.get(name)) {
        Some(file) => {
            let first: Vec<String> = file.lines.iter().take(SAMPLE_FIRST_LINES).cloned().collect();
            let start = file.lines.len().saturating_sub(SAMPLE_LAST_LINES);
            let last: Vec<String> = file.lines[start..].to_vec();
            (first, last)
        }
        None => (Vec::new(), Vec::new()),
    };

    let manifest = BundleManifest {
        id: id.clone(),
        kind,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: get_os_version(),
        arch: std::env::consts::ARCH.to_string(),
        active_settings: cached_active_settings(app).clone(),
        user_note: user_note.and_then(|n| {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        generated_at: now_utc.to_rfc3339(),
    };

    let zip_bytes = build_zip(&manifest, &prepared, now_system).map_err(|e| format!("build zip: {e}"))?;

    Ok(BuiltBundle {
        id,
        zip_bytes,
        manifest,
        total_redacted_lines,
        sample_first,
        sample_last,
    })
}

/// Read a single log file, redact each line, and apply the `scope`-driven filter.
///
/// Returns `None` if the file can't be opened or its mtime can't be read; returns
/// `Some((lines, mtime))` otherwise. `lines` may be empty if the entire file was
/// outside the scope window — callers drop empty results so the bundle doesn't ship
/// empty `logs/<name>` entries.
fn load_and_filter_log_file(
    path: &Path,
    scope: BundleScope,
    now_utc: DateTime<Utc>,
    now_system: SystemTime,
) -> Option<(Vec<String>, SystemTime)> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(err) => {
            log::warn!(
                target: "cmdr_lib::error_reporter",
                "Skipping log file {} (couldn't stat: {err})",
                path.display(),
            );
            return None;
        }
    };
    let mtime = metadata.modified().unwrap_or(now_system);

    // File-level filter. If a file's mtime is older than the lower bound of the scope's
    // window, skip it entirely — its newest line is older than what we want.
    let lower_bound = match scope {
        BundleScope::Last24Hours => now_utc - FLOW_A_MAX_AGE,
        BundleScope::Window { first_error_at } => first_error_at - FLOW_B_PRE_ERROR_WINDOW,
    };
    let mtime_utc: DateTime<Utc> = mtime.into();
    if mtime_utc < lower_bound {
        return Some((Vec::new(), mtime));
    }

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(err) => {
            log::warn!(
                target: "cmdr_lib::error_reporter",
                "Skipping log file {} (couldn't open: {err})",
                path.display(),
            );
            return None;
        }
    };
    let reader = BufReader::new(file);

    // Per-line filter for the Flow B window: drop lines whose leading ISO-8601 stamp
    // is older than `lower_bound`. Lines without a parseable stamp pass through (the
    // log line wasn't written by us — we keep it as-is rather than risk dropping
    // something useful).
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if let BundleScope::Window { .. } = scope
            && let Some(line_ts) = parse_leading_iso8601(&line)
            && line_ts < lower_bound
        {
            continue;
        }
        lines.push(redact::redact_line(&line).into_owned());
    }

    Some((lines, mtime))
}

/// Parses an ISO-8601 stamp at the start of a log line (matches the format produced by
/// [`logging::dispatch::file_timestamp`]: `YYYY-MM-DDTHH:MM:SS.mmm±HH:MM`).
///
/// Returns `None` for lines that don't start with one — pre-fix-3 lines that just have
/// `HH:MM:SS.mmm`, blank lines, redacted-payload lines, etc. Callers fall back to keeping
/// the line in that case rather than risk a false drop.
fn parse_leading_iso8601(line: &str) -> Option<DateTime<Utc>> {
    // The timestamp is always 29 chars: 23 for the date+time+ms + 6 for `±HH:MM`.
    // If the line doesn't have at least that many chars, bail.
    if line.len() < 29 {
        return None;
    }
    let candidate = &line[..29];
    DateTime::parse_from_str(candidate, "%Y-%m-%dT%H:%M:%S%.3f%:z")
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Convert a [`SystemTime`] into a [`zip::DateTime`] (the format the zip crate stores
/// per entry). On parse failure (system clock before 1980, post-2107) returns the zip
/// crate's default — not a hard error; the bundle still ships, just with a placeholder
/// mtime on that one entry.
fn zip_dt(time: SystemTime) -> ZipDateTime {
    let local: DateTime<chrono::Local> = time.into();
    ZipDateTime::from_date_and_time(
        local.year() as u16,
        local.month() as u8,
        local.day() as u8,
        local.hour() as u8,
        local.minute() as u8,
        local.second() as u8,
    )
    .unwrap_or_default()
}

/// Build the zip archive from a manifest and a set of prepared log files.
///
/// Each entry's mtime is set explicitly: `manifest.json` uses `now`, log entries use
/// the source file's mtime. Without this, the `zip` crate writes 1980-01-01 00:00 for
/// every entry (DOS epoch), which makes downstream tooling — and humans inspecting the
/// archive — unable to tell when anything was actually captured.
///
/// Split out from [`build_bundle`] so tests can feed synthetic inputs without needing
/// a Tauri app handle or a real log directory.
fn build_zip(
    manifest: &BundleManifest,
    files: &BTreeMap<String, PreparedFile>,
    now: SystemTime,
) -> std::io::Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut writer = ZipWriter::new(cursor);
        let manifest_opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip_dt(now));

        let manifest_json =
            serde_json::to_string_pretty(manifest).map_err(|e| std::io::Error::other(format!("manifest: {e}")))?;
        writer.start_file("manifest.json", manifest_opts)?;
        writer.write_all(manifest_json.as_bytes())?;

        for (file_name, prepared) in files {
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(zip_dt(prepared.mtime));
            writer.start_file(format!("logs/{file_name}"), opts)?;
            for line in &prepared.lines {
                writer.write_all(line.as_bytes())?;
                writer.write_all(b"\n")?;
            }
        }

        writer.finish()?;
    }
    Ok(buf)
}

/// Generate a short ID like `ERR-8F3A2` using rejection sampling (no modulo bias).
pub fn generate_short_id() -> String {
    let mut rng = rand::rng();
    let alphabet_len = SHORT_ID_ALPHABET.len(); // 31
    // 256 - (256 % 31) = 232 — bytes at or above this would skew the distribution.
    let max_unbiased = 256 - (256 % alphabet_len);
    let mut out = String::with_capacity(SHORT_ID_PREFIX.len() + 1 + SHORT_ID_LEN);
    out.push_str(SHORT_ID_PREFIX);
    out.push('-');
    let mut remaining = SHORT_ID_LEN;
    while remaining > 0 {
        let byte: u8 = rng.random();
        if (byte as usize) < max_unbiased {
            out.push(SHORT_ID_ALPHABET[(byte as usize) % alphabet_len] as char);
            remaining -= 1;
        }
    }
    out
}

/// POST the bundle to the ingestion server. In dev/CI this skips the network call and
/// synthesizes a response using the locally generated ID.
pub async fn upload(zip_bytes: Vec<u8>, manifest: &BundleManifest, server_url: &str) -> Result<UploadResult, String> {
    let should_skip = cfg!(debug_assertions) || std::env::var("CI").is_ok();
    if should_skip {
        log::info!(
            target: "cmdr_lib::error_reporter",
            "Skipping error report upload (dev mode or CI). Local ID: {}",
            manifest.id,
        );
        return Ok(UploadResult {
            id: manifest.id.clone(),
        });
    }

    let meta_json = serde_json::to_string(manifest).map_err(|e| format!("serialize manifest: {e}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let form = reqwest::multipart::Form::new()
        .part(
            "bundle",
            reqwest::multipart::Part::bytes(zip_bytes)
                .file_name(format!("{}.zip", manifest.id))
                .mime_str("application/zip")
                .map_err(|e| format!("bundle part: {e}"))?,
        )
        .text("meta", meta_json);

    let response = client
        .post(server_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("upload request: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("server returned {}", response.status()));
    }

    response
        .json::<UploadResult>()
        .await
        .map_err(|e| format!("parse upload response: {e}"))
}

/// Cap the bundle to `cap_mb` megabytes, **trimming log content from the head**, not
/// dropping whole files.
///
/// Behavior:
/// 1. `manifest.json` is always preserved in full (verbatim, with its original mtime).
/// 2. `logs/*` entries are sorted newest-first by their stored mtime so the most
///    recent context wins the budget race.
/// 3. Each log entry's content is line-split. Lines are packed into the output zip
///    starting from the **end** of the file (the newest lines) until the compressed
///    output approaches the cap. Older lines get dropped.
/// 4. If a single entry won't fit even partially, we still preserve the last
///    `MIN_TAIL_LINES_OF_NEWEST_FILE` lines of the newest file, even if it pushes the
///    output ~10% over the cap. Shipping a 1.1 MB bundle with useful tail beats
///    shipping a 542-byte bundle with only the manifest (which is what the broken
///    pre-fix-6 implementation did — see the bug report).
///
/// If the input zip is already under the cap, returns it untouched.
pub fn cap_bundle_to_mb(zip_bytes: Vec<u8>, cap_mb: usize) -> Vec<u8> {
    let cap_bytes = cap_mb * 1024 * 1024;
    if zip_bytes.len() <= cap_bytes {
        return zip_bytes;
    }

    let Ok(mut archive) = ZipArchive::new(std::io::Cursor::new(&zip_bytes)) else {
        return zip_bytes;
    };

    // Pull the manifest (preserve verbatim with its mtime).
    let manifest_bytes_and_mtime = read_entry_with_mtime(&mut archive, "manifest.json");

    // Inventory log entries with their mtimes so we can sort newest-first.
    struct LogEntry {
        name: String,
        mtime: ZipDateTime,
        content: Vec<u8>,
    }
    let mut log_entries: Vec<LogEntry> = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else { continue };
        let name = entry.name().to_string();
        if !name.starts_with("logs/") {
            continue;
        }
        let mtime = entry.last_modified().unwrap_or_default();
        let mut content = Vec::new();
        if entry.read_to_end(&mut content).is_err() {
            continue;
        }
        log_entries.push(LogEntry { name, mtime, content });
    }
    // Newest first.
    log_entries.sort_by_key(|e| std::cmp::Reverse(e.mtime));

    // Headroom: leave 10% for the central directory plus per-entry overhead. Compressed
    // text is hard to predict from line count alone, but 10% has been reliable in the
    // 30 MB → 1 MB regression test.
    let target = (cap_bytes * 9) / 10;

    let mut out_buf: Vec<u8> = Vec::with_capacity(cap_bytes);
    let finish_result = {
        let cursor = std::io::Cursor::new(&mut out_buf);
        let mut writer = ZipWriter::new(cursor);

        // 1. Manifest, verbatim.
        if let Some((bytes, mtime)) = &manifest_bytes_and_mtime {
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(*mtime);
            if writer.start_file("manifest.json", opts).is_err() || writer.write_all(bytes).is_err() {
                // Manifest write failed — bail to the original.
                return zip_bytes;
            }
        }

        // 2. Logs, newest-first. Pack lines from the end inward.
        //
        // We can't read `out_buf.len()` while the writer mutably borrows it, so we
        // budget against an *uncompressed* byte tally. Real log text deflates ~5–10×,
        // pathological pseudo-random text only ~1.1×; we pick a budget that lands the
        // compressed output near `target` in the worst case rather than the best.
        // Concretely: uncompressed_budget = target * 1.0. On real logs we'll be far
        // under the cap (acceptable — cap is a ceiling, not a quota). On worst-case
        // input we'll be just under the cap. The minimum-tail floor below covers the
        // pathological "every line still wouldn't fit" case.
        let uncompressed_budget = target;
        let mut uncompressed_used: usize = manifest_bytes_and_mtime.as_ref().map(|(b, _)| b.len()).unwrap_or(0);

        for (i, entry) in log_entries.iter().enumerate() {
            let lines: Vec<&[u8]> = split_into_lines(&entry.content);
            let remaining_budget = uncompressed_budget.saturating_sub(uncompressed_used);

            // Pick how many lines from the tail of this entry to keep.
            let kept_lines: Vec<&[u8]> = if remaining_budget == 0 {
                // No budget at all. Honor the minimum-tail floor for the newest entry only.
                if i == 0 {
                    take_tail(&lines, MIN_TAIL_LINES_OF_NEWEST_FILE)
                } else {
                    Vec::new()
                }
            } else {
                let mut kept = pick_tail_within_budget(&lines, remaining_budget);
                // Floor: ensure the newest file ships at least N lines (even if we'd
                // marginally exceed the cap — see the doc comment).
                if i == 0 && kept.len() < MIN_TAIL_LINES_OF_NEWEST_FILE.min(lines.len()) {
                    kept = take_tail(&lines, MIN_TAIL_LINES_OF_NEWEST_FILE);
                }
                kept
            };

            if kept_lines.is_empty() {
                continue;
            }

            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(entry.mtime);
            if writer.start_file(&entry.name, opts).is_err() {
                continue;
            }
            for line in &kept_lines {
                if writer.write_all(line).is_err() {
                    break;
                }
                if writer.write_all(b"\n").is_err() {
                    break;
                }
                uncompressed_used += line.len() + 1;
            }
        }

        writer.finish()
    };

    if finish_result.is_err() {
        return zip_bytes;
    }
    if out_buf.is_empty() {
        return zip_bytes;
    }
    out_buf
}

/// Split a log file's content into lines (without trailing newlines) so we can pack
/// them tail-first into the capped zip. Empty trailing slice is dropped — we add the
/// `\n` separator on write-out anyway.
fn split_into_lines(content: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = content.split(|b| *b == b'\n').collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

/// Take the last `n` lines (or all of them if there are fewer).
fn take_tail<'a>(lines: &[&'a [u8]], n: usize) -> Vec<&'a [u8]> {
    let start = lines.len().saturating_sub(n);
    lines[start..].to_vec()
}

/// Pick the newest tail of `lines` whose **uncompressed** byte total fits within
/// `budget` bytes.
///
/// Uses a simple back-to-front scan rather than a binary search: cap-trimming runs once
/// per dispatch and the line count is bounded by the rotation cap. Each iteration is a
/// `len()` lookup. The result is the longest tail of `lines` whose summed lengths
/// (plus per-line `\n`) stay under `budget`.
///
/// Heuristic: assume worst-case 1:1 deflate ratio on the line bytes (log text deflates
/// to ~10–20% of source, so this is conservative). Headroom for the central directory
/// is the caller's concern via `target = cap * 9 / 10`.
fn pick_tail_within_budget<'a>(lines: &[&'a [u8]], budget: usize) -> Vec<&'a [u8]> {
    let mut total: usize = 0;
    let mut start = lines.len();
    for (i, line) in lines.iter().enumerate().rev() {
        let cost = line.len() + 1; // +1 for newline
        if total + cost > budget {
            break;
        }
        total += cost;
        start = i;
    }
    lines[start..].to_vec()
}

/// Reads an entry's bytes plus its stored mtime. `None` if the entry doesn't exist or
/// can't be read.
fn read_entry_with_mtime<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Option<(Vec<u8>, ZipDateTime)> {
    let mut entry = archive.by_name(name).ok()?;
    let mtime = entry.last_modified().unwrap_or_default();
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some((bytes, mtime))
}

/// Write the built bundle to the app data dir as `error-report-debug-<timestamp>.zip`.
/// Gated on `debug_assertions` by the caller (see `commands/error_reporter.rs`).
#[cfg(debug_assertions)]
pub fn save_bundle_to_disk<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    bundle: &BuiltBundle,
) -> Result<PathBuf, String> {
    let dir = config::resolved_app_data_dir(app)?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("error-report-debug-{timestamp}.zip"));
    std::fs::write(&path, &bundle.zip_bytes).map_err(|e| format!("write debug bundle: {e}"))?;
    Ok(path)
}

// --- Helpers ---

/// Cached snapshot of active settings. Populated lazily from the settings loader the
/// first time a bundle is built, then reused. Mirrors the crash reporter's cache but
/// stays local to this module so we don't depend on init ordering.
fn cached_active_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> &'static ActiveSettings {
    static CACHE: OnceLock<ActiveSettings> = OnceLock::new();
    CACHE.get_or_init(|| {
        let s = crate::settings::load_settings(app);
        ActiveSettings {
            indexing_enabled: s.indexing_enabled,
            ai_provider: s.ai_provider,
            mcp_enabled: s.developer_mcp_enabled,
            verbose_logging: s.verbose_logging,
        }
    })
}

fn get_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return format!("macOS {version}");
            }
        }
        "macOS (unknown version)".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(release) = std::fs::read_to_string("/etc/os-release") {
            for line in release.lines() {
                if let Some(name) = line.strip_prefix("PRETTY_NAME=") {
                    return name.trim_matches('"').to_string();
                }
            }
        }
        "Linux (unknown distro)".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        std::env::consts::OS.to_string()
    }
}
