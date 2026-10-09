//! Volume eject: takes a volume away, whichever kind it is (USB, SD, DMG, SMB,
//! MTP, ADB, SFTP, WebDAV).
//!
//! Dispatches by volume kind:
//! - **Device volume** (a `device_volumes::DeviceVolumeProvider` owns the id):
//!   hands the eject to that provider. MTP closes the device session and the
//!   `mtp-device-disconnected` event removes its storages from the picker; ADB
//!   has nothing to detach (`adb` has no per-client detach), so it retires the
//!   volume and hides the device until it re-enumerates.
//! - **Remote session** (`BackendKind::detaches_by_session_drop`: SFTP, WebDAV):
//!   drops the session through `commands::servers::disconnect_place_inner`. The
//!   server stays SAVED; forgetting it is `forget_server`, a different act. ❗
//!   Asked BEFORE the mount questions, since `is_ejectable` is a question about a
//!   mount and a dialed server has no mount-table row to answer it.
//! - **SMB volume** (registered `SmbVolume` in `VolumeManager`): runs `diskutil
//!   unmount`. FSEvents fires `NSWorkspaceDidUnmount`, which calls
//!   `Volume::on_unmount` (drops the smb2 session, stops the watcher) and the
//!   volume manager unregisters it. Same pattern as `disconnect_smb_volume`.
//! - **Physical or disk-image volume** (NSURL reports `isEjectable`): runs
//!   `diskutil eject`. On USB drives this also powers the device down so it's
//!   safe to unplug; on DMG-mounted disk images, `eject` is the verb that
//!   detaches the image (`unmount` would leave it attached).
//!
//! Volumes that can't be detached at all return an error.
//!
//! Every teardown that reaches a device provider, a remote session, or the
//! unmount tool (`unmount_tool`) runs through [`run_teardown`], the one place a
//! refusal is logged.
//!
//! Both answers are typed, because an agent reaches this through the MCP `eject`
//! tool and may not branch on a sentence: [`EjectOutcome`] says which teardown
//! ran, [`EjectError`] says why one didn't. The `commands::eject` IPC layer is a
//! thin delegate over [`eject`], so nothing is flattened on the way out.

mod answers;
mod deadlines;
#[cfg(target_os = "macos")]
mod disk_flight;
#[cfg(target_os = "macos")]
mod disk_target;
pub mod holders;
mod in_flight;
mod unmount_tool;

// Real-image pins of today's eject; `#[ignore]`d, hand-run (see the module).
#[cfg(all(test, target_os = "macos"))]
mod real_image;

use std::path::PathBuf;
use std::sync::Arc;

use crate::device_volumes::DeviceVolumeProvider;
use unmount_tool::UnmountVerb;

// The whole vocabulary, so a caller reaches `eject::EjectError` rather than
// knowing which file inside the module holds it.
pub use answers::*;
pub(crate) use deadlines::INDEX_STOP_DEADLINE;
pub(crate) use in_flight::is_ejecting;
pub use in_flight::{VolumesEjectingChanged, ejecting_volume_ids, init_ejecting_volume_emitter};
pub(in crate::file_system::volume) use unmount_tool::is_still_mounted;

/// Ejects a volume. Picks the right teardown for the volume's kind.
///
/// One eject at a time per volume: a call for a volume whose eject is still
/// running joins it and gets the same answer (`in_flight`), so a repeat click,
/// the native menu, and MCP never start a second teardown.
///
/// Answers which teardown ran ([`EjectOutcome`]) once the unmount or disconnect
/// is initiated. The frontend shouldn't wait for the volume to fully disappear:
/// `volume-unmounted` (for disk volumes) or `mtp-device-disconnected` (for MTP)
/// will fire shortly after and panes rooted at the volume redirect to root.
pub async fn eject(volume_id: &str) -> Result<EjectOutcome, EjectError> {
    let owned_id = volume_id.to_string();
    in_flight::join_or_start(volume_id, move || eject_now(owned_id)).await
}

