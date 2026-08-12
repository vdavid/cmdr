//! Volume wiring: the backends, the registry, and teardown.
//!
//! The `Volume` trait itself, its sub-traits, the data types it exchanges, the
//! volume ID helpers, and the typed error classification all live in
//! `cmdr_fs::volume` — the index speaks in them and must not need the app. They
//! are re-exported here, so callers keep importing `volume::Volume`,
//! `volume::VolumeError`, `volume::smb_volume_id`, and so on unchanged.
//!
//! What can't move sits below: the real-storage backends (`backends`, with their
//! `smb2` / `mtp-rs` / git / mount-detection dependencies), the process-wide
//! `manager` registry, and macOS/Linux `eject`.

pub use cmdr_fs::volume::*;

// Per-backend `Volume` implementations live in `backends/`. The trait surface
// stays here; submodule names are re-exported below so external callers keep
// importing `volume::LocalPosixVolume`, `volume::MtpVolume`, etc. without
// caring about the `backends/` split.
pub mod backends;
// Volume teardown (USB/SD/DMG/SMB/MTP), used only by the macOS+Linux eject
// command. The macOS-vs-Linux difference (diskutil vs umount, NSURL vs
// `/sys/block`) lives inside via per-fn `#[cfg]`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod eject;
pub(crate) mod manager;

pub use backends::LocalPosixVolume;
pub(crate) use backends::rename_local_exclusive;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backends::{MtpVolume, SmbVolume};

// `smb` is re-exported as a module path because callers reach into it for
// `SmbConnectionParams` / `connect_smb_volume` / `set_app_handle`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backends::smb;

/// Observe a failed volume operation and, when its errno PROVES the active mount
/// root is gone, hand the volume's ID to a live sibling mount.
///
/// The lazy half of "liveness outranks path shape". A volume ID can be reached
/// through several mount points and the registry keeps the canonical-looking one
/// active, which is right until that mount wedges: macOS leaves a dead
/// `/Volumes/naspi` in place and lands the reconnect at `/Volumes/naspi-1`, and
/// the shortest-path rule then picks the corpse on every launch. Nothing may
/// PROBE a mount to find out (a `statfs` on a wedged one blocks 30–120 s, see
/// `volumes/DETAILS.md` § "Hung mounts"), so the evidence has to arrive as a
/// failed operation. Call this wherever one is observed with its volume ID in
/// hand; it does no I/O, and an errno that says something about the FILE rather
/// than the mount changes nothing.
pub fn note_root_failure(volume_id: &str, error: &VolumeError) {
    let VolumeError::IoError {
        raw_os_error: Some(errno),
        ..
    } = error
    else {
        return;
    };
    if !manager::is_stale_mount_errno(*errno) {
        return;
    }

    let registry = manager::get_volume_manager();
    let Some(volume) = registry.get(volume_id) else {
        return;
    };
    let failed_root = volume.root().to_path_buf();

    if let manager::StaleRootOutcome::Promoted { new_root } = registry.mark_root_stale(volume_id, &failed_root) {
        log::info!(
            target: "cmdr_lib::file_system::volume",
            "Volume {volume_id}'s mount at {} is gone (errno {errno}); promoted it to {}, which is still live.",
            failed_root.display(),
            new_root.display(),
        );
        // The switcher and every pane still point at the old root, so tell them.
        crate::volume_broadcast::emit_volumes_changed();
    }
}

#[cfg(test)]
mod inmemory_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod mtp_scan_oracle_tests;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod smb_index_scan_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod smb_scan_oracle_tests;
