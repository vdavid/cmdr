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
use crate::logging;
use crate::redact;
use chrono::{DateTime, Datelike, Timelike, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use zip::DateTime as ZipDateTime;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

#[cfg(test)]
mod tests;

pub mod auto_dispatcher;
pub mod breadcrumbs;
mod tail_walker;

#[cfg(test)]
mod auto_dispatcher_tests;

/// Log an error and (if Flow B is opted in) feed it to the auto-dispatcher.
///
/// Drop-in replacement for [`log::error!`] at user-visible failure sites — anything that
/// already produces a user-facing toast or that an end user would consider "this didn't
/// work." Don't migrate noisy library-level errors (`smb2`, `nusb`, etc.); the goal is
/// signal, not coverage.
///
/// Captures a backtrace at the call site via [`std::backtrace::Backtrace::force_capture`]
/// and emits it as a separate **debug-level** record under the
/// `cmdr_lib::error_reporter::backtrace` target. The fern dispatch tree pins the file
/// chain at Debug regardless of `RUST_LOG`/verbose, so the backtrace always lands in the
/// log file (and therefore in error report bundles), but stdout's Info default keeps the
/// terminal clean. The error-level message stays a single readable line.
///
/// The auto-dispatcher and the manifest's `userNote` see only the user-supplied message
/// — the backtrace lives in the log file. Backtrace lines are redacted by the same
/// path-redactor every other log line goes through, so build-machine paths embedded in
/// symbol metadata don't leak.
///
/// The macro evaluates its arguments exactly once. The `format!()` and backtrace capture
/// happen whether or not the auto-dispatcher is enabled — `force_capture` runs ~0.1–1 ms,
/// negligible at error-event rates. The dispatcher's hot path bails out on a single
/// atomic load when the opt-in flag is off.
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
        let __bt = ::std::backtrace::Backtrace::force_capture();
        ::log::error!(target: $target, "{}", __msg);
        ::log::debug!(
            target: "cmdr_lib::error_reporter::backtrace",
            "Backtrace for [{}] {}:\n{}",
            $target, __msg, __bt,
        );
        $crate::error_reporter::auto_dispatcher::on_error_logged($target, &__msg);
    }};
    ($($arg:tt)+) => {{
        let __msg = format!($($arg)+);
        let __bt = ::std::backtrace::Backtrace::force_capture();
        ::log::error!("{}", __msg);
        ::log::debug!(
            target: "cmdr_lib::error_reporter::backtrace",
            "Backtrace for [{}] {}:\n{}",
            module_path!(), __msg, __bt,
        );
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum BundleKind {
    User,
    Auto,
}

/// Whether this bundle was built by a release or a debug build of the desktop app.
/// Forwarded to the api server in the manifest so triage can tell dev-run reports
/// (which the api server tags `[DEV]` in Discord) apart from production reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum BuildMode {
    Release,
    Debug,
}

impl BuildMode {
    /// Resolved at compile time from `cfg!(debug_assertions)`.
    pub fn current() -> Self {
        if cfg!(debug_assertions) {
            BuildMode::Debug
        } else {
            BuildMode::Release
        }
    }
}

/// Time filter applied when picking which log content to include.
///
/// - `Recent { window }`: include only log lines whose leading ISO-8601 timestamp falls
///   within `[now - window, now]`. The default for Flow A's manual-send path is one
///   hour (`flow_a_default()`). Implemented as a tail-walker that reads each log file
///   from the end backward in 64 KB chunks, stops the moment it crosses the cutoff,
///   and streams lines straight into the zip writer — no full-file read, no
///   intermediate `Vec<String>`. Lines without a parseable timestamp (panic backtrace
///   continuation, state YAML) pass through untouched; the cut boundary always lands
///   on a timestamped line. See [`tail_walker`] for the implementation.
/// - `Window { first_error_at }`: include content whose timestamp falls inside
///   `[first_error_at - 30 min, now]`. Files entirely outside that window are dropped;
///   surviving files are line-filtered by parsing the leading ISO-8601 stamp. Used by
///   Flow B (auto-send) — the window is anchored on the actual error, so we ship
///   surrounding context without the noise. This path still uses the full-read +
///   per-line filter pipeline because the bundle-build runs off the user's hot path
///   (in a debounced background task) and the simpler code is easier to reason about
///   for the auto-send flow.
#[derive(Debug, Clone, Copy)]
pub enum BundleScope {
    Recent { window: Duration },
    Window { first_error_at: DateTime<Utc> },
}

impl BundleScope {
    /// Default Flow A scope: last hour of log content. Manual error reports are about
    /// "something that just happened" — anything older is irrelevant noise.
    pub fn flow_a_default() -> Self {
        BundleScope::Recent {
            window: FLOW_A_DEFAULT_WINDOW,
        }
    }
}

/// 30 minutes of pre-error context for Flow B.
const FLOW_B_PRE_ERROR_WINDOW: chrono::Duration = chrono::Duration::minutes(30);

/// Default window for Flow A's manual-send path. Picked from "what would a user mean
/// when they click 'send error report'?" — anything that happened in the past hour, not
/// last week's session. Lowered from the original 24 h after the streaming rewrite: with
/// tail-walking we could afford a wider window cheaply, but a wider window dilutes triage
/// signal more than it adds context.
const FLOW_A_DEFAULT_WINDOW: Duration = Duration::from_secs(60 * 60);

/// Hard cap for Flow A bundles. 1 MB compressed lands at roughly 19 MB uncompressed,
/// which still gives plenty of recent log context. Lowered from the original 10 MB
/// after live QA showed user-initiated bundles routinely topped 100 MB uncompressed —
/// excessive for triage when the tail of the most recent file is what we actually need.
pub const FLOW_A_BUNDLE_CAP_MB: usize = 1;
/// Hard cap for Flow B bundles. Same 1 MB ceiling as Flow A; both flows ship the same
/// shape of payload, so there's no good reason to diverge.
pub const FLOW_B_BUNDLE_CAP_MB: usize = 1;

/// Always preserve at least this many lines of the most recent file, even if the cap
/// would otherwise force it down to nothing. `cap_bundle_to_mb` may exceed the cap by
/// up to ~10% to honor this — better to ship 1.1 MB of useful context than 0.
const MIN_TAIL_LINES_OF_NEWEST_FILE: usize = 50;

/// Settings snapshot used in error report manifests, with all `Option<bool>` from the
/// settings struct resolved against the registry defaults so triagers never see `null`.
///
/// **Source of defaults**: `apps/desktop/src/lib/settings/settings-registry.ts`. If a
/// default changes there, mirror it here — and add a comment if the discrepancy is
/// intentional (it usually isn't). The defaults are duplicated rather than fetched at
/// runtime because the manifest is built before the frontend can answer round trips
/// (and even when it could, paying a 5 s round-trip per error report is silly).
///
/// Distinct from [`crate::crash_reporter::ActiveSettings`]: that struct is the on-disk
/// crash-file format and stays in `Option<bool>` shape for backward compatibility with
/// crash files written by older app versions. Manifests don't have that constraint.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSettings {
    pub indexing_enabled: bool,
    pub ai_provider: String,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub verbose_logging: bool,
    pub max_log_storage_mb: u64,
    pub error_reports_enabled: bool,
    pub crash_reports_enabled: bool,
}

