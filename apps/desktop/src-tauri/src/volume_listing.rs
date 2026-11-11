//! The one volume-list pipeline, cross-platform.
//!
//! Discovery is per-platform (the `volumes` seam), but nothing downstream of it
//! is: the `list_volumes` IPC call and the `volumes-changed` push publish the
//! same list to the same frontend, so they have to assemble it the same way. This
//! module owns the assembly, and every consumer goes through
//! [`complete`] rather than re-deriving the steps. Device-backed volumes (MTP,
//! ADB) come from `device_volumes`, which also answers the per-path questions
//! (`device_volume_for_path`, `device_space_for_path`).

use std::time::Duration;

use crate::volumes::list_locations;
pub(crate) use crate::volumes::{LocationCategory, LocationInfo, NetworkMount};

/// The network mount `path` lies on: the deepest mount point that's a whole-segment prefix of it.
/// `None` when that mount is a local disk, or the table couldn't be read. Lexical, so a symlink on
/// the way isn't followed.
pub(crate) use crate::volumes::network_mount_under;

// ============================================================================
// The hub row
// ============================================================================

/// What the synthetic hub row is called: the row that opens the servers hub.
///
/// This is the name agents see in every UI language (this row's own `VolumeInfo` in
/// `commands/volumes.rs`, the MCP volumes resource, and `tools/actions/nav.rs`'s selector), and it
/// equals the English `fileExplorer.navigation.networkVolume` catalog value. It says nothing about
/// where a pane is: the frontend pushes its UI language's word, so that's told by the id, `network`.
///
/// The GROUP the row sits in is still called "Network"
/// (`fileExplorer.navigation.groupNetwork`), and the row's path is still the
/// `smb://` sentinel: the label grew up, the identity didn't.
pub(crate) const SERVERS_VOLUME_NAME: &str = "Servers";

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
/// every device provider's storages (`device_volumes`) and every server place
/// (`server_volumes`), enriches every entry from the volume registry, then lets
/// the favorites reach pass (`favorites/reach.rs`) decide every favorite row's
/// target from the finished rows.
///
/// **The order is the reason this function exists.** Enrichment copies across
/// what only the registered `Volume` knows (its capability surface and its
/// connection state), and device storages and server places are registered
/// volumes too, so appending them after enrichment ships mobile devices and live
/// servers to the frontend with `capabilities: None`, and the pane falls back to
/// per-kind defaults instead of what the backend actually offers. Callers hand
/// over a local listing and get the finished list back; they can't get the order
/// wrong.
pub(crate) async fn complete(local: Vec<LocationInfo>) -> Vec<LocationInfo> {
    #[cfg(target_os = "macos")]
    let local = mark_shared_disks(local).await;
    let mut volumes = local;

    crate::device_volumes::append_device_volumes(&mut volumes).await;
    // Servers: the registered SFTP and WebDAV volumes, plus the pinned saved
    // ones. ❗ Before enrichment for the same reason device storages are, and
    // from CACHED state only: this runs on every `volumes-changed`, and a probe
    // here would turn a refresh into a round of network traffic.
    crate::server_volumes::append_server_volumes(&mut volumes);

    crate::volumes::enrich_from_volume_registry(&mut volumes);

    // Favorites last: the reach pass reads every row's FINAL connection state and capabilities,
    // and rows only (`favorites/reach.rs`). The claims it makes for legacy entries are a file
    // write, so they persist off this path.
    let claims = crate::favorites::reach::annotate(&mut volumes, &crate::favorites::reach::ReachFacts::now());
    if !claims.is_empty() {
        // A claim that doesn't land is re-made by the next listing.
        tauri::async_runtime::spawn_blocking(move || {
            crate::favorites::store::claim_volumes(&claims);
        });
    }

    volumes
}

/// How long asking which rows share a physical disk may hold a listing up. The answer is IOKit and
/// the mount table, so only a wedged daemon gets near this.
#[cfg(target_os = "macos")]
const SHARED_DISK_DEADLINE: Duration = Duration::from_secs(2);

/// Says which rows share a physical disk, off this task, under a deadline. A stalled answer leaves
/// every row reading "shares nothing": that costs the "Eject all" item for one listing, and each
/// Eject still reads the disk for itself.
#[cfg(target_os = "macos")]
async fn mark_shared_disks(local: Vec<LocationInfo>) -> Vec<LocationInfo> {
    // The usual listing: rows carry their icons, so don't copy them for nothing.
    if local.iter().filter(|row| row.is_ejectable).count() < 2 {
        return local;
    }
    let fallback = local.clone();
    let marked = cmdr_fs::deadline::timeout_detached_typed(SHARED_DISK_DEADLINE, || (), |_detail| (), async move {
        tokio::task::spawn_blocking(move || {
            let mut rows = local;
            crate::file_system::volume::eject::mark_shared_disks(&mut rows, crate::file_system::volume::eject::disk_at);
            rows
        })
        .await
        .map_err(|_join_err| ())
    })
    .await;
    marked.unwrap_or_else(|()| {
        log::warn!(target: "eject", "Couldn't tell which volumes share a disk in time, so none offers Eject all this time");
        fallback
    })
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
