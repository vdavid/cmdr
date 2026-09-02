//! Crash reporter: captures panic and signal crashes to disk for next-launch reporting.
//!
//! Two capture paths handle different crash types:
//! - **Panic hook**: full stdlib access, writes JSON crash file directly
//! - **Signal handler**: async-signal-safe only, writes raw addresses to a pre-opened fd

mod contain;
mod panic_courier;
#[cfg(unix)]
mod signal_handler;
mod survival;
mod symbolicate;

#[cfg(test)]
mod contain_tests;
#[cfg(test)]
mod panic_courier_tests;
#[cfg(test)]
mod survival_tests;
#[cfg(test)]
mod tests;

use crate::config;
use crate::redact;
use crate::settings;
pub use contain::contain_panics;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Once, OnceLock};
use std::time::Instant;

const CRASH_FILE_NAME: &str = "crash-report.json";
const RAW_CRASH_FILE_NAME: &str = "crash-report.raw";
const CRASH_FILE_VERSION: u32 = 1;
/// If the crash file is less than this many seconds old, it's a potential crash loop.
const CRASH_LOOP_THRESHOLD_SECS: u64 = 5;
/// Short-ID prefix used in `CRASH-XXXXX`. The alphabet lives in [`crate::short_id`]
/// and is shared with error reports.
const CRASH_SHORT_ID_PREFIX: &str = "CRASH";
/// Max chars kept from a redacted panic message. Generous enough for a real message plus a
/// chained `caused by:` tail, small enough that it can't crowd out the backtrace in the
/// 64 KB report budget. The api server caps again on its own side.
const PANIC_MESSAGE_MAX_CHARS: usize = 2_000;
/// Appended when [`cap_panic_message`] trims, so a truncated message never reads as complete.
const PANIC_MESSAGE_TRUNCATION_MARKER: &str = "… (truncated)";

/// `"release"` or `"debug"` resolved at compile time. Same shape the error reporter
/// already ships in its manifest, so the api server can store both report types
/// with a single column.
fn current_build_mode() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "release" }
}

static APP_START_TIME: OnceLock<Instant> = OnceLock::new();
static CACHED_SETTINGS: OnceLock<ActiveSettings> = OnceLock::new();

/// Where the panic hook writes the crash file. [`init`] sets it once the app data dir
/// resolves; it stays `None` before that, and for the whole session if the lookup fails.
/// A hook with no path still logs and still dispatches in-session, so a fallible directory
/// lookup can no longer cost a session its panic reporting entirely.
static CRASH_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Makes [`install_panic_hook`] idempotent. It's called from `run()` (as early as this
/// crate gets control) and again from [`init`]; without this the second call would chain
/// our hook onto itself and report every panic twice.
static HOOK_INSTALLED: Once = Once::new();

/// True once this session's hook has written a crash file.
///
/// **Keep-first.** The pending crash file holds ONE report, and the first panic of a
/// session is the causal one; the panics that follow are usually its consequences. Before
/// this flag, panic number two overwrote the evidence for panic number one. Nothing is
/// lost by keeping the first: every panic, first or not, is logged by the courier and so
/// rides along in the log tail of any error report bundle.
static SESSION_CRASH_FILE_WRITTEN: AtomicBool = AtomicBool::new(false);

/// Active settings snapshot cached at startup for inclusion in crash reports.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSettings {
    pub indexing_enabled: Option<bool>,
    pub ai_provider: Option<String>,
    pub mcp_enabled: Option<bool>,
    pub verbose_logging: Option<bool>,
}

