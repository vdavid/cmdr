//! The one way into the wake loop: a channel the tap hands rollups to and returns.
//!
//! ⚠️ **Nothing on the live-loop thread may take a lock or touch SQLite.** The indexer's event
//! sink calls `route()` synchronously on the caller's thread, and that caller is the live loop.
//! A mutex around the inbox would block every live batch for the length of an LLM call; a write
//! connection per admit would run the whole migration ladder against a 5 s busy timeout. So the
//! tap owns nothing: it builds a [`FolderActivity`] and sends it here.
//!
//! ⚠️ **A process-global with lazy init, ❌ not managed Tauri state.** The indexer starts before
//! `agent::start` runs, and not reliably before — one of its two starts is inside a `spawn`, so
//! it is a race rather than an ordering. Anything registered in `agent::start` would miss launch
//! replay, the busiest window the tap will ever see. `restricted_paths` is the precedent.
//!
//! Rollups arriving before the consumer comes up sit in the buffer and are consumed once it
//! does, so some of launch replay survives. ❌ The buffer is deliberately not sized to catch all
//! of it: readiness can't even be evaluated before the store is open (consent lives in
//! `main.db`), so anything older than that would be refused admission anyway.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};

use super::{ChangeCounters, EventBundle, WAKE_WINDOW};
use crate::ignore_poison::IgnorePoison;

/// How many unserviced ROLLUPS the channel holds before it starts dropping them.
///
/// ⚠️ The bound is for rollups ONLY. A pathological burst should drop rather than grow without
/// limit, and the tap's payload is signal rather than correctness: the folder will change again.
/// Control messages bypass this entirely — see [`send_control`].
pub const MAX_QUEUED_ROLLUPS: usize = 4_096;

/// One folder's activity in one live batch, as the tap hands it over.
///
/// The agent-side vocabulary starts here: `cmdr-index` may never name the agent, so the crate's
/// own rollup type crosses on the `IndexEvent` seam and the `route()` handler maps into this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderActivity {
    /// Which volume the folder lives on, so the importance lookup opens the right index.
    pub volume_id: String,
    /// The folder the changes happened IN, absolute. A directory's own event counts in its
    /// PARENT: a bundle describes the folder a change happened in.
    pub folder: String,
    pub counters: ChangeCounters,
    /// The batch's own instant, unix seconds. ❌ Never a window start: the APP quantizes (a 60 s
    /// agent policy must not leak into `cmdr-index`), so a field named for a window here would
    /// lie about who decided the policy.
    pub observed_at: u64,
    /// The newest change in the batch, unix seconds. What a deadline gets measured from.
    pub last_event_at: u64,
}

impl FolderActivity {
    /// Quantize to the coalescing window and become an inbox-shaped bundle.
    ///
    /// ⚠️ This is the step that stops every ~1 s live batch becoming its own inbox row:
    /// `Inbox::admit` merges on exact `(folder, window_start)` equality, so without the flooring
    /// a busy folder would fill the table with one-second slivers.
    pub fn into_bundle(self) -> EventBundle {
        let window = WAKE_WINDOW.as_secs().max(1);
        EventBundle {
            folder: self.folder,
            counters: self.counters,
            window_start: (self.observed_at / window) * window,
            last_event_at: self.last_event_at,
        }
    }
}

/// What the wake loop's thread receives.
///
/// One channel with two kinds of message rather than two channels, so the loop can service both
/// with one `recv_timeout` and its timer in the same wait. The bound applies to the rollup
/// variant only.
pub enum WakeMessage {
    Rollup(FolderActivity),
    Control(WakeControl),
}

/// A message that must NEVER be dropped: each one changes what the loop does next, so losing
/// one is a bug rather than degraded signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeControl {
    /// Consent, disk access, or the API key moved. Re-read the snapshot and re-arm.
    ReadinessChanged,
    /// The user's cadence or the proactive toggle moved. Re-read the settings and re-arm.
    SettingsChanged,
    /// The wake thread finished, so another may be prepared.
    WakeFinished,
}

/// The endpoint: one sender, the receiver waiting to be claimed, and the rollup accounting the
/// bound is enforced against. There is exactly one of these in the process, but it is a plain
/// struct so the bound can be tested without the global.
struct TapChannel {
    tx: Sender<WakeMessage>,
    /// Handed to the consumer exactly once. The `Mutex` is only ever taken on the startup path,
    /// never by a producer.
    rx: Mutex<Option<Receiver<WakeMessage>>>,
    queued_rollups: AtomicUsize,
    dropped_rollups: AtomicU64,
}

impl TapChannel {
    fn new() -> Self {
        let (tx, rx) = channel();
        TapChannel {
            tx,
            rx: Mutex::new(Some(rx)),
            queued_rollups: AtomicUsize::new(0),
            dropped_rollups: AtomicU64::new(0),
        }
    }

    fn send_rollup(&self, activity: FolderActivity) {
        if self.queued_rollups.load(Ordering::Relaxed) >= MAX_QUEUED_ROLLUPS {
            self.dropped_rollups.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.queued_rollups.fetch_add(1, Ordering::Relaxed);
        if self.tx.send(WakeMessage::Rollup(activity)).is_err() {
            self.queued_rollups.fetch_sub(1, Ordering::Relaxed);
        }
    }

    fn send_control(&self, control: WakeControl) {
        let _ = self.tx.send(WakeMessage::Control(control));
    }

    fn note_rollup_consumed(&self) {
        // Saturating: an underflow here would wrap the bound to "never drop anything".
        let _ = self
            .queued_rollups
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| Some(n.saturating_sub(1)));
    }
}

