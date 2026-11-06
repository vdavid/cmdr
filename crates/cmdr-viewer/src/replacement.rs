//! The hook a session's volume reader calls when its file changed under it. Its own
//! module because both `origin.rs` (which carries it) and `volume_source.rs` (which
//! fires it) need it, and either holding it would make the two depend on each other.

use std::sync::{Arc, OnceLock};

/// What a session asks its file's reader to do when the file turns out to have
/// changed under it (`VolumeError::SourceChanged`: SMB refusing to splice two versions
/// after a reconnect): reload, as a watcher seeing the file replaced does. Shared by
/// every source its origin opens, so a reload's fresh source answers the same way, and
/// set once the session exists (`Viewer::reload_on_replacement`).
#[derive(Clone, Default)]
pub struct ReplacementHook(Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>);

impl ReplacementHook {
    /// Runs `on_replaced` whenever a source this hook was handed to finds its file
    /// changed. The first call wins; a later one changes nothing.
    pub(super) fn set(&self, on_replaced: impl Fn() + Send + Sync + 'static) {
        let _ = self.0.set(Box::new(on_replaced));
    }

    /// Tells the session its file changed. Nothing before the session set the hook.
    pub(super) fn fire(&self) {
        if let Some(on_replaced) = self.0.get() {
            on_replaced();
        }
    }
}
