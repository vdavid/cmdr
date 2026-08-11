//! Feature-gated Tauri commands for E2E testing and debug support.

/// Injects a listing error into an in-memory volume so the next `list_directory` call
/// returns a `VolumeError::IoError` with the given errno. The error is cleared after
/// one use, enabling retry testing.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn inject_listing_error(volume_id: String, error_code: i32) -> Result<(), String> {
    let volume = crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| format!("Volume `{}` not found", volume_id))?;
    volume.inject_error(error_code);
    Ok(())
}

/// Arms `count` consecutive failures of `get_brief_column_text_widths`.
///
/// Lets a spec watch the Brief pane run with NO measured widths: the cursor must stay
/// visible and columns must render at their provisional width rather than swallowing
/// the pane. Pass a count above the frontend's retry budget to keep it degraded, or a
/// smaller one to watch it recover. Counting down (rather than a boolean) is what makes
/// "fails twice, then succeeds" expressible.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn fail_next_brief_column_widths(count: usize) {
    crate::file_system::listing::brief_columns::FAIL_NEXT_WIDTH_CALLS.store(count, std::sync::atomic::Ordering::SeqCst);
}

/// Debug-only: makes sure the dialog gallery's throwaway fixture directory
/// exists under the app data dir, and returns its path plus the landmarks inside
/// it the gallery addresses by name.
///
/// Idempotent, so the Debug window can call it on every trigger. The tree itself
/// (and why the disk-backed dialogs need a real one) lives in `dev_fixtures`.
///
/// Present in dev AND E2E builds (a release build with `playwright-e2e`), because
/// the gallery is a test instrument: `dialog-inset.spec.ts` measures every dialog
/// through it, and five of them do real work on mount. Absent from a shipped build,
/// which has neither.
#[cfg(any(debug_assertions, feature = "playwright-e2e"))]
#[tauri::command]
#[specta::specta]
pub async fn create_dialog_gallery_fixtures(
    app: tauri::AppHandle,
) -> Result<crate::dev_fixtures::DialogGalleryFixtures, crate::commands::util::IpcError> {
    use crate::commands::util::{IpcError, blocking_result_with_timeout};
    use tokio::time::Duration;

    // Generous for a local write, because the FIRST call creates a few dozen
    // files; every later one only stats them.
    const FIXTURE_TIMEOUT: Duration = Duration::from_secs(30);

    let data_dir = crate::config::resolved_app_data_dir(&app).map_err(IpcError::from_err)?;
    blocking_result_with_timeout(FIXTURE_TIMEOUT, move || {
        crate::dev_fixtures::ensure_dialog_gallery_fixtures(&data_dir.join(crate::dev_fixtures::FIXTURE_DIR_NAME))
    })
    .await
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
