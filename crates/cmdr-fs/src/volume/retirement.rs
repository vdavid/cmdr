//! Whether the volume registry still serves a volume under its ID, and the
//! handle a backend asks through.
//!
//! A backend that runs work outliving one operation (a watcher, a reconnect
//! backoff loop) has to keep answering "am I still the volume my ID points
//! at?". It can't answer that from its own state: being replaced, unmounted, or
//! simply removed are all the registry's decisions. So the registry writes the
//! answer down here, and the backend reads it back through a
//! [`SelfHandle`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

/// The registry's record that a volume is no longer the one its ID points at.
///
/// **Only a hand-over writes it.** The registry retires a volume it REMOVES; a
/// backend retires itself from `Volume::on_superseded`, where the registry sees a
/// replace and the id lives on under the successor. Nothing else may call
/// [`retire`](Self::retire): a volume that retires itself while still registered
/// goes quiet while the app is still routing work to it.
///
/// One-way, deliberately. Coming back means a fresh volume instance and a fresh
/// registration, which is what every re-register path already builds, so there
/// is no `unretire` to race a shutdown against.
// DEFAULT-OK: the zero value is "not retired", which is the truth about a volume nobody has removed yet.
#[derive(Debug, Default)]
pub struct Retirement {
    retired: AtomicBool,
}

impl Retirement {
    /// A volume that is still live.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the registry no longer serves this volume. Idempotent.
    pub fn retire(&self) {
        self.retired.store(true, Ordering::Release);
    }

    /// Whether the registry has retired this volume.
    pub fn is_retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }
}

/// A backend's handle back to its own live state, for the background work that
/// outlives a single operation.
///
/// [`live`](Self::live) answers with the state only while the volume is both
/// still allocated and still registered, which is exactly the question a watcher
/// or a reconnect loop asks every iteration. Holding the state directly would
/// keep a removed volume's session alive forever; holding its ID and re-reading
/// the registry answers with the SUCCESSOR after a replace, which is how a
/// healthy volume ends up marked disconnected by its predecessor's dying
/// watcher. A `Weak` plus the registry's [`Retirement`] answers both without
/// either failure, and identity is a pointer rather than an ID and a counter.
///
/// Cheap to clone and to hold: one `Weak` and one `Arc`.
pub struct SelfHandle<T> {
    target: Weak<T>,
    retirement: Arc<Retirement>,
}

impl<T> SelfHandle<T> {
    /// A handle to `target`, live until it is dropped or `retirement` is retired.
    ///
    /// Takes the `Weak` rather than the `Arc` so a backend can hand itself its
    /// own handle from inside `Arc::new_cyclic`, which is where the state a
    /// watcher hangs off is usually built. From an `Arc` in hand, pass
    /// `Arc::downgrade(&state)`.
    ///
    /// `retirement` must be the very flag the volume publishes through
    /// `Volume::retirement`, or the registry's answer lands somewhere nobody
    /// reads.
    pub fn new(target: Weak<T>, retirement: &Arc<Retirement>) -> Self {
        Self {
            target,
            retirement: Arc::clone(retirement),
        }
    }

    /// The state, while the volume is still allocated and still registered.
    pub fn live(&self) -> Option<Arc<T>> {
        if self.retirement.is_retired() {
            return None;
        }
        self.target.upgrade()
    }
}

impl<T> Clone for SelfHandle<T> {
    fn clone(&self) -> Self {
        Self {
            target: Weak::clone(&self.target),
            retirement: Arc::clone(&self.retirement),
        }
    }
}

impl<T> std::fmt::Debug for SelfHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelfHandle")
            .field("live", &(self.target.strong_count() > 0))
            .field("retired", &self.retirement.is_retired())
            .finish()
    }
}
