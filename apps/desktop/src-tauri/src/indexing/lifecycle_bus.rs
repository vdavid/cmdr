//! A minimal neutral in-process lifecycle bus: `indexing/` publishes a volume's
//! scan-completion transitions so a backend subsystem (the importance scheduler,
//! and later the media-ML enrichment scheduler) can react without polling and
//! without `indexing/` depending on it.
//!
//! ## Why a `watch`, not a `broadcast`
//!
//! A `broadcast` channel does NOT replay a value sent before a receiver
//! subscribes, so a `ScanCompleted` that fires during `setup()` — before the
//! importance scheduler has subscribed — would be lost. A [`tokio::sync::watch`]
//! retains the last value, so a late subscriber immediately observes the current
//! state. That late-subscriber replay is the whole reason for the choice (plan
//! Decision 4).
//!
//! ## The one-way boundary
//!
//! `indexing/` publishes through [`publish`] from the neutral scan-completion
//! chokepoint (`state::apply_freshness_event_on`) without knowing who listens.
//! Consumers call [`subscribe`] with a volume id. The clean direction — consumers
//! depend on `indexing/`, never the reverse — mirrors `search/`→`indexing/`.
//!
//! ## Lifecycle independence from the registry
//!
//! The bus keyed its senders in a module-level map, NOT in `IndexInstance`, on
//! purpose: a `watch::Sender` must outlive the index instance so a subscriber
//! that took its receiver keeps seeing the last state after the volume unmounts
//! (its instance is removed from `INDEX_REGISTRY`). The sender is created lazily
//! on first publish or subscribe for a volume and lives for the process.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;

use tokio::sync::broadcast;
use tokio::sync::watch;

use super::state::IndexVolumeKind;
use crate::ignore_poison::IgnorePoison;

/// A volume's coarse scan-lifecycle state, as seen on the bus.
///
/// Deliberately minimal: the bus carries "did a scan just complete?" and nothing
/// else. The scheduler drives readiness off [`ScanState::Completed`], never off
/// the richer `IndexPhase`/`Freshness` machines (those stay `indexing`-internal).
/// A monotonically increasing `generation` lets a consumer distinguish a fresh
/// completion from the retained initial value and coalesce repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanState {
    /// No completion has been observed on the bus yet (the initial retained
    /// value a late subscriber sees before any scan completes).
    Pending,
    /// A full scan completed. `generation` increments on each completion so a
    /// consumer can tell two completions apart and coalesce a repeat of the same.
    Completed { generation: u64 },
}

/// A batch of directories whose listings changed while the app runs (live
/// FSEvents, verifier corrections), for a volume.
///
/// The importance scheduler drives its INCREMENTAL recompute off this: rescore
/// only the touched subtrees + their (capped) ancestor chains, instead of the
/// full-volume pass a `ScanCompleted` triggers (plan Decision 5). A monotonically
/// increasing `batch` lets a late subscriber tell the retained initial value from
/// a real change and coalesce repeats.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DirsChanged {
    /// `0` is the retained initial value (nothing published yet); each publish
    /// bumps it so a consumer can distinguish batches.
    pub batch: u64,
    /// The changed directory paths (absolute). A `"/"` sentinel means "refresh
    /// everything" (a full-refresh emit); a consumer treats it as a full pass.
    pub paths: Vec<String>,
}

/// The per-volume `watch` senders. A sender is created on first
/// `publish`/`subscribe` for a volume and lives for the process, so a receiver
/// keeps replaying the last state even after the volume's index instance is
/// gone. Keyed by volume id, independent of `INDEX_REGISTRY`.
static BUS: LazyLock<Mutex<HashMap<String, watch::Sender<ScanState>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The per-volume dir-changed senders, separate from `BUS` for the same
/// lifecycle-independence reason. A live listing change publishes the changed
/// paths here.
static DIR_BUS: LazyLock<Mutex<HashMap<String, watch::Sender<DirsChanged>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A volume that just registered with the index (reserved its
/// `Initializing` slot), carrying its typed kind so a consumer branches on the
/// kind (score Local + SMB, exclude MTP) without touching the volume-id string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegisteredVolume {
    pub volume_id: String,
    pub kind: IndexVolumeKind,
}

/// The registration bus: a single fan-out `broadcast` of every volume that
/// registers after subscribe. A `broadcast` (not a per-volume `watch`) fits
/// because each registration is a distinct EVENT for a distinct volume, not a
/// latest-state-per-volume a late subscriber must replay — and the consumer's
/// startup registry sweep already covers volumes registered before it subscribed
/// (plan M4). The scheduler subscribes ONCE (before its sweep, so no registration
/// in the gap is lost) and reacts to each late-registering volume.
///
/// Capacity is generous: registrations are rare (one per mounted volume), so a
/// receiver never lags. A lagged receiver only misses a registration whose volume
/// the next scan-completion (on the scan bus) still covers, so a miss self-heals.
static REGISTRATION_BUS: LazyLock<broadcast::Sender<RegisteredVolume>> = LazyLock::new(|| broadcast::channel(256).0);

