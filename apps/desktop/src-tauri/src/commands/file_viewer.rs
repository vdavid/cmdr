//! Tauri commands for the file viewer.

use std::sync::Arc;

use tauri_specta::Event as _;
use tokio::time::Duration;

use super::util::{StallWatch, blocking_typed_result_until_stalled, blocking_typed_result_with_timeout};
use crate::file_viewer::{
    self, AbandonReason, EncodingOptions, FileEncoding, LineChunk, PendingOpen, RangeEnd, SearchMode, SearchPollResult,
    SeekTarget, SeekTargetKind, ViewerError, ViewerOpenResult, ViewerPullProgress, ViewerSessionStatus,
};
use log::debug;
use tauri::Manager;
#[cfg(not(target_os = "macos"))]
use tauri::menu::MenuItemKind;

const VIEWER_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a pull into the preview temp may go without a byte before the open answers
/// `StoppedResponding`. A pull has no total limit: the viewer window shows its progress,
/// and closing the window stops it.
///
/// Above every backend's own read timeout, so a backend that notices a dead connection
/// answers first with its own error: MTP gives one USB transfer 30 s
/// (`USB_TRANSFER_TIMEOUT_SECS`), WebDAV allows 10 s between body chunks
/// (`REQUEST_BUDGET`). ADB's sync stream and SFTP's read window carry no read timeout of
/// their own (verified in code, 2026-09-10), so for them this is the only guard. The
/// 15 s over MTP's limit leaves room for a phone that answers a window slowly but does
/// answer.
const PULL_STALL_LIMIT: Duration = Duration::from_secs(45);

/// Maximum read timeout for `viewer_read_range`. The 100 MiB hard ceiling (enforced
/// at the FE) means even on a slow disk we shouldn't blow this. The backend's per-read
/// cancel flag covers the actually-stuck case via Escape.
const READ_RANGE_TIMEOUT: Duration = Duration::from_secs(60);

/// Opens a viewer session for the given file.
/// Returns session metadata + initial lines from the start of the file.
///
/// `window_label` is the opening viewer window's label (`viewer-<timestamp>`).
/// It links the window to the session so the Rust window-destroyed handler can
/// free the session when the user closes the window via the titlebar X (a path
/// that never fires the FE `viewer_close` IPC). Pass an empty string when there's
/// no owning window (no mapping is recorded).
///
/// An open that pulls the file into a temp first (a `.zip` entry, a `.git` blob, a
/// file on a phone or server) reports `viewer-pull-progress` to that window and has no
/// total deadline, only the stall rule (`PULL_STALL_LIMIT`). Any other open keeps the
/// strict 2 s read tier. Errors stay the typed `ViewerError`, so the FE words each
/// variant (`TooLargeToPreview`, `Archive`, `StoppedResponding`, `TimedOut`).
#[tauri::command]
#[specta::specta]
pub async fn viewer_open(
    app_handle: tauri::AppHandle,
    path: String,
    volume_id: String,
    window_label: String,
) -> Result<ViewerOpenResult, ViewerError> {
    open_viewer(app_handle, path, volume_id, window_label, /*force_text=*/ false).await
}

/// The shared body of `viewer_open` and `viewer_open_as_text`.
///
/// Every open, pulling or not, is registered as the window's pending open for its
/// whole run: the window can close before the open resolves, and the session it may
/// still build must then be closed rather than leaked.
///
/// `open_may_materialize` is a string check plus a registry lookup, no I/O: over-
/// granting the pull path to a mislabeled `.zip` is harmless, since the stall rule
/// only ever gives up on an open that went quiet.
async fn open_viewer(
    app_handle: tauri::AppHandle,
    path: String,
    volume_id: String,
    window_label: String,
    force_text: bool,
) -> Result<ViewerOpenResult, ViewerError> {
    let open = file_viewer::begin_pending_open(&window_label);
    let expanded = crate::commands::file_system::expand_tilde(&path);
    let result = if file_viewer::materialize::open_may_materialize(std::path::Path::new(&expanded), &volume_id) {
        let target = window_label.clone();
        let emit = move |progress: ViewerPullProgress| {
            if !target.is_empty() {
                // Best-effort: a window that closed mid-pull has no one left to tell.
                let _ = progress.emit_to(&app_handle, target.as_str());
            }
        };
        let label = window_label.clone();
        open_pulling(&open, path, volume_id, label, force_text, PULL_STALL_LIMIT, emit).await
    } else {
        let work_open = Arc::clone(&open);
        let work_label = window_label.clone();
        let result = blocking_typed_result_with_timeout(
            VIEWER_TIMEOUT,
            || ViewerError::TimedOut,
            |message| ViewerError::Io { message },
            move || file_viewer::open_for_window(&path, &volume_id, &work_label, force_text, &work_open),
        )
        .await;
        if matches!(result, Err(ViewerError::TimedOut)) {
            // The window offers Retry, which starts a fresh open; a session this one
            // builds late would have no one to show it to.
            open.abandon(AbandonReason::Cancelled);
        }
        result
    };
    file_viewer::end_pending_open(&window_label, &open);
    result
}

