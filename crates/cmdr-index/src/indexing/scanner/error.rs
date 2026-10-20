//! [`ScanError`], why a local walk stopped short, and the one text a log line marks its paths in.

use std::path::PathBuf;

use cmdr_fs::log_error::{ErrorLogText, ErrorText, write_plain};
use cmdr_fs::log_private::LogPrivate;

use super::ScanSummary;

/// Errors that can occur during scanning.
#[derive(Debug)]
pub enum ScanError {
    Io(std::io::Error),
    WriterSend(String),
    /// A subtree walk's root doesn't cross into the volume's index path space, so the index has
    /// no entry to walk it from. Carries the root, typed so a log line marks it.
    OutsideIndex(PathBuf),
    /// The volume ROOT listing SUCCEEDED but returned zero children, so a
    /// reconcile rescan would see an empty live tree and delete every existing
    /// child (blanking the index). Surfaced by the LOCAL reconcile walker
    /// (`local_reconcile`) before it diffs the root, so the completion handler
    /// takes its `Err` arm and writes NO `scan_completed_at`: the prior
    /// stale-but-real index is kept and heals on the next launch. Mirrors the
    /// network path's `VolumeScanError::EmptyRoot`; see
    /// `indexing/DETAILS.md` § "No completion marker on an empty root".
    EmptyRoot,
    /// The volume ROOT itself couldn't be listed (its read errored or timed out),
    /// which for a mount-rooted external drive means the mount VANISHED mid-scan (a
    /// yanked USB stick / SD card). Distinct from [`EmptyRoot`](ScanError::EmptyRoot):
    /// an empty-but-readable root lists successfully (zero children), whereas a
    /// vanished root can't be read at all. The completion handler treats this as an
    /// aborted scan — it writes NO `scan_completed_at` and emits `index-scan-aborted`
    /// so the frontend clears the stuck "scanning" row — mirroring the network path's
    /// disconnect arm. Surfaced by the fresh guarded-walker scan (`run_scan`, when the root is
    /// the only dir and never read, or when the drive left the host's mount table by
    /// the end of the walk) and the LOCAL reconcile walk (`local_reconcile`, when its
    /// root read returns `None` or its drive left).
    RootUnlistable,
    /// The walk stopped because its cancellation token fired, carrying the
    /// partial totals it had reached. A distinct variant rather than a flag on
    /// the summary: `scan_completed_at` is written only on the `Ok` path, so a
    /// caller cannot mark a half-built index complete by forgetting to check.
    /// It is NOT a failure — the completion handlers route it to its own arm,
    /// which keeps the prior freshness and fires no abort. See
    /// `lifecycle/scan_completion.rs`.
    Cancelled(ScanSummary),
    /// A [`ScanRoot::Virgin`](super::ScanRoot::Virgin) walk was pointed at a directory the index already
    /// holds children for, so the add-only walk it wanted to run isn't safe: its
    /// fresh ids would collide with the existing rows, `INSERT OR IGNORE` would
    /// drop them silently, and everything below a dropped id would be orphaned.
    /// NOT a failure — the caller repairs the directory with the serial
    /// reconcile, which compares by name and writes only differences.
    NotVirgin,
    /// The reconcile walk panicked. `local_reconcile::start_local_reconcile`
    /// wraps the walk in `catch_unwind` and converts the panic payload into this
    /// typed variant (carrying the panic message), so the thread's `JoinHandle`
    /// resolves to `Ok(Err(ScanError::Panicked(_)))` instead of a raw thread
    /// panic. That routes it through the completion handler's `Ok(Err(_))` arm,
    /// which logs cleanly and fires `FreshnessEvent::ScanFailed` ⇒ Stale.
    Panicked(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_plain(self, f)
    }
}

/// The one text behind `Display` and `LogError`: a root outside the index is marked.
impl ErrorLogText for ScanError {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        match self {
            ScanError::Io(e) => write!(out, "I/O error: {e}"),
            ScanError::WriterSend(msg) => write!(out, "Writer send failed: {msg}"),
            ScanError::OutsideIndex(root) => {
                out.value(LogPrivate::dir_path(root))?;
                out.write_str(" is outside the volume's index")
            }
            ScanError::EmptyRoot => out.write_str("root listing returned no children (treating as a failed rescan)"),
            ScanError::RootUnlistable => out.write_str("volume root became unlistable (mount vanished mid-scan)"),
            ScanError::Cancelled(s) => write!(
                out,
                "scan cancelled after {} entries in {} dirs",
                s.total_entries, s.total_dirs
            ),
            ScanError::NotVirgin => out.write_str("the scan root already has children, so an add-only walk isn't safe"),
            ScanError::Panicked(msg) => write!(out, "reconcile walk panicked: {msg}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<std::io::Error> for ScanError {
    fn from(err: std::io::Error) -> Self {
        ScanError::Io(err)
    }
}
