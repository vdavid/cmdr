//! Recovery from a PTP session reset (`MtpConnectionError::SessionReset`).
//!
//! A session reset is NOT a disconnect: the device is still plugged in, only its
//! PTP session died, so the sibling of `handle_device_disconnected` here drops
//! the dead session and reopens it instead of tearing the device out of the
//! sidebar. Hardware evidence and the reopen sequence: [DETAILS.md](DETAILS.md)
//! § "Session reset is not a disconnect".

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::Duration;

use log::{info, warn};
use tauri::AppHandle;

use super::{MtpConnectionError, MtpConnectionManager, connection_manager};
use crate::ignore_poison::IgnorePoison;

/// How many times we try to reopen the device after a session reset.
///
/// The first one or two attempts are EXPECTED to fail (`Timeout`, then
/// `SessionAlreadyOpen`), so "try once and give up" would declare a device dead
/// that is two seconds from working. Ten spaced attempts span ~100 s, which
/// covers every recovery observed on hardware with room to spare.
pub(super) const MAX_REOPEN_ATTEMPTS: u32 = 10;

/// The quiet pause before the FIRST reopen attempt. The device needs a moment
/// with nothing on the wire after the drain; reopening immediately re-wedges it.
const FIRST_PAUSE: Duration = Duration::from_millis(1_500);

/// Ceiling for the spaced backoff, so a long recovery still polls at a useful
/// rate instead of drifting into minutes.
const MAX_DELAY: Duration = Duration::from_secs(15);

/// Devices whose recovery is already in flight, so N operations failing against
/// the same dead session schedule ONE recovery.
static RECOVERING: LazyLock<StdMutex<HashSet<String>>> = LazyLock::new(|| StdMutex::new(HashSet::new()));

/// How long to wait before reopen attempt `attempt` (0-based).
///
/// ❌ Never zero and never a tight loop: hammering a freshly reset device
/// re-wedges it into a hard `Timeout` (observed on hardware, and mtp-rs's own
/// notes say the same). Growth is 1.5×, capped at [`MAX_DELAY`].
pub(super) fn reopen_delay(attempt: u32) -> Duration {
    let scaled = FIRST_PAUSE.mul_f64(1.5_f64.powi(attempt.min(MAX_REOPEN_ATTEMPTS) as i32));
    scaled.min(MAX_DELAY)
}

/// Marks `device_id` as recovering. `false` when a recovery is already running
/// for it (the caller should do nothing).
fn claim_recovery(device_id: &str) -> bool {
    RECOVERING.lock_ignore_poison().insert(device_id.to_string())
}

fn release_recovery(device_id: &str) {
    RECOVERING.lock_ignore_poison().remove(device_id);
}

/// Kicks off recovery for a device whose PTP session just reset, if one isn't
/// already running. Fire-and-forget: the failing operation returns its own
/// retryable error immediately rather than waiting for the reopen.
///
/// Called from `map_mtp_error`'s `DeviceReset` arm, which is the one choke point
/// every device operation funnels through. Outside a tokio runtime (pure unit
/// tests calling the mapper directly) it's a no-op.
pub(super) fn schedule_recovery(device_id: &str) {
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    if !claim_recovery(device_id) {
        return;
    }
    let device_id = device_id.to_string();
    tokio::spawn(async move {
        let app = crate::mtp::watcher::app_handle();
        connection_manager()
            .handle_device_session_reset(&device_id, app.as_ref())
            .await;
        release_recovery(&device_id);
    });
}

impl MtpConnectionManager {
    /// Recovers a device whose PTP session reset: drop the dead session, then
    /// reopen it with spaced backoff. The sibling of `handle_device_disconnected`
    /// — ❌ never call that one for a reset, it would remove a live device from
    /// the sidebar and can't reopen anything.
    ///
    /// ❌ Never add a USB transport reset between the two steps: on Android it's
    /// a kill switch that costs the user a replug, and the reopen self-heals
    /// without it. Evidence: [DETAILS.md](DETAILS.md) § "No transport reset in
    /// recovery"; enforced by `pnpm check mtp-no-transport-reset`.
    pub(super) async fn handle_device_session_reset(&self, device_id: &str, app: Option<&AppHandle>) {
        if !self.tear_down_reset_session(device_id).await {
            return;
        }
        self.reopen_after_session_reset(device_id, app).await;
    }