/// Runs `open`, which may pull its file, watched for stalls, handing each change in
/// progress to `emit`. No `AppHandle`, so tests drive it directly.
async fn open_pulling(
    open: &Arc<PendingOpen>,
    path: String,
    volume_id: String,
    window_label: String,
    force_text: bool,
    stall_limit: Duration,
    emit: impl FnMut(ViewerPullProgress) + Send,
) -> Result<ViewerOpenResult, ViewerError> {
    let mut watch = PullWatch {
        open: Arc::clone(open),
        emit,
        last_emitted: None,
    };
    let work_open = Arc::clone(open);
    blocking_typed_result_until_stalled(
        stall_limit,
        &mut watch,
        |message| ViewerError::Io { message },
        move || file_viewer::open_for_window(&path, &volume_id, &window_label, force_text, &work_open),
    )
    .await
}

/// Watches one pulling open for [`blocking_typed_result_until_stalled`].
struct PullWatch<F> {
    open: Arc<PendingOpen>,
    emit: F,
    /// What the window last heard, so a poll with no new bytes sends nothing.
    last_emitted: Option<ViewerPullProgress>,
}

impl<F: FnMut(ViewerPullProgress)> StallWatch<ViewerError> for PullWatch<F> {
    fn idle_for(&self) -> Duration {
        self.open.idle_for()
    }

    fn on_poll(&mut self) {
        if let Some(progress) = self.open.pull_progress()
            && self.last_emitted != Some(progress)
        {
            (self.emit)(progress);
            self.last_emitted = Some(progress);
        }
    }

    fn give_up(&mut self) -> Option<ViewerError> {
        // An open already abandoned (its window closed) keeps that first reason.
        if self.open.abandon(AbandonReason::StoppedResponding) {
            self.open.abandoned_error()
        } else {
            None
        }
    }
}

/// Opens a fresh, full **text** session for `path`, ignoring media classification.
///
/// Backs the viewer's "View as text" override: a media (Image/PDF) session isn't
/// upgraded in place; the FE calls this, swaps to the returned text session, and closes
/// the old one. Re-registers the window -> session link so the window-destroyed handler
/// frees the new session.
#[tauri::command]
#[specta::specta]
pub async fn viewer_open_as_text(
    app_handle: tauri::AppHandle,
    path: String,
    volume_id: String,
    window_label: String,
) -> Result<ViewerOpenResult, ViewerError> {
    open_viewer(app_handle, path, volume_id, window_label, /*force_text=*/ true).await
}

/// Fetches a range of lines from a viewer session.
///
/// # Arguments
/// * `session_id` - The session ID from `viewer_open`.
/// * `target_kind` - Which of the three seeks `target_value` means.
/// * `target_value` - The seek value (line number, byte offset, or fraction 0.0-1.0).
/// * `count` - Number of lines to fetch.
#[tauri::command]
#[specta::specta]
pub async fn viewer_get_lines(
    session_id: String,
    target_type: SeekTargetKind,
    target_value: f64,
    count: usize,
) -> Result<LineChunk, ViewerError> {
    let target = match target_type {
        SeekTargetKind::Line => SeekTarget::Line(target_value as usize),
        SeekTargetKind::Byte => SeekTarget::ByteOffset(target_value as u64),
        SeekTargetKind::Fraction => SeekTarget::Fraction(target_value),
    };

    debug!(
        "viewer_get_lines: session={}, target_type={:?}, target_value={}, count={}",
        session_id, target_type, target_value, count
    );

    let result = blocking_typed_result_with_timeout(
        VIEWER_TIMEOUT,
        || ViewerError::TimedOut,
        |message| ViewerError::Io { message },
        move || file_viewer::get_lines(&session_id, target, count),
    )
    .await?;

    debug!(
        "viewer_get_lines: returned {} lines, first_line_number={}, byte_offset={}, first_line_preview={:?}",
        result.lines.len(),
        result.first_line_number,
        result.byte_offset,
        result.lines.first().map(|s| s.chars().take(50).collect::<String>())
    );

    Ok(result)
}

