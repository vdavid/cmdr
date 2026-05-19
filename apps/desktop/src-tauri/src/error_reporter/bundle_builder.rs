//! Bundle construction. Reads + redacts log content and emits a zip with a manifest.
//!
//! Two pipelines live here:
//! - [`build_bundle_streaming`] (Flow A): tail-walks each log file, streams in-window lines
//!   straight into the zip, stops as soon as the compressed cap or the timestamp cutoff fires.
//! - [`build_bundle_legacy_window`] (Flow B): reads each file in full, line-filters by timestamp,
//!   packs into a `BTreeMap`, then calls [`build_zip`].
//!
//! Public entry point is [`build_bundle`], which dispatches on the scope. Capping is in
//! the sibling [`super::bundle_capper`] module. Flow A enforces the cap inline; Flow B
//! relies on a post-build trim from the auto-dispatcher.

use super::tail_walker;
use super::{
    BuildMode, BuiltBundle, BundleKind, BundleManifest, BundleScope, FLOW_A_BUNDLE_CAP_MB, breadcrumbs,
    build_log_level_snapshot, cached_active_settings, get_os_version,
};
use crate::logging;
use crate::redact;
use chrono::{DateTime, Datelike, Timelike, Utc};
use rand::RngExt;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use zip::DateTime as ZipDateTime;
use zip::write::{SimpleFileOptions, ZipWriter};

/// Max lines in the first-lines preview sample shown in the dialog.
const SAMPLE_FIRST_LINES: usize = 5;
/// Max lines in the last-lines preview sample.
const SAMPLE_LAST_LINES: usize = 20;

/// 30 minutes of pre-error context for Flow B.
const FLOW_B_PRE_ERROR_WINDOW: chrono::Duration = chrono::Duration::minutes(30);

/// One log file selected for inclusion: redacted lines plus the source file's mtime
/// (used to date the zip entry; without an explicit mtime, `zip` writes 1980-01-01).
#[derive(Debug, Clone)]
pub(super) struct PreparedFile {
    pub(super) lines: Vec<String>,
    pub(super) mtime: SystemTime,
}

