//! Auto-connect for a device the hotplug watcher sees, retried for as long as it
//! stays plugged in.
//!
//! An Android phone that's replugged can hold its MTP session back until the
//! user answers "Allow access to phone data?" on the phone. The open then runs
//! out its 30 s transfer bound and fails with `Timeout`, and the phone stays
//! enumerated the whole time, so no hotplug event ever comes to trigger a second
//! try. Retrying here, keyed on the watcher's own view of the device, is what
//! makes such a phone show up once the prompt is answered.
//!
//! Two pieces, both pure so they test without a device or a clock:
//!
//! - [`KnownDevices`]: the devices the watcher last saw, each with the
//!   [`Arrival`] that saw it come. A retry loop holds its arrival and stops the
//!   moment it's no longer current, which covers an unplug AND a replug (a new
//!   arrival starts its own loop, so the old one must not keep going). It also
//!   holds which phones have a waiting row: an open can run for 30 s or forever,
//!   and the user needs to see the phone meanwhile.
//! - [`connect_while_present`]: the loop itself, with the connect, the presence
//!   check, and the sleep handed in.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::time::Duration;

use cmdr_fs::log_error::LogError;
use log::{debug, info};

use super::{MtpConnectionError, MtpDeviceInfo};

/// How long an auto-connect runs before the phone gets a row saying what's up. A
/// healthy open takes a fraction of a second (verified on a Pixel 9 Pro XL, dev
/// build, `attempt 1` in 0.2 s, 2026-10-30, #834), and a row that flashes up for
/// those would only be noise. An open that stalls runs for 30 s, far too long to
/// say nothing.
pub(super) const WAITING_ROW_DELAY: Duration = Duration::from_secs(3);

/// One sighting of a device: from the diff that saw it arrive to the one that saw
/// it leave. A replug is a new `Arrival`, even under the same device id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Arrival(u64);

/// What one reconciliation changed.
#[derive(Debug, Default)]
pub(super) struct DeviceChanges {
    /// Devices that weren't there before, each with its fresh arrival.
    pub(super) arrived: Vec<(String, Arrival)>,
    /// Devices that were there and are gone.
    pub(super) left: Vec<String>,
    /// Whether one of `left` had a waiting row, so the volume list has to hear about it.
    pub(super) a_waiting_row_left: bool,
}

/// Where a device's auto-connect stands.
#[derive(Debug)]
enum ConnectPhase {
    /// Under way, with nothing said about it yet.
    Trying,
    /// Under way for long enough that the phone has a row.
    Waiting(MtpDeviceInfo),
    /// Over, however it ended. Its row, if it had one, is gone, and ❗ a timer still
    /// counting can't bring it back: that's the race this state exists for.
    Settled,
}

#[derive(Debug)]
struct KnownDevice {
    arrival: Arrival,
    phase: ConnectPhase,
}

/// The devices the hotplug watcher currently sees, when each one arrived, and how
/// far its auto-connect got.
///
/// ❗ The waiting rows live here, on the arrival, rather than in a list of their own:
/// an unplug, a replug, and MTP being turned off all end the row by the same stroke
/// that ends the arrival, even while an attempt is still running out its 30 s.
#[derive(Debug, Default)]
pub(super) struct KnownDevices {
    present: HashMap<String, KnownDevice>,
    next_arrival: u64,
}

impl KnownDevices {
    /// Makes `current` the known set and reports what changed. A device that
    /// stays keeps its arrival; a new one gets a fresh one.
    pub(super) fn reconcile(&mut self, current: &HashSet<String>) -> DeviceChanges {
        let left: Vec<String> = self
            .present
            .keys()
            .filter(|id| !current.contains(*id))
            .cloned()
            .collect();
        let mut a_waiting_row_left = false;
        for id in &left {
            if let Some(gone) = self.present.remove(id) {
                a_waiting_row_left |= matches!(gone.phase, ConnectPhase::Waiting(_));
            }
        }

        let mut arrived = Vec::new();
        for id in current {
            if !self.present.contains_key(id) {
                let arrival = Arrival(self.next_arrival);
                self.next_arrival += 1;
                self.present.insert(
                    id.clone(),
                    KnownDevice {
                        arrival,
                        phase: ConnectPhase::Trying,
                    },
                );
                arrived.push((id.clone(), arrival));
            }
        }
        DeviceChanges {
            arrived,
            left,
            a_waiting_row_left,
        }
    }