/// Starts a background search in the viewer session.
/// Poll with `viewer_search_poll` to get results.
///
/// `mode` carries the case-sensitivity and literal-vs-regex toggles. Invalid regex
/// patterns and multiline patterns surface via `viewer_search_poll` as
/// `SearchStatus::InvalidQuery`, not as a command-level error: the session moves
/// into a "you typed something the engine can't run" state, and the FE renders the
/// typed message.
#[tauri::command]
#[specta::specta]
pub fn viewer_search_start(session_id: String, query: String, mode: SearchMode) -> Result<(), String> {
    if query.is_empty() {
        return Err("Search query cannot be empty".to_string());
    }
    file_viewer::search_start(&session_id, query, mode).map_err(|e| e.to_string())
}

/// Polls search progress and new matches since `since_index`.
#[tauri::command]
#[specta::specta]
pub fn viewer_search_poll(session_id: String, since_index: usize) -> Result<SearchPollResult, String> {
    file_viewer::search_poll(&session_id, since_index).map_err(|e| e.to_string())
}

/// Cancels an ongoing search.
#[tauri::command]
#[specta::specta]
pub fn viewer_search_cancel(session_id: String) -> Result<(), String> {
    file_viewer::search_cancel(&session_id).map_err(|e| e.to_string())
}

/// Gets the current status of a viewer session (backend type, indexing state).
#[tauri::command]
#[specta::specta]
pub fn viewer_get_status(session_id: String) -> Result<ViewerSessionStatus, String> {
    file_viewer::get_session_status(&session_id).map_err(|e| e.to_string())
}

/// Closes a viewer session and frees resources.
#[tauri::command]
#[specta::specta]
pub fn viewer_close(session_id: String) -> Result<(), String> {
    file_viewer::close_session(&session_id).map_err(|e| e.to_string())
}

/// Reads a logical range of the file (`anchor` to `focus`) and returns the bytes as a
/// UTF-8 string. Endpoints are normalised internally; either may be `Eof` (used by ⌘A
/// in ByteSeek-no-index mode where the FE doesn't know `totalLines`). Offsets on the
/// wire are UTF-16 code units; the backend clamps lone surrogates to the nearest
/// codepoint boundary.
///
/// Errors come through the typed `ViewerError` enum. The FE matches on the variant tag
/// (per the no-string-classification rule); `Cancelled` and `TimedOut` are the two the
/// copy flow specifically handles.
#[tauri::command]
#[specta::specta]
pub async fn viewer_read_range(
    session_id: String,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
) -> Result<String, ViewerError> {
    match tokio::time::timeout(
        READ_RANGE_TIMEOUT,
        tokio::task::spawn_blocking(move || file_viewer::read_range(&session_id, read_id, anchor, focus)),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(ViewerError::Io {
            message: join_err.to_string(),
        }),
        Err(_) => Err(ViewerError::TimedOut),
    }
}

/// Flips the cancel flag for an in-flight range read. The reader sees the flag at its
/// next per-chunk check and returns `ViewerError::Cancelled`. If the read has already
/// finished, this is a no-op.
#[tauri::command]
#[specta::specta]
pub fn viewer_cancel_read(session_id: String, read_id: u64) -> Result<(), ViewerError> {
    file_viewer::cancel_read(&session_id, read_id)
}