    /// Drops the dead session: removes the `DeviceEntry` (closing the `MtpDevice`
    /// and taking its path / listing / storage caches with it) and stops the
    /// event loop. Returns `false` when the device wasn't in the registry.
    ///
    /// The caches MUST go: object handles don't survive the reset, and a stale
    /// reverse `PathHandleCache` entry resolves a NEW object to a dead path
    /// (devices reuse handles). They live on the entry, so dropping it clears
    /// them.
    ///
    /// ❌ Does NOT emit `MtpDeviceDisconnected` and does NOT unregister the
    /// volumes: the device is still attached, so it stays in the sidebar while
    /// the reopen runs.
    pub(super) async fn tear_down_reset_session(&self, device_id: &str) -> bool {
        let removed = {
            let mut devices = self.devices.lock().await;
            devices.remove(device_id).is_some()
        };
        if !removed {
            return false;
        }
        self.stop_event_loop(device_id);

        // Watch continuity broke exactly as it does on an unplug: events fired
        // while the session was dead are lost, and the handles the index stored
        // per entry may no longer identify the same objects. A Fresh badge would
        // lie, so every indexed storage on the device goes Stale.
        crate::indexing::on_mtp_watch_continuity_lost(device_id);

        warn!(
            "MTP {device_id}: PTP session reset, dropped the dead session and will reopen it (the device is still attached)"
        );
        true
    }

    /// Reopens the device with idle-spaced backoff.
    ///
    /// ❌ Don't collapse this into a single retry. On hardware the first
    /// attempt returned `Timeout` and the second `SessionAlreadyOpen` before the
    /// third succeeded; giving up early would declare a live device dead. And
    /// don't tighten the spacing either — hammering re-wedges the device. See
    /// [DETAILS.md](DETAILS.md) § "Session reset is not a disconnect".
    pub(super) async fn reopen_after_session_reset(&self, device_id: &str, app: Option<&AppHandle>) {
        for attempt in 0..MAX_REOPEN_ATTEMPTS {
            tokio::time::sleep(reopen_delay(attempt)).await;

            if !crate::mtp::watcher::is_mtp_enabled() {
                info!("MTP {device_id}: reopen abandoned, MTP support was turned off");
                return;
            }

            match self.connect(device_id, app).await {
                Ok(info) => {
                    info!(
                        "MTP {device_id}: reopened after a session reset on attempt {} ({} storages)",
                        attempt + 1,
                        info.storages.len()
                    );
                    crate::volume_broadcast::emit_volumes_changed();
                    return;
                }
                // The device really is gone (unplugged while recovering), so this
                // IS a disconnect now: run the real teardown and stop retrying.
                Err(MtpConnectionError::DeviceNotFound { .. }) => {
                    info!("MTP {device_id}: gone while recovering from a session reset, treating it as a disconnect");
                    self.handle_device_disconnected(device_id, app).await;
                    return;
                }
                Err(e) => {
                    info!(
                        "MTP {device_id}: reopen attempt {} of {MAX_REOPEN_ATTEMPTS} didn't take yet ({e})",
                        attempt + 1
                    );
                }
            }
        }
        // allowed-pluralize-noun: MAX_REOPEN_ATTEMPTS is a compile-time constant of 10, never 1
        warn!("MTP {device_id}: couldn't reopen after {MAX_REOPEN_ATTEMPTS} attempts; unplug and replug the device");
        self.handle_device_disconnected(device_id, app).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopen_is_always_spaced_never_a_tight_loop() {
        // Hammering a freshly reset device re-wedges it into a hard `Timeout`, so
        // every attempt — the first one included — waits.
        for attempt in 0..MAX_REOPEN_ATTEMPTS {
            assert!(
                reopen_delay(attempt) >= Duration::from_secs(1),
                "attempt {attempt} must wait at least a second, got {:?}",
                reopen_delay(attempt),
            );
        }
    }

    #[test]
    fn reopen_delay_grows_then_caps() {
        // Idle-spaced: each attempt backs off further than the last, until the
        // ceiling keeps a long recovery polling at a useful rate.
        for attempt in 1..MAX_REOPEN_ATTEMPTS {
            let previous = reopen_delay(attempt - 1);
            let current = reopen_delay(attempt);
            assert!(
                current >= previous,
                "attempt {attempt} ({current:?}) must not be shorter than {previous:?}",
            );
            assert!(current <= MAX_DELAY, "attempt {attempt} must stay under the cap");
        }
        assert_eq!(reopen_delay(MAX_REOPEN_ATTEMPTS - 1), MAX_DELAY);
    }

    #[test]
    fn the_attempt_budget_covers_a_slow_recovery_without_running_forever() {
        // Recovery on hardware took three attempts; the budget has to leave room
        // for a slower device without retrying into the next coffee break.
        let total: Duration = (0..MAX_REOPEN_ATTEMPTS).map(reopen_delay).sum();
        assert!(total >= Duration::from_secs(30), "budget too short: {total:?}");
        assert!(total <= Duration::from_secs(180), "budget too long: {total:?}");
    }

    #[test]
    fn one_recovery_per_device_at_a_time() {
        // Every operation queued behind the dead session fails at once; they must
        // schedule ONE reopen between them, not one each.
        let device_id = "mtp-claim-test";
        release_recovery(device_id);
        assert!(claim_recovery(device_id), "the first claim wins");
        assert!(!claim_recovery(device_id), "a second claim must be refused");
        release_recovery(device_id);
        assert!(claim_recovery(device_id), "claimable again once released");
        release_recovery(device_id);
    }
}

/// Behavior against a live (virtual) device: what the reset teardown keeps and
/// what it throws away.
#[cfg(all(test, feature = "virtual-mtp"))]
mod device_tests {
    use super::*;
    use crate::mtp::virtual_device::{
        VirtualDeviceFixture, setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };
    use std::path::Path;