/// What we know about the app's fate AFTER this report hit disk.
///
/// A crash file is written at panic *initiation*, before anyone can know whether the
/// process will live: since the lock-poison policy in [`cmdr_fs::ignore_poison`], a panic
/// on a background thread routinely leaves the app running. The next launch is where the
/// answer is finally readable, and this is what carries it, so the dialog can say
/// something TRUE about every report it opens on.
///
/// Deliberately a tri-state at read time rather than a `survived: bool`: a bool's `false`
/// default would claim "the app quit" about every crash file written before the field
/// existed. [`Self::Unknown`] claims nothing instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AppFate {
    /// Written by a build that didn't record a fate. Claim nothing about the app.
    #[default]
    Unknown,
    /// The panic hook wrote the report and nothing has confirmed the app is still alive.
    ///
    /// **Transient, on-disk only.** A living process upgrades it to [`Self::KeptRunning`]
    /// (see `survival.rs`), and [`process_pending_crash`] resolves whatever is left to
    /// [`Self::Ended`] at the next launch, where the absence of that upgrade is proof the
    /// process didn't outlive the panic. The frontend therefore never sees this value.
    Unconfirmed,
    /// The app went away: an unrecoverable signal, or a panic it didn't outlive.
    Ended,
    /// The app was still running after the panic, proved either by the survival watchdog's
    /// timer or by the app reaching its own quit path (`app_lifecycle.rs`).
    KeptRunning,
}

/// The crash report written to disk (JSON).
#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub version: u32,
    pub timestamp: String,
    pub signal: Option<String>,
    pub panic_message: Option<String>,
    pub backtrace_frames: Vec<String>,
    pub thread_name: Option<String>,
    pub thread_count: usize,
    pub app_version: String,
    pub os_version: String,
    pub arch: String,
    pub uptime_secs: f64,
    pub active_settings: ActiveSettings,
    /// True if this crash happened less than 5 seconds after the previous launch
    /// (potential crash loop). The frontend uses this to suppress auto-send.
    #[serde(default)]
    pub possible_crash_loop: bool,
    /// Whether the app outlived what this report describes. Drives which sentence the
    /// next-launch dialog opens with, so it must never overstate what we know: see
    /// [`AppFate`]. Defaults to [`AppFate::Unknown`] for crash files written before this
    /// field existed.
    #[serde(default)]
    pub app_fate: AppFate,
    /// True once this panic has already gone out in-session, via the error reporter's Flow B.
    ///
    /// Set only when a Flow B bundle is actually UPLOADED (`error_reporter::auto_dispatcher`), so
    /// it means "delivered", never "attempted". The next launch deletes a stamped report instead of
    /// offering it: telling someone about the same panic twice spends trust and buys nothing.
    ///
    /// `false` is honest as a default here, unlike [`AppFate`]: it claims only that nothing recorded
    /// a delivery, which for a crash file from an older build is exactly true, and lands on the
    /// pre-existing behavior of offering the report.
    #[serde(default)]
    pub reported_in_session: bool,
    /// `"release"` or `"debug"`, resolved at compile time from `cfg!(debug_assertions)`.
    /// `None` only when read from a crash file written by an older app version that
    /// didn't carry this field; new reports always set it.
    #[serde(default)]
    pub build_mode: Option<String>,
    /// User-visible short ID (`CRASH-XXXXX`) generated at write time. Surfaced in
    /// the next-launch dialog so the user can reference the report. `None` only when
    /// read from a crash file written by an older app version; new reports always
    /// set it.
    #[serde(default)]
    pub short_id: Option<String>,
    /// The `diag_<uuid>` diagnostics id, attached at report-assembly time (panic hook reads
    /// the `OnceLock` snapshot; the signal path attaches it at next-launch assembly). Groups
    /// sequential reports from one install. NEVER the `anal_` analytics id: the two-id split
    /// keeps a voluntarily-attached email unjoinable to the analytics stream. `default`
    /// (empty string) only when read from a crash file written before this field existed.
    #[serde(default)]
    pub diag_id: String,
    /// Beta contact email, populated ONLY by the dialog at send time when the user ticks the
    /// attach-email box (see `commands/crash_reporter.rs`). NEVER read in the crash build path
    /// or the signal handler (no settings access there, and the email isn't known yet). `None`
    /// for every report the user didn't opt to attach an email to.
    #[serde(default)]
    pub email: Option<String>,
    /// Machine snapshot (model, CPU, RAM, disk headroom, drive-index sizes) attached at next-launch
    /// assembly in [`process_pending_crash`], NEVER in the panic hook or signal handler (compromised
    /// context). Always the stable form (`live: None`): a crash report is assembled after relaunch,
    /// where live values describe the fresh process, not the crash. `None` only for reports written
    /// before this field existed, or when the data dir can't be resolved.
    #[serde(default)]
    pub system_snapshot: Option<crate::diagnostics_snapshot::SystemSnapshot>,
    /// Load address of the main executable at crash time, as `"0x…"`.
    ///
    /// `backtrace_frames` are absolute virtual addresses, and ASLR randomizes the base
    /// on every launch, so on their own they can't be compared across launches or users.
    /// With this, `frame - image_base` is a stable per-build offset: identical crash sites
    /// group across installs, and `atos -o <binary> -l <image_base>` resolves them when the
    /// matching build's symbols are available.
    ///
    /// PII-free by construction: a randomized virtual address, no user data. Deliberately
    /// only the numeric base, NEVER a loaded-image path list (those embed `/Users/<name>`).
    /// `None` for reports from builds before this field existed, and on platforms where we
    /// can't resolve it (non-macOS Unix).
    #[serde(default)]
    pub image_base: Option<String>,
}

