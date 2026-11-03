//! One `connect()` per device at a time.
//!
//! Three callers open a device on their own schedule: the app's hotplug
//! auto-connect retry loop, the user's connect (the dialogs' Retry), and the
//! session-reset reopen loop. The registry check in `connect()` is a
//! check-then-act, so two that overlapped both opened the device: on real USB
//! the loser's open reads as exclusive access and raises the ptpcamerad dialog
//! for a phone that just connected, and where both opens succeed the second
//! entry replaces the first and orphans its session.
//!
//! The gate is per device, so one phone stalling its open for 30 s never holds
//! up another. A caller that waits here holds nothing on the wire, so dropping
//! the wait is safe; the caller that got through re-checks the registry and
//! returns the session the first one opened.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::sync::OwnedMutexGuard;

/// The per-device connect slots. A slot exists only while someone holds or
/// waits for it, so a manager that saw a hundred phones keeps no hundred locks.
#[derive(Default)]
pub(super) struct ConnectGates {
    slots: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

/// The right to open `device_id`. Dropping it lets the next waiter in.
pub(super) struct ConnectSlot<'a> {
    gates: &'a ConnectGates,
    device_id: String,
    guard: Option<OwnedMutexGuard<()>>,
}

impl ConnectGates {
    /// Waits until no other connect for `device_id` is under way, then holds the slot.
    pub(super) async fn claim(&self, device_id: &str) -> ConnectSlot<'_> {
        let slot = Arc::clone(
            self.slots
                .lock_ignore_poison()
                .entry(device_id.to_string())
                .or_default(),
        );
        // A waiter dropped here (its caller gave up) leaves its clone behind for
        // a moment; the holder's `Drop` counts clones, so the slot stays until
        // the last one is gone.
        let guard = slot.lock_owned().await;
        ConnectSlot {
            gates: self,
            device_id: device_id.to_string(),
            guard: Some(guard),
        }
    }

    /// How many devices have a slot right now.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.slots.lock_ignore_poison().len()
    }

    /// How many connects for `device_id` hold or wait for its slot right now.
    #[cfg(test)]
    pub(super) fn claimants(&self, device_id: &str) -> usize {
        self.slots
            .lock_ignore_poison()
            .get(device_id)
            .map_or(0, |slot| Arc::strong_count(slot) - 1)
    }
}

impl Drop for ConnectSlot<'_> {
    fn drop(&mut self) {
        // Release first, so the guard's clone of the slot is gone before counting.
        drop(self.guard.take());
        let mut slots = self.gates.slots.lock_ignore_poison();
        // Only the map's own clone left means nobody holds or waits for it. A new
        // claimant clones under this same lock, so nothing can sneak in between.
        if slots
            .get(&self.device_id)
            .is_some_and(|slot| Arc::strong_count(slot) == 1)
        {
            slots.remove(&self.device_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::testing::wait_until_async;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn a_second_connect_for_the_same_device_waits_for_the_first() {
        let gates = Arc::new(ConnectGates::default());
        let first = gates.claim("mtp-A").await;

        let waiter = {
            let gates = Arc::clone(&gates);
            tokio::spawn(async move {
                let _slot = gates.claim("mtp-A").await;
            })
        };
        wait_until_async(WAIT, "the second connect to queue", || gates.claimants("mtp-A") == 2).await;
        assert!(!waiter.is_finished(), "it must wait while the first connect runs");

        drop(first);
        waiter.await.unwrap();
        assert_eq!(gates.len(), 0, "a finished connect leaves no slot behind");
    }

    /// A phone stalling its open for 30 s mustn't hold up another phone.
    #[tokio::test]
    async fn different_devices_never_wait_on_each_other() {
        let gates = ConnectGates::default();
        let _a = gates.claim("mtp-A").await;
        let _b = tokio::time::timeout(Duration::from_secs(1), gates.claim("mtp-B"))
            .await
            .expect("another device's slot is free");
        assert_eq!(gates.len(), 2);
    }

    /// The slot outlives its holder while someone else still waits for it.
    #[tokio::test]
    async fn the_slot_stays_while_a_waiter_is_queued() {
        let gates = Arc::new(ConnectGates::default());
        let first = gates.claim("mtp-A").await;
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let waiter = {
            let gates = Arc::clone(&gates);
            tokio::spawn(async move {
                let _slot = gates.claim("mtp-A").await;
                let _ = released.await;
            })
        };
        wait_until_async(WAIT, "the second connect to queue", || gates.claimants("mtp-A") == 2).await;
        drop(first);
        assert_eq!(gates.len(), 1, "the waiter still needs it");
        let _ = release.send(());
        waiter.await.unwrap();
        assert_eq!(gates.len(), 0);
    }

    /// A waiter whose caller gave up holds nothing on the wire, so dropping it is safe and
    /// leaves nothing behind.
    #[tokio::test]
    async fn a_waiter_that_gives_up_leaves_no_slot_behind() {
        let gates = ConnectGates::default();
        let first = gates.claim("mtp-A").await;
        assert!(
            tokio::time::timeout(Duration::from_millis(10), gates.claim("mtp-A"))
                .await
                .is_err()
        );
        drop(first);
        assert_eq!(gates.len(), 0);
    }
}