/// Reads a logical range and writes it to `dest_path` atomically (temp+rename). Used
/// by the "Save as file…" action in the > 100 MB refuse dialog and the 10 to 100 MB
/// confirm dialog. Cancellation works the same as `viewer_read_range`.
#[tauri::command]
#[specta::specta]
pub async fn viewer_write_range_to_file(
    session_id: String,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: String,
) -> Result<(), ViewerError> {
    // The source may be a routed preview temp (it writes fine via `std::fs` off the
    // open session), but the DESTINATION must not be a path a ROUTE serves: inside a
    // `.zip`, or inside a repo's virtual `.git` trees. Neither has a directory on
    // disk for the write to land in, so without this the save falls through to a raw
    // `std::fs` errno the user can make nothing of. Saving over the `.zip` file
    // itself is a normal overwrite (allowed); only a path INSIDE one is refused.
    // Typed error, the same gate the write-path guards use.
    if crate::file_system::volume::manager::path_routes_over_its_parent(std::path::Path::new(
        &crate::commands::file_system::expand_tilde(&dest_path),
    )) {
        return Err(ViewerError::DestinationIsReadOnly);
    }
    match tokio::time::timeout(
        READ_RANGE_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            file_viewer::write_range_to_file(&session_id, read_id, anchor, focus, std::path::Path::new(&dest_path))
        }),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(ViewerError::Io {
            message: join_err.to_string(),
        }),
        Err(_) => Err(ViewerError::TimedOut),
    }
}

/// Returns the encoding dropdown payload: current selection, detected encoding, and the
/// full list of selectable encodings (with their labels and groups). The FE renders the
/// dropdown directly from this payload — no encoding list lives on the FE.
#[tauri::command]
#[specta::specta]
pub fn viewer_get_encoding_options(session_id: String) -> Result<EncodingOptions, String> {
    file_viewer::get_encoding_options(&session_id).map_err(|e| e.to_string())
}

/// Runs a filesystem-touching viewer op off the IPC handler thread (`spawn_blocking`)
/// under a timeout. Without this, a synchronous reopen / encoding swap / tail catch-up
/// scan on a slow disk would block the viewer window's IPC thread and freeze the other
/// in-flight calls (scroll, search) behind it. Mirrors `blocking_result_with_timeout`
/// but keeps the plain `String` error the FE call sites already expect. On timeout the
/// detached blocking task still finishes its work; the next FS event or reload settles
/// the backend.
async fn blocking_viewer_op<F>(op: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    match tokio::time::timeout(VIEWER_TIMEOUT, tokio::task::spawn_blocking(op)).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(join_err.to_string()),
        Err(_) => Err("Viewer operation timed out".to_string()),
    }
}

/// Switches the active encoding for a session. Returns immediately; if the swap
/// requires a background reindex (most cases except UTF-8 ↔ Windows-1252-family),
/// the FE polls `viewer_get_status` for `is_indexing` to track completion.
#[tauri::command]
#[specta::specta]
pub async fn viewer_set_encoding(session_id: String, encoding: FileEncoding) -> Result<(), String> {
    blocking_viewer_op(move || file_viewer::set_encoding(&session_id, encoding).map_err(|e| e.to_string())).await
}

/// Toggles tail mode for a viewer session. When enabled, the backend extends its line index
/// in response to filesystem `Grew` events so the viewport can auto-follow new bytes.
/// When disabled, the FE still receives `viewer:file-changed:<sid>` events and renders a
/// persistent reload toast.
#[tauri::command]
#[specta::specta]
pub async fn viewer_set_tail_mode(session_id: String, enabled: bool) -> Result<(), String> {
    blocking_viewer_op(move || file_viewer::set_tail_mode(&session_id, enabled).map_err(|e| e.to_string())).await
}

/// Reopens the viewer's backend against the file on disk under the session's current
/// encoding. Called by the FE reload toast and on file rotation.
#[tauri::command]
#[specta::specta]
pub async fn viewer_reload(session_id: String) -> Result<(), String> {
    blocking_viewer_op(move || file_viewer::reload(&session_id).map_err(|e| e.to_string())).await
}