/// Get (or lazily create) the dir-changed `watch::Sender` for a volume.
fn with_dir_sender<T>(volume_id: &str, f: impl FnOnce(&watch::Sender<DirsChanged>) -> T) -> T {
    let mut bus = DIR_BUS.lock_ignore_poison();
    let sender = bus
        .entry(volume_id.to_string())
        .or_insert_with(|| watch::channel(DirsChanged::default()).0);
    f(sender)
}

/// Publish a batch of changed directory paths for a volume.
///
/// Called from the live-change sites (`event_loop`, `verifier`) alongside the
/// existing frontend `index-dir-updated` emit. `indexing/` publishes without
/// knowing who listens (the one-way boundary); the importance scheduler
/// subscribes and rescopes an incremental rescore to the touched paths. A no-op
/// if `paths` is empty.
pub(crate) fn publish_dirs_changed(volume_id: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    with_dir_sender(volume_id, |sender| {
        let batch = sender.borrow().batch + 1;
        sender.send_replace(DirsChanged {
            batch,
            paths: paths.to_vec(),
        });
    });
}

/// Subscribe to a volume's dir-changed bus. The returned receiver carries the
/// last published batch (or the empty initial value); a change bumps `batch`.
pub(crate) fn subscribe_dirs_changed(volume_id: &str) -> watch::Receiver<DirsChanged> {
    with_dir_sender(volume_id, |sender| sender.subscribe())
}

/// Get (or lazily create) the `watch::Sender` for a volume, running `f` with it.
///
/// Centralizes the create-if-absent so `publish` and `subscribe` can't diverge
/// on the initial value.
fn with_sender<T>(volume_id: &str, f: impl FnOnce(&watch::Sender<ScanState>) -> T) -> T {
    let mut bus = BUS.lock_ignore_poison();
    let sender = bus
        .entry(volume_id.to_string())
        .or_insert_with(|| watch::channel(ScanState::Pending).0);
    f(sender)
}

/// Publish a scan completion for a volume, advancing its generation.
///
/// Called from the neutral scan-completion chokepoint. Idempotent in the sense
/// that every call bumps the generation and notifies subscribers; a coalescing
/// consumer decides whether a given completion means new work.
pub(crate) fn publish_scan_completed(volume_id: &str) {
    with_sender(volume_id, |sender| {
        let next = match *sender.borrow() {
            ScanState::Pending => ScanState::Completed { generation: 1 },
            ScanState::Completed { generation } => ScanState::Completed {
                generation: generation + 1,
            },
        };
        // `send_replace` (not `send`) updates the retained value even when there
        // are NO receivers — which is exactly the case when a scan completes
        // before the importance scheduler subscribes. A plain `send` would be a
        // no-op with no receivers and lose the completion, defeating the whole
        // point of using a `watch` for late-subscriber replay.
        sender.send_replace(next);
    });
}

/// Subscribe to a volume's scan-lifecycle bus.
///
/// The returned receiver immediately carries the last published state (or
/// [`ScanState::Pending`] if nothing has published yet) — the late-subscriber
/// replay a `broadcast` couldn't give. A subscription created before any publish
/// still sees the first completion.
pub(crate) fn subscribe(volume_id: &str) -> watch::Receiver<ScanState> {
    with_sender(volume_id, |sender| sender.subscribe())
}

/// Announce that a volume registered (reserved its index slot), with its kind.
///
/// Published once from the neutral registration funnel (`state::start_indexing_for`,
/// right after the reservation wins). A `send` with no receivers is a harmless
/// no-op — the consumer's startup sweep covers any volume that registered before
/// it subscribed, so nothing is lost.
pub(crate) fn publish_volume_registered(volume_id: &str, kind: IndexVolumeKind) {
    let _ = REGISTRATION_BUS.send(RegisteredVolume {
        volume_id: volume_id.to_string(),
        kind,
    });
}