    /// Whether `device_id` is still here from the SAME arrival. False once it
    /// left, and false after a replug, whose own loop takes over.
    pub(super) fn is_still_present(&self, device_id: &str, arrival: Arrival) -> bool {
        self.current(device_id, arrival).is_some()
    }

    /// Whether this arrival's auto-connect is still under way with no row yet.
    pub(super) fn is_still_trying(&self, device_id: &str, arrival: Arrival) -> bool {
        self.current(device_id, arrival)
            .is_some_and(|known| matches!(known.phase, ConnectPhase::Trying))
    }

    /// Gives the phone its row, if this arrival is still trying. Returns whether it did.
    pub(super) fn mark_waiting(&mut self, device_id: &str, arrival: Arrival, device: MtpDeviceInfo) -> bool {
        match self.present.get_mut(device_id) {
            Some(known) if known.arrival == arrival && matches!(known.phase, ConnectPhase::Trying) => {
                known.phase = ConnectPhase::Waiting(device);
                true
            }
            _ => false,
        }
    }

    /// Ends this arrival's auto-connect. Returns whether that took a row away.
    pub(super) fn settle(&mut self, device_id: &str, arrival: Arrival) -> bool {
        match self.present.get_mut(device_id) {
            Some(known) if known.arrival == arrival => {
                let had_row = matches!(known.phase, ConnectPhase::Waiting(_));
                known.phase = ConnectPhase::Settled;
                had_row
            }
            _ => false,
        }
    }

    /// The phones with a row, in a stable order so the list doesn't reshuffle between publishes.
    pub(super) fn waiting_devices(&self) -> Vec<MtpDeviceInfo> {
        let mut waiting: Vec<MtpDeviceInfo> = self
            .present
            .values()
            .filter_map(|known| match &known.phase {
                ConnectPhase::Waiting(device) => Some(device.clone()),
                ConnectPhase::Trying | ConnectPhase::Settled => None,
            })
            .collect();
        waiting.sort_by(|a, b| a.id.cmp(&b.id));
        waiting
    }

    /// Forgets every device, so the next reconciliation sees them all as new.
    /// Returns whether that took a row away.
    pub(super) fn clear(&mut self) -> bool {
        let had_row = !self.waiting_devices().is_empty();
        self.present.clear();
        had_row
    }

    fn current(&self, device_id: &str, arrival: Arrival) -> Option<&KnownDevice> {
        self.present.get(device_id).filter(|known| known.arrival == arrival)
    }
}

/// Whether a failed auto-connect is worth trying again while the device stays
/// plugged in.
///
/// Only failures that a wait can fix: the phone holding its session back behind
/// the "Allow access" prompt (`Timeout`), a device busy with something else, or
/// a session that died during the open. Everything else either has its own way
/// back (the ptpcamerad and udev dialogs carry a Retry button, and re-emitting
/// their event every few seconds would re-raise the dialog), means the device is
/// gone, or is something a retry would only repeat.
///
/// Exhaustive on purpose, so a new variant has to decide.
pub(super) fn is_worth_retrying(error: &MtpConnectionError) -> bool {
    match error {
        MtpConnectionError::Timeout { .. }
        | MtpConnectionError::DeviceBusy { .. }
        | MtpConnectionError::SessionReset { .. } => true,
        MtpConnectionError::DeviceNotFound { .. }
        | MtpConnectionError::NotConnected { .. }
        | MtpConnectionError::ExclusiveAccess { .. }
        | MtpConnectionError::Disconnected { .. }
        | MtpConnectionError::Protocol { .. }
        | MtpConnectionError::StorageFull { .. }
        | MtpConnectionError::StoreReadOnly { .. }
        | MtpConnectionError::PermissionDenied { .. }
        | MtpConnectionError::ReadRefused { .. }
        | MtpConnectionError::Cancelled { .. }
        | MtpConnectionError::ObjectNotFound { .. }
        | MtpConnectionError::AmbiguousName { .. }
        | MtpConnectionError::DirectoryNotEmpty { .. }
        | MtpConnectionError::StaleParentHandle { .. }
        | MtpConnectionError::NameTooLong { .. }
        | MtpConnectionError::Other { .. } => false,
    }
}

