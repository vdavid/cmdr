//! Reads macOS's own crash report for a crash we just recorded, and lifts the two things it has
//! that we can't produce ourselves: the exception subtype, and a symbolicated stack.
//!
//! Our signal handler can only capture raw instruction-pointer addresses (it has to stay
//! async-signal-safe), and the frames that matter in a native crash are usually in WebKit or
//! AppKit, where our own symbols would be no help even with an `imageBase`. macOS writes a fully
//! symbolicated report to `~/Library/Logs/DiagnosticReports/` for the same crash, because our
//! handler installs with `SA_RESETHAND` and ends in `raise(sig)`, so the default disposition runs
//! and `ReportCrash` does its work. This module reads that file at the next launch.
//!
//! # What we take, and what we leave
//!
//! An allowlist, never a denylist: a future macOS can add a field, and a denylist would ship it.
//! We take the `exception` block and the faulting thread's frames, rendered to
//! `"<image> <symbol> + <offset>"` strings against the report's own image table. Everything else
//! stays on the user's disk, including the parts that identify a machine or name other apps:
//! `crashReporterKey` (stable per-machine), `bootSessionUUID`, `sleepWakeUUID`, `incident_id`,
//! `userID`, and `parentProc` / `responsibleProc` / `coalitionName`, which name whatever launched
//! Cmdr (a terminal, a launcher) rather than Cmdr itself.
//!
//! Rendering the frames here rather than shipping the report's JSON is what keeps the payload
//! small and the PII surface flat: no image path list travels, because the image table is consumed
//! to produce the names and then discarded. Every rendered line still goes through
//! [`crate::redact`], which costs nothing and covers a symbol that somehow embeds a path.
//!
//! Gotcha: macOS already collapses home paths in a third-party app's report (`procPath` reads
//! `/Users/USER/*/Cmdr`), so the redactor is insurance here, not the load-bearing defense. Don't
//! let that tempt anyone into widening the allowlist; the fields above are not path-shaped and the
//! redactor would pass them straight through (verified against a real `Cmdr-*.ips` on macOS 26.6.2,
//! 2026-09-18).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How far from the recorded crash time a report may sit and still be the same crash. `ReportCrash`
/// writes within seconds, so this is slack for a busy machine rather than a real window. Anything
/// wider risks attaching the previous crash's stack to this report, which is worse than no stack.
const MATCH_WINDOW: Duration = Duration::from_secs(120);

/// Frames kept from the faulting thread. Deep enough for the whole run-loop path in a WebKit crash
/// (the real ones run around 20 to 35), shallow enough that the report stays far inside the
/// server's 64 KB payload cap.
const MAX_FRAMES: usize = 40;

/// Per-frame character cap. C++ symbols carry full template expansions and a handful of them would
/// otherwise dominate the payload.
const MAX_FRAME_CHARS: usize = 300;

/// Biggest `.ips` we'll read. Real ones run 20 KB to 500 KB (most of it threads we don't keep);
/// the cap is only here so a pathological file can't be pulled into memory at launch.
const MAX_IPS_BYTES: u64 = 8 * 1024 * 1024;

/// What we lift out of one macOS crash report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsCrashReport {
    /// One line: `"EXC_BAD_ACCESS (SIGSEGV), KERN_INVALID_ADDRESS at 0x0000000000000010"`. The
    /// subtype is the single most diagnostic field in the whole file: a fault at a small address
    /// is a null dereference at that struct offset, which our own report can never say.
    pub exception: Option<String>,
    /// The faulting thread's frames, outermost call last, already rendered and redacted.
    pub frames: Vec<String>,
}

/// Find macOS's crash report for `process_name` written around `crash_time`, and extract it.
///
/// `None` whenever anything is missing or unreadable, which is a normal outcome rather than a
/// problem: the directory may be unreadable, `ReportCrash` may not have finished writing yet when
/// the user relaunches quickly, or the crash may predate this code. The caller still has its own
/// raw addresses and `imageBase`.
pub fn extract_near(process_name: &str, crash_time: SystemTime) -> Option<OsCrashReport> {
    let dir = diagnostic_reports_dir()?;
    let path = find_report(&dir, process_name, crash_time)?;
    let metadata = std::fs::metadata(&path).ok()?;
    if metadata.len() > MAX_IPS_BYTES {
        log::debug!("Crash reporter: macOS crash report is too large to read, skipping it");
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    parse_report(&text)
}

fn diagnostic_reports_dir() -> Option<PathBuf> {
    // `$HOME` rather than a crate: this runs at launch on the startup path, and the per-user
    // directory has been at this exact location for every macOS we support.
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Logs/DiagnosticReports"))
}

/// The `.ips` closest in time to `crash_time`, among those `ReportCrash` wrote for `process_name`.
///
/// Matching is on the file's mtime, not the timestamp inside the name: the name's time is local
/// and format-fragile, while the mtime is exactly when `ReportCrash` finished. The closest match
/// wins rather than the first, so a user who crashed twice in a minute still gets the right one.
/// ❌ Never fall back to "the newest report for this process": that silently attaches an unrelated
/// stack to the report, and a wrong stack costs more than a missing one.
fn find_report(dir: &Path, process_name: &str, crash_time: SystemTime) -> Option<PathBuf> {
    let prefix = format!("{process_name}-");
    let mut best: Option<(Duration, PathBuf)> = None;

    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ips") {
            continue;
        }
        // `Cmdr-2026-09-18-113458.ips`. The prefix also keeps us away from the reports macOS files
        // for our helper processes (`cmdr_lib-<hash>-….ips`), whose stacks are a different crash.
        if path
            .file_name()
            .is_none_or(|n| !n.to_string_lossy().starts_with(&prefix))
        {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let distance = modified
            .duration_since(crash_time)
            .or_else(|_| crash_time.duration_since(modified))
            .unwrap_or(Duration::MAX);
        if distance > MATCH_WINDOW {
            continue;
        }
        if best.as_ref().is_none_or(|(best_distance, _)| distance < *best_distance) {
            best = Some((distance, path));
        }
    }

    best.map(|(_, path)| path)
}

