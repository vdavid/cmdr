//! A test-only gate for the subtree-rescan thread (`#[cfg(test)]`, so it exists in
//! no release build and in no build of the app).
//!
//! A test arms it for one anchor. The next rescan walk of exactly that anchor stops
//! just before its first read, with its share of the volume's hold already taken,
//! until the test opens the gate. That's what lets a test prove the thread holds the
//! drive while it runs, without racing a walk over a tiny tree to its end. Armed
//! with [`GateHandle::arm_after_listing`], it stops instead between one directory's
//! listing and its diff, which is where a live write can land in between.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;

/// Gates armed right now, one per anchor a test is holding.
static ARMED: Mutex<Vec<Arc<Gate>>> = Mutex::new(Vec::new());

/// A test's hold on the next rescan walk of one anchor: armed by
/// [`GateHandle::arm`], opened by [`GateHandle::open`] or on drop.
pub(crate) struct GateHandle {
    gate: Arc<Gate>,
}

impl GateHandle {
    /// Holds the next rescan walk of exactly `anchor` before its first read.
    pub(crate) fn arm(anchor: &Path) -> Self {
        Self::armed_at(anchor, Point::BeforeWalk)
    }

    /// Holds the next rescan walk that lists exactly `dir`, once the listing is
    /// taken and before it's diffed.
    pub(crate) fn arm_after_listing(dir: &Path) -> Self {
        Self::armed_at(dir, Point::AfterListing)
    }

    fn armed_at(anchor: &Path, point: Point) -> Self {
        let gate = Arc::new(Gate {
            anchor: anchor.to_path_buf(),
            point,
            state: Mutex::new(State::default()),
            cv: Condvar::new(),
        });
        ARMED.lock_ignore_poison().push(Arc::clone(&gate));
        Self { gate }
    }

    /// Waits until a walk is held at the gate. `false` when none got there within
    /// `timeout`.
    pub(crate) fn wait_until_held(&self, timeout: Duration) -> bool {
        let state = self.gate.state.lock_ignore_poison();
        let (state, _) = self
            .gate
            .cv
            .wait_timeout_while(state, timeout, |state| !state.held)
            .unwrap_or_else(|e| e.into_inner());
        state.held
    }

    /// Lets the held walk go on. The gate never holds a walk again.
    pub(crate) fn open(&self) {
        self.gate.state.lock_ignore_poison().open = true;
        self.gate.cv.notify_all();
    }
}

impl Drop for GateHandle {
    fn drop(&mut self) {
        self.open();
        ARMED.lock_ignore_poison().retain(|gate| !Arc::ptr_eq(gate, &self.gate));
    }
}

/// One armed gate, shared by the handle and the walk that meets it.
struct Gate {
    anchor: PathBuf,
    point: Point,
    state: Mutex<State>,
    /// Wakes the held walk on open, and the test's wait when a walk arrives.
    cv: Condvar,
}

/// Where in a walk a gate holds it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    BeforeWalk,
    AfterListing,
}

#[derive(Default)]
struct State {
    /// A walk is waiting at the gate.
    held: bool,
    /// The test let it go.
    open: bool,
}

/// The point a rescan walk passes before its first read: a no-op unless a test armed
/// a gate for `anchor`, in which case it waits there until the test opens it.
pub(super) fn pass(anchor: &Path) {
    wait_at(Point::BeforeWalk, anchor);
}

/// The point a walk passes between `dir`'s listing and its diff: the same no-op
/// unless a test armed [`GateHandle::arm_after_listing`] for `dir`.
pub(in crate::indexing::reconcile::reconciler) fn pass_after_listing(dir: &Path) {
    wait_at(Point::AfterListing, dir);
}

fn wait_at(point: Point, anchor: &Path) {
    let Some(gate) = ARMED
        .lock_ignore_poison()
        .iter()
        .find(|gate| gate.point == point && gate.anchor == anchor)
        .cloned()
    else {
        return;
    };
    let mut state = gate.state.lock_ignore_poison();
    if state.open {
        return;
    }
    state.held = true;
    gate.cv.notify_all();
    while !state.open {
        state = gate.cv.wait(state).unwrap_or_else(|e| e.into_inner());
    }
    state.held = false;
}
