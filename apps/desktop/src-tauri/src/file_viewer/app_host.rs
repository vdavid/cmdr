//! The desktop app's answers to the viewer's host seams (`host/`): the volume manager,
//! the materializer, the Markdown image fetches, analytics, and the Tauri events and
//! opener a viewer window hears through.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use log::debug;
use tauri::{AppHandle, Emitter};

use crate::file_system::volume::Volume;
use crate::file_system::volume::manager::{get_volume_manager, path_routes_over_its_parent};

use super::host::analytics::ViewerAnalytics;
use super::host::events::{FileChange, ViewerEvents};
use super::host::images::DocumentImages;
use super::host::opener::{LinkOpener, NoOpener};
use super::host::pulls::{PreviewTemp, ViewerPulls};
use super::host::volumes::ViewerVolumes;
use super::host::{ViewerHost, install_host};
use super::markdown_image_fetch::{RemoteImage, VolumeImage};
use super::media::MediaSource;
use super::pending_open::PendingOpen;
use super::{ViewerError, ViewerOpenResult, materialize};

/// The app's host before any window exists: everything but the events and the opener,
/// which need the app handle. What the app's unit tests run against.
pub(super) fn app_host_without_windows() -> ViewerHost {
    ViewerHost {
        volumes: Arc::new(AppVolumes),
        pulls: Arc::new(AppPulls),
        images: Arc::new(AppImages),
        events: Arc::new(super::host::events::NoEvents),
        analytics: Arc::new(AppAnalytics),
        opener: Arc::new(NoOpener),
        log_display: display_line,
    }
}

/// Installs the app's full host, with `handle` for the viewer windows' events and the
/// opener. Called once at startup.
pub fn init_app_handle(handle: AppHandle) {
    install_host(ViewerHost {
        events: Arc::new(AppEvents { handle: handle.clone() }),
        opener: Arc::new(AppOpener { handle }),
        ..app_host_without_windows()
    });
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

struct AppEvents {
    handle: AppHandle,
}

impl ViewerEvents for AppEvents {
    fn file_changed(&self, session_id: &str, change: FileChange) {
        let (event, payload) = file_changed_event(session_id, change);
        if let Err(e) = self.handle.emit(&event, payload) {
            debug!("emit viewer:file-changed failed: {}", e);
        }
    }
}

struct AppOpener {
    handle: AppHandle,
}

impl LinkOpener for AppOpener {
    fn open_url(&self, url: &str) -> Result<(), ViewerError> {
        use tauri_plugin_opener::OpenerExt;
        self.handle
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| ViewerError::Io {
                message: e.to_string().into(),
            })
    }
}