/// Sets up a viewer-specific menu on the given window (adds "Word wrap" to View submenu).
///
/// macOS has no per-window menus (one app-level menu bar, tauri-apps/tauri#5768): `window.set_menu`
/// is a no-op there. Instead the viewer menu is built once at startup and swapped app-level via
/// `activate_window_menu("viewer")` on the viewer's focus-gain, so this command is a no-op on macOS.
/// Linux keeps its working per-window menu.
#[tauri::command]
#[specta::specta]
pub fn viewer_setup_menu(app_handle: tauri::AppHandle, label: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = (&app_handle, &label);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let window = app_handle
            .get_webview_window(&label)
            .ok_or_else(|| format!("Window '{}' not found", label))?;
        let viewer_menu = crate::menu::build_viewer_menu(&app_handle).map_err(|e| e.to_string())?;
        window.set_menu(viewer_menu.menu).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Syncs the viewer menu "Word wrap" check state (called when toggled via keyboard).
///
/// On macOS the viewer menu is shared app-level (one menu bar), so we flip the single stored
/// `CheckMenuItem` ref in `MenuState` directly (O(1), no tree walk). On Linux the menu is per-window,
/// so we walk that window's menu to find the item.
#[tauri::command]
#[specta::specta]
pub fn viewer_set_word_wrap(app_handle: tauri::AppHandle, label: String, checked: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::ignore_poison::IgnorePoison;
        let _ = &label;
        let menu_state = app_handle.state::<crate::menu::MenuState<tauri::Wry>>();
        let guard = menu_state.viewer_word_wrap.lock_ignore_poison();
        if let Some(check) = guard.as_ref() {
            check.set_checked(checked).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let window = app_handle
            .get_webview_window(&label)
            .ok_or_else(|| format!("Window '{}' not found", label))?;
        let Some(menu) = window.menu() else {
            return Ok(());
        };
        for item in menu.items().map_err(|e| e.to_string())? {
            if let MenuItemKind::Submenu(submenu) = item
                && submenu.text().map_err(|e| e.to_string())? == "View"
            {
                for sub_item in submenu.items().map_err(|e| e.to_string())? {
                    if let MenuItemKind::Check(check) = sub_item
                        && check.id().as_ref() == crate::menu::VIEWER_WORD_WRAP_ID
                    {
                        check.set_checked(checked).map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_viewer::RangeEnd;
    use std::io::Write as _;

    /// Writes a minimal real zip (one stored entry) so the boundary magic check passes.
    fn write_zip(path: &std::path::Path) {
        use zip::write::SimpleFileOptions;
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "inner.txt",
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("start");
        writer.write_all(b"hello").expect("write");
        writer.finish().expect("finish");
    }

    /// Registers an in-memory phone under `id` whose 64 KiB chunks arrive `delay`
    /// apart, holding `len` bytes at `big.log`, and returns that file's path.
    async fn a_phone_sending_slowly(id: &str, len: usize, delay: Duration) -> String {
        use crate::file_system::volume::manager::get_volume_manager;
        use crate::file_system::volume::{InMemoryVolume, Volume as _};

        let root = format!("mtp://{id}/1");
        let volume = InMemoryVolume::new("Phone")
            .with_root(&root)
            .with_read_chunk_delay(delay);
        let path = format!("{root}/big.log");
        volume
            .create_file(std::path::Path::new(&path), &vec![b'x'; len])
            .await
            .expect("seed the file");
        get_volume_manager().register(id, Arc::new(volume));
        path
    }

    /// A pull that gets no bytes for the stall limit answers the typed
    /// `StoppedResponding` without waiting for the source, and the pull, detached
    /// rather than dropped, still stops at its chunk boundary and removes its temp.
    /// Pre-fix a pull had a flat 30 s budget and no stall rule at all.
    #[tokio::test]
    async fn a_pull_that_goes_quiet_answers_stopped_responding_and_leaves_no_temp() {
        let extract = crate::test_support::TestDir::new("viewer_pull_stall");
        file_viewer::init_materialize_dir(extract.to_path_buf());
        let path = a_phone_sending_slowly("viewer-stall-cell", 3 * 64 * 1024, Duration::from_millis(900)).await;

        let outcome = open_pulling(
            &Arc::new(PendingOpen::new()),
            path,
            "viewer-stall-cell".to_string(),
            String::new(),
            false,
            Duration::from_millis(250),
            |_| {},
        )
        .await;

        assert!(
            matches!(outcome, Err(ViewerError::StoppedResponding)),
            "a quiet pull answers StoppedResponding, got {outcome:?}"
        );
        crate::test_support::wait_until_async(Duration::from_secs(5), "the quiet pull to remove its temp", || {
            std::fs::read_dir(&extract).is_ok_and(|entries| entries.count() == 0)
        })
        .await;
    }

    /// While a pull runs, its window hears how far it got, against the size the source
    /// declared, and only when there's news. That's what the viewer's bar draws.
    #[tokio::test]
    async fn a_pull_reports_its_progress_to_the_window_as_bytes_arrive() {
        use crate::ignore_poison::IgnorePoison;

        let extract = crate::test_support::TestDir::new("viewer_pull_emit");
        file_viewer::init_materialize_dir(extract.to_path_buf());
        let len = 6 * 64 * 1024;
        let path = a_phone_sending_slowly("viewer-emit-cell", len, Duration::from_millis(100)).await;
        let heard = Arc::new(std::sync::Mutex::new(Vec::<ViewerPullProgress>::new()));

        let outcome = {
            let heard = Arc::clone(&heard);
            open_pulling(
                &Arc::new(PendingOpen::new()),
                path,
                "viewer-emit-cell".to_string(),
                String::new(),
                false,
                Duration::from_secs(5),
                move |progress| heard.lock_ignore_poison().push(progress),
            )
            .await
        };
        let opened = outcome.expect("the file opens");
        file_viewer::close_session(&opened.session_id).expect("close");

        let heard = heard.lock_ignore_poison();
        assert!(!heard.is_empty(), "the window hears at least one progress report");
        assert!(
            heard
                .iter()
                .all(|p| p.bytes_total == Some(len as u64) && p.bytes_done <= len as u64),
            "every report counts against the declared size, got {heard:?}"
        );
        assert!(
            heard.windows(2).all(|pair| pair[0].bytes_done < pair[1].bytes_done),
            "every report is news, got {heard:?}"
        );
    }

    #[tokio::test]
    async fn write_range_rejects_a_destination_inside_an_archive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        write_zip(&zip);

        // The guard runs before any session lookup, so a bogus session id is fine: the
        // point is that an archive-inner DESTINATION is refused with the typed error.
        let dest = zip.join("inner.txt");
        let err = viewer_write_range_to_file(
            "no-such-session".to_string(),
            0,
            RangeEnd::Eof,
            RangeEnd::Eof,
            dest.to_string_lossy().into_owned(),
        )
        .await
        .expect_err("archive-inner destination must be refused");
        assert!(
            matches!(err, ViewerError::DestinationIsReadOnly),
            "expected DestinationIsReadOnly, got {err:?}"
        );

        // A plain sibling destination passes the guard (proves it's not a blanket reject);
        // the bogus session then surfaces as SessionNotFound.
        let plain = dir.path().join("out.txt");
        let err = viewer_write_range_to_file(
            "no-such-session".to_string(),
            0,
            RangeEnd::Eof,
            RangeEnd::Eof,
            plain.to_string_lossy().into_owned(),
        )
        .await
        .expect_err("bogus session should fail past the guard");
        assert!(
            matches!(err, ViewerError::SessionNotFound { .. }),
            "expected the guard to pass and the session lookup to fail, got {err:?}"
        );
    }

    /// The guard is about ROUTES, not about archives: a path inside a repo's
    /// virtual `.git` trees has no directory on disk either, so a save there must
    /// meet the same typed refusal rather than a raw `std::fs` errno. A real file
    /// under `.git` is an ordinary local path and still writes.
    #[tokio::test]
    async fn saving_into_a_repos_history_is_refused_the_way_saving_into_a_zip_is() {
        use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

        let dir = temp_dir("viewer_save_guard", "snapshot");
        let mut fixture = Fixture::init(dir.clone());
        fixture.commit_file("README.md", b"hello\n", "initial");
        crate::file_system::git::wiring::set_virtual_portal_enabled(true);

        let snapshot = dir.join(".git/branches/main/saved.txt");
        let err = viewer_write_range_to_file(
            "no-such-session".to_string(),
            0,
            RangeEnd::Eof,
            RangeEnd::Eof,
            snapshot.to_string_lossy().into_owned(),
        )
        .await
        .expect_err("a snapshot destination must be refused");
        assert!(
            matches!(err, ViewerError::DestinationIsReadOnly),
            "expected DestinationIsReadOnly, got {err:?}"
        );

        // A REAL file under `.git` is the parent volume's and takes ordinary
        // writes, so it passes the guard and fails only on the bogus session.
        let real = dir.join(".git/config.bak");
        let err = viewer_write_range_to_file(
            "no-such-session".to_string(),
            0,
            RangeEnd::Eof,
            RangeEnd::Eof,
            real.to_string_lossy().into_owned(),
        )
        .await
        .expect_err("bogus session should fail past the guard");
        assert!(
            matches!(err, ViewerError::SessionNotFound { .. }),
            "a real path under `.git` must pass the guard, got {err:?}"
        );

        cleanup(&dir);
    }
}