/// The eject itself. [`eject`] runs at most one of these per volume at a time.
async fn eject_now(volume_id: String) -> Result<EjectOutcome, EjectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume_id = volume_id.as_str();

    // Safety gate: never tear down a volume while a write op is reading from or
    // writing to it. The picker disables Eject for busy volumes, so reaching
    // here means a race (or an MCP / automation caller); refuse rather than
    // disconnect mid-transfer and risk a truncated file. See the volume picker's
    // `volumes-busy-changed` wiring.
    if crate::file_system::busy_volume_ids().iter().any(|id| id == volume_id) {
        return Err(EjectError::Busy);
    }

    // A device provider answers for its own volumes from live state, so an id
    // that merely looks device-shaped can't route an eject at nothing. Asked
    // first: MTP storages aren't registered under a mount path at all.
    let provider = crate::device_volumes::provider_for_volume_id(volume_id).await;

    let (mount_path, is_smb, is_remote_session) = if provider.is_some() {
        (String::new(), false, false)
    } else {
        let volume = get_volume_manager()
            .get(volume_id)
            .ok_or_else(|| nothing_registered_under(volume_id))?;
        let mount_path = volume.root().to_string_lossy().to_string();
        // A drive whose eject just landed lingers in the switcher until
        // `volumes-changed` arrives, and a click there would read "not ejectable"
        // from `resolve_is_ejectable` on a path that's gone. Its goal is met.
        if is_already_unmounted(volume_id, || is_still_mounted(&mount_path)) {
            log::info!(
                target: "eject",
                "{volume_id} at {mount_path} is no longer mounted, so there's nothing left to eject"
            );
            return Ok(EjectOutcome::AlreadyGone);
        }
        let is_smb = volume.backend_kind() == cmdr_fs::volume::BackendKind::Smb;
        (mount_path, is_smb, volume.backend_kind().detaches_by_session_drop())
    };

    // For physical volumes, ejectability comes from NSURL (macOS) /
    // `/sys/block/*/removable` (Linux). Look it up via the fast statfs-based
    // resolver instead of enumerating all volumes. Under a deadline: a disk image
    // backed by a file on a hung share can block this for good, and a stalled
    // check answers `NotResponding`, ❌ never a guessed "not ejectable".
    let is_ejectable = if provider.is_some() || is_smb || is_remote_session {
        false
    } else {
        deadlines::within_deadline(
            EjectStep::EjectabilityCheck,
            volume_id,
            deadlines::EJECTABILITY_CHECK_DEADLINE,
            resolve_is_ejectable(mount_path.clone()),
        )
        .await?
    };

    let action = decide_eject_action(&EjectContext {
        volume_id,
        is_ejectable,
        is_smb,
        is_remote_session,
        device_provider: provider.as_ref().map(|p| p.id()),
    })
    .map_err(EjectError::from)?;

    match action {
        EjectAction::DeviceDisconnect { volume_id, .. } => {
            let provider = provider.ok_or_else(|| EjectError::VolumeNotFound {
                volume_id: volume_id.clone(),
            })?;
            run_teardown(&volume_id, Teardown::Device(provider)).await?;
            Ok(EjectOutcome::DeviceDisconnected)
        }
        // No index to stop first: a server's scheme root is nothing the index has a
        // transport for (`BackendKind::can_be_indexed`).
        EjectAction::RemoteDisconnect { volume_id } => {
            run_teardown(&volume_id, Teardown::Remote).await?;
            Ok(EjectOutcome::RemoteDisconnected)
        }
        // For disk volumes, stop the index BEFORE the unmount (the wedge-safe point).
        // A device provider tears its index down through its own disconnect hook,
        // so it isn't stopped here.
        EjectAction::DiskutilUnmount => {
            eject_one_volume(volume_id, &mount_path, UnmountVerb::Unmount).await?;
            Ok(EjectOutcome::Unmounted)
        }
        // A `diskutil eject` takes the whole PHYSICAL disk down, so every volume on it
        // is gated and stopped first, and success means the whole disk went.
        #[cfg(target_os = "macos")]
        EjectAction::DiskutilEject => {
            let resolved =
                deadlines::within_deadline(EjectStep::DiskResolve, volume_id, deadlines::DISK_RESOLVE_DEADLINE, {
                    let mount_path = mount_path.clone();
                    async move { tokio::task::spawn_blocking(move || disk_target::resolve(&mount_path)).await }
                })
                .await?;
            match resolved {
                Ok(disk_target::Resolution::Disk(target)) => {
                    disk_flight::eject_disk(volume_id, &mount_path, target).await
                }
                // Its root left the mount table while the eject was getting ready: the
                // person's goal is met, the same as `is_already_unmounted`.
                Ok(disk_target::Resolution::Gone) => {
                    log::info!(target: "eject", "{volume_id} at {mount_path} left the mount table before its eject ran");
                    Ok(EjectOutcome::AlreadyGone)
                }
                // Mounted, but no physical disk backs it (a macFUSE mount): today's
                // per-volume teardown is the honest answer.
                Ok(disk_target::Resolution::NoDisk) => {
                    log::info!(target: "eject", "No physical disk backs {volume_id} at {mount_path}; ejecting the volume alone");
                    eject_one_volume(volume_id, &mount_path, UnmountVerb::Eject).await?;
                    // The volume alone left the mount table: no physical disk powered down,
                    // so ❌ never `DiskEjected`.
                    Ok(EjectOutcome::Unmounted)
                }
                Err(join_err) => Err(EjectError::Unexpected {
                    detail: format!("{}: {join_err}", EjectStep::DiskResolve),
                }),
            }
        }
        #[cfg(not(target_os = "macos"))]
        EjectAction::DiskutilEject => {
            eject_one_volume(volume_id, &mount_path, UnmountVerb::Eject).await?;
            Ok(EjectOutcome::Unmounted)
        }
    }
}