/// Build an error report bundle in memory. No network. No disk writes (except reading logs).
///
/// `user_note` is trimmed and dropped if empty. Callers are expected to cap its length
/// (the commands layer enforces 100 000 chars); we store it verbatim.
///
/// `scope` controls which log content makes it into the bundle. See [`BundleScope`].
///
/// ## Pipeline
///
/// `BundleScope::Recent { window }` (Flow A) walks each log file from the end backward
/// via [`tail_walker::walk_tail`], redacts each in-window line on the fly, and streams
/// it directly into the zip writer. The streaming path tracks a running compressed-size
/// estimate (via the underlying buffer's length) and stops adding content once it
/// crosses [`FLOW_A_BUNDLE_CAP_MB`]. No `cap_bundle_to_mb` post-pass is needed for this
/// scope.
///
/// `BundleScope::Window { first_error_at }` (Flow B / auto-send) uses the legacy
/// "read whole file, line-filter by timestamp, redact, BTreeMap, build_zip" pipeline.
/// The auto-dispatcher then runs `cap_bundle_to_mb` on the result. This path is left
/// unchanged because the auto-send flow runs off the user's hot path and the simpler
/// code is easier to reason about for that flow.
pub fn build_bundle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: BundleKind,
    user_note: Option<String>,
    scope: BundleScope,
) -> Result<BuiltBundle, String> {
    let id = super::generate_short_id();
    let now_utc = Utc::now();
    let now_system = SystemTime::now();

    // Per-bundle redaction salt: 16 random bytes mixed into every path-segment hash so
    // a triager can spot "same dir mentioned 12 times" within this bundle while the
    // same path in another bundle hashes differently. The salt itself never ships.
    let salt: [u8; 16] = rand::rng().random();

    let manifest = BundleManifest {
        id: id.clone(),
        kind,
        build_mode: BuildMode::current(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: get_os_version(),
        arch: std::env::consts::ARCH.to_string(),
        active_settings: cached_active_settings(app).clone(),
        log_levels: build_log_level_snapshot(),
        breadcrumbs: breadcrumbs::snapshot(),
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

    match scope {
        BundleScope::Recent { window } => {
            let cutoff = now_utc - chrono::Duration::from_std(window).unwrap_or(chrono::Duration::hours(1));
            let files = match logging::log_dir() {
                Some(dir) => logging::list_recent_log_files(dir),
                None => Vec::new(),
            };
            build_bundle_streaming(id, manifest, files, cutoff, now_system, &salt)
        }
        BundleScope::Window { .. } => build_bundle_legacy_window(id, manifest, scope, now_utc, now_system, &salt),
    }
}

/// Streaming Flow A pipeline. Walks log files newest-first via the tail walker, redacts
/// each in-window line, and streams it into the zip writer. Stops the moment the
/// compressed output crosses [`FLOW_A_BUNDLE_CAP_MB`] OR the tail walker hits the
/// timestamp cutoff in every file we visit.
///
/// Compressed-size tracking: the `ZipWriter` is constructed over a [`CountingCursor`]
/// (a `Cursor<Vec<u8>>` wrapper holding an `Arc<AtomicU64>` counting bytes written
/// through it). The counter increments on every `Write::write` to the inner cursor,
/// which is what the `zip` crate's deflater calls after compressing each chunk. We poll
/// the counter after each line to decide whether to stop. Reading is lock-free
/// (`Ordering::Relaxed`) and adds zero latency to the hot path.
pub(super) fn build_bundle_streaming(
    id: String,
    manifest: BundleManifest,
    files: Vec<PathBuf>,
    cutoff: DateTime<Utc>,
    now_system: SystemTime,
    salt: &[u8],
) -> Result<BuiltBundle, String> {
    let cap_bytes = FLOW_A_BUNDLE_CAP_MB * 1024 * 1024;

    let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cursor = CountingCursor::new(counter.clone());

    // Sample lines: oldest-first (head of what we kept) and newest-first reversed back
    // to chronological order (the very tail of the live file).
    let mut sample_first: Vec<String> = Vec::new();
    let mut sample_last: Vec<String> = Vec::new();
    let mut total_redacted_lines: usize = 0;

    let mut writer = ZipWriter::new(cursor);

    // Manifest first. Deflate level 1 (the rest of the bundle uses 1 too). We keep
    // deflate-stored over `Stored` here so the `zip` crate writes a consistent layout.
    let manifest_opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(1))
        .last_modified_time(zip_dt(now_system));
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| format!("manifest: {e}"))?;
    writer
        .start_file("manifest.json", manifest_opts)
        .map_err(|e| format!("start manifest: {e}"))?;
    writer
        .write_all(manifest_json.as_bytes())
        .map_err(|e| format!("write manifest: {e}"))?;

    let mut is_first_file = true;
    let mut budget_exhausted = false;

    for path in files {
        if budget_exhausted {
            break;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // Cheap pre-check: skip files whose mtime is older than the cutoff. Nothing
        // newer than its mtime can be inside the file.
        let mtime = match std::fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(err) => {
                log::warn!(
                    target: "cmdr_lib::error_reporter",
                    "Skipping log file {} (couldn't stat: {err})",
                    path.display(),
                );
                continue;
            }
        };
        let mtime_utc: DateTime<Utc> = mtime.into();
        if mtime_utc < cutoff {
            continue;
        }

        let walk = match tail_walker::walk_tail(&path, cutoff) {
            Ok(r) => r,
            Err(err) => {
                log::warn!(
                    target: "cmdr_lib::error_reporter",
                    "Skipping log file {} (tail-walk failed: {err})",
                    path.display(),
                );
                continue;
            }
        };

        if walk.lines.is_empty() {
            // If we bailed at the cutoff with no lines, older rotations would be
            // older still. Stop walking entirely.
            if walk.hit_cutoff {
                break;
            }
            continue;
        }

        let entry_opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(1))
            .last_modified_time(zip_dt(mtime));
        if writer.start_file(format!("logs/{file_name}"), entry_opts).is_err() {
            continue;
        }

        for line in &walk.lines {
            let redacted = redact::redact_line_salted(line, salt);
            if writer.write_all(redacted.as_bytes()).is_err() || writer.write_all(b"\n").is_err() {
                budget_exhausted = true;
                break;
            }
            total_redacted_lines += 1;

            if is_first_file {
                if sample_first.len() < SAMPLE_FIRST_LINES {
                    sample_first.push(redacted.clone().into_owned());
                }
                if sample_last.len() < SAMPLE_LAST_LINES {
                    sample_last.push(redacted.clone().into_owned());
                } else {
                    // Sliding window: drop oldest, push newest. The Vec is small so
                    // the O(n) shift is irrelevant.
                    sample_last.remove(0);
                    sample_last.push(redacted.clone().into_owned());
                }
            }

            // Mid-file cap check. The deflater holds an internal buffer of up to ~64 KB
            // that hasn't been flushed to the cursor yet, so the counter is a lower
            // bound on the eventual on-disk size. There's a small overshoot risk on
            // the order of one chunk + the central directory tail (~few hundred bytes
            // per entry). Both are well inside the cap's headroom.
            let bytes_so_far = counter.load(std::sync::atomic::Ordering::Relaxed) as usize;
            if bytes_so_far >= cap_bytes {
                budget_exhausted = true;
                break;
            }
        }

        is_first_file = false;

        // Older rotations contain only older lines; if we just hit the cutoff in
        // this file, anything older is by definition outside the window.
        if walk.hit_cutoff {
            break;
        }
    }

    let buf = match writer.finish() {
        Ok(cur) => cur.into_inner(),
        Err(e) => return Err(format!("finish zip: {e}")),
    };

    Ok(BuiltBundle {
        id,
        zip_bytes: buf,
        manifest,
        total_redacted_lines,
        sample_first,
        sample_last,
    })
}

