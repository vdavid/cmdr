//! Memory watchdog: monitors the app's memory and takes action at safety
//! thresholds to prevent unbounded memory growth.
//!
//! - 8 GB: logs a warning with a full memory breakdown.
//! - 16 GB: stops EVERY volume's index, emits a user-visible event, and logs
//!   the same breakdown.
//!
//! **The threshold basis is `phys_footprint`, not `resident_size`.** On macOS,
//! `resident_size` (RSS) counts GPU/WebView graphics mappings (the WebKit Metal
//! compositor's `IOAccelerator` region can be multiple GB) that are NOT real
//! memory pressure. `phys_footprint` is the metric macOS itself keys memory
//! pressure and jetsam on, and it's what Activity Monitor's "Memory" column
//! shows. Basing the stop on RSS would let WebView graphics memory trip the
//! machine-protection stop while the indexing heap is a couple hundred MB. So
//! the per-tick check reads `phys_footprint`; when a threshold trips, a full
//! `MemorySnapshot` (phys, resident, the resident−phys graphics delta, and the
//! actual malloc heap) goes into the log so a rare event carries real
//! diagnostic context, not a bare number.
//!
//! **The budget is GLOBAL, not per-volume** (plan rabbit hole #8, resolved by
//! David). Scans run in parallel — the network/USB wire is the bottleneck, not
//! RAM — so there's no one-at-a-time serialization; instead a single process-
//! wide budget is the safety net that stops ALL indexing if total memory
//! crosses the catastrophe line. The 16 GB number is a machine-protection stop,
//! NOT expected usage (real scan memory is the accumulator maps + the 20K
//! writer channel — hundreds of MB per normal volume).
//!
//! On non-macOS platforms this is a no-op stub (platform memory queries
//! differ and can be added later).

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use crate::indexing::lifecycle::state;
#[cfg(target_os = "macos")]
use crate::pluralize::grouped;

/// 8 GB in bytes.
#[cfg(target_os = "macos")]
const WARN_THRESHOLD: u64 = 8 * 1024 * 1024 * 1024;

/// 16 GB in bytes.
#[cfg(target_os = "macos")]
const STOP_THRESHOLD: u64 = 16 * 1024 * 1024 * 1024;

/// How often the watchdog checks memory (seconds).
#[cfg(target_os = "macos")]
const CHECK_INTERVAL_SECS: u64 = 5;

/// Whether the single global watchdog task is already running. The watchdog is
/// process-wide (one global budget over all volumes), so the first `start()`
/// wins and later per-volume `start_indexing_for` calls are no-ops — without
/// this, every volume start would spawn a redundant watchdog loop all racing to
/// stop indexing.
#[cfg(target_os = "macos")]
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);