/// Why nothing is registered under `volume_id`: a saved server nobody is
/// connected to, or an id the app doesn't know at all.
///
/// ❗ Asked of the places store, ❌ never of the id's SHAPE. A saved place is the
/// common way to meet this (a pinned server sits in the switcher disconnected),
/// and it deserves words about a server rather than about a drive.
fn nothing_registered_under(volume_id: &str) -> EjectError {
    if crate::server_volumes::place_root(volume_id).is_some() {
        return EjectError::RemoteNotConnected {
            volume_id: volume_id.to_string(),
        };
    }
    EjectError::VolumeNotFound {
        volume_id: volume_id.to_string(),
    }
}

/// Drops the session behind a remote place (SFTP, WebDAV).
///
/// ❗ Dropping the session IS the clean shutdown and it can't refuse, so the one
/// unhappy answer is that there was no live session left to close. Goes through
/// `disconnect_place_inner`, which also emits the event that sends a pane
/// standing on the place home.
async fn disconnect_remote_session(volume_id: &str) -> Result<(), EjectError> {
    if crate::commands::servers::disconnect_place_inner(volume_id).await {
        return Ok(());
    }
    Err(EjectError::RemoteNotConnected {
        volume_id: volume_id.to_string(),
    })
}

/// Stops one volume's index and tears down that volume alone: an SMB share, a mount
/// no physical disk backs, and every Linux eject.
async fn eject_one_volume(volume_id: &str, mount_path: &str, verb: UnmountVerb) -> Result<(), EjectError> {
    let teardown = Teardown::Tool {
        verb,
        mount_path,
        #[cfg(target_os = "macos")]
        disk: None,
    };
    stop_index_then_unmount(
        volume_id,
        stop_index_blocking(volume_id.to_string(), stop_removable_index),
        || run_teardown(volume_id, teardown),
    )
    .await
}

