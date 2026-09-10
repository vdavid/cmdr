//! One viewer open that may pull its file into a temp first: how far the pull got,
//! whether anyone still wants it, and the handoff of the session it produces.
//!
//! The viewer window exists before `viewer_open` runs, and pulling a file off a phone
//! can take minutes, so three parties touch an open while it runs:
//!
//! - the pull loop (`materialize::stream_to_file`) records bytes after every chunk and
//!   stops at the next chunk boundary once the open is abandoned;
//! - the command's waiter reads the progress to emit [`ViewerPullProgress`] to the
//!   window, and abandons the open when no bytes arrive for the stall limit;
//! - the window-destroyed handler abandons the open of a window that went away.
//!
//! **Abandon and deliver share one lock.** The open records its window → session link
//! inside [`PendingOpen::deliver`]. An abandon either lands first (the open closes the
//! session it just built and answers the abandon's error) or finds the open delivered
//! (the link is recorded, so the ordinary close path frees the session). No
//! interleaving leaves a session nobody owns.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::ViewerError;
use crate::ignore_poison::IgnorePoison;

/// `viewer-pull-progress`: how much of the file a viewer has pulled so far. Emitted to
/// the viewer window that asked for the file, while its open pulls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPullProgress {
    pub bytes_done: u64,
    /// `None` when the source didn't say how big the file is.
    pub bytes_total: Option<u64>,
}

/// Why an open was abandoned, which decides what its pull answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbandonReason {
    /// The window closed, or a newer open for the same window replaced this one.
    Cancelled,
    /// No bytes arrived for the stall limit.
    StoppedResponding,
}

impl AbandonReason {
    fn error(self) -> ViewerError {
        match self {
            Self::Cancelled => ViewerError::Cancelled,
            Self::StoppedResponding => ViewerError::StoppedResponding,
        }
    }
}

enum Fate {
    Running,
    Abandoned(AbandonReason),
    Delivered,
}

/// The shared state of one open. See the module docs for who touches it.
pub struct PendingOpen {
    fate: Mutex<Fate>,
    /// Mirrors `fate == Abandoned` so the pull loop checks it without the lock.
    abandoned: AtomicBool,
    progress: Mutex<Option<ViewerPullProgress>>,
    /// The open's start, then the last time a pull recorded bytes.
    last_activity: Mutex<Instant>,
}

impl Default for PendingOpen {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingOpen {
    pub fn new() -> Self {
        Self {
            fate: Mutex::new(Fate::Running),
            abandoned: AtomicBool::new(false),
            progress: Mutex::new(None),
            last_activity: Mutex::new(Instant::now()),
        }
    }

    /// Records that the pull has `bytes_done` of the file, and that it's alive.
    pub(super) fn record_pull(&self, bytes_done: u64, bytes_total: Option<u64>) {
        *self.progress.lock_ignore_poison() = Some(ViewerPullProgress {
            bytes_done,
            bytes_total,
        });
        *self.last_activity.lock_ignore_poison() = Instant::now();
    }

    /// The pull's latest progress, or `None` while nothing is being pulled.
    pub fn pull_progress(&self) -> Option<ViewerPullProgress> {
        *self.progress.lock_ignore_poison()
    }

    /// How long since the open started or last recorded bytes.
    pub fn idle_for(&self) -> Duration {
        self.last_activity.lock_ignore_poison().elapsed()
    }

    /// The error an abandoned open answers with, or `None` while someone still wants it.
    pub fn abandoned_error(&self) -> Option<ViewerError> {
        if !self.abandoned.load(Ordering::Relaxed) {
            return None;
        }
        match *self.fate.lock_ignore_poison() {
            Fate::Abandoned(reason) => Some(reason.error()),
            Fate::Running | Fate::Delivered => None,
        }
    }

    /// Gives up on the open. Returns `false` when it already delivered its session,
    /// which then belongs to its window like any other. A second abandon keeps the
    /// first reason.
    pub fn abandon(&self, reason: AbandonReason) -> bool {
        let mut fate = self.fate.lock_ignore_poison();
        match *fate {
            Fate::Delivered => false,
            Fate::Abandoned(_) => true,
            Fate::Running => {
                *fate = Fate::Abandoned(reason);
                self.abandoned.store(true, Ordering::Relaxed);
                true
            }
        }
    }

    /// Hands the open's session to its window: runs `hand_off` and marks the open
    /// delivered, unless it was abandoned first, in which case it answers the abandon's
    /// error and the caller closes the session.
    pub(super) fn deliver(&self, hand_off: impl FnOnce()) -> Result<(), ViewerError> {
        let mut fate = self.fate.lock_ignore_poison();
        match *fate {
            Fate::Abandoned(reason) => Err(reason.error()),
            Fate::Running | Fate::Delivered => {
                hand_off();
                *fate = Fate::Delivered;
                Ok(())
            }
        }
    }
}

/// The in-flight opens that belong to a window, by window label.
static PENDING_OPENS: LazyLock<Mutex<HashMap<String, Arc<PendingOpen>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Starts watching an open for `window_label`, so closing that window abandons it. A
/// newer open for the same window replaces (and abandons) an older one. An empty label
/// has no window to close, so its open is watched but not registered.
pub fn begin_pending_open(window_label: &str) -> Arc<PendingOpen> {
    let open = Arc::new(PendingOpen::new());
    if !window_label.is_empty()
        && let Some(older) = PENDING_OPENS
            .lock_ignore_poison()
            .insert(window_label.to_string(), Arc::clone(&open))
    {
        older.abandon(AbandonReason::Cancelled);
    }
    open
}

/// Stops watching `open` for `window_label`, once its waiter is done with it. Leaves a
/// newer open for the same window registered.
pub fn end_pending_open(window_label: &str, open: &Arc<PendingOpen>) {
    let mut opens = PENDING_OPENS.lock_ignore_poison();
    if opens
        .get(window_label)
        .is_some_and(|current| Arc::ptr_eq(current, open))
    {
        opens.remove(window_label);
    }
}

/// Abandons the in-flight open of a window that closed, if it has one.
pub(super) fn abandon_pending_open_for_window(window_label: &str) {
    let open = PENDING_OPENS.lock_ignore_poison().remove(window_label);
    if let Some(open) = open {
        open.abandon(AbandonReason::Cancelled);
    }
}
