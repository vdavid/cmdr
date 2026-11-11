//! Linux: `xdg-open` for a reveal (the parent folder) and for an editor, and no Get Info window.

use std::process::Command;

use crate::file_system::get_info::GetInfoError;
use crate::file_system::text_editor::{EditorOpenReport, OpenInEditorError};

/// Opens the parent folder in the default file manager.
pub(crate) fn reveal_in_file_manager(path: String) -> Result<(), String> {
    let parent = std::path::Path::new(&path)
        .parent()
        .unwrap_or(std::path::Path::new("/"));
    Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// No Get Info window; a no-op.
pub(crate) async fn get_info(_path: String) -> Result<(), GetInfoError> {
    Ok(())
}

/// `xdg-open` decides, as it always did. Both choice arguments are ignored (there's no Settings
/// row off macOS), and there's nothing to report beyond the launch. The E2E build records the
/// file instead, into the same `crate::open_mock` store as `open_path`, so no orphan windows
/// leak.
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
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| OpenInEditorError::LaunchRefused {
                errno: e.raw_os_error(),
            })?;
        Ok(EditorOpenReport {
            outcome: crate::file_system::text_editor::EditorOpenOutcome::Opened,
            opened_in_name: None,
            other_editors_installed: None,
        })
    }
}