/// Points the crash reporter at the app data dir: settings cache, previous session's
/// pending report, the crash-file path the hook writes to, and the signal handlers.
///
/// The panic hook itself is NOT installed here. It goes up in `run()`, before Tauri builds
/// anything, so a panic during startup is still caught; this only hands it a path to write
/// to. A failure to resolve the data dir therefore costs the crash FILE, not the hook.
pub fn init<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    APP_START_TIME.get_or_init(Instant::now);
    // Belt and braces: `run()` installs it, and this makes the module correct even if some
    // future entry point reaches `init` first.
    install_panic_hook();

    let Ok(data_dir) = config::resolved_app_data_dir(app) else {
        log::warn!(
            "Crash reporter: couldn't resolve the app data dir. The panic hook stays installed \
             (panics are still logged and, with error reports opted in, still reported in-session), \
             but no crash file can be written this session."
        );
        return;
    };

    let crash_path = data_dir.join(CRASH_FILE_NAME);
    let raw_crash_path = data_dir.join(RAW_CRASH_FILE_NAME);

    // Cache active settings for crash reports, using the same loader as the rest of the app
    cache_active_settings(app);

    // Process any pending crash file from a previous session
    process_pending_crash(&crash_path, &raw_crash_path);

    // Only now: arming the hook's disk write before the previous session's report has been
    // read would let a panic in `process_pending_crash` overwrite the report it was reading.
    let _ = CRASH_PATH.set(crash_path);

    #[cfg(unix)]
    install_signal_handlers(&raw_crash_path);
}

/// Returns the pending crash report from a previous session, if any.
/// Returns `None` if the file doesn't exist, is corrupt, or can't be parsed.
/// Used by milestone 2 (crash report dialog) to check for pending reports.
pub fn take_pending_crash_report<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<CrashReport> {
    let data_dir = config::resolved_app_data_dir(app).ok()?;
    let crash_path = data_dir.join(CRASH_FILE_NAME);
    read_crash_report(&crash_path)
}

// --- Panic hook ---

/// Installs the panic hook. Idempotent, and safe to call before anything else in the
/// process: with no [`CRASH_PATH`] yet it simply skips the disk write.
///
/// Call it as early as possible. Anything that panics before this runs gets the default
/// hook only (a stderr line), with no crash file and no in-session report.
pub fn install_panic_hook() {
    HOOK_INSTALLED.call_once(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let disposition = handle_panic(
                info,
                CRASH_PATH.get().map(PathBuf::as_path),
                &SESSION_CRASH_FILE_WRITTEN,
            );
            // Call the default hook so the app still aborts normally. A contained panic
            // is expected behavior, not an incident, so it skips the stderr line too.
            if disposition == PanicDisposition::Reported {
                default_hook(info);
            }
        }));
    });
}

