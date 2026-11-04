//! A Markdown session's per-document state, which `ViewerSession.markdown` holds.
//!
//! Its own module, below both `markdown_session.rs` (which creates and renders it) and
//! `session_store.rs` (which stores it), so the two don't depend on each other in a circle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::host::host;
use super::markdown_images::{ImageTarget, resolve_image_src};
use super::markdown_render::ImageMarker;
use super::media;

/// A Markdown session's document: where it lives, and the image tokens its render minted.
pub(super) struct MarkdownDocument {
    /// The document's path on its volume (never a temp copy's), which relative image
    /// sources resolve against.
    path: PathBuf,
    volume_id: String,
    /// Set when the session closes, so an image pull still running stops.
    closed: Arc<AtomicBool>,
    /// Set by the user's "Load remote images"; until then no web image is fetched.
    remote_allowed: Arc<AtomicBool>,
    /// The tokens the last render minted, by what they point at, so a repeated image
    /// shares one token (and one pull).
    images: Mutex<HashMap<ImageTarget, String>>,
}

impl MarkdownDocument {
    pub(super) fn new(path: PathBuf, volume_id: String) -> Self {
        Self {
            path,
            volume_id,
            closed: Arc::new(AtomicBool::new(false)),
            remote_allowed: Arc::new(AtomicBool::new(false)),
            images: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn volume_id(&self) -> &str {
        &self.volume_id
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Lets the document's web images load.
    pub(super) fn allow_remote_images(&self) {
        self.remote_allowed.store(true, Ordering::Relaxed);
    }

    /// What an image `src` becomes, minting its token on first sight.
    pub(super) fn image_marker(&self, src: &str, volume_root: &Path) -> ImageMarker {
        let target = resolve_image_src(src, &self.path, volume_root);
        let mut images = self.images.lock_ignore_poison();
        let token = |images: &mut HashMap<ImageTarget, String>, source: &dyn Fn() -> media::MediaSource| {
            images
                .entry(target.clone())
                .or_insert_with(|| media::mint(source()))
                .clone()
        };
        let host = host();
        match &target {
            ImageTarget::Local(path) => ImageMarker::Local(token(&mut images, &|| {
                host.images
                    .on_volume(path.clone(), self.volume_id.clone(), Arc::clone(&self.closed))
            })),
            ImageTarget::Remote(url) => ImageMarker::Remote(token(&mut images, &|| {
                host.images
                    .on_the_web(url.clone(), Arc::clone(&self.remote_allowed), Arc::clone(&self.closed))
            })),
            ImageTarget::Inline => ImageMarker::Inline,
            ImageTarget::Blocked => ImageMarker::Blocked,
        }
    }

    pub(super) fn has_remote_images(&self) -> bool {
        self.images
            .lock_ignore_poison()
            .keys()
            .any(|target| matches!(target, ImageTarget::Remote(_)))
    }

    /// Drops every image token, so none outlives the render it came from.
    pub(super) fn drop_images(&self) {
        for (_, token) in self.images.lock_ignore_poison().drain() {
            media::drop_token(&token);
        }
    }

    /// The session closed: stop the pulls and drop the tokens. Called from the single
    /// `close_session` choke point, like a media session's token drop.
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.drop_images();
    }
}
