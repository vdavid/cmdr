//! What the session layer tells the world when a device's life changes.
//!
//! The session layer knows WHAT happened to a device; it must not know how the
//! app says it. So it reports a typed [`MtpDeviceEvent`] through this trait and
//! the app maps each variant onto the `tauri_specta` event the frontend
//! subscribes to (`crate::mtp::events`), where every English word and every
//! `specta` derive lives.
//!
//! ## Why a crate-local trait rather than a `cmdr-fs` host seam
//!
//! The host seams (`cmdr_fs::volume::host`) are the questions EVERY backend
//! asks: what are the panes showing, where are the credentials, which runtime.
//! These five are MTP-shaped — a phone's storages arriving one at a time,
//! `ptpcamerad` holding the USB device, a missing udev rule — and no other
//! backend has them. ADB's equivalent lives beside its tracker for the same
//! reason.
//!
//! The two `ptpcamerad` events aren't here: the hotplug watcher emits them, and
//! it stays app-side.

use std::sync::{Arc, LazyLock};

use super::MtpDisconnectReason;
use crate::mtp::types::MtpStorageInfo;

/// Something that happened to a device, as a value rather than as a sentence.
#[derive(Debug, Clone)]
pub enum MtpDeviceEvent {
    /// A device came up, or a late-arriving storage was registered on one that
    /// is already up. In the late-storage case `device_name` is empty and
    /// `storages` carries only the new storage.
    Connected {
        /// The device this is about.
        device_id: String,
        /// The product name to show, or empty for a late-arriving storage.
        device_name: String,
        /// The storages now browsable.
        storages: Vec<MtpStorageInfo>,
    },
    /// A device went away, either because the user asked or because the USB
    /// stack said so.
    Disconnected {
        /// The device this is about.
        device_id: String,
        /// Which of the two it was.
        reason: MtpDisconnectReason,
    },
    /// One storage area left a device that is still connected.
    StorageRemoved {
        /// The device this is about.
        device_id: String,
        /// The PTP storage id that went away.
        storage_id: u32,
    },
    /// Opening a device failed because another process holds it exclusively
    /// (`ptpcamerad` on macOS).
    ExclusiveAccess {
        /// The device this is about.
        device_id: String,
        /// The claiming process name, when the OS would say.
        blocking_process: Option<String>,
    },
    /// Opening a device failed for lack of USB permission (Linux: no udev rule).
    ///
    /// Only Linux reports one: it's the platform where a missing udev rule is
    /// the likely cause and there's an install command to offer. The wire event
    /// is registered on every platform, so the variant stays unconditional.
    #[cfg_attr(
        not(target_os = "linux"),
        expect(dead_code, reason = "only the Linux open path can classify a permission denial")
    )]
    PermissionDenied {
        /// The device this is about.
        device_id: String,
    },
}

/// Where the session layer reports a device's lifecycle.
///
/// Fire-and-forget in both directions: nothing comes back, so ❌ don't try to
/// learn from a call whether anyone was listening.
pub trait MtpDeviceEvents: Send + Sync {
    /// Report one lifecycle event.
    fn device_event(&self, event: MtpDeviceEvent);
}

/// Nobody is listening: every event goes nowhere.
///
/// The right answer for a test, a bench, or any session driven with no window
/// open, and it's why the session layer never needs an `Option<AppHandle>`.
pub struct NoMtpDeviceEvents;

impl MtpDeviceEvents for NoMtpDeviceEvents {
    fn device_event(&self, _event: MtpDeviceEvent) {}
}

/// The shared detached sink, so a caller with nowhere to report doesn't allocate
/// one per call.
pub fn no_device_events() -> Arc<dyn MtpDeviceEvents> {
    static DETACHED: LazyLock<Arc<dyn MtpDeviceEvents>> = LazyLock::new(|| Arc::new(NoMtpDeviceEvents));
    Arc::clone(&DETACHED)
}

// `cfg(test)` alone while this module is app-resident: nothing outside the app
// crate can name the recorder, so a `testing` feature would only make it dead
// code in every non-test build. It widens to `any(test, feature = "testing")`
// with the move to `cmdr-mtp`, where `cfg(test)` is off in a consumer's test
// build and the arm would silently vanish.
#[cfg(test)]
pub use recording::RecordingMtpDeviceEvents;

#[cfg(test)]
mod recording {
    use std::sync::Mutex;

    use super::{MtpDeviceEvent, MtpDeviceEvents};
    use crate::ignore_poison::IgnorePoison;

    /// An [`MtpDeviceEvents`] that remembers what it was told, so a test can
    /// assert on the sequence a user would have seen: connect, storage arrives,
    /// session resets, device comes back.
    #[derive(Default)]
    pub struct RecordingMtpDeviceEvents {
        events: Mutex<Vec<MtpDeviceEvent>>,
    }

