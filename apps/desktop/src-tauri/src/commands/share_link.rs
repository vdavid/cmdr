//! "Copy share link": mints a link to one file and puts it on the clipboard.
//!
//! Backend-neutral: any volume answering `Volume::supports_share_links` serves
//! it (S3 today, a presigned GET signed offline). The frontend offers it on that
//! capability, ❌ never on a backend name.
//!
//! ❗ **The link never leaves Rust except onto the clipboard.** Its signature is
//! a credential, so the command returns nothing but the outcome: the URL can't
//! reach an IPC log, a frontend logger, or an uncaught-rejection report, and ❌
//! nothing here logs it.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::{ShareLink, ShareLinkExpiry, Volume, VolumeError};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::file_system::volume::manager::get_volume_manager;

/// Mints a share link to the file at `path` on `volume_id`, valid for
/// `expires_in`, and copies it to the clipboard.
///
/// No timeout wrapper: minting reads a lock and signs a string; it reaches no
/// server and no disk.
#[tauri::command]
#[specta::specta]
pub async fn copy_share_link(
    app: AppHandle,
    volume_id: String,
    path: String,
    expires_in: ShareLinkExpiry,
) -> Result<(), VolumeError> {
    let link = mint(get_volume_manager().get(&volume_id), &path, expires_in).await?;
    app.clipboard()
        .write_text(link.into_url())
        // The clipboard's own words, for the details panel; they carry no URL.
        .map_err(|e| VolumeError::IoError {
            message: format!("couldn't write the clipboard: {e}"),
            raw_os_error: None,
        })
}

/// The link, from whichever volume is registered under the id. A volume that
/// isn't there (forgotten, or never connected this session) is `NotConnected`:
/// opening it in a pane is what connects it.
async fn mint(
    volume: Option<Arc<dyn Volume>>,
    path: &str,
    expires_in: ShareLinkExpiry,
) -> Result<ShareLink, VolumeError> {
    let Some(volume) = volume else {
        return Err(VolumeError::NotConnected(path.to_string()));
    };
    volume.share_link(Path::new(path), expires_in).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::volume::InMemoryVolume;

    #[tokio::test]
    async fn a_volume_that_isnt_registered_is_not_connected() {
        let outcome = mint(None, "s3://AKIA@host:443/b/a.txt", ShareLinkExpiry::SevenDays).await;
        assert!(
            matches!(&outcome, Err(VolumeError::NotConnected(path)) if path == "s3://AKIA@host:443/b/a.txt"),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_volume_without_share_links_says_so() {
        let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Test"));
        let outcome = mint(Some(volume), "/a.txt", ShareLinkExpiry::OneHour).await;
        assert!(matches!(outcome, Err(VolumeError::NotSupported)), "{outcome:?}");
    }
}