/// Start the global memory watchdog as a fire-and-forget background task.
///
/// On macOS, spawns ONE task (idempotent across volumes) that checks
/// `phys_footprint` every 5 seconds via `task_info`. On other platforms, no-op.
#[cfg(target_os = "macos")]
pub fn start(app: tauri::AppHandle) {
    // Idempotent: only the first caller spawns the single global watchdog.
    if WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        run_watchdog(app).await;
        // Let a future `start()` respawn it (e.g. after the budget stop returned).
        WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn start(_app: tauri::AppHandle) {
    // No-op on non-macOS platforms
}

#[cfg(target_os = "macos")]
async fn run_watchdog(app: tauri::AppHandle) {
    use std::time::Duration;

    let mut interval = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    let mut warned = false;

    loop {
        interval.tick().await;

        // Per-tick check is cheap: one `task_info` call for `phys_footprint`.
        // The full breakdown is gathered only when a threshold actually trips.
        let phys_footprint = match crate::process_memory::current_phys_footprint() {
            Some(b) => b,
            None => continue,
        };

        if phys_footprint >= STOP_THRESHOLD {
            let snapshot = MemorySnapshot::capture();

            // Drives a user-visible toast; exactly the kind of error we want to ship
            // diagnostic context for when the user has opted in.
            crate::log_error!(
                "Memory watchdog: phys_footprint {:.2} GB exceeded the {} GB safety limit. \
                 Stopping all indexing to prevent a system crash.\n{}",
                gb(phys_footprint),
                STOP_THRESHOLD / (1024 * 1024 * 1024),
                snapshot.as_ref().map(MemorySnapshot::report).unwrap_or_default(),
            );

            // Emit user-visible event, carrying the discriminating figures (not
            // just RSS) so a shipped error report tells the real story.
            use tauri_specta::Event;
            let _ =
                MemorySnapshot::memory_warning_event(snapshot.as_ref(), phys_footprint, "stopped_indexing").emit(&app);

            // Global budget: stop EVERY registered volume's index, not just
            // `root`. Scans run in parallel (the wire, not RAM, is the
            // bottleneck), so the safety net is one process-wide stop rather than
            // per-volume serialization (plan rabbit hole #8).
            state::stop_all_indexing();
            return;
        }

        if phys_footprint >= WARN_THRESHOLD && !warned {
            warned = true;
            let snapshot = MemorySnapshot::capture();
            log::warn!(
                "Memory watchdog: phys_footprint {:.2} GB crossed the {} GB warning threshold. \
                 Indexing continues but the system may be under memory pressure.\n{}",
                gb(phys_footprint),
                WARN_THRESHOLD / (1024 * 1024 * 1024),
                snapshot.as_ref().map(MemorySnapshot::report).unwrap_or_default(),
            );
        }

        // Reset warning flag if memory drops back below the threshold
        if phys_footprint < WARN_THRESHOLD && warned {
            warned = false;
        }
    }
}

/// Bytes as gibibytes, for log formatting.
#[cfg(target_os = "macos")]
fn gb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

/// Bytes as mebibytes, for log formatting.
#[cfg(target_os = "macos")]
fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

// ── Memory snapshot ──────────────────────────────────────────────────

/// A full memory breakdown, gathered when a threshold trips. Splitting the real
/// heap (malloc) from resident and phys_footprint is the whole point: it tells
/// at a glance whether a spike is the indexing heap or WebView/GPU graphics.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct MemorySnapshot {
    /// The machine-pressure metric the thresholds key on (what Activity Monitor
    /// shows, what jetsam watches).
    phys_footprint: u64,
    /// Peak `phys_footprint` over the process lifetime, if the running kernel
    /// reports it (`ledger_phys_footprint_peak`).
    phys_footprint_peak: Option<u64>,
    /// Resident set size (RSS). Over-counts GPU/WebView graphics mappings.
    resident_size: u64,
    /// High-water mark of RSS.
    resident_size_max: u64,
    /// The real Rust/C heap in use across all malloc zones — indexing's actual
    /// footprint.
    heap_in_use: u64,
    /// Heap bytes reserved from the OS across all zones (in use + free).
    heap_reserved: u64,
    /// Number of malloc zones.
    zone_count: u32,
    /// The largest zone by in-use bytes: `(name, in_use)`.
    largest_zone: Option<(String, u64)>,
    /// Live FSEvents processed so far (a cheap indexing-internal pressure
    /// signal already tracked in this module's `super`).
    // TODO: also surface writer-channel depth and reconciler `pending_events`
    // len here once they're exposed as atomics — both are real indexing-memory
    // signals but neither is reachable from the watchdog today without new
    // plumbing.
    live_event_count: u64,
}

#[cfg(target_os = "macos")]
impl MemorySnapshot {
    /// Gather the full breakdown. Returns `None` only if the load-bearing
    /// `phys_footprint` query fails; the heap and peak degrade gracefully.
    fn capture() -> Option<MemorySnapshot> {
        let vm = crate::process_memory::query_task_vm_info()?;
        let basic = query_basic_info().unwrap_or(BasicInfo {
            resident_size: vm.resident_size,
            resident_size_max: 0,
        });
        let heap = query_malloc_heap();

        Some(MemorySnapshot {
            phys_footprint: vm.phys_footprint,
            phys_footprint_peak: vm.phys_footprint_peak,
            resident_size: basic.resident_size,
            resident_size_max: basic.resident_size_max,
            heap_in_use: heap.in_use,
            heap_reserved: heap.reserved,
            zone_count: heap.zone_count,
            largest_zone: heap.largest_zone,
            live_event_count: crate::indexing::DEBUG_STATS.live_event_count.load(Ordering::Relaxed),
        })
    }

    /// The resident−phys_footprint delta: the tell for graphics/shared memory.
    /// A large delta means GPU/WebView mappings (which RSS counts but
    /// phys_footprint largely excludes), NOT the indexing heap.
    fn graphics_delta(&self) -> u64 {
        self.resident_size.saturating_sub(self.phys_footprint)
    }

