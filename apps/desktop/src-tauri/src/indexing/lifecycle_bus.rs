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

use tokio::sync::watch;

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

/// The per-volume `watch` senders. A sender is created on first
/// `publish`/`subscribe` for a volume and lives for the process, so a receiver
/// keeps replaying the last state even after the volume's index instance is
/// gone. Keyed by volume id, independent of `INDEX_REGISTRY`.
static BUS: LazyLock<Mutex<HashMap<String, watch::Sender<ScanState>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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
