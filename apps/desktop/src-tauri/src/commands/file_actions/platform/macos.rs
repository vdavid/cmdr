//! macOS: Finder reveals, Finder's Get Info window, and the text editor LaunchServices names.

use cmdr_fs::deadline::blocking_typed_result_with_timeout;
use tokio::time::Duration;

use crate::file_system::get_info::GetInfoError;
use crate::file_system::text_editor::{EditorOpenReport, OpenInEditorError};

/// Get Info is one no-prompt permission read and one `spawn` of `osascript`. The
/// window, or the first-time prompt, appears after the command has answered.
const GET_INFO_TIMEOUT: Duration = Duration::from_secs(5);

/// Opening a file is one `spawn` of `open`; the wait is for the installed-app
/// lookup in front of it and the name read behind it.
const OPEN_IN_EDITOR_TIMEOUT: Duration = Duration::from_secs(5);

/// Reveals `path` in Finder. The E2E build records it instead: Finder would take the front
/// position and write a `.DS_Store` into the fixture tree.
pub(crate) fn reveal_in_file_manager(path: String) -> Result<(), String> {
    #[cfg(feature = "playwright-e2e")]
    {
        crate::open_mock::record(path);
        Ok(())
    }
    #[cfg(not(feature = "playwright-e2e"))]
    {
        crate::reveal::finder_handoff::reveal_in_finder(&path)
    }
}

/// Finder's Get Info window, under a timeout.
pub(crate) async fn get_info(path: String) -> Result<(), GetInfoError> {
    blocking_typed_result_with_timeout(
        GET_INFO_TIMEOUT,
        || GetInfoError::TimedOut,
        |detail| {
            crate::log_error!(target: "file_actions", "get_info panicked: {detail}");
            GetInfoError::TimedOut
        },
        move || crate::file_system::get_info::open_get_info(std::path::Path::new(&path)),
    )
    .await
}

/// The chosen editor, through `file_system::text_editor` (which records instead of launching in
/// the E2E build).
pub(crate) async fn open_in_editor(
    path: String,
    app_choice: String,
    ask_about_other_editors: bool,
) -> Result<EditorOpenReport, OpenInEditorError> {
    blocking_typed_result_with_timeout(
        OPEN_IN_EDITOR_TIMEOUT,
        || OpenInEditorError::TimedOut,
        |detail| {
            crate::log_error!(target: "file_actions", "open_in_editor panicked: {detail}");
            OpenInEditorError::TimedOut
        },
        move || {
            crate::file_system::text_editor::open_in_editor(
                std::path::Path::new(&path),
                &app_choice,
                ask_about_other_editors,
            )
        },
    )
    .await
}
