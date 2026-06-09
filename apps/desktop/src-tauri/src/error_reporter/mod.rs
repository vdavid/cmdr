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
//!
//! ## Module layout
//!
//! - [`bundle_builder`]: the two build pipelines (Flow A streaming and Flow B legacy window) plus
//!   the shared zip-writing path.
//! - [`bundle_capper`]: post-hoc size cap that trims log content from the head of the newest file.
//!   Used by Flow B and as defense-in-depth on Flow A.
//! - [`tail_walker`]: reads a log file from the end backward in 64 KB chunks.
//! - [`auto_dispatcher`]: Flow B (opt-in auto-send on user-visible errors).
//! - [`breadcrumbs`]: bounded ring buffer of recent triage events.
//!
//! `mod.rs` keeps the public types ([`BundleKind`], [`BundleScope`], [`BundleManifest`],
//! [`ResolvedSettings`], [`BuiltBundle`], [`UploadResult`]), the [`log_error!`] macro,
//! [`upload`], [`generate_short_id`], [`save_bundle_to_disk`], plus the cached-settings
//! and log-level-snapshot helpers shared between the two pipelines.

#[cfg(debug_assertions)]
use crate::config;
use crate::logging;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(test)]
mod tests;

pub mod auto_dispatcher;
pub mod breadcrumbs;
pub(crate) mod bundle_builder;
pub(crate) mod bundle_capper;
mod tail_walker;

#[cfg(test)]
mod auto_dispatcher_tests;

pub use bundle_builder::build_bundle;
pub use bundle_capper::cap_bundle_to_mb;

/// Log an error and (if Flow B is opted in) feed it to the auto-dispatcher.
///
/// Drop-in replacement for [`log::error!`] at user-visible failure sites: anything that
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
/// The backtrace lives in the log file. Backtrace lines are redacted by the same
/// path-redactor every other log line goes through, so build-machine paths embedded in
/// symbol metadata don't leak.
///
/// The macro evaluates its arguments exactly once. The `format!()` and backtrace capture
/// happen whether or not the auto-dispatcher is enabled; `force_capture` runs ~0.1–1 ms,
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

/// Short ID prefix for error reports. The alphabet, length, and generation routine
/// live in [`crate::short_id`] so the crash reporter can reuse them.
const SHORT_ID_PREFIX: &str = "ERR";

/// Flavor of the bundle, kept separate so Phase 5's auto-sender can share the same builder.
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
/// - `Recent { window }`: include only log lines whose leading ISO-8601 timestamp falls within
///   `[now - window, now]`. The default for Flow A's manual-send path is one hour
///   (`flow_a_default()`). Implemented as a tail-walker that reads each log file from the end
///   backward in 64 KB chunks, stops the moment it crosses the cutoff, and streams lines straight
///   into the zip writer (no full-file read, no intermediate `Vec<String>`). Lines without a
///   parseable timestamp (panic backtrace continuation, state YAML) pass through untouched; the cut
///   boundary always lands on a timestamped line. See [`tail_walker`] for the implementation.
/// - `Window { first_error_at }`: include content whose timestamp falls inside `[first_error_at -
///   30 min, now]`. Files entirely outside that window are dropped; surviving files are
///   line-filtered by parsing the leading ISO-8601 stamp. Used by Flow B (auto-send): the window is
///   anchored on the actual error, so we ship surrounding context without the noise. This path
///   still uses the full-read + per-line filter pipeline because the bundle-build runs off the
///   user's hot path (in a debounced background task) and the simpler code is easier to reason
///   about for the auto-send flow.
#[derive(Debug, Clone, Copy)]
pub enum BundleScope {
    Recent { window: Duration },
    Window { first_error_at: chrono::DateTime<Utc> },
}

impl BundleScope {
    /// Default Flow A scope: last hour of log content. Manual error reports are about
    /// "something that just happened"; anything older is irrelevant noise.
    pub fn flow_a_default() -> Self {
        BundleScope::Recent {
            window: FLOW_A_DEFAULT_WINDOW,
        }
    }
}

/// Default window for Flow A's manual-send path. Picked from "what would a user mean
/// when they click 'send error report'?": anything that happened in the past hour, not
/// last week's session. Lowered from the original 24 h after the streaming rewrite: with
/// tail-walking we could afford a wider window cheaply, but a wider window dilutes triage
/// signal more than it adds context.
const FLOW_A_DEFAULT_WINDOW: Duration = Duration::from_secs(60 * 60);

/// Hard cap for Flow A bundles. 1 MB compressed lands at roughly 19 MB uncompressed,
/// which still gives plenty of recent log context. Lowered from the original 10 MB
/// after live QA showed user-initiated bundles routinely topped 100 MB uncompressed;
/// excessive for triage when the tail of the most recent file is what we actually need.
pub const FLOW_A_BUNDLE_CAP_MB: usize = 1;
/// Hard cap for Flow B bundles. Same 1 MB ceiling as Flow A; both flows ship the same
/// shape of payload, so there's no good reason to diverge.
pub const FLOW_B_BUNDLE_CAP_MB: usize = 1;