/// The pause before the first retry. Long enough that a busy device isn't
/// hammered, short enough that a prompt answered right away connects promptly.
const FIRST_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Ceiling for the pause, so a phone left locked for an hour costs a USB open
/// every ~45 s (this pause plus the open's own 30 s bound), and one unlocked at
/// minute 59 still connects within seconds of the next try.
const MAX_RETRY_DELAY: Duration = Duration::from_secs(15);

/// After this many attempts the per-attempt log line drops to `debug`, so a
/// phone left locked overnight doesn't fill the log.
const LOUD_ATTEMPTS: u32 = 5;

/// How long to wait before retry `retry` (0-based). Grows 1.5x per retry, capped
/// at [`MAX_RETRY_DELAY`], never zero.
pub(super) fn retry_delay(retry: u32) -> Duration {
    FIRST_RETRY_DELAY
        .mul_f64(1.5_f64.powi(retry.min(16) as i32))
        .min(MAX_RETRY_DELAY)
}

/// How an auto-connect ended.
#[derive(Debug)]
pub(super) enum AutoConnectEnd<T> {
    /// The device connected on attempt `attempts`.
    Connected { value: T, attempts: u32 },
    /// The device refused in a way a retry won't fix (see [`is_worth_retrying`]).
    Refused { error: MtpConnectionError, attempts: u32 },
    /// The device left, was replugged, or MTP was turned off while waiting.
    Withdrawn { attempts: u32 },
}

