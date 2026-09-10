//! Why a volume id has no volume registered under it.
//!
//! Every site that looks a volume up and finds nothing asks this one question, so
//! a listing, a copy or move, a delete, a copy preview, and a new folder all tell
//! the same story about the same id:
//!
//! - **Not connected**: a device its provider lists (an ADB phone before its
//!   pane's connect lands, or after an eject), or a saved SFTP / WebDAV server,
//!   that nothing has connected. Opening it in a pane is what connects it.
//! - **Gone**: any other id, which is a volume that left the registry (an unmount
//!   race).
//!
//! ❗ Asking never dials: both answers come from cached state (the provider's last
//! device list, the saved-server stores). ❌ Never word a not-connected id as
//! `DeviceDisconnected`, which says a session dropped that never existed, or as
//! `NotFound`, which the frontend reads as "this folder was deleted" and walks the
//! pane off the device. Each caller maps the answer into its own vocabulary.

/// Why no volume is registered under an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unregistered {
    /// A listed device or a saved server that nothing has connected yet.
    NotConnected,
    /// An id nothing lists or saves: a volume that left the registry.
    Gone,
}

/// Classifies an id the volume registry had nothing for. Ask it only after a
/// lookup came back empty: it doesn't consult the registry itself.
pub(crate) async fn why_unregistered(volume_id: &str) -> Unregistered {
    let listed = crate::device_volumes::provider_for_volume_id(volume_id).await.is_some()
        || crate::server_volumes::place_root(volume_id).is_some();
    if listed {
        Unregistered::NotConnected
    } else {
        Unregistered::Gone
    }
}
