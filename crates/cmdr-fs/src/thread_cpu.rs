//! How much CPU time the CALLING thread has burned since it started.
//!
//! A dedicated background thread's CPU cost is invisible from outside the
//! process on macOS: `ps -M` prints per-thread cumulative CPU but no thread
//! names, so a profile can't tell the index writer from a tokio worker. The only
//! honest way to attribute CPU to one named thread is to have that thread read
//! its own counter and report it. This is that read.
//!
//! **Cumulative, never a rate.** The value only grows, so a caller measuring a
//! window subtracts two readings. That's deliberate: a sampled instantaneous rate
//! is exactly the instrument that produced a wrong answer for this codebase
//! before (`docs/notes/size-only-subtrees-before-baseline-2026-08-06.md` § "CPU
//! and memory under churn" — a 20-second `sample` put a thread at 45% of CPU and
//! a cumulative reading refuted it at 3.4%). A difference of two monotone
//! readings cannot lie that way.
//!
//! **`CLOCK_THREAD_CPUTIME_ID`, not Mach `thread_info`.** Both give per-thread
//! cumulative CPU; the POSIX clock is one call with no port to manage and no
//! `count`-word protocol to get wrong, and it's the same spelling on Linux if
//! this is ever needed there. Verified per-thread rather than per-process: a
//! thread that sleeps while a sibling burns 400 M iterations reads 0.03 ms.
//!
//! On non-macOS targets this returns `None` (`libc` is macOS-only in this
//! crate); callers degrade to "no counter", never to a wrong number.

use std::time::Duration;

/// The calling thread's cumulative CPU time (user + system) since it started, or
/// `None` if the platform has no per-thread clock or the read failed.
///
/// Cheap enough to call on a logging heartbeat: one `clock_gettime`, no
/// allocation. Don't call it per message — the point is a per-window delta, and
/// the syscall would then be a real fraction of what it measures.
pub fn current_thread_cpu_time() -> Option<Duration> {
    #[cfg(target_os = "macos")]
    {
        read_thread_cpu_time_macos()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// The real FFI, shared by the public wrapper and the test that verifies it.
#[cfg(target_os = "macos")]
fn read_thread_cpu_time_macos() -> Option<Duration> {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `clock_gettime` writes a `timespec` through the pointer we hand it.
    // `ts` is a live, writable local of exactly that type, and
    // `CLOCK_THREAD_CPUTIME_ID` is a valid clock id on macOS 10.12+. The call
    // reads no Rust-side state and cannot invalidate any. A non-zero return means
    // it wrote nothing, which the check below honors.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    if rc != 0 {
        return None;
    }
    Some(Duration::new(
        ts.tv_sec.max(0) as u64,
        ts.tv_nsec.clamp(0, 999_999_999) as u32,
    ))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// The counter must move with work done ON THIS THREAD and stay monotone.
    #[test]
    fn burning_cpu_advances_the_calling_threads_counter() {
        let before = read_thread_cpu_time_macos().expect("CLOCK_THREAD_CPUTIME_ID must be readable");
        let mut sink = 0u64;
        for i in 0..20_000_000u64 {
            sink = sink.wrapping_add(i).wrapping_mul(2_654_435_761);
        }
        std::hint::black_box(sink);
        let after = read_thread_cpu_time_macos().expect("second read");
        assert!(
            after > before,
            "thread CPU time must advance after real work: {before:?} → {after:?}"
        );
    }

    /// The whole reason this exists: it must attribute CPU to ONE thread. A
    /// thread that only waits must not accumulate a busy sibling's time, or the
    /// writer's number would just be the process's.
    #[test]
    fn a_sibling_threads_work_does_not_land_on_this_threads_counter() {
        let before = read_thread_cpu_time_macos().expect("first read");
        std::thread::spawn(|| {
            let mut sink = 0u64;
            for i in 0..200_000_000u64 {
                sink = sink.wrapping_add(i).wrapping_mul(2_654_435_761);
            }
            std::hint::black_box(sink);
        })
        .join()
        .expect("worker thread panicked");
        let after = read_thread_cpu_time_macos().expect("second read");
        assert!(
            after.saturating_sub(before) < Duration::from_millis(50),
            "a joining thread must not be charged the worker's CPU: {before:?} → {after:?}"
        );
    }
}
