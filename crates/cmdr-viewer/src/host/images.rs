//! Where a rendered Markdown document's pictures come from.
//!
//! The viewer decides WHAT a `src` names (`markdown_images::resolve_image_src`, pure); the
//! host decides how those bytes are reached: served in place, pulled into a temp, or
//! downloaded. Either way the answer is a [`MediaSource`] behind a `cmdr-media://` token,
//! fetched only when the webview asks, and only a file whose bytes classify as an image is
//! ever served, so a crafted document can't use an `<img>` to read any other file.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cmdr_fs::rt::Runtime;

use super::super::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
use super::super::media::{DeferredMedia, MediaEntry, MediaSource};
use super::super::media_volume::VolumeMedia;
use super::super::source::read_head;
use super::super::volume_source::{PositionedRead, VolumeSource};
use super::volumes::ViewerVolumes;

/// Reaches a document's pictures.
pub trait DocumentImages: Send + Sync {
    /// A picture at `path` on the document's volume `volume_id`. `closed` flips when the
    /// document's session closes: a fetch still running stops at its next chunk.
    fn on_volume(&self, path: PathBuf, volume_id: String, closed: Arc<AtomicBool>) -> MediaSource;

    /// A picture on the web. Fetched only once `allowed` is set (the user's "Load remote
    /// images"), never before.
    fn on_the_web(&self, url: String, allowed: Arc<AtomicBool>, closed: Arc<AtomicBool>) -> MediaSource;
}

/// No picture can be reached: every token answers 404.
pub struct NoImages;

impl DocumentImages for NoImages {
    fn on_volume(&self, _path: PathBuf, _volume_id: String, _closed: Arc<AtomicBool>) -> MediaSource {
        MediaSource::Deferred(Arc::new(Unreachable))
    }

    fn on_the_web(&self, _url: String, _allowed: Arc<AtomicBool>, _closed: Arc<AtomicBool>) -> MediaSource {
        MediaSource::Deferred(Arc::new(Unreachable))
    }
}

/// Pictures read in place, by position, from the volume that holds them, and none from the
/// web: the browser demo's answer, whose volumes are in memory and whose page may reach no
/// network. A file that isn't an image (by its bytes) answers 404, like the app's.
pub struct InPlace {
    volumes: Arc<dyn ViewerVolumes>,
    runtime: Runtime,
}

impl InPlace {
    /// Reads pictures through `volumes`, bridging on `runtime`.
    pub fn new(volumes: Arc<dyn ViewerVolumes>, runtime: Runtime) -> Self {
        Self { volumes, runtime }
    }
}

impl DocumentImages for InPlace {
    fn on_volume(&self, path: PathBuf, volume_id: String, _closed: Arc<AtomicBool>) -> MediaSource {
        let Some(volume) = self.volumes.get(&volume_id) else {
            return MediaSource::Deferred(Arc::new(Unreachable));
        };
        let Ok((source, total)) = VolumeSource::open(volume, &path, PositionedRead::Ranged, &self.runtime) else {
            return MediaSource::Deferred(Arc::new(Unreachable));
        };
        let head = read_head(&source, total, CLASSIFY_HEAD_LEN).unwrap_or_default();
        let kind = classify_viewer_content(&head, None);
        match media_mime(&head, kind).filter(|_| kind == ViewerContentKind::Image) {
            Some(mime) => MediaSource::Volume(Arc::new(VolumeMedia::new(Arc::new(source), total, mime.to_string()))),
            None => MediaSource::Deferred(Arc::new(Unreachable)),
        }
    }

    fn on_the_web(&self, _url: String, _allowed: Arc<AtomicBool>, _closed: Arc<AtomicBool>) -> MediaSource {
        MediaSource::Deferred(Arc::new(Unreachable))
    }
}

/// A picture nothing can fetch.
struct Unreachable;

impl DeferredMedia for Unreachable {
    fn fetch(&self) -> Option<MediaEntry> {
        None
    }

    fn idle_for(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }

    fn give_up(&self) {}
}
