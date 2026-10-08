//! Delivery and dismissal of the backend-authoritative pending crash report.

use super::{CRASH_FILE_NAME, CRASH_SHORT_ID_PREFIX, CrashReport, read_crash_report};
use crate::config;
use crate::managed_policy::Egress;
use crate::server_request::{self, ServerRequestError};
use std::future::Future;
use std::path::{Path, PathBuf};

/// A report a send is uploading lives under `crash-report.sending.<short id>.json` beside the
/// pending file. See [`send_pending_crash_report_from_path`].
const CLAIMED_CRASH_FILE_PREFIX: &str = "crash-report.sending.";
const CLAIMED_CRASH_FILE_SUFFIX: &str = ".json";

/// Crash-report ingestion endpoint. Debug builds keep their existing localhost target.
#[cfg(debug_assertions)]
const CRASH_REPORT_URL: &str = "http://localhost:8787/crash-report";
#[cfg(not(debug_assertions))]
const CRASH_REPORT_URL: &str = "https://api.getcmdr.com/crash-report";

/// Deletes the pending report when the user dismisses its dialog.
pub fn dismiss_pending_crash_report<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(data_dir) = config::resolved_app_data_dir(app) else {
        return;
    };
    discard_pending_at(&data_dir.join(CRASH_FILE_NAME));
}

/// Deletes the pending slot only. A claim a send holds (`crash-report.sending.<id>.json`) is a
/// different file, so this can't reach a report mid-upload.
pub(super) fn discard_pending_at(crash_path: &Path) {
    let _ = std::fs::remove_file(crash_path);
}

/// Reloads and sends the backend-owned pending report identified by the preview's short id.
///
/// The frontend supplies consent, not report contents. Reloading here makes the file authoritative,
/// and comparing its id before upload prevents a stale preview from sending a replacement report.
/// The send then CLAIMS the file (renames it out of the pending slot) before uploading, so a crash
/// written meanwhile gets a fresh file, and the delete after upload can only reach the claimed one.
/// A request that doesn't land puts the report back, unless a newer crash took the slot meanwhile:
/// then the claim waits for a later launch.
pub async fn send_pending_crash_report<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    report_id: &str,
    email: Option<crate::error_reporter::AttachedEmail>,
) -> Result<(), ServerRequestError> {
    let data_dir = config::resolved_app_data_dir(app)
        .map_err(|e| ServerRequestError::unexpected(format!("resolve crash-report directory: {e}")))?;
    let crash_path = data_dir.join(CRASH_FILE_NAME);
    let should_skip = cfg!(debug_assertions) || cfg!(feature = "playwright-e2e") || std::env::var("CI").is_ok();

    send_pending_crash_report_from_path(&crash_path, report_id, email, should_skip, |report| async move {
        post_crash_report(CRASH_REPORT_URL, &report).await
    })
    .await
}

pub(super) async fn send_pending_crash_report_from_path<Upload, UploadFuture>(
    crash_path: &Path,
    report_id: &str,
    email: Option<crate::error_reporter::AttachedEmail>,
    should_skip: bool,
    upload: Upload,
) -> Result<(), ServerRequestError>
where
    Upload: FnOnce(CrashReport) -> UploadFuture,
    UploadFuture: Future<Output = Result<(), ServerRequestError>>,
{
    // Before the claim: a blocked send leaves the pending file exactly where it was.
    server_request::check_policy(Egress::CrashReport).await?;
    if pending_report_id(crash_path).as_deref() != Some(report_id) {
        return Err(ServerRequestError::unexpected(
            "pending crash report changed before send",
        ));
    }
    // A plain rename can't clobber anything here: the claim's name carries the report's id, and
    // ids are unique, so a claim still waiting from an earlier launch keeps its own name.
    let claimed = claimed_crash_path(crash_path, report_id);
    std::fs::rename(crash_path, &claimed)
        .map_err(|e| ServerRequestError::unexpected(format!("claim pending crash report: {e}")))?;

    // A crash could have replaced the pending file between the id check and the rename, so what
    // we claimed may be someone else's report. It goes back unsent.
    let Some(mut report) = read_crash_report(&claimed).filter(|r| r.short_id.as_deref() == Some(report_id)) else {
        release_claim(&claimed, crash_path);
        return Err(ServerRequestError::unexpected(
            "pending crash report changed before send",
        ));
    };
    report.prepare_for_delivery();
    report.prepare_for_send(email);

    if should_skip {
        log::info!("Crash reporter: skipping send (debug build, E2E build, or CI)");
    } else if let Err(e) = upload(report).await {
        release_claim(&claimed, crash_path);
        return Err(e);
    }

    let _ = std::fs::remove_file(&claimed);
    Ok(())
}

/// Moves a claim back into the pending slot WITHOUT replacing whatever holds it: a hard link fails
/// on an existing name, where a rename would overwrite a crash written while the claim was out. A
/// claim that can't go back stays where it is, and a later launch retries.
fn release_claim(claimed: &Path, crash_path: &Path) {
    match std::fs::hard_link(claimed, crash_path) {
        Ok(()) => {
            let _ = std::fs::remove_file(claimed);
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            log::info!("Crash reporter: a newer crash report holds the pending slot, keeping the unsent one for later");
        }
        Err(e) => log::warn!("Crash reporter: couldn't put an unsent crash report back: {e}"),
    }
}

/// Where a send parks the report `report_id` while it uploads, beside `crash_path`. `report_id` must
/// already be a validated short id (it names a file).
pub(super) fn claimed_crash_path(crash_path: &Path, report_id: &str) -> PathBuf {
    crash_path.with_file_name(format!(
        "{CLAIMED_CRASH_FILE_PREFIX}{report_id}{CLAIMED_CRASH_FILE_SUFFIX}"
    ))
}

/// Puts back a report whose send never finished (the app crashed or quit mid-upload, or a newer
/// crash held the slot when its upload failed), so it's offered again. Runs at launch, first thing
/// in `process_pending_crash`. The slot holds one report: while it's taken, a claim keeps waiting for
/// a later launch, and a claim that duplicates the pending report (same id) is dropped.
pub(super) fn restore_unfinished_sends(crash_path: &Path) {
    let Some(dir) = crash_path.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut claims: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
                name.starts_with(CLAIMED_CRASH_FILE_PREFIX) && name.ends_with(CLAIMED_CRASH_FILE_SUFFIX)
            })
        })
        .collect();
    claims.sort();
    for claim in claims {
        if pending_report_id(&claim).is_some_and(|id| pending_report_id(crash_path) == Some(id)) {
            let _ = std::fs::remove_file(&claim);
        } else {
            release_claim(&claim, crash_path);
        }
    }
}

pub(super) fn pending_report_id(path: &Path) -> Option<String> {
    read_crash_report(path)?
        .short_id
        .filter(|id| crate::short_id::matches(CRASH_SHORT_ID_PREFIX, id))
}

/// POSTs one backend-owned report to `url`. Split from the send flow so tests can point it at a
/// mock server without weakening the file/id boundary above.
pub(super) async fn post_crash_report(url: &str, report: &CrashReport) -> Result<(), ServerRequestError> {
    let client = cmdr_http::client_builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| ServerRequestError::unexpected(format!("HTTP client: {e}")))?;
    server_request::send(Egress::CrashReport, client.post(url).json(report)).await?;
    Ok(())
}