/// What the hook did with a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanicDisposition {
    /// Inside a [`contain_panics`] closure: one warning line, nothing else.
    Contained,
    /// Crash file (keep-first), watchdog, courier.
    Reported,
}

/// The hook body: write the crash file, then hand a notice to the courier thread.
///
/// ❌ **Nothing added here may be able to panic.** A panic inside a panic hook aborts the
/// process before unwinding starts, so `catch_unwind` here would not help; see
/// [`panic_courier`] for the mechanism. That's also why in-session delivery runs on
/// another thread instead of calling the dispatcher from here.
///
/// `crash_path` and `already_written` are [`CRASH_PATH`] and [`SESSION_CRASH_FILE_WRITTEN`]
/// in production and a temp path with a fresh flag in tests, which is what lets a test
/// prove the contained branch wrote nothing without burning the process-wide ones.
fn handle_panic(
    info: &std::panic::PanicHookInfo<'_>,
    crash_path: Option<&Path>,
    already_written: &AtomicBool,
) -> PanicDisposition {
    // The containment mark first: a panic inside `contain_panics` is a parser choking on
    // untrusted input, not a crash. One warning (message and thread, never a path: the
    // message goes through the same sanitizer as a report's), then nothing else.
    if contain::panic_is_contained() {
        let message = extract_panic_message(info).map(|m| sanitize_panic_message(&m));
        log::warn!(
            "Contained a panic on thread {}: {}",
            std::thread::current().name().unwrap_or("<unnamed>"),
            message.as_deref().unwrap_or("<no message>")
        );
        return PanicDisposition::Contained;
    }

    let report = build_panic_report(info);

    // Disk first, and unchanged: for a panic that kills the app this is the ONLY delivery
    // path, and the process may be gone microseconds from now.
    let wrote_crash_file = write_first_crash_report(crash_path, already_written, &report);

    // Then the survival watchdog, for the same panic. Keep-first means only the panic
    // that owns the crash file arms one, so there's at most one per session.
    if wrote_crash_file && let Some(crash_path) = crash_path {
        survival::arm(crash_path);
    }

    // Then the in-session path, for the panic the app is about to survive. Clones out of
    // the report that just went to disk, so the two can't describe the panic differently.
    // The short id rides along ONLY when this panic is the one on disk: keep-first means a
    // follow-on panic has no crash file, and quoting an id for a report nobody will find
    // sends triage after the wrong panic.
    panic_courier::notify(panic_courier::PanicNotice {
        message: report.panic_message.clone(),
        thread_name: report.thread_name.clone(),
        backtrace_frames: report.backtrace_frames.clone(),
        crash_file_short_id: wrote_crash_file.then(|| report.short_id.clone()).flatten(),
    });
    PanicDisposition::Reported
}

/// Writes `report` to `crash_path`, unless there's no path or `already_written` is already
/// set. Returns whether it wrote.
///
/// `already_written` is [`SESSION_CRASH_FILE_WRITTEN`] in production and a fresh flag in
/// tests; passing it in keeps the keep-first rule testable without burning the real one.
fn write_first_crash_report(crash_path: Option<&Path>, already_written: &AtomicBool, report: &CrashReport) -> bool {
    let Some(crash_path) = crash_path else {
        // No app data dir this session (see `init`). The hook still runs; only the file is lost.
        return false;
    };
    if already_written.swap(true, Ordering::SeqCst) {
        return false;
    }
    if let Err(e) = write_crash_report(crash_path, report) {
        // Straight to stderr: `log` might be the thing that panicked, and taking its
        // mutex from inside the hook would deadlock if the panicking thread still holds it.
        #[allow(clippy::print_stderr, reason = "log may be the thing that panicked")]
        {
            eprintln!("Crash reporter: couldn't write crash file: {e}");
        }
    }
    true
}