/// One teardown [`run_teardown`] performs.
enum Teardown<'a> {
    /// Hand it to the device provider that owns the volume (MTP, ADB).
    Device(Arc<dyn DeviceVolumeProvider>),
    /// Drop a remote server's session (SFTP, WebDAV). The wiring unregisters the
    /// volume and retires it, so its watcher and reconnect loop stand down.
    Remote,
    /// Run `diskutil` / `umount` against the volume's mount root, or, with `disk` set,
    /// against a whole physical disk: each attempt aims at one of the disk's mounts
    /// that's still listed, and success needs every one of them gone.
    Tool {
        verb: UnmountVerb,
        mount_path: &'a str,
        #[cfg(target_os = "macos")]
        disk: Option<&'a disk_flight::DiskTeardown>,
    },
}

/// Runs one teardown and reports how it went. A refused unmount is retried first,
/// in `unmount_tool::settle_with_retries`, which also writes the tool's log lines.
///
/// ❗ The ONE place a teardown refusal is logged: every eject and SMB disconnect
/// that reaches a device provider or the unmount tool passes through here, so a
/// refusal can't go unlogged or be logged twice. It has to be Rust's log, because
/// the frontend's line (`wordEjectRefusal`) is a warn, which a production build drops.
async fn run_teardown(volume_id: &str, teardown: Teardown<'_>) -> Result<(), EjectError> {
    match teardown {
        Teardown::Device(provider) => {
            // Under a detached deadline: MTP closes its session when the last handle
            // drops, which a wedged phone can stall. Expiry answers `TimedOut`, which
            // is TRUE here: the disconnect already started and runs on to its end.
            let task_provider = Arc::clone(&provider);
            let task_volume_id = volume_id.to_string();
            let result = crate::deadline::timeout_detached_typed(
                deadlines::DEVICE_EJECT_DEADLINE,
                || EjectError::TimedOut,
                |detail| EjectError::Unexpected { detail },
                async move {
                    task_provider
                        .eject(&task_volume_id)
                        .await
                        .map_err(|detail| EjectError::DeviceDisconnectRefused {
                            provider: task_provider.id().to_string(),
                            detail,
                        })
                },
            )
            .await;
            if let Err(error) = &result {
                log::warn!(
                    target: "eject",
                    "{} disconnect of {volume_id} didn't go through: {error}",
                    provider.id()
                );
            }
            result
        }
        Teardown::Remote => {
            let result = disconnect_remote_session(volume_id).await;
            if let Err(error) = &result {
                log::warn!(target: "eject", "The disconnect of {volume_id} didn't go through: {error}");
            }
            result
        }
        Teardown::Tool {
            verb,
            mount_path,
            #[cfg(target_os = "macos")]
            disk,
        } => {
            let target = unmount_tool::Target {
                volume_id,
                verb,
                mount_path,
            };
            #[cfg(target_os = "macos")]
            if let Some(disk) = disk {
                let settled = unmount_tool::settle_with_retries(
                    target,
                    || {
                        // ❗ Re-aimed per attempt: a partial unmount leaves the volume the
                        // person clicked gone, and a retry aimed there proves nothing.
                        let path = disk.aim(mount_path);
                        async move { unmount_tool::run_keeping_abandoned(verb, &path, disk.abandoned()).await }
                    },
                    || disk.is_still_mounted(),
                )
                .await;
                return name_the_holders(volume_id, settled, disk.captured(), deadlines::HOLDER_BUDGET).await;
            }
            let settled = unmount_tool::settle_with_retries(
                target,
                || unmount_tool::run(verb, mount_path),
                || is_still_mounted(mount_path),
            )
            .await;
            name_the_holders(
                volume_id,
                settled,
                &[PathBuf::from(mount_path)],
                deadlines::HOLDER_BUDGET,
            )
            .await
        }
    }
}

