//! macOS: NSPasteboard (or the E2E mock behind the same fns), read and written on the AppKit
//! main thread, since NSPasteboard isn't thread-safe.

use std::path::PathBuf;

use cmdr_operations::MutationError;
use objc2::MainThreadMarker;
use tauri::AppHandle;

use super::super::PasteClipboardReply;

/// Writes `paths` as file URLs (and their newline-joined text) on the main thread.
pub(crate) fn write_file_urls(app: AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = super::super::write_file_urls_to_clipboard(mtm, &paths);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))?
}

/// Reads the file URLs on the main thread.
pub(crate) fn read_file_urls(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let result = super::super::read_file_urls_from_clipboard(mtm);
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))?
}

/// Reads the plain text on the main thread.
pub(crate) fn read_text(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let text = super::super::read_text_from_clipboard(mtm);
        let _ = tx.send(text);
    })
    .map_err(|e| format!("Couldn't run on main thread: {e}"))?;

    rx.recv()
        .map_err(|e| format!("Couldn't receive pasteboard result: {e}"))
}

/// Reads the RAW flavors on the main thread, picks the highest-intent one and converts
/// TIFF→PNG off it, and writes the file.
pub(crate) async fn paste_as_file(
    app: AppHandle,
    volume_id: Option<String>,
    directory: String,
) -> Result<PasteClipboardReply, MutationError> {
    // 1. Read the RAW flavors on the main thread (NSPasteboard requires it). This
    // does the minimum on-main: just copies bytes off the pasteboard.
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
        let data = super::super::read_pasteboard_data(mtm);
        let _ = tx.send(data);
    })
    .map_err(|e| MutationError::Unexpected {
        detail: format!("the main-thread hop didn't run: {e}").into(),
    })?;

    let data = rx.recv().map_err(|e| MutationError::Unexpected {
        detail: format!("the pasteboard read didn't come back: {e}").into(),
    })?;

    // 2. Pick the highest-intent flavor OFF the main thread, in a blocking task —
    // the TIFF→PNG decode is CPU-bound and can be hundreds of ms, so it must not
    // sit on the UI thread OR block the async reactor.
    let payload = tokio::task::spawn_blocking(move || super::super::pick_clipboard_payload(data))
        .await
        .map_err(|e| MutationError::Unexpected {
            detail: format!("the clipboard decode task didn't finish: {e}").into(),
        })?;

    // Tilde-expand for the local `root` volume only; volume paths are volume-relative.
    let expanded = if volume_id.as_deref().unwrap_or("root") == "root" {
        crate::commands::file_system::expand_tilde(&directory)
    } else {
        directory
    };

    // 3. Write, detached: a slow write (a large image onto a busy share) answers
    // `StillRunning` and reports its end on `clipboard-paste-settled`, never a
    // timeout that drops the write mid-flight.
    super::super::write_payload_replying(
        volume_id,
        PathBuf::from(expanded),
        payload,
        crate::commands::settled::broadcast_settled(app, std::convert::identity),
    )
    .await
}