impl ResolvedSettings {
    /// Build a snapshot from the loaded backend settings, substituting registry defaults
    /// for any field the user hasn't explicitly set.
    ///
    /// Default resolution order, per field:
    /// 1. The user's persisted value, if any (`Some(_)` in the loader struct).
    /// 2. The FE-pushed registry default (see [`settings_defaults`]). Avoids drift when
    ///    the FE registry's default changes.
    /// 3. A hardcoded fallback. Used only before the FE has called
    ///    `record_settings_defaults` (very early errors, unit tests with no FE) — it's
    ///    a safety net, not the primary source.
    fn from_settings(s: &crate::settings::loader::Settings) -> Self {
        Self {
            indexing_enabled: s
                .indexing_enabled
                .or_else(|| settings_defaults::lookup_bool("indexing.enabled"))
                .unwrap_or(true),
            ai_provider: s.ai_provider.clone().unwrap_or_else(|| {
                settings_defaults::lookup_string("ai.provider").unwrap_or_else(|| "local".to_string())
            }),
            mcp_enabled: s
                .developer_mcp_enabled
                .or_else(|| settings_defaults::lookup_bool("developer.mcpEnabled"))
                .unwrap_or(false),
            mcp_port: s
                .developer_mcp_port
                .or_else(|| settings_defaults::lookup_u16("developer.mcpPort"))
                .unwrap_or(crate::mcp::config::DEFAULT_PORT),
            verbose_logging: s
                .verbose_logging
                .or_else(|| settings_defaults::lookup_bool("developer.verboseLogging"))
                .unwrap_or(false),
            max_log_storage_mb: s
                .max_log_storage_mb
                .or_else(|| settings_defaults::lookup_u64("advanced.maxLogStorageMb"))
                .unwrap_or(200),
            error_reports_enabled: s
                .error_reports_enabled
                .or_else(|| settings_defaults::lookup_bool("updates.errorReports"))
                .unwrap_or(false),
            crash_reports_enabled: s
                .crash_reports_enabled
                .or_else(|| settings_defaults::lookup_bool("updates.crashReports"))
                .unwrap_or(false),
        }
    }
}