/// Names who held the drive on a refusal: ONE scan, here, after the last attempt.
///
/// ❗ Once, and after the retries. `proc_listpidspath` took 142 ms to 9.4 s under load,
/// so a scan inside `settle_with_retries` would multiply that by up to four attempts,
/// and three of those answers would be about holds that had already let go. Every
/// teardown passes through here, so the disk flight and the per-volume path (SMB,
/// macFUSE, Linux) are named the same way, and a flight that hands an index back has
/// already named its holders.
///
/// Only the mounts still in the table are scanned: a volume that really went takes its
/// holders with it. ❗ An empty list of those is `Incomplete`, ❌ never "nobody is
/// holding it".
///
/// `budget` is a parameter so the real-image lane can give a loaded machine more room
/// than a person waiting on a spinner would.
async fn name_the_holders(
    volume_id: &str,
    settled: Result<(), EjectError>,
    paths: &[PathBuf],
    budget: std::time::Duration,
) -> Result<(), EjectError> {
    let Err(EjectError::UnmountRefused { detail, .. }) = settled else {
        return settled;
    };
    let listed: Vec<PathBuf> = paths
        .iter()
        .filter(|path| is_still_mounted(&path.to_string_lossy()))
        .cloned()
        .collect();
    let holders = holders::scan(listed, budget).await;
    log::info!(target: "eject", "The refused unmount of {volume_id} is {holders}");
    Err(EjectError::UnmountRefused { holders, detail })
}

/// Whether a registered volume's eject is already done before anything runs: its
/// mount root has left the OS mount table. Pure: `still_mounted` is the
/// non-probing table read ([`unmount_tool::is_still_mounted`]).
///
/// Trusts "not listed" only for an ID whose scheme names a mount
/// ([`VolumeScheme::is_mount_backed`](cmdr_fs::volume::VolumeScheme::is_mount_backed)), and doesn't read the table
/// for any other: a cloud drive's root is a plain folder that was never listed, so
/// answering `Ok` for it would be a false success where `NotEjectable` is right.
fn is_already_unmounted(volume_id: &str, still_mounted: impl FnOnce() -> bool) -> bool {
    cmdr_fs::volume::VolumeScheme::of(volume_id).is_mount_backed() && !still_mounted()
}

/// Stop the volume's index (if any) BEFORE running the unmount/eject.
///
/// This is the ONE reliable wedge-safe point: releasing the FSEvents watcher +
/// open SQLite handles while the filesystem is still healthy is the only thing that
/// keeps an open stream/handle from wedging a FSKit (`msdos`) unmount (see
/// `crates/cmdr-index/src/indexing/transports/DETAILS.md` § "Unmount/eject lifecycle"
/// and the 2026-07-15 kernel panic). The ordering is unconditional: the index stop is awaited to completion,
/// then the unmount runs. ❗ The stop runs under [`deadlines::INDEX_STOP_DEADLINE`],
/// and any stop that can't say the index let go of the volume (it didn't finish,
/// the index was still releasing, or the stop panicked) answers WITHOUT running the
/// unmount: an index that may still hold the volume must never meet one.
/// `stop_index` and `unmount` are parameters so the ordering and the stall can be
/// asserted in a test without a real volume or `diskutil`. No-op stop for an
/// unindexed volume.
async fn stop_index_then_unmount<S, F, Fut>(volume_id: &str, stop_index: S, unmount: F) -> Result<(), EjectError>
where
    S: Future<Output = Result<(), EjectError>> + Send + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), EjectError>>,
{
    deadlines::within_deadline(EjectStep::IndexStop, volume_id, INDEX_STOP_DEADLINE, stop_index).await??;
    unmount().await
}

/// The index stop an eject runs: the drive's own index, waiting for it to let go
/// of the drive for as long as the eject waits for the stop.
fn stop_removable_index(volume_id: &str) -> cmdr_index::RemovableStop {
    crate::index_host::index().stop_removable_volume(volume_id, INDEX_STOP_DEADLINE)
}