/// Hex-formatted load address of the main image for THIS process, or `None` when we
/// can't resolve it. Only valid for reports built in-process (the panic hook); the
/// signal path must use the base recorded in the raw file, since a relaunched process
/// has a different ASLR slide.
fn current_image_base_hex() -> Option<String> {
    #[cfg(unix)]
    {
        let base = signal_handler::current_image_base();
        (base != 0).then(|| format!("0x{base:x}"))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn build_panic_report(info: &std::panic::PanicHookInfo<'_>) -> CrashReport {
    let backtrace = std::backtrace::Backtrace::force_capture();
    let backtrace_frames = parse_backtrace_frames(&backtrace.to_string());

    let message = extract_panic_message(info);
    let sanitized_message = message.map(|m| sanitize_panic_message(&m));

    let thread = std::thread::current();
    let thread_name = thread.name().map(String::from);

    CrashReport {
        version: CRASH_FILE_VERSION,
        timestamp: now_iso8601(),
        signal: Some("panic".to_string()),
        panic_message: sanitized_message,
        backtrace_frames,
        thread_name,
        thread_count: current_thread_count(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: crate::platform::os_version(),
        arch: std::env::consts::ARCH.to_string(),
        uptime_secs: uptime_secs(),
        active_settings: CACHED_SETTINGS.get().cloned().unwrap_or_default(),
        possible_crash_loop: false,
        // Nobody knows yet whether the app will live: the hook runs at panic initiation,
        // before unwinding. `survival.rs` upgrades this from a thread that can only run if
        // the process is still here; `process_pending_crash` resolves it if nothing did.
        app_fate: AppFate::Unconfirmed,
        reported_in_session: false,
        build_mode: Some(current_build_mode().to_string()),
        short_id: Some(crate::short_id::generate(CRASH_SHORT_ID_PREFIX)),
        // The panic hook runs in normal Rust, so it can read the pre-resolved diag-id
        // snapshot directly (cheap `OnceLock` clone, no mint/lock). `email` is a send-time
        // field; the dialog populates it later, never the build path.
        diag_id: crate::install_id::diagnostics_id_snapshot().unwrap_or_default(),
        email: None,
        // Filled at next-launch assembly in `process_pending_crash`: the panic hook must stay light
        // (no sysctl/sysinfo/shell-outs in a compromised context).
        system_snapshot: None,
        // Safe here (unlike the signal handler): the panic hook runs in normal Rust, and this
        // process IS the crashing one, so its slide is the right one to record.
        image_base: current_image_base_hex(),
    }
}

fn extract_panic_message(info: &std::panic::PanicHookInfo<'_>) -> Option<String> {
    // Try to get the payload as &str or String
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        return Some((*s).to_string());
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return Some(s.clone());
    }
    // Fall back to Display if PanicMessage is available (Rust 1.81+)
    Some(info.to_string())
}

/// Strip PII from panic messages, then cap the length. Redaction is the shared
/// [`crate::redact`] pipeline (same one the error reporter runs over log lines), so path,
/// URL-userinfo, and home-dir scrubbing stay single-sourced.
fn sanitize_panic_message(message: &str) -> String {
    let redacted = redact::redact_panic_message(message);
    cap_panic_message(redacted)
}

/// Cap a redacted panic message. `assert_eq!` on large structs yields multi-KB payloads,
/// and the ingestion endpoint rejects the whole report body over 64 KB, so an uncapped
/// message would cost us the entire report rather than just its own tail. Counts chars,
/// not bytes: slicing mid-codepoint would panic inside the panic hook.
fn cap_panic_message(message: String) -> String {
    if message.chars().count() <= PANIC_MESSAGE_MAX_CHARS {
        return message;
    }
    let cut = message
        .char_indices()
        .nth(PANIC_MESSAGE_MAX_CHARS)
        .map_or(message.len(), |(i, _)| i);
    let mut capped = message;
    capped.truncate(cut);
    capped.push_str(PANIC_MESSAGE_TRUNCATION_MARKER);
    capped
}

fn parse_backtrace_frames(backtrace_str: &str) -> Vec<String> {
    // Each frame line from std::backtrace looks like:
    //   0: std::backtrace::Backtrace::create
    //   1: cmdr_lib::crash_reporter::build_panic_report
    // We keep the function name part, stripping the frame number prefix.
    backtrace_str
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            // Skip empty lines and lines that are just addresses or "at ..." source locations
            if trimmed.is_empty() || trimmed.starts_with("at ") {
                return None;
            }
            // Strip leading frame number: "  12: some::function" -> "some::function"
            if let Some(idx) = trimmed.find(": ") {
                let prefix = &trimmed[..idx];
                if prefix.trim().chars().all(|c| c.is_ascii_digit()) {
                    return Some(trimmed[idx + 2..].to_string());
                }
            }
            Some(trimmed.to_string())
        })
        .collect()
}

