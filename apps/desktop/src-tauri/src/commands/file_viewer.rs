//! Tauri commands for the file viewer.

use std::sync::Arc;

use tauri_specta::Event as _;
use tokio::time::Duration;

use crate::deadline::{StallWatch, blocking_typed_result_until_stalled, blocking_typed_result_with_timeout};
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

/// How long a save may go without writing a byte before `viewer_write_range_to_file`
/// gives up on it.
///
/// A save has no total budget, unlike the read above: it's what the copy dialog offers
/// when a selection is too big for the clipboard, so its size is unbounded by
/// construction and any total deadline would kill exactly the saves the button exists
/// for. It streams in 1 MiB chunks, so a save that is merely slow still reports
/// constantly, and Escape stops one the user has given up on. Silence is the only thing
/// that can't be honest.
///
/// 90 s is measured against the two things that can hold up a save's next byte: a read
/// from the source and a write to the destination, either of which can be a network
/// mount, where one syscall blocks 30-120 s before the OS answers (`src-tauri/CLAUDE.md`)
/// and on a hard mount may never answer at all, which is why this guard is real rather
/// than a formality. 90 s clears the common end of that band, so a merely slow volume
/// keeps its save, and deliberately stops short of the far end: a mount that needs more
/// than a minute and a half for one 1 MiB write has no chance of finishing a save the
/// user is still waiting on.
const SAVE_STALL_LIMIT: Duration = Duration::from_secs(90);

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
        "viewer_get_lines: returned {} rows, first_row_number={}, byte_offset={}, end={:?}, first_row_preview={:?}",
        result.rows.len(),
        result.first_row_number,
        result.byte_offset,
        result.end,
        result
            .rows
            .first()
            .map(|row| row.text.chars().take(50).collect::<String>())
    );

    Ok(result)
}

/// Reads at most 64 KiB of original file bytes for the binary and hex views.
#[tauri::command]
#[specta::specta]
pub async fn viewer_get_bytes(session_id: String, offset: u64, count: usize) -> Result<Vec<u8>, ViewerError> {
    blocking_typed_result_with_timeout(
        VIEWER_TIMEOUT,
        || ViewerError::TimedOut,
        |message| ViewerError::Io { message },
        move || file_viewer::get_bytes(&session_id, offset, count),
    )
    .await
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

/// Streams a logical range to `dest_path` and writes it atomically (temp+rename). Used
/// by the "Save as file…" action in the > 100 MB refuse dialog and the 10 to 100 MB
/// confirm dialog. Cancellation works the same as `viewer_read_range`.
///
/// Watched for silence (`SAVE_STALL_LIMIT`) rather than held to a total deadline: this
/// is the way out of the clipboard's size refusal, so it has to be allowed to take as
/// long as the selection honestly takes.
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
    write_range_watched(session_id, read_id, anchor, focus, dest_path, SAVE_STALL_LIMIT).await
}

/// Runs one save, watched for stalls. `stall_limit` is a parameter so tests drive it.
async fn write_range_watched(
    session_id: String,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
    dest_path: String,
    stall_limit: Duration,
) -> Result<(), ViewerError> {
    let progress = Arc::new(file_viewer::SaveProgress::new());
    let mut watch = SaveWatch {
        progress: Arc::clone(&progress),
        session_id: session_id.clone(),
        read_id,
        bytes_seen: 0,
        last_change: std::time::Instant::now(),
    };
    blocking_typed_result_until_stalled(
        stall_limit,
        &mut watch,
        |message| ViewerError::Io { message },
        move || {
            file_viewer::write_range_to_file(
                &session_id,
                read_id,
                anchor,
                focus,
                std::path::Path::new(&dest_path),
                &progress,
            )
        },
    )
    .await
}

/// Watches one save for [`blocking_typed_result_until_stalled`].
///
/// Progress is the written-byte count moving. The watch keeps its own clock so the
/// save's write path stays a single atomic add, the same division of labour
/// [`PullWatch`] has with the pull.
struct SaveWatch {
    progress: Arc<file_viewer::SaveProgress>,
    session_id: String,
    read_id: u64,
    /// The count at the last poll, so only a change counts as a sign of life.
    bytes_seen: u64,
    last_change: std::time::Instant,
}

impl StallWatch<ViewerError> for SaveWatch {
    fn idle_for(&self) -> Duration {
        self.last_change.elapsed()
    }

    fn on_poll(&mut self) {
        let written = self.progress.bytes_written();
        if written != self.bytes_seen {
            self.bytes_seen = written;
            self.last_change = std::time::Instant::now();
        }
    }

    fn give_up(&mut self) -> Option<ViewerError> {
        if self.progress.is_done() {
            // It returned a moment ago; the waiter takes its real result instead.
            return None;
        }
        // The waiter detaches the work rather than dropping it, so the save has to be
        // told: it sees the flag at its next chunk, removes its temp file, and answers
        // `Cancelled` to nobody.
        let _ = file_viewer::cancel_read(&self.session_id, self.read_id);
        // `TimedOut`, not `StoppedResponding`: that one means "the source stopped
        // sending the file", which says the wrong thing about a save's destination, and
        // the FE already words this variant for the save ("that took too long").
        Some(ViewerError::TimedOut)
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

/// Tells the viewer menu bar whether this viewer's search box holds keyboard focus, which is what
/// decides whether its Edit > Cut and Edit > Paste look live.
///
/// They're the search box's items: it's the only editable field in a viewer window, and while it
/// doesn't have focus the two act on nothing. The viewer pushes `true` when the input takes focus
/// and `false` when it gives it up, when the search bar closes (removing a focused input fires no
/// `blur`), and its current answer again on the window's focus-gain, which is what keeps a switch
/// between two viewers from leaving a stale verdict on the shared bar.
///
/// macOS only, where the viewer bar is app-level and those two items are Custom. Elsewhere they're
/// Predefined and nothing can grey them, so this is a no-op.
#[tauri::command]
#[specta::specta]
pub fn viewer_set_search_input_focused(
    app_handle: tauri::AppHandle,
    label: String,
    focused: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::ignore_poison::IgnorePoison;
        let menu_state = app_handle.state::<crate::menu::MenuState<tauri::Wry>>();
        {
            let mut holder = menu_state.viewer_search_focus.lock_ignore_poison();
            crate::menu::note_viewer_search_focus(&mut holder, &label, focused);
        }
        // Dropped the lock first: `apply_menu_item_states` takes it again.
        crate::menu::apply_menu_item_states(&menu_state);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&app_handle, &label, focused);
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
#[path = "file_viewer_test.rs"]
mod tests;