    struct Device {
        id: String,
        storage_id: u32,
        fixture: VirtualDeviceFixture,
    }

    /// Connects a virtual device with the root listing primed, so the path and
    /// listing caches hold something a reset has to throw away.
    async fn connect_device() -> Device {
        let fixture = setup_virtual_mtp_device();
        let device_id = crate::mtp::list_mtp_devices()
            .into_iter()
            .find(|d| d.location_id == fixture.location_id)
            .map(|d| d.id)
            .expect("the virtual device must appear in discovery");
        let info = connection_manager()
            .connect(&device_id, None)
            .await
            .expect("virtual-mtp connect should succeed");
        let storage_id = info.storages.first().expect("a storage").id;
        connection_manager()
            .list_directory(&device_id, storage_id, "/")
            .await
            .expect("list root should succeed");
        Device {
            id: device_id,
            storage_id,
            fixture,
        }
    }

    /// Polls `condition` until it holds, or gives up. The budget covers the
    /// quiet pause plus a reopen attempt with room to spare.
    async fn wait_for(mut condition: impl FnMut() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if condition() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        condition()
    }

    async fn teardown(device: Device) {
        connection_manager()
            .disconnect(&device.id, None, crate::mtp::connection::MtpDisconnectReason::User)
            .await
            .ok();
        unregister_virtual_mtp_device(device.fixture.location_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_reset_drops_the_dead_session_and_its_handle_caches() {
        let _guard = virtual_device_test_lock().lock().await;
        let device = connect_device().await;

        assert!(
            connection_manager()
                .cached_handle_for_path(&device.id, device.storage_id, Path::new("/DCIM"))
                .await
                .is_some(),
            "the primed listing must have cached a handle to begin with",
        );

        assert!(
            connection_manager().tear_down_reset_session(&device.id).await,
            "the reset teardown must find the connected device",
        );

        assert!(
            !connection_manager().is_connected(&device.id),
            "the dead session's DeviceEntry must be gone",
        );
        assert!(
            connection_manager()
                .cached_handle_for_path(&device.id, device.storage_id, Path::new("/DCIM"))
                .await
                .is_none(),
            "handles don't survive a reset, so the path cache must be cleared",
        );

        teardown(device).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_reset_keeps_the_volume_in_the_sidebar() {
        let _guard = virtual_device_test_lock().lock().await;
        let device = connect_device().await;
        let volume_id = crate::mtp::identity::mtp_volume_id(&device.id, device.storage_id);

        assert!(
            connection_manager().tear_down_reset_session(&device.id).await,
            "the reset teardown must have run for this assertion to mean anything",
        );

        assert!(
            crate::file_system::get_volume_manager().get(&volume_id).is_some(),
            "the device is still attached, so its volume must stay registered",
        );

        teardown(device).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_reset_reopens_the_device_where_a_disconnect_would_not() {
        // The guardrail: a reset must NOT take the `handle_device_disconnected`
        // path. That one only tears down; this one comes back.
        let _guard = virtual_device_test_lock().lock().await;
        let device = connect_device().await;

        super::super::directory_ops::disconnect_test_hooks::reset_count();
        assert!(
            connection_manager().tear_down_reset_session(&device.id).await,
            "the reset teardown must run first",
        );
        assert!(
            !connection_manager().is_connected(&device.id),
            "the dead session must be gone before the reopen",
        );

        connection_manager().reopen_after_session_reset(&device.id, None).await;

        assert_eq!(
            super::super::directory_ops::disconnect_test_hooks::count(),
            0,
            "a session reset must never run the disconnect teardown",
        );
        assert!(
            connection_manager().is_connected(&device.id),
            "the device must be reopened after a session reset",
        );

        teardown(device).await;
    }

    /// The whole chain, driven through Cmdr's ordinary code path: an operation
    /// hits a reset, and the device comes back on its own.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_wedged_listing_recovers_without_dropping_the_device() {
        let _guard = virtual_device_test_lock().lock().await;
        let device = connect_device().await;
        let volume_id = crate::mtp::identity::mtp_volume_id(&device.id, device.storage_id);
        super::super::directory_ops::disconnect_test_hooks::reset_count();

        // Arm the one-shot, then drive a real listing. `/DCIM` (not `/`) so the
        // 5 s `ListingCache` entry the primed root left behind can't answer from
        // memory and skip the device entirely.
        assert!(
            mtp_rs::force_operation_wedge(crate::mtp::virtual_device::VIRTUAL_DEVICE_SERIAL),
            "the connected virtual device must be armable",
        );
        let err = connection_manager()
            .list_directory(&device.id, device.storage_id, "/DCIM")
            .await
            .expect_err("the wedged operation must fail");
        assert!(
            matches!(err, MtpConnectionError::SessionReset { .. }),
            "a wedged operation must surface as a session reset, got {err:?}",
        );

        // Recovery is fire-and-forget, so the failing op returned before the
        // spawned task had done anything. Wait for the teardown FIRST, or
        // polling straight for `is_connected` passes on the not-yet-dropped entry.
        assert!(
            wait_for(|| !connection_manager().is_connected(&device.id)).await,
            "recovery must drop the dead session",
        );
        assert!(
            wait_for(|| connection_manager().is_connected(&device.id)).await,
            "recovery must reopen the device on its own",
        );

        assert_eq!(
            super::super::directory_ops::disconnect_test_hooks::count(),
            0,
            "the device never left, so nothing may run the disconnect teardown",
        );
        assert!(
            crate::file_system::get_volume_manager().get(&volume_id).is_some(),
            "the volume must stay in the sidebar throughout the recovery",
        );
        assert!(
            connection_manager()
                .cached_handle_for_path(&device.id, device.storage_id, Path::new("/DCIM"))
                .await
                .is_none(),
            "handles don't survive a reset, so the reopened session must start with empty caches",
        );

        // The one-shot is spent: the device is genuinely usable again. Listing
        // the root first isn't ceremony: `resolve_path_to_handle` is cache-only,
        // so the emptied caches make a re-navigation start from the top, which is
        // what the pane does anyway.
        connection_manager()
            .list_directory(&device.id, device.storage_id, "/")
            .await
            .expect("the recovered device must serve listings again");
        let entries = connection_manager()
            .list_directory(&device.id, device.storage_id, "/DCIM")
            .await
            .expect("the recovered device must serve listings again");
        assert!(
            entries.iter().any(|e| e.name == "Burst"),
            "the recovered listing must show the fixture tree, got {entries:?}",
        );

        teardown(device).await;
    }
}