#[cfg(unix)]
fn install_signal_handlers(raw_crash_path: &Path) {
    signal_handler::install(raw_crash_path);
}

/// Records that the app is still running, so a crash file THIS session wrote stops
/// claiming the app didn't outlive its panic.
///
/// Called from the app's quit path (`app_lifecycle.rs`): an app alive enough to be asked
/// to quit outlived whatever it wrote a crash file about. Two atomic-ish loads and out on
/// the normal path, where this session never panicked. See `survival.rs` for why both this
/// and the watchdog's timer record the same fact.
pub fn note_app_still_running() {
    if !SESSION_CRASH_FILE_WRITTEN.load(Ordering::SeqCst) {
        return;
    }
    if let Some(crash_path) = CRASH_PATH.get() {
        survival::confirm_survival(crash_path);
    }
}

/// Records that a Flow B bundle just landed on the server, so a crash file THIS session wrote has
/// already been reported and the next launch shouldn't offer it a second time.
///
/// Called from `error_reporter::auto_dispatcher` on a successful upload and nowhere else. The
/// bundle is a log-tail bundle, so a panic logged before the upload rode along in it; the panic's
/// own courier is what opened the window in the first place. Free unless this session panicked.
///
/// ❌ Never move this call above the `updates.errorReports` gate or the upload's `Ok`: a stamp
/// means DELIVERED, and stamping an attempt would silently swallow the report of a user who
/// opted out of error reports, or whose upload never made it.
pub fn note_in_session_report_delivered() {
    if !SESSION_CRASH_FILE_WRITTEN.load(Ordering::SeqCst) {
        return;
    }
    if let Some(crash_path) = CRASH_PATH.get() {
        survival::record_in_session_delivery(crash_path);
    }
}

// --- Crash file I/O ---

fn write_crash_report(path: &Path, report: &CrashReport) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|e| format!("serialize crash report: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("write crash file: {e}"))
}

fn read_crash_report(path: &Path) -> Option<CrashReport> {
    let contents = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<CrashReport>(&contents) {
        Ok(report) if report.version == CRASH_FILE_VERSION => Some(report),
        Ok(report) => {
            log::info!(
                "Crash reporter: crash file version {} != expected {CRASH_FILE_VERSION}, discarding",
                report.version
            );
            let _ = std::fs::remove_file(path);
            None
        }
        Err(e) => {
            log::info!("Crash reporter: corrupt crash file ({e}), discarding");
            let _ = std::fs::remove_file(path);
            None
        }
    }
}

