//! Tauri commands for the file viewer.

use tokio::time::Duration;

use super::util::{IpcError, blocking_result_with_timeout};
use crate::file_viewer::{
    self, EncodingOptions, FileEncoding, LineChunk, RangeEnd, SearchMode, SearchPollResult, SeekTarget, ViewerError,
    ViewerOpenResult, ViewerSessionStatus,
};
use log::debug;
use tauri::Manager;
#[cfg(not(target_os = "macos"))]
use tauri::menu::MenuItemKind;

const VIEWER_TIMEOUT: Duration = Duration::from_secs(2);

/// Open budget for a preview INSIDE an archive: the whole entry is streamed out to a
/// bounded temp first, so it needs the recursive-scan tier, not the 2 s read tier. The
/// extraction cap keeps the worst case bounded. A non-archive open keeps the strict 2 s.
const VIEWER_ARCHIVE_TIMEOUT: Duration = Duration::from_secs(30);

/// Picks the open timeout for `path`: the generous archive budget when the path is
/// INSIDE a `.zip` (a temp-extract, which is slow — more so pulling from a remote
/// parent), else the strict 2 s. Viewing the `.zip` file itself is a normal read, so
/// it keeps the strict budget.
///
/// This is a pure string check (a non-empty inner under a `.zip` component), no I/O
/// and no confirm: the budget is a heuristic, not a correctness gate, so it needs no
/// `volume_id` and never touches the disk or network. Over-granting the archive
/// budget to a mislabeled `.zip` is harmless (the open fails fast on its own).
fn open_timeout_for(path: &str) -> Duration {
    let expanded = crate::commands::file_system::expand_tilde(path);
    let looks_archive_inner =
        crate::file_system::volume::backends::archive::archive_boundary_candidate(std::path::Path::new(&expanded))
            .is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty());
    if looks_archive_inner {
        VIEWER_ARCHIVE_TIMEOUT
    } else {
        VIEWER_TIMEOUT
    }
}

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
#[tauri::command]
#[specta::specta]
pub async fn viewer_open(
    path: String,
    volume_id: String,
    window_label: String,
) -> Result<ViewerOpenResult, ViewerError> {
    let timeout = open_timeout_for(&path);
    // Typed `ViewerError` (not a stringified `IpcError`) so the FE can render friendly
    // copy for the archive family — `ExtractTooLarge` (preview cap), `Archive`
    // (encrypted / corrupt / unsupported codec) — matching `viewer_read_range`.
    match tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || {
            let result = file_viewer::open_session(&path, &volume_id)?;
            file_viewer::register_window_session(&window_label, &result.session_id);
            Ok(result)
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

/// Opens a fresh, full **text** session for `path`, ignoring media classification.
///
/// Backs the viewer's "View as text" override: a media (Image/PDF) session isn't
/// upgraded in place; the FE calls this, swaps to the returned text session, and closes
/// the old one. Re-registers the window -> session link so the window-destroyed handler
/// frees the new session.
#[tauri::command]
#[specta::specta]
pub async fn viewer_open_as_text(
    path: String,
    volume_id: String,
    window_label: String,
) -> Result<ViewerOpenResult, ViewerError> {
    let timeout = open_timeout_for(&path);
    match tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || {
            let result = file_viewer::open_session_as_text(&path, &volume_id)?;
            file_viewer::register_window_session(&window_label, &result.session_id);
            Ok(result)
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

/// Fetches a range of lines from a viewer session.
///
/// # Arguments
/// * `session_id` - The session ID from `viewer_open`.
/// * `target_type` - One of "line", "byte", or "fraction".
/// * `target_value` - The seek value (line number, byte offset, or fraction 0.0-1.0).
/// * `count` - Number of lines to fetch.
#[tauri::command]
#[specta::specta]
pub async fn viewer_get_lines(
    session_id: String,
    target_type: String,
    target_value: f64,
    count: usize,
) -> Result<LineChunk, IpcError> {
    let target = match target_type.as_str() {
        "line" => SeekTarget::Line(target_value as usize),
        "byte" => SeekTarget::ByteOffset(target_value as u64),
        "fraction" => SeekTarget::Fraction(target_value),
        other => {
            return Err(IpcError::from_err(format!(
                "Unknown target type: {}. Use 'line', 'byte', or 'fraction'.",
                other
            )));
        }
    };

    debug!(
        "viewer_get_lines: session={}, target_type={}, target_value={}, count={}",
        session_id, target_type, target_value, count
    );

    let result = blocking_result_with_timeout(VIEWER_TIMEOUT, move || {
        file_viewer::get_lines(&session_id, target, count).map_err(|e| e.to_string())
    })
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
    // The source may be an archive preview temp (it writes fine via `std::fs` off the
    // open session), but the DESTINATION must not be INSIDE an archive: archives are
    // read-only in this phase. Saving over a `.zip` file itself is a normal file
    // overwrite (allowed); only a path inside one is refused. Typed error, matching
    // the write-path guards.
    if crate::file_system::volume::backends::archive::path_is_inside_archive(std::path::Path::new(
        &crate::commands::file_system::expand_tilde(&dest_path),
    )) {
        return Err(ViewerError::DestinationInsideArchive);
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
            matches!(err, ViewerError::DestinationInsideArchive),
            "expected DestinationInsideArchive, got {err:?}"
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
}
