//! The answers of an OS with no clipboard to reach yet: each says it isn't supported here.
//! Shared by the Linux and Windows arms.

use std::path::PathBuf;

use cmdr_operations::MutationError;
use tauri::AppHandle;

use super::super::PasteClipboardReply;

/// Not supported yet.
pub(crate) fn write_file_urls(_app: AppHandle, _paths: Vec<PathBuf>) -> Result<(), String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

/// Not supported yet.
pub(crate) fn read_file_urls(_app: AppHandle) -> Result<Vec<PathBuf>, String> {
    Err("Clipboard file operations are not yet supported on this platform".to_string())
}

/// Not supported yet.
pub(crate) fn read_text(_app: AppHandle) -> Result<Option<String>, String> {
    Err("Clipboard operations are not yet supported on this platform".to_string())
}

/// Not supported yet.
pub(crate) async fn paste_as_file(
    _app: AppHandle,
    _volume_id: Option<String>,
    _directory: String,
) -> Result<PasteClipboardReply, MutationError> {
    Err(MutationError::Volume {
        error: cmdr_fs::volume::VolumeError::NotSupported,
    })
}
