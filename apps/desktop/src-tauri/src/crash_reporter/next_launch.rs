//! Reading the previous session's crash evidence and assembling a report from it.
//!
//! The counterpart to the two capture paths in `mod.rs` and `signal_handler.rs`, and the split is
//! the one the rest of this module keeps making: capture runs in a COMPROMISED context (a panic
//! hook that must not panic, a signal handler that must stay async-signal-safe), while everything
//! here runs at the next launch in an ordinary process with the full stdlib.
//!
//! That's why every field a crash report can't gather at crash time is attached here: the system
//! snapshot, the diagnostics id, macOS's own crash report, and the app's settled fate. When adding
//! a field, the question is which side of that line it belongs on, and anything that reads the
//! filesystem or allocates belongs on this one.

use std::path::Path;

use super::{
    AppFate, CACHED_SETTINGS, CRASH_FILE_VERSION, CRASH_LOOP_THRESHOLD_SECS, CRASH_SHORT_ID_PREFIX, CrashReport,
    current_build_mode, read_crash_report, write_crash_report,
};

/// Turn whatever the previous session left behind into a report the frontend can offer.
///
/// Three outcomes: a JSON report from the panic hook is topped up with the fields the hook couldn't
/// gather and left in place; a raw signal-crash file is read and converted to one; and a report
/// already delivered in-session is deleted rather than offered twice.
pub(super) fn process_pending_crash(crash_json_path: &Path, raw_crash_path: &Path) {
    if let Some(mut report) = read_crash_report(crash_json_path) {
        finish_panic_report(crash_json_path, &mut report);
        return;
    }

    #[cfg(unix)]
    if raw_crash_path.exists() {
        convert_raw_signal_crash(crash_json_path, raw_crash_path);
        let _ = std::fs::remove_file(raw_crash_path);
    }
    #[cfg(not(unix))]
    let _ = raw_crash_path;
}

/// Top up the panic hook's report with what it couldn't know, rewriting only if something changed.
fn finish_panic_report(crash_json_path: &Path, report: &mut CrashReport) {
    // Already delivered in-session, so the user has heard about this panic once. Saying it again on
    // the next launch spends trust and adds nothing. Dropped here rather than hidden in the
    // frontend, so a report nobody will be offered can't linger on disk.
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
    // The panic hook can't know the app's fate, so it writes `Unconfirmed` and `survival.rs`
    // upgrades it from a thread that can only run while the process is alive. Reaching a LATER
    // LAUNCH still unconfirmed is therefore the proof that no such upgrade happened: the app went
    // down with the panic. Resolving it here, at the one moment the evidence is conclusive, is what
    // lets the dialog pick its opening sentence from a settled value.
    if report.app_fate == AppFate::Unconfirmed {
        report.app_fate = AppFate::Ended;
        dirty = true;
    }
    // The panic hook couldn't gather the snapshot (compromised context), so attach the stable form
    // now. Only when missing, so we don't rewrite on every launch the report lingers.
    if report.system_snapshot.is_none()
        && let Some(dir) = crash_json_path.parent()
    {
        report.system_snapshot = Some(crate::diagnostics_snapshot::SystemSnapshot::collect_stable(dir));
        dirty = true;
    }
    // A panic that UNWINDS leaves macOS nothing to report, so this usually misses and costs one
    // directory scan. It hits for the aborting kind ("panic in a function that cannot unwind"),
    // which is exactly the class where our backtrace stops at the tao/wry event loop and the
    // symbolicated stack says what the app was actually doing.
    if report.os_exception.is_none()
        && report.os_frames.is_empty()
        && let Some(crash_time) = parse_timestamp(&report.timestamp)
        && attach_os_crash_report(report, crash_time)
    {
        dirty = true;
    }
    if dirty {
        let _ = write_crash_report(crash_json_path, report);
    }
}

