//! Feature-gated Tauri commands for E2E testing and debug support.

/// Injects a listing error into an in-memory volume so the next `list_directory` call
/// returns a `VolumeError::IoError` with the given errno. The error is cleared after
/// one use, enabling retry testing.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn inject_listing_error(volume_id: String, error_code: i32) -> Result<(), String> {
    let volume = crate::file_system::get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| format!("Volume `{}` not found", volume_id))?;
    volume.inject_error(error_code);
    Ok(())
}

/// Debug-only command that generates a real typed `ListingError` for the debug
/// error pane preview.
///
/// Accepts either an errno code (for `IoError` variants) or a `VolumeError` variant name.
/// Optionally sets the detected provider when `provider_path` is set.
#[cfg(debug_assertions)]
#[tauri::command]
#[specta::specta]
pub fn preview_friendly_error(
    error_code: Option<i32>,
    variant: Option<String>,
    provider_path: Option<String>,
) -> Result<crate::file_system::volume::friendly_error::ListingError, String> {
    use crate::file_system::volume::VolumeError;
    use crate::file_system::volume::friendly_error::{enrich_with_provider, listing_error_from_volume_error};
    use std::path::Path;

    let path_str = provider_path
        .clone()
        .unwrap_or_else(|| "/Users/demo/Documents/test".to_string());
    let path = Path::new(&path_str);

    let volume_error = if let Some(code) = error_code {
        VolumeError::IoError {
            message: format!("os error {}", code),
            raw_os_error: Some(code),
        }
    } else if let Some(ref name) = variant {
        match name.as_str() {
            "NotFound" => VolumeError::NotFound(path_str.clone()),
            "PermissionDenied" => VolumeError::PermissionDenied(path_str.clone()),
            "AlreadyExists" => VolumeError::AlreadyExists(path_str.clone()),
            "NotSupported" => VolumeError::NotSupported,
            "DeviceDisconnected" => VolumeError::DeviceDisconnected("device went away".into()),
            "ReadOnly" => VolumeError::ReadOnly(path_str.clone()),
            "StorageFull" => VolumeError::StorageFull {
                message: "not enough space".into(),
            },
            "ConnectionTimeout" => VolumeError::ConnectionTimeout("timed out".into()),
            "Cancelled" => VolumeError::Cancelled("cancelled by user".into()),
            "IoError (no errno)" => VolumeError::IoError {
                message: "unknown I/O problem".into(),
                raw_os_error: None,
            },
            _ => return Err(format!("Unknown VolumeError variant: {}", name)),
        }
    } else {
        return Err("Provide either error_code or variant".into());
    };

    let mut listing_error = listing_error_from_volume_error(&volume_error, path);

    if provider_path.is_some() {
        enrich_with_provider(&mut listing_error, path);
    }

    Ok(listing_error)
}
