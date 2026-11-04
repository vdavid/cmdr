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

use super::super::media::{DeferredMedia, MediaEntry, MediaSource};

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
