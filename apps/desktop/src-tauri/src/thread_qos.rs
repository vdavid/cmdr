//! Thread scheduling priority (QoS) hints for background work.
//!
//! macOS schedules threads by QoS ("quality of service") class: under CPU contention a
//! `USER_INTERACTIVE` / `USER_INITIATED` thread preempts a `UTILITY` / `BACKGROUND` one,
//! and the class also steers I/O throttling and CPU-frequency selection. Cmdr's UI and
//! IPC handlers run at the process-default (user-interactive) tier; the heavy background
//! indexing threads lower themselves to `UTILITY` so a runaway or failing scan can never
//! starve the webview for CPU. This is the in-process safety net that complements the
//! per-subsystem fixes (fatal errors stop instead of retry-looping, bounded logging).
//!
//! On non-macOS targets this is a no-op: we rely on the OS scheduler's fair share and
//! don't currently lower Linux/Windows priorities. If we ever want to, `setpriority(2)`
//! (Linux nice) or thread priorities (Windows) would slot in behind the same API.
//!
//! ## Why `Utility` and not `Background`
//!
//! We expose only `Utility`. macOS also has a `Background` tier, but it's aggressively
//! I/O-throttled: it would make scans crawl and interact badly with disk throughput. Since
//! every background indexing thread bears user-visible progress, `Utility` (below the UI,
//! but a fair CPU share and normal I/O) is the right tier for all of them. `Background`
//! would only fit a genuinely idle maintenance tick with nobody waiting; we have none
//! today, so it isn't wired up (add the variant when a real use appears).
//!
//! ## Usage discipline
//!
//! Call [`set_current_thread_qos`] once, at the very top of a **dedicated** background
//! thread's body. Never call it on a pooled/shared thread (a tokio worker, a rayon
//! worker): the change persists for the thread's whole lifetime, so it would leak a
//! lowered priority onto later, unrelated tasks that land on the same thread. The
//! indexing live/replay loops run as tokio tasks and are deliberately NOT lowered here
//! for exactly this reason.
//!
//! ## No-op under `cfg(test)`
//!
//! In the crate's own test builds this is a no-op. Unit tests run massively parallel
//! under nextest (roughly one process per core), so lowering a heavy background thread
//! there can starve it past a per-test timeout, with no UI to protect and no production
//! meaning. Production and dev builds (neither is `cfg(test)`) get the real class. The FFI
//! itself is still verified directly by this module's macOS test.

/// Scheduling tier for a dedicated background thread. Maps to a macOS `qos_class_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QosClass {
    /// User-initiated, progress-bearing work the user is (loosely) waiting on, but that
    /// must yield to the UI. Indexing shows a progress indicator, so this is its tier:
    /// below user-interactive, yet still a fair CPU share and normal (unthrottled) I/O.
    Utility,
}

/// Lowers the calling thread's scheduling priority to `class`.
///
/// See the module docs for the call-site discipline (dedicated threads only) and the
/// `cfg(test)` no-op. On non-macOS this is a no-op.
pub fn set_current_thread_qos(class: QosClass) {
    // Real work only on macOS in non-test builds. See the module docs for why tests skip it.
    #[cfg(all(target_os = "macos", not(test)))]
    set_thread_qos_macos(class);
    #[cfg(not(all(target_os = "macos", not(test))))]
    let _ = class;
}

/// Sets the calling thread's macOS QoS class via `pthread_set_qos_class_self_np`. The real
/// FFI, shared by the production path and the test that verifies it.
#[cfg(target_os = "macos")]
fn set_thread_qos_macos(class: QosClass) {
    let qos = match class {
        QosClass::Utility => libc::qos_class_t::QOS_CLASS_UTILITY,
    };
    // SAFETY: `pthread_set_qos_class_self_np` acts only on the calling thread (which is
    // live — we are running on it) and takes a valid `qos_class_t` plus a relative
    // priority offset. `qos` is one of the enum's own variants, and `0` is the documented
    // default offset, so the arguments are always in range; the call has no other
    // preconditions and cannot invalidate any Rust-side state. We log rather than unwrap
    // the return code so a hypothetical failure never aborts a worker mid-scan.
    let rc = unsafe { libc::pthread_set_qos_class_self_np(qos, 0) };
    if rc != 0 {
        log::debug!(
            target: "thread_qos",
            "pthread_set_qos_class_self_np(class={class:?}) returned {rc}"
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Reads back the calling thread's QoS class (as its raw `u32`) so the setter can be
    /// verified against what the OS actually recorded, not just the return code.
    /// `qos_class_t` is `#[repr(u32)]` and doesn't implement `PartialEq`, so compare values.
    fn current_qos_class_raw() -> u32 {
        let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
        // SAFETY: `pthread_get_qos_class_np` reads the given thread's QoS into `class`.
        // `pthread_self()` is always valid on the calling thread; `class` is a live,
        // writable local; the priority out-param is null, which the API accepts.
        let rc = unsafe { libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, std::ptr::null_mut()) };
        assert_eq!(rc, 0, "pthread_get_qos_class_np failed with {rc}");
        class as u32
    }

    /// The FFI must move the current thread to the requested class. Calls
    /// `set_thread_qos_macos` directly (the public wrapper is a no-op under `cfg(test)`),
    /// on its own spawned thread so the harness thread's QoS isn't mutated for other tests.
    #[test]
    fn sets_utility_class_on_current_thread() {
        let handle = std::thread::spawn(|| {
            set_thread_qos_macos(QosClass::Utility);
            current_qos_class_raw()
        });
        let observed = handle.join().expect("qos test thread panicked");
        assert_eq!(
            observed,
            libc::qos_class_t::QOS_CLASS_UTILITY as u32,
            "thread QoS should be UTILITY after set_thread_qos_macos(Utility)"
        );
    }
}
