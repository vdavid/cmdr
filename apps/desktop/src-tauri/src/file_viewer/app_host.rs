//! The desktop app's viewer ([`app_viewer`]) and its answers to the host seams (`host/`): the volume manager,
//! the materializer, the Markdown image fetches, analytics, and the Tauri events and
//! opener a viewer window hears through.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, OnceLock};

use log::debug;
use tauri::{AppHandle, Emitter};

use crate::file_system::volume::Volume;
use crate::file_system::volume::manager::{get_volume_manager, path_routes_over_its_parent};

use super::Viewer;
use super::host::ViewerHost;
use super::host::analytics::ViewerAnalytics;
use super::host::events::{FileChange, ViewerEvents};
use super::host::images::DocumentImages;
use super::host::opener::LinkOpener;
use super::host::pulls::{PreviewTemp, ViewerPulls};
use super::host::volumes::ViewerVolumes;
use super::markdown_image_fetch::{RemoteImage, VolumeImage};
use super::media::MediaSource;
use super::pending_open::PendingOpen;
use super::{ViewerError, ViewerOpenResult, materialize};

/// The app's one viewer, over the app's host, on Tauri's runtime. Its tokens are the ones
/// the `cmdr-media://` handler and the thumbnails share.
pub fn app_viewer() -> &'static Viewer {
    static VIEWER: LazyLock<Viewer> =
        LazyLock::new(|| Viewer::new(app_host(), tauri::async_runtime::handle().inner().clone()));
    &VIEWER
}

/// The window handle the viewer's events and the link opener reach the app through, set
/// once at startup. Until then (the app's unit tests) both answer nothing.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Gives the viewer the app handle. Called once at startup, before any window exists.
pub fn init_app_handle(handle: AppHandle) {
    if APP_HANDLE.set(handle).is_err() {
        debug!("viewer: the app handle was already set");
    }
}

fn app_host() -> ViewerHost {
    ViewerHost {
        volumes: Arc::new(AppVolumes),
        pulls: Arc::new(AppPulls),
        images: Arc::new(AppImages),
        events: Arc::new(AppEvents),
        analytics: Arc::new(AppAnalytics),
        opener: Arc::new(AppOpener),
        log_display: display_line,
        local_files: true,
    }
}

fn display_line(line: &str) -> Cow<'_, str> {
    crate::redact::display_line(line)
}

/// The `viewer:file-changed:<sid>` event's name and payload, which the viewer window
/// listens for by that name (no typed binding).
pub(super) fn file_changed_event(session_id: &str, change: FileChange) -> (String, serde_json::Value) {
    let event = format!("viewer:file-changed:{}", session_id);
    let payload = match change {
        FileChange::Grew { new_size } => serde_json::json!({ "kind": "grew", "newSize": Some(new_size) }),
        FileChange::Rotated => serde_json::json!({ "kind": "rotated", "newSize": None::<u64> }),
    };
    (event, payload)
}

struct AppVolumes;

impl ViewerVolumes for AppVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        get_volume_manager().get(volume_id)
    }

    fn routes_over_its_parent(&self, path: &Path) -> bool {
        path_routes_over_its_parent(path)
    }
}

struct AppPulls;

impl ViewerPulls for AppPulls {
    fn pull_for_viewer(
        &self,
        requested: &Path,
        volume_id: &str,
        open: &PendingOpen,
    ) -> Result<Option<PreviewTemp>, ViewerError> {
        Ok(materialize::materialize_for_viewer(requested, volume_id, open)?
            .map(|file| materialize::preview_temp(file, requested.to_path_buf(), volume_id)))
    }
}

struct AppImages;

impl DocumentImages for AppImages {
    fn on_volume(&self, path: PathBuf, volume_id: String, closed: Arc<AtomicBool>) -> MediaSource {
        MediaSource::Deferred(Arc::new(VolumeImage::new(path, volume_id, closed)))
    }

    fn on_the_web(&self, url: String, allowed: Arc<AtomicBool>, closed: Arc<AtomicBool>) -> MediaSource {
        MediaSource::Deferred(Arc::new(RemoteImage::new(url, allowed, closed)))
    }
}

struct AppAnalytics;

impl ViewerAnalytics for AppAnalytics {
    fn viewer_opened(&self, result: &Result<ViewerOpenResult, ViewerError>, from_archive: bool, force_text: bool) {
        super::analytics::emit_viewer_opened(result, from_archive, force_text);
    }
}

struct AppEvents;

impl ViewerEvents for AppEvents {
    fn file_changed(&self, session_id: &str, change: FileChange) {
        let Some(handle) = APP_HANDLE.get() else {
            return;
        };
        let (event, payload) = file_changed_event(session_id, change);
        if let Err(e) = handle.emit(&event, payload) {
            debug!("emit viewer:file-changed failed: {}", e);
        }
    }

    /// The scheme handler resolves tokens per request, so a dropped one is already a 404;
    /// nothing else holds on to it.
    fn media_token_dropped(&self, _token: &str) {}
}

struct AppOpener;

impl LinkOpener for AppOpener {
    fn open_url(&self, url: &str) -> Result<(), ViewerError> {
        use tauri_plugin_opener::OpenerExt;
        let Some(handle) = APP_HANDLE.get() else {
            return Ok(());
        };
        handle
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| ViewerError::Io {
                message: e.to_string().into(),
            })
    }
}