/// Settings snapshot used in error report manifests, with all `Option<bool>` from the
/// settings struct resolved against the registry defaults so triagers never see `null`.
///
/// **Source of defaults**: `apps/desktop/src/lib/settings/settings-registry.ts`. If a
/// default changes there, mirror it here, and add a comment if the discrepancy is
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
    /// 2. The FE-pushed registry default (see [`settings_defaults`]). Avoids drift when the FE
    ///    registry's default changes.
    /// 3. A hardcoded fallback. Used only before the FE has called `record_settings_defaults` (very
    ///    early errors, unit tests with no FE); it's a safety net, not the primary source.
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
/// - `sample_first` is the **oldest** lines we kept for the live file (the head of the in-window
///   content, NOT the head of the file on disk (that one is hours/days old and not in the bundle)).
/// - `sample_last` is the **newest** lines (the very tail of what we shipped).
///
/// The field names are kept for FE compatibility with `apps/desktop/src/lib/error-reporter/`
/// (see the dialog's "Sample of first/last N lines" headings). The semantics changed
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

/// Generate a short ID like `ERR-8F3A2`. Thin wrapper around [`crate::short_id::generate`]
/// kept for the existing public surface; new code should call `crate::short_id::generate`
/// directly with the prefix it wants.
pub fn generate_short_id() -> String {
    crate::short_id::generate(SHORT_ID_PREFIX)
}

/// POST the bundle to the ingestion server. In CI this skips the network call and
/// synthesizes a response using the locally generated ID; CI runs shouldn't pollute
/// the live error-report channel even if a test triggers a report. Debug builds DO
/// upload; the manifest's `buildMode: "debug"` field lets the server tag those
/// reports `[DEV]` so triage can separate them from production traffic.
///
/// E2E builds (`playwright-e2e` feature) NEVER upload, compile-time. They're
/// release builds, so without this gate their reports said `prod` and were
/// indistinguishable from real users' — a local E2E run once flooded the live
/// channel with 11 reports in a day. Errors during an E2E run are already
/// visible in the test output; the report channel is for failures we can't
/// observe directly. The feature gate beats an env-var check because the only
/// binaries carrying the feature are purpose-built for tests, with no way to
/// launch one "for real."
pub async fn upload(zip_bytes: Vec<u8>, manifest: &BundleManifest, server_url: &str) -> Result<UploadResult, String> {
    #[cfg(feature = "playwright-e2e")]
    {
        let _ = (zip_bytes, server_url); // the network path is compiled out below
        log::info!(
            target: "cmdr_lib::error_reporter",
            "Skipping error report upload (E2E build). Local ID: {}",
            manifest.id,
        );
        return Ok(UploadResult {
            id: manifest.id.clone(),
        });
    }
    #[cfg(not(feature = "playwright-e2e"))]
    {
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
}

/// Write the built bundle to the app data dir as `error-report-debug-<timestamp>.zip`.
/// Gated on `debug_assertions` by the caller (see `commands/error_reporter.rs`).
#[cfg(debug_assertions)]
pub fn save_bundle_to_disk<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    bundle: &BuiltBundle,
) -> Result<std::path::PathBuf, String> {
    let dir = config::resolved_app_data_dir(app)?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("error-report-debug-{timestamp}.zip"));
    std::fs::write(&path, &bundle.zip_bytes).map_err(|e| format!("write debug bundle: {e}"))?;
    Ok(path)
}

// --- Helpers shared between bundle_builder and the manifest assembly ---

/// Cached snapshot of active settings. Populated lazily from the settings loader the
/// first time a bundle is built, then reused. Mirrors the crash reporter's cache but
/// stays local to this module so we don't depend on init ordering.
pub(crate) fn cached_active_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> &'static ResolvedSettings {
    static CACHE: OnceLock<ResolvedSettings> = OnceLock::new();
    CACHE.get_or_init(|| {
        let s = crate::settings::load_settings(app);
        ResolvedSettings::from_settings(&s)
    })
}

/// Build a [`LogLevelSnapshot`] from the live state of `logging::dispatch`. The static
/// `LOG_LEVEL_OVERRIDES` is populated once at logger init; `stdout_threshold()` is a
/// live atomic load.
pub(crate) fn build_log_level_snapshot() -> LogLevelSnapshot {
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
    /// calls are no-ops; `OnceLock::set` returns `Err` after the first set.
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
    /// primitives via `#[serde(untagged)]`; TS sees `boolean | number | string`.
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
    /// default available"; `lookup_*` falls through to the hardcoded fallback.
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