    /// A multi-line breakdown for the log. Deliberately verbose: this fires
    /// rarely, and when it does we want a real head start on diagnosis.
    fn report(&self) -> String {
        let peak = match self.phys_footprint_peak {
            Some(p) => format!(", peak {:.2} GB", gb(p)),
            None => String::new(),
        };
        let largest = match &self.largest_zone {
            Some((name, bytes)) => format!(" (largest: {} {:.0} MB)", name, mb(*bytes)),
            None => String::new(),
        };
        format!(
            "  phys_footprint: {:.2} GB{} — machine-pressure metric (Activity Monitor's Memory, what jetsam keys on)\n\
             \x20 resident_size:  {:.2} GB (max {:.2} GB) — RSS; includes GPU/shared mappings phys_footprint excludes\n\
             \x20 resident−phys:  {:.2} GB — likely WebView/GPU memory (IOAccelerator), NOT the indexing heap\n\
             \x20 malloc heap:    {:.0} MB in use, {:.0} MB reserved across {} zone(s){} — the real Rust/C heap; indexing lives here\n\
             \x20 live FSEvents:  {} processed\n\
             \x20 Hint: a large resident−phys_footprint delta usually means WebView/GPU memory, not the indexing heap.",
            gb(self.phys_footprint),
            peak,
            gb(self.resident_size),
            gb(self.resident_size_max),
            gb(self.graphics_delta()),
            mb(self.heap_in_use),
            mb(self.heap_reserved),
            self.zone_count,
            largest,
            grouped(self.live_event_count),
        )
    }

    /// Build the frontend event. Falls back to whatever the caller already knows
    /// (`phys_footprint`) if the full snapshot couldn't be gathered.
    fn memory_warning_event(
        snapshot: Option<&MemorySnapshot>,
        phys_footprint: u64,
        action: &str,
    ) -> crate::indexing::IndexMemoryWarningEvent {
        crate::indexing::IndexMemoryWarningEvent {
            resident_gb: snapshot.map(|s| s.resident_size).unwrap_or(phys_footprint) / (1024 * 1024 * 1024),
            phys_footprint_gb: phys_footprint / (1024 * 1024 * 1024),
            heap_mb: snapshot.map(|s| s.heap_in_use).unwrap_or(0) / (1024 * 1024),
            action: action.to_string(),
        }
    }
}

// ── Mach `task_info` queries ─────────────────────────────────────────

/// The prefix of `mach_task_basic_info` we read.
#[cfg(target_os = "macos")]
struct BasicInfo {
    resident_size: u64,
    resident_size_max: u64,
}

/// Query `mach_task_basic_info` for resident size and its high-water mark.
///
/// Uses raw FFI because the `libc` crate doesn't expose `MACH_TASK_BASIC_INFO`.
#[cfg(target_os = "macos")]
fn query_basic_info() -> Option<BasicInfo> {
    // Mach task info flavor (from <mach/task_info.h>).
    const MACH_TASK_BASIC_INFO: u32 = 20;

    #[repr(C)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time_seconds: i32,
        user_time_microseconds: i32,
        system_time_seconds: i32,
        system_time_microseconds: i32,
        policy: i32,
        suspend_count: i32,
    }

    let info_count = (size_of::<MachTaskBasicInfo>() / size_of::<libc::c_int>()) as u32;

    #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
    // SAFETY: `info` is zeroed before use, and `count` is set to the struct's size measured in
    // `c_int` (natural_t) words, the count layout `task_info` with `MACH_TASK_BASIC_INFO` expects;
    // `MachTaskBasicInfo` is `#[repr(C)]` and matches the `mach_task_basic_info` layout, so the
    // kernel writes only within `info`. We read `info` only when `result == 0`.
    unsafe {
        let mut info: MachTaskBasicInfo = std::mem::zeroed();
        let mut count = info_count;
        let result = libc::task_info(
            libc::mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info as *mut MachTaskBasicInfo as *mut i32,
            &mut count,
        );
        if result == 0 {
            Some(BasicInfo {
                resident_size: info.resident_size,
                resident_size_max: info.resident_size_max,
            })
        } else {
            log::debug!("Memory watchdog: task_info(BASIC) failed with code {result}");
            None
        }
    }
}

// The `task_vm_info` FFI (`phys_footprint`, resident size, ledger peak) lives in
// `crate::process_memory`, the shared reader used by both this watchdog and the
// log RAM gauge. `MemorySnapshot::capture` calls it directly.

// ── malloc-heap query ────────────────────────────────────────────────

/// `malloc_statistics_t` from `<malloc/malloc.h>`.
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct MallocStatistics {
    blocks_in_use: libc::c_uint,
    size_in_use: libc::size_t,
    max_size_in_use: libc::size_t,
    size_allocated: libc::size_t,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// With a NULL zone, aggregates statistics across every zone in the process.
    fn malloc_zone_statistics(zone: *mut libc::c_void, stats: *mut MallocStatistics);
    /// Fills `*addresses` with a pointer to an array of `*count` zone addresses.
    /// A NULL `reader` uses the default in-process reader.
    fn malloc_get_all_zones(
        task: libc::mach_port_t,
        reader: *mut libc::c_void,
        addresses: *mut *mut usize,
        count: *mut libc::c_uint,
    ) -> libc::c_int;
    /// Returns the zone's name (a NUL-terminated string owned by the zone), or NULL.
    fn malloc_get_zone_name(zone: *mut libc::c_void) -> *const libc::c_char;
}