/// How the pre-unmount index stop of a drive's volumes ended.
///
/// The release rides along for the per-disk flight, which hands back what DID let go;
/// only macOS has one, so off it nothing reads the release.
#[cfg_attr(
    not(target_os = "macos"),
    expect(dead_code, reason = "only the macOS disk flight hands a partial release back")
)]
enum IndexStopped {
    /// Every volume let go of the drive.
    LetGo(super::drive_release::Release),
    /// At least one didn't, so ❌ nothing unmounts. The release still says which
    /// volumes DID let go, which is what a disk flight hands back.
    Refused {
        release: Option<super::drive_release::Release>,
        error: EjectError,
    },
}

/// Stop `volume_id`'s index through the drive-release gate, awaited so the stop
/// COMPLETES before the caller unmounts.
async fn stop_index_blocking(volume_id: String, stop: fn(&str) -> cmdr_index::RemovableStop) -> Result<(), EjectError> {
    let late_volume_id = volume_id.clone();
    let stopped = stop_indexes_blocking(vec![volume_id], stop, move |late| {
        log::info!(
            target: "eject",
            "the index for {late_volume_id} answered after the eject stopped waiting: {:?}",
            late.outcome
        );
    })
    .await;
    match stopped {
        IndexStopped::LetGo(_) => Ok(()),
        IndexStopped::Refused { error, .. } => Err(error),
    }
}

/// Stop every volume of the drive through the drive-release gate, all under one
/// deadline, awaited so the stops COMPLETE before the caller unmounts. Per volume the
/// gate moves its epoch, waits for a start in flight (a person's enable still probing
/// the drive) to return, then runs `stop`, all inside
/// [`deadlines::INDEX_STOP_DEADLINE`]. It blocks for that long at most, so it runs on
/// the blocking pool.
///
/// Only a `LocalExternal` index is stopped here: it's the one carrying an FSEvents
/// watcher + open SQLite handles that can wedge a FSKit (`msdos`) unmount, and it's
/// the kind whose DB stays usable via a later reconcile. SMB/MTP indexes tear down
/// through their own disconnect paths and stay registered (Stale, offline-browsable)
/// across an eject, so this must not remove them. No-op for a non-`LocalExternal` or
/// unindexed volume. `stop` and `record_late` are parameters so a test can hand in an
/// answer and a flight can hand a late release back to the gate.
async fn stop_indexes_blocking(
    volume_ids: Vec<String>,
    stop: fn(&str) -> cmdr_index::RemovableStop,
    record_late: impl Fn(super::drive_release::LateRelease) + Send + Sync + 'static,
) -> IndexStopped {
    use super::drive_release::{self, VolumeRelease};
    use std::sync::atomic::{AtomicBool, Ordering};

    // The gate reads a stop that panicked as still releasing; this remembers the
    // panic so the answer can say nobody knows how the stop ended.
    let panicked = Arc::new(AtomicBool::new(false));
    let guarded_stop = {
        let panicked = Arc::clone(&panicked);
        move |id: &str| {
            std::panic::catch_unwind(|| stop(id)).unwrap_or_else(|_| {
                panicked.store(true, Ordering::SeqCst);
                cmdr_index::RemovableStop::StillReleasing
            })
        }
    };
    let asked = volume_ids.join(", ");
    let released = tokio::task::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + INDEX_STOP_DEADLINE;
        drive_release::gate().release(&volume_ids, deadline, guarded_stop, record_late)
    })
    .await;

    let release = match released {
        Ok(release) => release,
        Err(join_err) => {
            log::warn!(target: "eject", "index-stop task for {asked} failed to join: {join_err}; leaving the drive mounted");
            return IndexStopped::Refused {
                release: None,
                error: EjectError::Unexpected {
                    detail: format!("{}: {join_err}", EjectStep::IndexStop),
                },
            };
        }
    };
    let stuck: Vec<&str> = release
        .volumes
        .iter()
        .filter(|volume| volume.outcome == VolumeRelease::StillReleasing)
        .map(|volume| volume.volume_id.as_str())
        .collect();
    if stuck.is_empty() {
        return IndexStopped::LetGo(release);
    }
    // A stop that panicked says nothing about whether the index let go. ❌ Never read
    // either one as done.
    let error = if panicked.load(Ordering::SeqCst) {
        log::warn!(target: "eject", "the index stop for {stuck:?} ended without an answer; leaving the drive mounted");
        EjectError::Unexpected {
            detail: format!("{}: the stop ended without an answer", EjectStep::IndexStop),
        }
    } else {
        log::warn!(
            target: "eject",
            "the index for {stuck:?} was still letting go of the drive when its stop ran out of time; leaving it mounted"
        );
        EjectError::NotResponding {
            step: EjectStep::IndexStop,
        }
    };
    IndexStopped::Refused {
        release: Some(release),
        error,
    }
}