static CHANNEL: OnceLock<TapChannel> = OnceLock::new();

fn tap_channel() -> &'static TapChannel {
    CHANNEL.get_or_init(TapChannel::new)
}

/// Hand one folder's batch activity to the wake loop and return. Called on the live-loop
/// thread, so it does exactly this and nothing else.
///
/// Over [`MAX_QUEUED_ROLLUPS`] waiting, the rollup is dropped and counted. The consumer logs
/// the count, so a burst that outran it is visible rather than silent.
pub fn send_rollup(activity: FolderActivity) {
    tap_channel().send_rollup(activity);
}

/// Tell the wake loop something changed about how it should behave. ❌ Never dropped: a settings
/// change re-arms the timer and re-prices what is queued, and losing one leaves a parked
/// scheduler that no longer matches what the user asked for.
pub fn send_control(control: WakeControl) {
    tap_channel().send_control(control);
}

/// Take the receiving end. The consumer calls this once, at `agent::start`; a second caller
/// gets `None` rather than a second consumer racing the first for messages.
pub fn take_receiver() -> Option<Receiver<WakeMessage>> {
    tap_channel().rx.lock_ignore_poison().take()
}

/// Account for one rollup leaving the queue. The consumer calls this as it pops.
pub fn note_rollup_consumed() {
    tap_channel().note_rollup_consumed();
}

/// How many rollups have been dropped for the bound since the last time anyone asked, and reset
/// the count.
pub fn take_dropped_rollups() -> u64 {
    tap_channel().dropped_rollups.swap(0, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(folder: &str, observed_at: u64) -> FolderActivity {
        FolderActivity {
            volume_id: "root".to_string(),
            folder: folder.to_string(),
            counters: ChangeCounters {
                created: 1,
                ..ChangeCounters::default()
            },
            observed_at,
            last_event_at: observed_at,
        }
    }

    /// The window is what stops a busy folder filling the table with one-second slivers: the
    /// inbox merges on exact `(folder, window_start)`, so two batches a few seconds apart have
    /// to land on the same key.
    #[test]
    fn batches_inside_one_window_quantize_onto_the_same_key() {
        // 1_780_000_020 is a window boundary; +7 and +59 share it, +60 opens the next one.
        let early = activity("/Users/someone/Downloads", 1_780_000_027).into_bundle();
        let late = activity("/Users/someone/Downloads", 1_780_000_079).into_bundle();
        let next = activity("/Users/someone/Downloads", 1_780_000_080).into_bundle();

        assert_eq!(early.window_start, late.window_start);
        assert_ne!(early.window_start, next.window_start, "and the next window is its own");
        assert_eq!(early.window_start % 60, 0, "windows tumble against the epoch");
    }

    /// ⚠️ The bound is for ROLLUPS. A pathological burst drops rather than growing without
    /// limit, and the drop is COUNTED so it shows up in the log rather than as an agent that
    /// quietly stopped noticing things.
    #[test]
    fn a_burst_past_the_bound_drops_rollups_and_counts_them() {
        let channel = TapChannel::new();
        for i in 0..MAX_QUEUED_ROLLUPS + 5 {
            channel.send_rollup(activity("/Users/someone/Downloads", 1_780_000_000 + i as u64));
        }

        assert_eq!(channel.dropped_rollups.load(Ordering::Relaxed), 5);
        assert_eq!(channel.queued_rollups.load(Ordering::Relaxed), MAX_QUEUED_ROLLUPS);
    }

    /// ❌ A control message must never drop. Each one changes what the loop does next — a
    /// re-armed timer, a re-priced inbox, a finished wake — so losing one to a rollup burst
    /// leaves a scheduler parked against a state that no longer exists.
    #[test]
    fn control_messages_ride_past_a_full_queue() {
        let channel = TapChannel::new();
        let rx = channel.rx.lock_ignore_poison().take().expect("the only claim");
        for i in 0..MAX_QUEUED_ROLLUPS + 100 {
            channel.send_rollup(activity("/Users/someone/Downloads", 1_780_000_000 + i as u64));
        }

        channel.send_control(WakeControl::SettingsChanged);

        let controls: Vec<WakeControl> = rx
            .try_iter()
            .filter_map(|message| match message {
                WakeMessage::Control(control) => Some(control),
                WakeMessage::Rollup(_) => None,
            })
            .collect();
        assert_eq!(controls, vec![WakeControl::SettingsChanged]);
    }

    /// Consuming makes room again, so a burst that outran the loop costs signal for as long as
    /// the burst lasts and no longer.
    #[test]
    fn consuming_a_rollup_makes_room_for_another() {
        let channel = TapChannel::new();
        for i in 0..MAX_QUEUED_ROLLUPS {
            channel.send_rollup(activity("/Users/someone/Downloads", 1_780_000_000 + i as u64));
        }
        channel.note_rollup_consumed();

        channel.send_rollup(activity("/Users/someone/Downloads", 1_781_000_000));

        assert_eq!(channel.dropped_rollups.load(Ordering::Relaxed), 0, "nothing was dropped");
    }

    /// The newest change survives the crossing untouched — it is what a deadline is measured
    /// from, and quantizing it too would age every deadline by up to a minute.
    #[test]
    fn the_last_change_is_carried_over_unquantized() {
        let bundle = FolderActivity {
            last_event_at: 1_780_000_079,
            ..activity("/Users/someone/Downloads", 1_780_000_027)
        }
        .into_bundle();

        assert_eq!(bundle.last_event_at, 1_780_000_079);
        assert_eq!(bundle.counters.created, 1);
        assert_eq!(bundle.folder, "/Users/someone/Downloads");
    }
}