/// `Cursor<Vec<u8>>` adapter that increments an `AtomicU64` by `buf.len()` on every
/// `Write::write` call. Lets the streaming zip pipeline poll the running compressed
/// byte count without taking an unsafe `get_mut()` borrow on the `ZipWriter`.
///
/// The counter measures bytes the `zip` crate emitted to the cursor, i.e. compressed
/// payload + per-entry headers up to the last deflate flush. The crate's internal
/// deflate buffer (up to ~64 KB) lags behind, so callers should treat the counter as a
/// lower bound on the final size and budget conservatively.
struct CountingCursor {
    inner: std::io::Cursor<Vec<u8>>,
    counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl CountingCursor {
    fn new(counter: std::sync::Arc<std::sync::atomic::AtomicU64>) -> Self {
        Self {
            inner: std::io::Cursor::new(Vec::new()),
            counter,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.inner.into_inner()
    }
}

impl Write for CountingCursor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.counter.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for CountingCursor {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Legacy "read full file, line-filter, redact, BTreeMap, build_zip" pipeline used by
/// Flow B (`BundleScope::Window`). Kept as-is because the auto-dispatcher already runs
/// `cap_bundle_to_mb` on the result and the auto-send code path runs in a debounced
/// background task off the user's hot path.
fn build_bundle_legacy_window(
    id: String,
    manifest: BundleManifest,
    scope: BundleScope,
    now_utc: DateTime<Utc>,
    now_system: SystemTime,
    salt: &[u8],
) -> Result<BuiltBundle, String> {
    // BTreeMap so zip order is deterministic: same inputs, same bytes out. Matters for
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
            let Some((lines, mtime)) = load_and_filter_log_file(&path, scope, now_utc, now_system, salt) else {
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
/// outside the scope window. Callers drop empty results so the bundle doesn't ship
/// empty `logs/<name>` entries.
pub(super) fn load_and_filter_log_file(
    path: &Path,
    scope: BundleScope,
    now_utc: DateTime<Utc>,
    now_system: SystemTime,
    salt: &[u8],
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
    // window, skip it entirely: its newest line is older than what we want.
    let lower_bound = match scope {
        BundleScope::Recent { window } => {
            now_utc - chrono::Duration::from_std(window).unwrap_or(chrono::Duration::hours(1))
        }
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
    // log line wasn't written by us, so we keep it as-is rather than risk dropping
    // something useful).
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if let BundleScope::Window { .. } = scope
            && let Some(line_ts) = tail_walker::parse_leading_iso8601(&line)
            && line_ts < lower_bound
        {
            continue;
        }
        lines.push(redact::redact_line_salted(&line, salt).into_owned());
    }

    Some((lines, mtime))
}

/// Convert a [`SystemTime`] into a [`zip::DateTime`] (the format the zip crate stores
/// per entry). On parse failure (system clock before 1980, post-2107) returns the zip
/// crate's default (not a hard error). The bundle still ships, just with a placeholder
/// mtime on that one entry.
pub(super) fn zip_dt(time: SystemTime) -> ZipDateTime {
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
/// every entry (DOS epoch), which makes downstream tooling and humans inspecting the
/// archive unable to tell when anything was actually captured.
///
/// Split out from [`build_bundle`] so tests can feed synthetic inputs without needing
/// a Tauri app handle or a real log directory.
pub(super) fn build_zip(
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