/// The malloc-heap breakdown: the real Rust/C footprint.
#[cfg(target_os = "macos")]
struct MallocHeap {
    in_use: u64,
    reserved: u64,
    zone_count: u32,
    largest_zone: Option<(String, u64)>,
}

/// Sum the malloc heap across all zones, plus the zone count and largest zone.
///
/// This is the single most useful discriminator: heap ~200 MB while resident is
/// multi-GB immediately says "not indexing, it's graphics."
#[cfg(target_os = "macos")]
fn query_malloc_heap() -> MallocHeap {
    // SAFETY: a NULL zone pointer asks `malloc_zone_statistics` for the
    // all-zones aggregate (documented behavior); `agg` is a `#[repr(C)]` match
    // of `malloc_statistics_t` and is fully written by the call.
    let agg = unsafe {
        let mut agg = MallocStatistics::default();
        malloc_zone_statistics(std::ptr::null_mut(), &mut agg);
        agg
    };

    let mut heap = MallocHeap {
        in_use: agg.size_in_use as u64,
        reserved: agg.size_allocated as u64,
        zone_count: 0,
        largest_zone: None,
    };

    let mut addresses: *mut usize = std::ptr::null_mut();
    let mut count: libc::c_uint = 0;
    #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
    // SAFETY: `mach_task_self()` is our own task; a NULL `reader` selects the
    // default in-process reader, which sets `addresses` to point at the live
    // zone registry (process-owned; we must NOT free it) and `count` to its
    // length. Both out-pointers are valid locals.
    let kr = unsafe { malloc_get_all_zones(libc::mach_task_self(), std::ptr::null_mut(), &mut addresses, &mut count) };

    if kr != 0 || addresses.is_null() {
        return heap;
    }
    heap.zone_count = count;

    // SAFETY: on success `malloc_get_all_zones` set `addresses` to a valid array
    // of `count` zone addresses in this process; we read exactly `count` of them
    // and never mutate or free the buffer.
    let zones = unsafe { std::slice::from_raw_parts(addresses, count as usize) };

    let mut largest = 0u64;
    let mut largest_name: Option<String> = None;
    for &addr in zones {
        let zone = addr as *mut libc::c_void;
        if zone.is_null() {
            continue;
        }
        // SAFETY: `zone` is a live zone pointer from `malloc_get_all_zones`;
        // `stats` is a `#[repr(C)]` match of `malloc_statistics_t`, fully written.
        let stats = unsafe {
            let mut stats = MallocStatistics::default();
            malloc_zone_statistics(zone, &mut stats);
            stats
        };
        let in_use = stats.size_in_use as u64;
        if in_use > largest {
            largest = in_use;
            // SAFETY: `zone` is a live zone pointer; `malloc_get_zone_name`
            // returns a NUL-terminated string owned by the zone, or NULL.
            let name_ptr = unsafe { malloc_get_zone_name(zone) };
            largest_name = if name_ptr.is_null() {
                None
            } else {
                // SAFETY: `name_ptr` is non-NULL and points at a NUL-terminated,
                // zone-owned C string that outlives this borrow.
                Some(
                    unsafe { std::ffi::CStr::from_ptr(name_ptr) }
                        .to_string_lossy()
                        .into_owned(),
                )
            };
        }
    }
    if largest > 0 {
        heap.largest_zone = Some((largest_name.unwrap_or_else(|| "?".to_string()), largest));
    }

    heap
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn query_basic_info_returns_positive_resident() {
        let basic = query_basic_info();
        assert!(basic.is_some(), "should be able to query resident memory");
        assert!(basic.unwrap().resident_size > 0, "resident memory should be positive");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn malloc_heap_sum_is_positive() {
        let heap = query_malloc_heap();
        assert!(heap.in_use > 0, "malloc heap in-use should be positive");
        assert!(heap.reserved >= heap.in_use, "reserved should be >= in-use");
        assert!(heap.zone_count >= 1, "there should be at least one malloc zone");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn snapshot_captures_and_reports_key_fields() {
        let snapshot = MemorySnapshot::capture().expect("snapshot should capture on macOS");
        assert!(snapshot.phys_footprint > 0, "phys_footprint should be positive");
        assert!(snapshot.resident_size > 0, "resident_size should be positive");
        assert!(snapshot.heap_in_use > 0, "heap should be positive");

        let report = snapshot.report();
        for needle in [
            "phys_footprint",
            "resident_size",
            "resident−phys",
            "malloc heap",
            "live FSEvents",
        ] {
            assert!(
                report.contains(needle),
                "report should mention {needle}; got:\n{report}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn thresholds_are_ordered() {
        const {
            assert!(
                WARN_THRESHOLD < STOP_THRESHOLD,
                "warn threshold must be below stop threshold"
            )
        };
    }
}