/// Process any pending raw signal crash file from a previous session.
/// Symbolicates if the version matches, then converts to JSON format.
fn process_pending_crash(crash_json_path: &Path, raw_crash_path: &Path) {
    // Check for a JSON crash report with crash loop detection
    if let Some(mut report) = read_crash_report(crash_json_path) {
        // Already delivered in-session, so the user has heard about this panic once. Saying it
        // again on the next launch spends trust and adds nothing. Dropped here rather than
        // hidden in the frontend, so a report nobody will be offered can't linger on disk.
        if report.reported_in_session {
            log::info!("Crash reporter: pending report already went out in-session, discarding");
            let _ = std::fs::remove_file(crash_json_path);
            return;
        }
        let mut dirty = false;
        if is_crash_loop(&report.timestamp) {
            report.possible_crash_loop = true;
            dirty = true;
        }
        // The panic hook can't know the app's fate, so it writes `Unconfirmed` and
        // `survival.rs` upgrades it from a thread that can only run while the process is
        // alive. Reaching a LATER LAUNCH still unconfirmed is therefore the proof that no
        // such upgrade happened: the app went down with the panic. Resolving it here, at
        // the one moment the evidence is conclusive, is what lets the dialog pick its
        // opening sentence from a settled value.
        if report.app_fate == AppFate::Unconfirmed {
            report.app_fate = AppFate::Ended;
            dirty = true;
        }
        // The panic hook couldn't gather the snapshot (compromised context), so attach the stable
        // form now, at next launch where the full stdlib is safe. Only when missing, so we don't
        // rewrite on every launch the report lingers.
        if report.system_snapshot.is_none()
            && let Some(dir) = crash_json_path.parent()
        {
            report.system_snapshot = Some(crate::diagnostics_snapshot::SystemSnapshot::collect_stable(dir));
            dirty = true;
        }
        if dirty {
            let _ = write_crash_report(crash_json_path, &report);
        }
        // JSON report exists, leave it for the frontend to handle
        return;
    }

    // Check for a raw signal crash file
    #[cfg(unix)]
    if raw_crash_path.exists() {
        if let Some((signal, addresses, image_base, crash_app_version)) = signal_handler::read_raw_crash(raw_crash_path)
        {
            let current_version = env!("CARGO_PKG_VERSION");
            let versions_match = crash_app_version == current_version;

            let backtrace_frames = if versions_match {
                symbolicate::symbolicate_addresses(&addresses)
            } else {
                log::info!(
                    "Crash reporter: version mismatch (crash={crash_app_version}, \
                     current={current_version}), sending raw addresses"
                );
                addresses.iter().map(|a| format!("0x{a:016x}")).collect()
            };

            let signal_name = signal_name(signal);

            let report = CrashReport {
                version: CRASH_FILE_VERSION,
                timestamp: now_iso8601(),
                signal: Some(signal_name),
                panic_message: None,
                backtrace_frames,
                thread_name: None,
                thread_count: 0,
                app_version: crash_app_version,
                os_version: crate::platform::os_version(),
                arch: std::env::consts::ARCH.to_string(),
                uptime_secs: 0.0, // Unknown for signal crashes from previous session
                active_settings: CACHED_SETTINGS.get().cloned().unwrap_or_default(),
                possible_crash_loop: false,
                // No ambiguity on this path: SIGSEGV/SIGBUS/SIGABRT are unrecoverable, the
                // handler re-raises them, and we're reading the evidence from the NEXT
                // launch. The app is definitively gone.
                app_fate: AppFate::Ended,
                // A signal crash is never delivered in-session: the process died in the handler.
                reported_in_session: false,
                build_mode: Some(current_build_mode().to_string()),
                short_id: Some(crate::short_id::generate(CRASH_SHORT_ID_PREFIX)),
                // Signal path: the async-signal-safe handler couldn't touch the diag id (no
                // alloc/lock). We attach it HERE, at next-launch assembly, where full stdlib is
                // available. `email` stays `None` (send-time field, set by the dialog).
                diag_id: crate::install_id::diagnostics_id(),
                email: None,
                // Stable snapshot, assembled here at next launch (full stdlib available). The data
                // dir is the crash file's parent; the snapshot reads only index sizes and capacity.
                system_snapshot: crash_json_path
                    .parent()
                    .map(crate::diagnostics_snapshot::SystemSnapshot::collect_stable),
                // The base recorded BY THE CRASHED PROCESS, never this one's: ASLR gives the
                // relaunched process a different slide, which would make every offset wrong.
                // `0` means that build couldn't resolve it (non-macOS Unix).
                image_base: (image_base != 0).then(|| format!("0x{image_base:x}")),
            };

            if let Err(e) = write_crash_report(crash_json_path, &report) {
                log::warn!("Crash reporter: couldn't write symbolicated crash report: {e}");
            }
        }

        let _ = std::fs::remove_file(raw_crash_path);
    }
}