/// Connects, and on a retryable failure waits and tries again for as long as
/// `still_present` says the device is the one this loop was started for.
///
/// `still_present` is checked after each pause and before each retry, never
/// during an attempt: an attempt in flight always runs to its own end, because
/// dropping an mtp-rs future mid-transaction wedges the phone until a replug.
pub(super) async fn connect_while_present<T, Connect, ConnectFut, Present, Sleep, SleepFut>(
    device_id: &str,
    mut connect: Connect,
    still_present: Present,
    mut sleep: Sleep,
) -> AutoConnectEnd<T>
where
    Connect: FnMut() -> ConnectFut,
    ConnectFut: Future<Output = Result<T, MtpConnectionError>>,
    Present: Fn() -> bool,
    Sleep: FnMut(Duration) -> SleepFut,
    SleepFut: Future<Output = ()>,
{
    let mut attempts: u32 = 0;
    loop {
        attempts = attempts.saturating_add(1);
        let error = match connect().await {
            Ok(value) => return AutoConnectEnd::Connected { value, attempts },
            Err(error) if !is_worth_retrying(&error) => return AutoConnectEnd::Refused { error, attempts },
            Err(error) => error,
        };

        let delay = retry_delay(attempts - 1);
        if attempts <= LOUD_ATTEMPTS {
            info!(
                "MTP {device_id}: auto-connect attempt {attempts} didn't take ({e}); retrying in {delay:?} while the device stays plugged in",
                e = LogError(&error)
            );
        } else {
            debug!(
                "MTP {device_id}: auto-connect attempt {attempts} didn't take ({e}); retrying in {delay:?}",
                e = LogError(&error)
            );
        }
        sleep(delay).await;

        if !still_present() {
            return AutoConnectEnd::Withdrawn { attempts };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn timeout() -> MtpConnectionError {
        MtpConnectionError::Timeout {
            device_id: "mtp-phone".to_string(),
        }
    }

    /// The Pixel case from #834: the open times out while the phone waits on its
    /// "Allow access" prompt, and nothing re-triggers a connect. The loop must
    /// keep trying until the prompt is answered.
    #[tokio::test]
    async fn a_timeout_is_retried_until_the_device_connects() {
        let calls = Cell::new(0u32);
        let slept = RefCell::new(Vec::new());
        let end = connect_while_present(
            "mtp-phone",
            || {
                calls.set(calls.get() + 1);
                let n = calls.get();
                async move { if n < 3 { Err(timeout()) } else { Ok("connected") } }
            },
            || true,
            |d| {
                slept.borrow_mut().push(d);
                std::future::ready(())
            },
        )
        .await;

        assert!(
            matches!(
                end,
                AutoConnectEnd::Connected {
                    value: "connected",
                    attempts: 3
                }
            ),
            "got {end:?}"
        );
        assert_eq!(*slept.borrow(), vec![retry_delay(0), retry_delay(1)]);
    }

    #[tokio::test]
    async fn retrying_stops_once_the_device_is_no_longer_present() {
        let calls = Cell::new(0u32);
        let end: AutoConnectEnd<()> = connect_while_present(
            "mtp-phone",
            || {
                calls.set(calls.get() + 1);
                async { Err(timeout()) }
            },
            || false,
            |_| std::future::ready(()),
        )
        .await;

        assert!(matches!(end, AutoConnectEnd::Withdrawn { attempts: 1 }), "got {end:?}");
        assert_eq!(calls.get(), 1, "no attempt may run after the device left");
    }

    /// ExclusiveAccess raises the ptpcamerad dialog, which has its own Retry.
    /// Looping would re-raise it every few seconds.
    #[tokio::test]
    async fn a_failure_a_wait_cannot_fix_ends_the_loop_without_sleeping() {
        let slept = Cell::new(0u32);
        let end: AutoConnectEnd<()> = connect_while_present(
            "mtp-phone",
            || async {
                Err(MtpConnectionError::ExclusiveAccess {
                    device_id: "mtp-phone".to_string(),
                    blocking_process: None,
                })
            },
            || true,
            |_| {
                slept.set(slept.get() + 1);
                std::future::ready(())
            },
        )
        .await;

        assert!(
            matches!(
                end,
                AutoConnectEnd::Refused {
                    error: MtpConnectionError::ExclusiveAccess { .. },
                    attempts: 1
                }
            ),
            "got {end:?}"
        );
        assert_eq!(slept.get(), 0);
    }

    #[test]
    fn only_failures_a_wait_can_fix_are_retried() {
        let id = || "mtp-phone".to_string();
        assert!(is_worth_retrying(&MtpConnectionError::Timeout { device_id: id() }));
        assert!(is_worth_retrying(&MtpConnectionError::DeviceBusy { device_id: id() }));
        assert!(is_worth_retrying(&MtpConnectionError::SessionReset { device_id: id() }));

        assert!(!is_worth_retrying(&MtpConnectionError::ExclusiveAccess {
            device_id: id(),
            blocking_process: None,
        }));
        assert!(!is_worth_retrying(&MtpConnectionError::PermissionDenied {
            device_id: id()
        }));
        assert!(!is_worth_retrying(&MtpConnectionError::DeviceNotFound {
            device_id: id()
        }));
        assert!(!is_worth_retrying(&MtpConnectionError::Disconnected {
            device_id: id()
        }));
    }

    #[test]
    fn retries_are_spaced_grow_and_cap() {
        assert!(retry_delay(0) >= Duration::from_secs(1), "never a tight loop");
        for retry in 1..40 {
            assert!(retry_delay(retry) >= retry_delay(retry - 1));
            assert!(retry_delay(retry) <= MAX_RETRY_DELAY);
        }
        assert_eq!(retry_delay(39), MAX_RETRY_DELAY);
        assert_eq!(retry_delay(u32::MAX), MAX_RETRY_DELAY);
    }

    fn ids(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_device_that_stays_keeps_its_arrival() {
        let mut known = KnownDevices::default();
        let first = known.reconcile(&ids(&["mtp-A"]));
        let (_, arrival) = first.arrived[0].clone();

        let again = known.reconcile(&ids(&["mtp-A"]));
        assert!(again.arrived.is_empty() && again.left.is_empty());
        assert!(known.is_still_present("mtp-A", arrival));
    }

    #[test]
    fn an_unplug_ends_the_arrival() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();

        let changes = known.reconcile(&ids(&[]));
        assert_eq!(changes.left, vec!["mtp-A".to_string()]);
        assert!(!known.is_still_present("mtp-A", arrival));
    }

    /// A replug starts its own retry loop, so the old one must see itself
    /// retired even though the device id is back.
    #[test]
    fn a_replug_is_a_new_arrival_that_retires_the_old_one() {
        let mut known = KnownDevices::default();
        let (_, old) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        known.reconcile(&ids(&[]));
        let (_, new) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();

        assert_ne!(old, new);
        assert!(!known.is_still_present("mtp-A", old));
        assert!(known.is_still_present("mtp-A", new));
    }

    fn phone(id: &str) -> MtpDeviceInfo {
        MtpDeviceInfo {
            id: id.to_string(),
            location_id: 1,
            vendor_id: 0x18d1,
            product_id: 0x4ee1,
            manufacturer: Some("Google".to_string()),
            product: Some("Pixel 9 Pro XL".to_string()),
            serial_number: Some("TESTSERIAL0001".to_string()),
            usb_speed: None,
        }
    }

    fn waiting_ids(known: &KnownDevices) -> Vec<String> {
        known.waiting_devices().into_iter().map(|d| d.id).collect()
    }

    #[test]
    fn a_new_device_has_no_row_until_its_connect_has_run_a_while() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        assert!(known.is_still_trying("mtp-A", arrival));
        assert!(waiting_ids(&known).is_empty());

        assert!(known.mark_waiting("mtp-A", arrival, phone("mtp-A")));
        assert_eq!(waiting_ids(&known), ["mtp-A"]);
        assert!(!known.is_still_trying("mtp-A", arrival), "it has its row now");
    }

    #[test]
    fn a_connect_that_ends_takes_the_row_away() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        known.mark_waiting("mtp-A", arrival, phone("mtp-A"));

        assert!(known.settle("mtp-A", arrival), "the volume list has to hear about it");
        assert!(waiting_ids(&known).is_empty());
        assert!(
            known.is_still_present("mtp-A", arrival),
            "settled, and still plugged in"
        );
    }

    /// The race the `Settled` state exists for: the delay's timer runs on its own task, so it
    /// can fire just after a fast connect finished. A row there would stick around forever.
    #[test]
    fn a_timer_firing_after_the_connect_ended_shows_no_row() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        assert!(!known.settle("mtp-A", arrival), "no row to take away");

        assert!(!known.is_still_trying("mtp-A", arrival));
        assert!(!known.mark_waiting("mtp-A", arrival, phone("mtp-A")));
        assert!(waiting_ids(&known).is_empty());
    }

    /// An attempt in flight runs out its 30 s, so nothing but the arrival can take the row away
    /// the moment the cable comes out.
    #[test]
    fn an_unplug_takes_the_row_away_at_once_and_says_so() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        known.mark_waiting("mtp-A", arrival, phone("mtp-A"));

        assert!(known.reconcile(&ids(&[])).a_waiting_row_left);
        assert!(waiting_ids(&known).is_empty());
    }

    #[test]
    fn an_unplug_with_no_row_has_nothing_to_announce() {
        let mut known = KnownDevices::default();
        known.reconcile(&ids(&["mtp-A"]));
        assert!(!known.reconcile(&ids(&[])).a_waiting_row_left);
    }

    /// The old loop's timer and its end both still hold the OLD arrival.
    #[test]
    fn the_old_arrivals_loop_cannot_touch_the_replugged_device() {
        let mut known = KnownDevices::default();
        let (_, old) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        known.reconcile(&ids(&[]));
        let (_, new) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        assert!(known.mark_waiting("mtp-A", new, phone("mtp-A")));

        assert!(!known.settle("mtp-A", old));
        assert_eq!(waiting_ids(&known), ["mtp-A"], "the new arrival's row stays");
        assert!(!known.mark_waiting("mtp-A", old, phone("mtp-A")));
    }

    #[test]
    fn turning_mtp_off_takes_every_row_away_and_says_so() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        known.mark_waiting("mtp-A", arrival, phone("mtp-A"));

        assert!(known.clear());
        assert!(waiting_ids(&known).is_empty());
        assert!(!known.clear(), "nothing left to take away");
    }

    #[test]
    fn waiting_rows_keep_one_order_between_publishes() {
        let mut known = KnownDevices::default();
        for (id, arrival) in known.reconcile(&ids(&["mtp-C", "mtp-A", "mtp-B"])).arrived {
            known.mark_waiting(&id, arrival, phone(&id));
        }
        assert_eq!(waiting_ids(&known), ["mtp-A", "mtp-B", "mtp-C"]);
    }

    /// Turning MTP off clears the set, so turning it back on sees every
    /// plugged-in device as new and connects it.
    #[test]
    fn clearing_makes_every_device_new_again() {
        let mut known = KnownDevices::default();
        let (_, arrival) = known.reconcile(&ids(&["mtp-A"])).arrived[0].clone();
        known.clear();

        assert!(!known.is_still_present("mtp-A", arrival));
        let changes = known.reconcile(&ids(&["mtp-A"]));
        assert_eq!(changes.arrived.len(), 1);
    }
}
