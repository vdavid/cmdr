//! The one volume-list pipeline, cross-platform.
//!
//! Discovery is per-platform (`volumes/` on macOS, `volumes_linux/` on Linux,
//! `stubs/volumes.rs` elsewhere), but nothing downstream of it is: the
//! `list_volumes` IPC call and the `volumes-changed` push publish the same list
//! to the same frontend, so they have to assemble it the same way. This module
//! owns the platform aliases and the assembly, and every consumer goes through
//! [`complete`] rather than re-deriving the steps. Device-backed volumes (MTP,
//! ADB) come from `device_volumes`, which also answers the per-path questions
//! (`device_volume_for_path`, `device_space_for_path`).

use std::time::Duration;

// ============================================================================
// Platform aliases
// ============================================================================

#[cfg(target_os = "macos")]
pub(crate) use crate::volumes::{LocationCategory, LocationInfo};

#[cfg(target_os = "linux")]
pub(crate) use crate::volumes_linux::{LocationCategory, LocationInfo};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) use crate::stubs::volumes::{LocationCategory, VolumeInfo as LocationInfo};

#[cfg(target_os = "macos")]
fn list_locations() -> Vec<LocationInfo> {
    crate::volumes::list_locations()
}

#[cfg(target_os = "linux")]
fn list_locations() -> Vec<LocationInfo> {
    crate::volumes_linux::list_locations()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn list_locations() -> Vec<LocationInfo> {
    crate::stubs::volumes::list_volumes()
}

// ============================================================================
// The pipeline
// ============================================================================

/// How one attempt at discovering the platform's volumes ended.
pub(crate) enum ListingOutcome {
    /// The listing returned.
    Listed(Vec<LocationInfo>),
    /// The listing didn't finish inside the caller's timeout.
    TimedOut,
    /// The blocking task panicked, so no list is coming.
    Panicked,
}

/// Discovers the platform's mounted volumes off the async thread, bounded by
/// `timeout`.
///
/// Discovery blocks: it stats mount points, and one hung mount can hold a
/// syscall for 30-120 s. Running it on the async thread wedges the IPC handler,
/// which looks to the user like a frozen app, so there is no unbounded path to
/// discovery in this module.
pub(crate) async fn discover_local(timeout: Duration) -> ListingOutcome {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(list_locations)).await {
        Ok(Ok(volumes)) => ListingOutcome::Listed(volumes),
        Ok(Err(e)) => {
            crate::log_error!("volume listing: spawn_blocking panicked: {}", e);
            ListingOutcome::Panicked
        }
        Err(_) => {
            log::warn!("volume listing: discovery timed out after {:?}", timeout);
            ListingOutcome::TimedOut
        }
    }
}

/// Completes a local listing into the list every consumer publishes: appends
/// every device provider's storages (`device_volumes`), then enriches every
/// entry from the volume registry.
///
/// **The order is the reason this function exists.** Enrichment copies across
/// what only the registered `Volume` knows (its capability surface, and on macOS
/// its SMB connection state), and device storages are registered volumes too, so
/// appending them after enrichment ships mobile devices to the frontend with
/// `capabilities: None`, and the pane falls back to per-kind defaults instead of
/// what the backend actually offers. Callers hand over a local listing and get
/// the finished list back; they can't get the order wrong.
pub(crate) async fn complete(local: Vec<LocationInfo>) -> Vec<LocationInfo> {
    let mut volumes = local;

    crate::device_volumes::append_device_volumes(&mut volumes).await;

    #[cfg(target_os = "macos")]
    crate::volumes::enrich_from_volume_registry(&mut volumes);
    #[cfg(target_os = "linux")]
    crate::volumes_linux::enrich_from_volume_registry(&mut volumes);

    volumes
}

/// Discovers and completes in one call: what a caller with nowhere to fall back
/// to wants. Returns the list and whether discovery came up short, which the
/// frontend voices as "some volumes may be missing" next to a retry.
///
/// A panic reports the same way a timeout does. Both mean the local half is
/// absent and re-running is the only move the user has.
pub(crate) async fn list_with_timeout(timeout: Duration) -> (Vec<LocationInfo>, bool) {
    let (local, timed_out) = match discover_local(timeout).await {
        ListingOutcome::Listed(volumes) => (volumes, false),
        ListingOutcome::TimedOut | ListingOutcome::Panicked => (Vec::new(), true),
    };
    (complete(local).await, timed_out)
}