/// Disconnects a single SMB volume by tearing down its OS mount.
///
/// The "Disconnect" affordance in the pane's reconnect view and the gave-up
/// `VolumeUnreachableBanner` calls this (via the `disconnect_smb_volume`
/// command). On macOS it runs `diskutil unmount`; FSEvents then drives the
/// standard `Volume::on_unmount` → `VolumeManager`-removal pipeline (same as an
/// SMB [`eject`]): `SmbVolume::on_unmount` flips `unmounted=true`, stops the
/// watcher task, and drops the smb2 session, then a `volumes-changed` event
/// flows to the frontend.
///
/// On other platforms the OS-level unmount isn't wired up yet (mirrors
/// `network::mount::unmount_smb_shares_from_host`), so it drops the smb2 session
/// directly via `Volume::on_unmount`; the OS mount stays alive for the user to
/// eject from the file manager.
///
/// Unlike [`eject`], this has no busy gate: the Disconnect affordance targets a
/// reconnecting or unreachable volume, so there's nothing actively transferring.
///
/// Errors:
/// - [`EjectError::VolumeNotFound`] if the id isn't registered (a race).
/// - [`EjectError::NotAnSmbVolume`] when the volume isn't SMB (a race or
///   automation caller; the UI only offers Disconnect for SMB volumes).
pub async fn disconnect_smb(volume_id: &str) -> Result<(), EjectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(volume_id)
        .ok_or_else(|| EjectError::VolumeNotFound {
            volume_id: volume_id.to_string(),
        })?;

    if volume.backend_kind() != cmdr_fs::volume::BackendKind::Smb {
        return Err(EjectError::NotAnSmbVolume {
            volume_id: volume_id.to_string(),
        });
    }

    #[cfg(target_os = "macos")]
    {
        let mount_path = volume.root().to_string_lossy().to_string();
        let teardown = Teardown::Tool {
            verb: UnmountVerb::Unmount,
            mount_path: &mount_path,
            disk: None,
        };
        run_teardown(volume_id, teardown).await?;
        // FSEvents will fire shortly and trigger on_unmount + volume-manager removal.
    }

    #[cfg(not(target_os = "macos"))]
    {
        volume.on_unmount();
        log::info!(
            target: "eject",
            "Dropped smb2 session for {} (OS unmount not yet implemented on this platform)",
            volume_id
        );
    }

    Ok(())
}

/// Looks up `is_ejectable` for the volume at `mount_path` via the per-path
/// statfs/NSURL fast resolver. Avoids the full volume enumeration. Blocks without
/// limit on a hung backing store, so its caller puts it under
/// [`deadlines::EJECTABILITY_CHECK_DEADLINE`].
async fn resolve_is_ejectable(mount_path: String) -> bool {
    #[cfg(target_os = "macos")]
    {
        let path = mount_path;
        tokio::task::spawn_blocking(move || {
            crate::volumes::resolve_path_volume_fast(&path)
                .map(|v| v.is_ejectable)
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        let path = mount_path;
        tokio::task::spawn_blocking(move || {
            crate::volumes_linux::list_locations()
                .into_iter()
                .find(|v| v.path == path)
                .map(|v| v.is_ejectable)
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