/// Read the async-signal-safe handler's raw file and write a real report in its place.
#[cfg(unix)]
fn convert_raw_signal_crash(crash_json_path: &Path, raw_crash_path: &Path) {
    use super::{signal_handler, symbolicate};

    // The raw file is written BY THE HANDLER, mid-crash, so its mtime is when the app actually
    // died. Read it before anything else touches the file. Everything downstream wants that moment
    // rather than now: it's what matches macOS's own report to this crash, and what a later log
    // bundle scopes itself around. `now` only as a fallback, which keeps the field populated on a
    // filesystem that won't answer.
    let crash_time = std::fs::metadata(raw_crash_path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| std::time::SystemTime::now());

    let Some((signal, addresses, image_base, crash_app_version)) = signal_handler::read_raw_crash(raw_crash_path)
    else {
        return;
    };

    let current_version = env!("CARGO_PKG_VERSION");
    let backtrace_frames = if crash_app_version == current_version {
        symbolicate::symbolicate_addresses(&addresses)
    } else {
        log::info!(
            "Crash reporter: version mismatch (crash={crash_app_version}, \
             current={current_version}), sending raw addresses"
        );
        addresses.iter().map(|a| format!("0x{a:016x}")).collect()
    };

    let mut report = CrashReport {
        version: CRASH_FILE_VERSION,
        // When the app DIED, taken from the raw file the handler wrote, never the moment this next
        // launch got round to assembling the report. The gap between the two is however long the
        // machine sat closed, and everything that reads this field (the macOS-report match, the
        // crash-loop check, a log bundle's window) wants the crash.
        timestamp: to_iso8601(crash_time),
        signal: Some(signal_name(signal)),
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
        // No ambiguity on this path: SIGSEGV/SIGBUS/SIGABRT are unrecoverable, the handler re-raises
        // them, and we're reading the evidence from the NEXT launch. The app is definitively gone.
        app_fate: AppFate::Ended,
        // A signal crash is never delivered in-session: the process died in the handler.
        reported_in_session: false,
        build_mode: Some(current_build_mode().to_string()),
        short_id: Some(crate::short_id::generate(CRASH_SHORT_ID_PREFIX)),
        // Signal path: the async-signal-safe handler couldn't touch the diag id (no alloc/lock). We
        // attach it HERE, where the full stdlib is available. `email` stays `None` (send-time
        // field, set by the dialog).
        diag_id: crate::install_id::diagnostics_id(),
        email: None,
        // The data dir is the crash file's parent; the snapshot reads only index sizes and capacity.
        system_snapshot: crash_json_path
            .parent()
            .map(crate::diagnostics_snapshot::SystemSnapshot::collect_stable),
        // The base recorded BY THE CRASHED PROCESS, never this one's: ASLR gives the relaunched
        // process a different slide, which would make every offset wrong. `0` means that build
        // couldn't resolve it (non-macOS Unix).
        image_base: (image_base != 0).then(|| format!("0x{image_base:x}")),
        // Filled in just below: the constructor stays a plain description of what the raw file
        // held, and the one field that comes from outside it is attached separately.
        os_exception: None,
        os_frames: Vec::new(),
    };

    // The payoff for the signal path. `backtrace_frames` above are bare addresses; this is the same
    // stack with names on it, system frames included.
    // The report is written unconditionally on the next line either way.
    // allowed-discarded-outcome: nothing on this path branches on whether a report attached
    attach_os_crash_report(&mut report, crash_time);

    if let Err(e) = write_crash_report(crash_json_path, &report) {
        log::warn!("Crash reporter: couldn't write symbolicated crash report: {e}");
    }
}

/// Attach macOS's own view of this crash, if it wrote one. Returns whether anything was attached.
///
/// Best-effort by design: a miss is normal (the report may not be written yet when someone
/// relaunches quickly, the directory may be unreadable, or the crash may be a panic that unwound
/// and produced no OS report at all), and the crash report is still worth sending without it.
#[cfg(target_os = "macos")]
fn attach_os_crash_report(report: &mut CrashReport, crash_time: std::time::SystemTime) -> bool {
    let Some(process_name) = current_process_name() else {
        return false;
    };
    let Some(extracted) = super::os_crash_report::extract_near(&process_name, crash_time) else {
        log::debug!("Crash reporter: no matching macOS crash report for this crash");
        return false;
    };
    log::info!(
        "Crash reporter: attached macOS crash report ({} symbolicated frames)",
        extracted.frames.len()
    );
    report.os_exception = extracted.exception;
    report.os_frames = extracted.frames;
    true
}

/// `.ips` files are macOS's, so everywhere else there is nothing to attach and the report ships
/// with its own backtrace, exactly as it did before any of this existed. ❗ The whole
/// `os_crash_report` module is gated too: leaving its parser compiled-but-unreachable here is what
/// made the Linux build fail on dead code.
#[cfg(not(target_os = "macos"))]
fn attach_os_crash_report(_report: &mut CrashReport, _crash_time: std::time::SystemTime) -> bool {
    false
}

/// The name macOS files our crash reports under: the executable's own file stem (`Cmdr` in a
/// shipped build). Taken from `current_exe` rather than hardcoded so a dev build, whose binary is
/// named differently, still finds its own reports.
#[cfg(target_os = "macos")]
fn current_process_name() -> Option<String> {
    std::env::current_exe()
        .ok()?
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
}

/// A [`SystemTime`](std::time::SystemTime) as the same ISO 8601 string the capture paths write, so
/// every crash-file timestamp has one shape whatever produced it.
fn to_iso8601(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()
}

/// The inverse, for a timestamp already on disk. `None` for a value that isn't ISO 8601, which
/// callers treat as "can't place this crash in time" rather than as an error.
fn parse_timestamp(timestamp: &str) -> Option<std::time::SystemTime> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc).into())
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

/// Whether the crash timestamp says this launch is following one that died within seconds, which
/// makes the frontend ask instead of auto-sending. A malformed timestamp answers `false`: claiming
/// a crash loop we can't prove would suppress a legitimate auto-send.
pub(super) fn is_crash_loop(crash_timestamp: &str) -> bool {
    let Ok(crash_time) = chrono::DateTime::parse_from_rfc3339(crash_timestamp) else {
        return false;
    };
    let elapsed = chrono::Utc::now().signed_duration_since(crash_time);
    elapsed.num_seconds() >= 0 && (elapsed.num_seconds() as u64) < CRASH_LOOP_THRESHOLD_SECS
}