/// Parse one `.ips` and pull the allowlisted parts out of it.
///
/// The file is two JSON documents: a one-line header, then the body. We only need the body, and it
/// is ordinary JSON (the raw control characters some tools report are an artifact of `echo`
/// re-expanding `\n`, not of the file).
fn parse_report(text: &str) -> Option<OsCrashReport> {
    let body = text.split_once('\n')?.1;
    let root: serde_json::Value = serde_json::from_str(body).ok()?;

    let exception = render_exception(root.get("exception"));
    let frames = render_faulting_thread(&root);

    // A report that yielded neither is not worth a field on the wire.
    (exception.is_some() || !frames.is_empty()).then_some(OsCrashReport { exception, frames })
}

/// `"EXC_BAD_ACCESS (SIGSEGV), KERN_INVALID_ADDRESS at 0x…"`, dropping whichever parts are absent.
fn render_exception(exception: Option<&serde_json::Value>) -> Option<String> {
    let exception = exception?;
    let kind = exception.get("type").and_then(|v| v.as_str());
    let signal = exception.get("signal").and_then(|v| v.as_str());
    let subtype = exception.get("subtype").and_then(|v| v.as_str());

    let head = match (kind, signal) {
        (Some(kind), Some(signal)) => format!("{kind} ({signal})"),
        (Some(one), None) | (None, Some(one)) => one.to_string(),
        (None, None) => String::new(),
    };
    let line = match (head.is_empty(), subtype) {
        (false, Some(subtype)) => format!("{head}, {subtype}"),
        (false, None) => head,
        (true, Some(subtype)) => subtype.to_string(),
        (true, None) => return None,
    };
    Some(redact_and_cap(&line))
}

/// The faulting thread's frames, rendered against the report's image table.
fn render_faulting_thread(root: &serde_json::Value) -> Vec<String> {
    let images = root.get("usedImages").and_then(|v| v.as_array());
    let threads = root.get("threads").and_then(|v| v.as_array());
    let (Some(images), Some(threads)) = (images, threads) else {
        return Vec::new();
    };
    // Absent `faultingThread` means the report isn't about a fault on a specific thread. Thread 0
    // would be a guess, and a guessed stack is the thing this module must not produce.
    let Some(index) = root.get("faultingThread").and_then(|v| v.as_u64()) else {
        return Vec::new();
    };
    let Some(thread) = threads.get(index as usize) else {
        return Vec::new();
    };
    let Some(frames) = thread.get("frames").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    frames
        .iter()
        .take(MAX_FRAMES)
        .map(|frame| render_frame(frame, images))
        .collect()
}

/// One frame as `"<image> <symbol> + <offset>"`, falling back to the image-relative offset when
/// the frame carries no symbol (an unsymbolicated image, which is what our own binary may be).
fn render_frame(frame: &serde_json::Value, images: &[serde_json::Value]) -> String {
    let image = frame
        .get("imageIndex")
        .and_then(|v| v.as_u64())
        .and_then(|i| images.get(i as usize))
        .and_then(|image| image.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let rendered = match frame.get("symbol").and_then(|v| v.as_str()) {
        Some(symbol) => {
            let location = frame.get("symbolLocation").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("{image} {symbol} + {location}")
        }
        None => {
            let offset = frame.get("imageOffset").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("{image} + {offset}")
        }
    };
    redact_and_cap(&rendered)
}

/// Redact, then cap by CHARS rather than bytes: a byte-index cut inside a multi-byte symbol would
/// panic, and this code runs on the crash-reporting path where a panic is the last thing we want.
fn redact_and_cap(line: &str) -> String {
    let redacted = crate::redact::redact_line(line);
    if redacted.chars().count() <= MAX_FRAME_CHARS {
        return redacted.into_owned();
    }
    redacted.chars().take(MAX_FRAME_CHARS).collect()
}

#[cfg(test)]
#[path = "os_crash_report_tests.rs"]
mod tests;