/// Metadata written into `manifest.json` at the root of the bundle.
/// Mirrors the shape expected by `apps/api-server/src/error-report.ts`'s `ErrorReportMeta`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub id: String,
    pub kind: BundleKind,
    /// Release vs debug build of the desktop app. Lets the api server tag dev-run
    /// reports so triage can keep them separate from production traffic.
    pub build_mode: BuildMode,
    pub app_version: String,
    pub os_version: String,
    pub arch: String,
    pub active_settings: ResolvedSettings,
    /// Effective log thresholds at bundle-build time. Lets a triager tell whether the
    /// absence of a debug line in the file means "didn't happen" or "filtered out."
    pub log_levels: LogLevelSnapshot,
    /// Rolling window of recent FE/BE events that led up to the bundle build, oldest
    /// first. The last entry of `kind: "command"` is the most recent user-driven UI
    /// command. Empty when nothing was recorded (e.g. very early failures, tests).
    /// See `breadcrumbs.rs` for the buffer semantics.
    pub breadcrumbs: Vec<breadcrumbs::Breadcrumb>,
    pub user_note: Option<String>,
    pub generated_at: String,
}

/// Effective log thresholds resolved at logger init + the runtime stdout knob.
///
/// `stdoutDefault` is the chain's startup level (from `RUST_LOG` directive without a
/// `module=`, or `Info` if unset). `stdoutCurrent` is the live `AtomicU8` which the
/// verbose toggle flips. They're usually the same; they differ when the user changed
/// the toggle after launch.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LogLevelSnapshot {
    pub stdout_default: String,
    pub stdout_current: String,
    pub file_chain: String,
    /// Per-module overrides applied to the stdout chain (noise suppression + RUST_LOG
    /// directives). Stable insertion order: noise overrides first, then RUST_LOG.
    pub stdout_module_overrides: Vec<(String, String)>,
}