/// Cache active settings for crash reports, using the app's settings loader.
/// This piggybacks on `settings::load_settings` so defaults stay in sync.
/// Fields that are `None` in the settings struct mean "user hasn't changed this":
/// the frontend registry owns the defaults. We pass through `None` as-is; the crash
/// report consumer can interpret null as "default."
fn cache_active_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let s = settings::load_settings(app);
    let settings = ActiveSettings {
        indexing_enabled: s.indexing_enabled,
        ai_provider: s.ai_provider,
        mcp_enabled: s.developer_mcp_enabled,
        verbose_logging: s.verbose_logging,
    };
    let _ = CACHED_SETTINGS.set(settings);
}

// --- Helpers ---

fn now_iso8601() -> String {
    // Use chrono (already a dependency) for ISO 8601 timestamp
    chrono::Utc::now().to_rfc3339()
}

/// Seconds since the process started, or 0.0 before [`init`] runs. Shared with the diagnostics
/// snapshot so error reports can carry the same uptime the crash reporter records.
pub(crate) fn uptime_secs() -> f64 {
    APP_START_TIME.get().map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
}

fn current_thread_count() -> usize {
    #[cfg(target_os = "macos")]
    {
        // Mach API: get thread list for the current task, return the count.
        unsafe extern "C" {
            fn mach_task_self() -> libc::mach_port_t;
        }
        // SAFETY: `thread_list` and `thread_count` are live locals whose addresses are valid
        // out-params for `task_threads`. On `KERN_SUCCESS` the kernel hands back an allocated
        // array of `thread_count` ports in `thread_list`; we hand that exact array and its byte
        // size (`thread_count * size_of::<mach_port_t>()`) back to `vm_deallocate`, freeing it
        // exactly once. We deallocate only on `KERN_SUCCESS`, where the out-params are valid.
        unsafe {
            let mut thread_list: libc::mach_port_t = 0;
            let mut thread_count: u32 = 0;
            let kr = libc::task_threads(
                mach_task_self(),
                std::ptr::addr_of_mut!(thread_list) as *mut *mut libc::mach_port_t,
                &raw mut thread_count,
            );
            if kr == libc::KERN_SUCCESS {
                // Deallocate the thread list (we only needed the count)
                libc::vm_deallocate(
                    mach_task_self(),
                    thread_list as libc::vm_address_t,
                    (thread_count as usize) * size_of::<libc::mach_port_t>(),
                );
                thread_count as usize
            } else {
                0
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(count) = line.strip_prefix("Threads:") {
                    return count.trim().parse().unwrap_or(0);
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

#[cfg(unix)]
fn signal_name(sig: i32) -> String {
    match sig {
        libc::SIGSEGV => "SIGSEGV".to_string(),
        libc::SIGBUS => "SIGBUS".to_string(),
        libc::SIGABRT => "SIGABRT".to_string(),
        other => format!("signal {other}"),
    }
}

/// Check if the crash timestamp indicates a crash loop (< 5 seconds before current launch).
fn is_crash_loop(crash_timestamp: &str) -> bool {
    let Ok(crash_time) = chrono::DateTime::parse_from_rfc3339(crash_timestamp) else {
        return false;
    };
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(crash_time);
    elapsed.num_seconds() >= 0 && (elapsed.num_seconds() as u64) < CRASH_LOOP_THRESHOLD_SECS
}