    impl RecordingMtpDeviceEvents {
        /// A recorder with nothing reported yet.
        pub fn new() -> Self {
            Self::default()
        }

        /// Every event reported so far, in order.
        pub fn events(&self) -> Vec<MtpDeviceEvent> {
            self.events.lock_ignore_poison().clone()
        }

        /// How many events were reported. The instrument for "one connect, not
        /// one per storage".
        pub fn count(&self) -> usize {
            self.events.lock_ignore_poison().len()
        }
    }

    impl MtpDeviceEvents for RecordingMtpDeviceEvents {
        fn device_event(&self, event: MtpDeviceEvent) {
            self.events.lock_ignore_poison().push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The detached sink is what a session with no window reports into, and it
    /// has to swallow rather than panic: every test binary and every bench runs
    /// that way, so a sink that failed here would take all of them down.
    #[test]
    fn the_detached_sink_swallows_every_variant() {
        let events = no_device_events();
        events.device_event(MtpDeviceEvent::Connected {
            device_id: "mtp-0-1".to_string(),
            device_name: "Phone".to_string(),
            storages: Vec::new(),
        });
        events.device_event(MtpDeviceEvent::StorageRemoved {
            device_id: "mtp-0-1".to_string(),
            storage_id: 65_537,
        });
        events.device_event(MtpDeviceEvent::Disconnected {
            device_id: "mtp-0-1".to_string(),
            reason: MtpDisconnectReason::Removed,
        });
    }

    /// The recorder keeps ORDER, which is the thing a lifecycle test asserts on:
    /// a device that reports its disconnect before its connect would leave the
    /// sidebar showing a phone that is gone.
    #[test]
    fn the_recorder_keeps_what_it_was_told_in_order() {
        let events = RecordingMtpDeviceEvents::new();

        events.device_event(MtpDeviceEvent::Connected {
            device_id: "mtp-0-1".to_string(),
            device_name: "Phone".to_string(),
            storages: Vec::new(),
        });
        events.device_event(MtpDeviceEvent::Disconnected {
            device_id: "mtp-0-1".to_string(),
            reason: MtpDisconnectReason::User,
        });

        assert_eq!(events.count(), 2);
        assert!(
            matches!(events.events().first(), Some(MtpDeviceEvent::Connected { device_name, .. }) if device_name == "Phone"),
            "the connect has to come first, and carry the name the sidebar shows"
        );
        assert!(
            matches!(
                events.events().get(1),
                Some(MtpDeviceEvent::Disconnected {
                    reason: MtpDisconnectReason::User,
                    ..
                })
            ),
            "the reason has to survive: `User` and `Removed` read very differently in a log"
        );
    }
}

#[cfg(all(test, feature = "virtual-mtp"))]
mod device_lifecycle_test {
    use super::{MtpDeviceEvent, RecordingMtpDeviceEvents};
    use crate::mtp::connection::{DeviceWatch, MtpDisconnectReason};
    use crate::mtp::connection_manager_reporting_to;
    use crate::mtp::virtual_device::{
        setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };
    use std::sync::Arc;

    /// The whole point of the trait: a real connect and disconnect report through
    /// the sink they were handed, with the storages the device actually has. The
    /// app's adapter turns exactly these into the two events the frontend's MTP
    /// store lives on, so a lost report leaves a phone in the sidebar forever.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_connect_and_a_disconnect_report_through_the_sink_they_were_given() {
        let _guard = virtual_device_test_lock().lock().await;
        let fixture = setup_virtual_mtp_device();
        let device_id = crate::mtp::list_mtp_devices()
            .into_iter()
            .find(|d| d.location_id == fixture.location_id)
            .map(|d| d.id)
            .expect("the virtual device must appear in discovery");

        let recorder = Arc::new(RecordingMtpDeviceEvents::new());
        let manager = connection_manager_reporting_to(recorder.clone());

        let info = manager
            .connect(&device_id, DeviceWatch::Off)
            .await
            .expect("virtual-mtp connect should succeed");
        manager
            .disconnect(&device_id, MtpDisconnectReason::User)
            .await
            .expect("disconnecting a device we just connected");
        unregister_virtual_mtp_device(fixture.location_id);

        let reported = recorder.events();
        assert_eq!(
            reported.len(),
            2,
            "one connect and one disconnect, never one report per storage"
        );
        match &reported[0] {
            MtpDeviceEvent::Connected {
                device_id: reported_id,
                storages,
                ..
            } => {
                assert_eq!(reported_id, &device_id);
                assert_eq!(
                    storages.len(),
                    info.storages.len(),
                    "the report carries the storages the sidebar is about to show"
                );
            }
            other => panic!("a connect must report first, got {other:?}"),
        }
        assert!(
            matches!(
                &reported[1],
                MtpDeviceEvent::Disconnected {
                    reason: MtpDisconnectReason::User,
                    ..
                }
            ),
            "an explicit disconnect is the user's, never a hotplug removal"
        );
    }
}