/// In-memory bundle ready to upload (or save to disk in dev).
///
/// `sample_first` and `sample_last` are the preview samples the dialog renders. With the
/// post-fix-7 tail-walker pipeline:
/// - `sample_first` is the **oldest** lines we kept for the live file (the head of the
///   in-window content, NOT the head of the file on disk — that one is hours/days old
///   and not in the bundle).
/// - `sample_last` is the **newest** lines (the very tail of what we shipped).
///
/// The field names are kept for FE compatibility with `apps/desktop/src/lib/error-reporter/`
/// — see the dialog's "Sample of first/last N lines" headings. The semantics changed
/// (under the old pipeline `sample_first` was the file's first lines, full stop), but
/// "oldest kept" / "newest kept" matches what a triager actually wants to see.
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
    let id = generate_short_id();
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
fn build_bundle_streaming(
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
            // older still — stop walking entirely.
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
            // bound on the eventual on-disk size — there's a small overshoot risk on
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
/// The counter measures bytes the `zip` crate emitted to the cursor — i.e. compressed
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
/// outside the scope window — callers drop empty results so the bundle doesn't ship
/// empty `logs/<name>` entries.
fn load_and_filter_log_file(
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
    // window, skip it entirely — its newest line is older than what we want.
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
        lines.push(redact::redact_line_salted(&line, salt).into_owned());
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

/// POST the bundle to the ingestion server. In CI this skips the network call and
/// synthesizes a response using the locally generated ID — CI runs shouldn't pollute
/// the live error-report channel even if a test triggers a report. Debug builds DO
/// upload; the manifest's `buildMode: "debug"` field lets the server tag those
/// reports `[DEV]` so triage can separate them from production traffic.
pub async fn upload(zip_bytes: Vec<u8>, manifest: &BundleManifest, server_url: &str) -> Result<UploadResult, String> {
    let should_skip = std::env::var("CI").is_ok();
    if should_skip {
        log::info!(
            target: "cmdr_lib::error_reporter",
            "Skipping error report upload (CI). Local ID: {}",
            manifest.id,
        );
        return Ok(UploadResult {
            id: manifest.id.clone(),
        });
    }

    let meta_json = serde_json::to_string(manifest).map_err(|e| format!("serialize manifest: {e}"))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
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
fn read_entry_with_mtime<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<(Vec<u8>, ZipDateTime)> {
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
fn cached_active_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> &'static ResolvedSettings {
    static CACHE: OnceLock<ResolvedSettings> = OnceLock::new();
    CACHE.get_or_init(|| {
        let s = crate::settings::load_settings(app);
        ResolvedSettings::from_settings(&s)
    })
}

/// Build a [`LogLevelSnapshot`] from the live state of `logging::dispatch`. The static
/// `LOG_LEVEL_OVERRIDES` is populated once at logger init; `stdout_threshold()` is a
/// live atomic load.
fn build_log_level_snapshot() -> LogLevelSnapshot {
    let (default_str, overrides) = log_level_overrides::snapshot();
    LogLevelSnapshot {
        stdout_default: default_str,
        stdout_current: format!("{:?}", logging::dispatch::stdout_threshold()).to_lowercase(),
        // The fern file chain is hard-coded to Debug+ (see logging::dispatch::init);
        // pinning this here avoids a second source of truth.
        file_chain: "debug".to_string(),
        stdout_module_overrides: overrides,
    }
}

/// Process-global capture of the stdout chain's startup default + per-module overrides.
/// Populated by [`logging::dispatch::init`] via [`record`]; read by the bundle builder.
pub mod log_level_overrides {
    use std::sync::OnceLock;

    /// `(default_level_str, [(module, level_str)])`. Stored verbatim so output is stable
    /// regardless of `LevelFilter`'s `Debug`/`Display` formatting choices.
    static SNAPSHOT: OnceLock<(String, Vec<(String, String)>)> = OnceLock::new();

    /// Called once from `logging::dispatch::init` after RUST_LOG parsing. Subsequent
    /// calls are no-ops — `OnceLock::set` returns `Err` after the first set.
    pub fn record(default_level: log::LevelFilter, overrides: Vec<(String, log::LevelFilter)>) {
        let default_str = format!("{default_level:?}").to_lowercase();
        let entries: Vec<(String, String)> = overrides
            .into_iter()
            .map(|(m, l)| (m, format!("{l:?}").to_lowercase()))
            .collect();
        let _ = SNAPSHOT.set((default_str, entries));
    }

    pub(crate) fn snapshot() -> (String, Vec<(String, String)>) {
        SNAPSHOT
            .get()
            .cloned()
            .unwrap_or_else(|| ("info".to_string(), Vec::new()))
    }
}

/// Holds the FE-pushed map of `settingId → default value`. Populated once at FE
/// startup via the `record_settings_defaults` Tauri command, after `settingsRegistry`
/// has loaded. Read by [`ResolvedSettings::from_settings`] to avoid duplicating
/// defaults between the TS registry and Rust.
///
/// Intentionally a `Mutex<Option<...>>` rather than `OnceLock`: in dev/HMR the FE may
/// reinitialize and push fresh defaults, and tests need to reset between runs.
/// Reads are O(1) hash lookups; the mutex is uncontended in practice (one writer at
/// FE init, then read-only).
pub mod settings_defaults {
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Settings registry default values pushed from FE. The wire format matches JSON
    /// primitives via `#[serde(untagged)]` — TS sees `boolean | number | string`.
    /// Extend with more variants only when settings of the new shape appear; any field
    /// shape outside this set means the value can't fit in `lookup_*` helpers anyway.
    #[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
    #[serde(untagged)]
    pub enum SettingValue {
        Bool(bool),
        Integer(i64),
        String(String),
    }

    static DEFAULTS: Mutex<Option<HashMap<String, SettingValue>>> = Mutex::new(None);

    /// Replace the stored defaults map. Called from the `record_settings_defaults`
    /// Tauri command. A `None` entry from a buggy FE caller is treated as "no
    /// default available" — `lookup_*` falls through to the hardcoded fallback.
    pub fn record(map: HashMap<String, SettingValue>) {
        if let Ok(mut guard) = DEFAULTS.lock() {
            *guard = Some(map);
        }
    }

    fn get(key: &str) -> Option<SettingValue> {
        DEFAULTS.lock().ok()?.as_ref()?.get(key).cloned()
    }

    pub(super) fn lookup_bool(key: &str) -> Option<bool> {
        if let SettingValue::Bool(b) = get(key)? {
            Some(b)
        } else {
            None
        }
    }

    pub(super) fn lookup_string(key: &str) -> Option<String> {
        if let SettingValue::String(s) = get(key)? {
            Some(s)
        } else {
            None
        }
    }

    pub(super) fn lookup_u16(key: &str) -> Option<u16> {
        if let SettingValue::Integer(n) = get(key)? {
            u16::try_from(n).ok()
        } else {
            None
        }
    }

    pub(super) fn lookup_u64(key: &str) -> Option<u64> {
        if let SettingValue::Integer(n) = get(key)? {
            u64::try_from(n).ok()
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn reset_for_test() {
        if let Ok(mut g) = DEFAULTS.lock() {
            *g = None;
        }
    }
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
