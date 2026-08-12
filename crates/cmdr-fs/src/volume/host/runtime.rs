//! The tokio runtime a backend spawns background work onto.
//!
//! This seam is a real [`Handle`], not a trait. A backend needs to abort the
//! task it spawned (a watcher that has to stop when its volume unmounts), so it
//! needs a `JoinHandle`; wrapping that in a trait would rebuild tokio's API
//! badly. The host injects its own handle so there is exactly ONE thread pool in
//! the process.
//!
//! ## ❌ Never call `tokio::spawn` from a backend
//!
//! `tokio::spawn` INHERITS an ambient runtime; [`VolumeHost::runtime`] RESOLVES
//! one. Backends routinely start tasks from places with no reactor running — a
//! `notify` watcher's OS thread, an SMB watcher thread, the app's synchronous
//! startup hook — and `tokio::spawn` panics there. Spawn through the seam:
//!
//! ```text
//! let task = self.host.runtime().spawn(async move { … });
//! ```
//!
//! ## Thread scheduling class is NOT this seam's business
//!
//! ❌ Nothing here lowers a thread's QoS, and neither should a backend's spawned
//! task. A class sticks to a thread for its whole life, so setting one on a
//! pooled runtime worker leaks it onto whatever unrelated work lands there next.
//! A backend that needs a low-priority thread creates a **dedicated**
//! `std::thread` and calls [`crate::thread_qos`] at the top of its body.
//!
//! ## The fallback runtime
//!
//! With nothing injected, the first call lazily builds one multi-threaded
//! runtime and every later call reuses it. That's what keeps test binaries,
//! benches, and CLI tools working with a [`VolumeHost::detached`]; the shipped
//! app always injects, before any volume is constructed.
//!
//! [`VolumeHost::runtime`]: super::VolumeHost::runtime
//! [`VolumeHost::detached`]: super::VolumeHost::detached

use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

/// Built on first use, only when no host injected a handle. Never dropped: tasks
/// spawned onto it outlive any scope we could tie it to.
static FALLBACK: OnceLock<Runtime> = OnceLock::new();

/// The injected handle, or the shared fallback.
pub(super) fn resolve(injected: Option<&Handle>) -> Handle {
    if let Some(handle) = injected {
        return handle.clone();
    }
    FALLBACK
        .get_or_init(|| {
            log::debug!(
                target: "volume",
                "no runtime injected; building the volume host's shared fallback"
            );
            Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("building the volume host's fallback tokio runtime")
        })
        .handle()
        .clone()
}
