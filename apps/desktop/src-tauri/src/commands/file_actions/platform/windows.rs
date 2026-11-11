//! Windows: no reveal, Get Info, or editor launch yet (`SHOpenFolderAndSelectItems` and
//! `ShellExecuteW` are the ones to add), so each answers as before it had an arm.

use crate::file_system::get_info::GetInfoError;
use crate::file_system::text_editor::{EditorOpenReport, OpenInEditorError};

/// Not available yet.
pub(crate) fn reveal_in_file_manager(_path: String) -> Result<(), String> {
    Err("Show in file manager is not available on this platform".to_string())
}

/// No Get Info window; a no-op.
pub(crate) async fn get_info(_path: String) -> Result<(), GetInfoError> {
    Ok(())
}

/// Refused, except in the E2E build, which records the file like the other arms.
pub(crate) async fn open_in_editor(
    path: String,
    _app_choice: String,
    _ask_about_other_editors: bool,
) -> Result<EditorOpenReport, OpenInEditorError> {
    #[cfg(feature = "playwright-e2e")]
    {
        crate::open_mock::record(path);
        Ok(EditorOpenReport {
            outcome: crate::file_system::text_editor::EditorOpenOutcome::Opened,
            opened_in_name: None,
            other_editors_installed: None,
        })
    }
    #[cfg(not(feature = "playwright-e2e"))]
    {
        let _ = path;
        Err(OpenInEditorError::LaunchRefused { errno: None })
    }
}