/// Subscribe to volume-registration events. The scheduler subscribes ONCE (before
/// its startup sweep) and wires per-volume subscriptions for each late-registering
/// volume. Only registrations AFTER this call are delivered (a `broadcast`); the
/// pre-subscribe set is the sweep's job.
pub(crate) fn subscribe_registrations() -> broadcast::Receiver<RegisteredVolume> {
    REGISTRATION_BUS.subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clear the bus so ids don't leak between tests (the map is a process
    /// global). Tests use unique ids anyway, but this keeps assertions on
    /// generation counts deterministic.
    fn reset(volume_id: &str) {
        BUS.lock_ignore_poison().remove(volume_id);
    }

    /// A subscriber created AFTER a completion still sees it — the late-subscriber
    /// replay that motivates `watch` over `broadcast`. Pre-fix (a `broadcast`),
    /// the receiver would see nothing until the NEXT completion.
    #[test]
    fn late_subscriber_sees_the_last_completion() {
        let vid = "bus-late-subscriber";
        reset(vid);

        // A scan completes BEFORE anyone subscribes.
        publish_scan_completed(vid);

        // Now subscribe. The retained value must already be Completed.
        let rx = subscribe(vid);
        assert_eq!(
            *rx.borrow(),
            ScanState::Completed { generation: 1 },
            "a late subscriber must replay the completion fired before it subscribed"
        );

        reset(vid);
    }

    /// A subscriber created before any publish starts at `Pending`, then observes
    /// the completion once it fires.
    #[test]
    fn early_subscriber_starts_pending_then_sees_completion() {
        let vid = "bus-early-subscriber";
        reset(vid);

        let mut rx = subscribe(vid);
        assert_eq!(*rx.borrow(), ScanState::Pending, "no completion yet ⇒ Pending");

        publish_scan_completed(vid);
        // `has_changed` is true because a new value was sent after the last borrow.
        assert!(rx.has_changed().expect("sender alive"));
        assert_eq!(*rx.borrow_and_update(), ScanState::Completed { generation: 1 });

        reset(vid);
    }

    /// Each completion bumps the generation, so a consumer can distinguish two
    /// separate scans and coalesce a repeat of the same one.
    #[test]
    fn each_completion_advances_the_generation() {
        let vid = "bus-generation";
        reset(vid);

        publish_scan_completed(vid);
        publish_scan_completed(vid);
        let rx = subscribe(vid);
        assert_eq!(
            *rx.borrow(),
            ScanState::Completed { generation: 2 },
            "two completions ⇒ generation 2"
        );

        reset(vid);
    }

    /// The sender outlives an "unmount": dropping every receiver doesn't drop the
    /// retained value, and a fresh subscribe after that still replays it. This is
    /// why the sender lives in the module map, not in `IndexInstance`.
    #[test]
    fn retained_value_survives_all_receivers_dropping() {
        let vid = "bus-survives-drop";
        reset(vid);

        publish_scan_completed(vid);
        {
            let rx = subscribe(vid);
            assert_eq!(*rx.borrow(), ScanState::Completed { generation: 1 });
            // rx drops here — the only receiver gone.
        }
        // A new subscriber still replays the retained completion.
        let rx2 = subscribe(vid);
        assert_eq!(
            *rx2.borrow(),
            ScanState::Completed { generation: 1 },
            "the retained value must survive all receivers dropping (sender lives in the module map)"
        );

        reset(vid);
    }

    /// A dir-changed publish carries the paths and bumps the batch; an empty
    /// publish is a no-op (no spurious rescore). A late subscriber sees the last
    /// batch (the same late-subscriber replay as the scan bus).
    #[test]
    fn dir_changed_publishes_paths_and_coalesces_batch() {
        let vid = "dir-bus-test";
        DIR_BUS.lock_ignore_poison().remove(vid);

        // An empty publish is a no-op: nothing to rescore.
        publish_dirs_changed(vid, &[]);
        assert_eq!(
            subscribe_dirs_changed(vid).borrow().batch,
            0,
            "empty publish is a no-op"
        );

        // A real publish carries the paths and bumps the batch.
        publish_dirs_changed(vid, &["/a".to_string(), "/b".to_string()]);
        let rx = subscribe_dirs_changed(vid);
        let state = rx.borrow();
        assert_eq!(state.batch, 1);
        assert_eq!(state.paths, vec!["/a".to_string(), "/b".to_string()]);
        drop(state);

        DIR_BUS.lock_ignore_poison().remove(vid);
    }

    /// A late-registering volume (a share mounted mid-session) reaches a subscriber
    /// through the registration bus, carrying its typed kind so the consumer can
    /// branch (score SMB, exclude MTP) without touching the id string. A subscriber
    /// created BEFORE the publish sees it; the kind rides along. This is the seam
    /// the importance scheduler subscribes to for late volumes (plan M4).
    #[test]
    fn registration_bus_delivers_a_late_volume_with_its_kind() {
        let mut rx = subscribe_registrations();

        // A share registers AFTER the subscribe (the mid-session mount case).
        publish_volume_registered("smb-late", IndexVolumeKind::Smb);

        let got = rx.try_recv().expect("the late registration is delivered");
        assert_eq!(
            got,
            RegisteredVolume {
                volume_id: "smb-late".to_string(),
                kind: IndexVolumeKind::Smb,
            },
            "the registration carries the volume id and its typed kind"
        );

        // An MTP registration is delivered too (the consumer, not the bus, applies
        // the exclusion) — the bus stays a neutral publisher.
        publish_volume_registered("mtp-cam:1", IndexVolumeKind::Mtp);
        let mtp = rx.try_recv().expect("mtp registration delivered");
        assert_eq!(mtp.kind, IndexVolumeKind::Mtp, "the bus reports the kind verbatim");
    }

    /// Two volumes are independent: publishing on one never moves the other.
    #[test]
    fn volumes_are_independent() {
        let (a, b) = ("bus-vol-a", "bus-vol-b");
        reset(a);
        reset(b);

        publish_scan_completed(a);
        assert_eq!(*subscribe(a).borrow(), ScanState::Completed { generation: 1 });
        assert_eq!(
            *subscribe(b).borrow(),
            ScanState::Pending,
            "publishing on A must not move B"
        );

        reset(a);
        reset(b);
    }
}
