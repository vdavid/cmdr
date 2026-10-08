//! The breakdown the memory watchdog logs when a threshold trips: where the
//! bytes are, and a verdict derived from the same numbers.
//!
//! **It reads the Rust heap and the other malloc zones apart, and says which one
//! holds the bytes.** Under mimalloc the Rust heap is invisible to the macOS
//! malloc-zone APIs, so a zone-only reading under-reports the heap the watchdog
//! polices by orders of magnitude; under the system allocator it's the default
//! zone. `cmdr_fs::process_memory` owns that split and the readers;
//! `MemoryAttribution` here turns the numbers into the log's verdict, so the
//! claim can never contradict the figures beside it.
//!
//! macOS-only, like the watchdog's policy loop (`memory_watchdog.rs`).

use std::sync::atomic::Ordering;

use cmdr_fs::pluralize::grouped;

/// Bytes as gibibytes, for log formatting.
pub(super) fn gb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

/// Bytes as mebibytes, for log formatting.
fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Which accountant explains the bulk of `phys_footprint`. Derived purely from
/// the numbers in a [`MemorySnapshot`], so the log's claim can't drift from the
/// figures printed next to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryAttribution {
    /// The Rust heap: every Rust allocation, indexing included.
    RustHeap,
    /// The system malloc zones: WebKit, Objective-C, C libraries.
    SystemMalloc,
    /// Neither allocator claims it: graphics surfaces, mapped files, stacks.
    Unattributed,
    /// No single source holds a majority.
    Mixed,
}

impl MemoryAttribution {
    /// Classify a footprint by its two allocator readings. `rust_heap` and
    /// `system_malloc` are disjoint (see `cmdr_fs::process_memory`), so whatever
    /// they don't cover is unattributed.
    fn classify(phys_footprint: u64, rust_heap: u64, system_malloc: u64) -> MemoryAttribution {
        let untracked = untracked_bytes(phys_footprint, rust_heap, system_malloc);
        let majority = phys_footprint / 2;
        if phys_footprint == 0 {
            return MemoryAttribution::Mixed;
        }
        if rust_heap >= majority && rust_heap >= system_malloc && rust_heap >= untracked {
            MemoryAttribution::RustHeap
        } else if system_malloc >= majority && system_malloc >= untracked {
            MemoryAttribution::SystemMalloc
        } else if untracked >= majority {
            MemoryAttribution::Unattributed
        } else {
            MemoryAttribution::Mixed
        }
    }

    /// The one-line verdict for the log.
    fn explanation(self) -> &'static str {
        match self {
            MemoryAttribution::RustHeap => {
                "the Rust heap holds most of it, so this IS backend memory: indexing, media, or another Rust subsystem"
            }
            MemoryAttribution::SystemMalloc => {
                "the system malloc zones hold most of it, so this is WebKit / Objective-C, not the Rust backend"
            }
            MemoryAttribution::Unattributed => {
                "neither allocator claims most of it: look at graphics surfaces, mapped files, and thread stacks"
            }
            MemoryAttribution::Mixed => "no single source holds a majority; read the lines above",
        }
    }
}

/// `phys_footprint` minus what both readings account for. Saturating: an
/// allocator can hold pages the footprint no longer counts (mimalloc's committed
/// pages, a zone's reserved ones), so the readings can sum past `phys_footprint`.
fn untracked_bytes(phys_footprint: u64, rust_heap: u64, system_malloc: u64) -> u64 {
    phys_footprint.saturating_sub(rust_heap.saturating_add(system_malloc))
}

/// A full memory breakdown, gathered when a threshold trips. The point is to
/// name where the bytes actually are: our Rust heap, the system allocator, or
/// neither.
#[derive(Debug, Clone)]
pub(super) struct MemorySnapshot {
    /// The machine-pressure metric the thresholds key on (what Activity Monitor
    /// shows, what jetsam watches).
    phys_footprint: u64,
    /// Peak `phys_footprint` over the process lifetime, if the running kernel
    /// reports it (`ledger_phys_footprint_peak`).
    phys_footprint_peak: Option<u64>,
    /// Resident set size (RSS). Counts graphics and shared mappings that
    /// `phys_footprint` excludes.
    resident_size: u64,
    /// High-water mark of RSS.
    resident_size_max: u64,
    /// The Rust heap, where indexing lives, as the global allocator reports it.
    rust_heap: cmdr_fs::process_memory::RustHeap,
    /// Bytes the malloc zones beyond the Rust heap hold. Disjoint from `rust_heap`.
    system_malloc_in_use: u64,
    /// Bytes those zones reserved from the OS (in use + free).
    system_malloc_reserved: u64,
    /// Number of those zones.
    zone_count: u32,
    /// The largest of those zones by in-use bytes: `(name, in_use)`.
    largest_zone: Option<(String, u64)>,
    /// Live FSEvents processed so far (a cheap indexing-internal pressure
    /// signal already tracked in this module's `super`).
    // TODO: also surface writer-channel depth and reconciler `pending_events`
    // len here once they're exposed as atomics — both are real indexing-memory
    // signals but neither is reachable from the watchdog today without new
    // plumbing.
    live_event_count: u64,
}

impl MemorySnapshot {
    /// Gather the full breakdown. Returns `None` only if the load-bearing
    /// `phys_footprint` query fails; everything else degrades gracefully.
    pub(super) fn capture() -> Option<MemorySnapshot> {
        let vm = cmdr_fs::process_memory::query_task_vm_info()?;
        let basic = cmdr_fs::process_memory::query_basic_info();
        let rust_heap = cmdr_fs::process_memory::query_rust_heap();
        let zones = cmdr_fs::process_memory::query_system_malloc_zones();

        Some(MemorySnapshot {
            phys_footprint: vm.phys_footprint,
            phys_footprint_peak: vm.phys_footprint_peak,
            resident_size: basic.as_ref().map_or(vm.resident_size, |b| b.resident_size),
            resident_size_max: basic.as_ref().map_or(0, |b| b.resident_size_max),
            rust_heap,
            system_malloc_in_use: zones.in_use,
            system_malloc_reserved: zones.reserved,
            zone_count: zones.zone_count,
            largest_zone: zones.largest_zone,
            live_event_count: crate::indexing::DEBUG_STATS.live_event_count.load(Ordering::Relaxed),
        })
    }

    /// `phys_footprint` neither allocator accounts for.
    fn untracked(&self) -> u64 {
        untracked_bytes(
            self.phys_footprint,
            self.rust_heap.held_bytes(),
            self.system_malloc_in_use,
        )
    }

    /// Where the bulk of the footprint actually is.
    fn attribution(&self) -> MemoryAttribution {
        MemoryAttribution::classify(
            self.phys_footprint,
            self.rust_heap.held_bytes(),
            self.system_malloc_in_use,
        )
    }

    /// A multi-line breakdown for the log. Deliberately verbose: this fires
    /// rarely, and when it does we want a real head start on diagnosis. Every
    /// line says what its number MEANS, because the previous version's unlabeled
    /// figures got read as the opposite of what they were.
    pub(super) fn report(&self) -> String {
        let peak = match self.phys_footprint_peak {
            Some(p) => format!(", peak {:.2} GB", gb(p)),
            None => String::new(),
        };
        let largest = match &self.largest_zone {
            Some((name, bytes)) => format!(" (largest: {} {:.0} MB)", name, mb(*bytes)),
            None => String::new(),
        };
        let (rust_heap, other_zones, vmmap_hint) = match self.rust_heap {
            cmdr_fs::process_memory::RustHeap::Mimalloc {
                committed,
                peak_committed,
            } => (
                format!(
                    "{:.0} MB committed (peak {:.0} MB) — mimalloc, OUR global allocator: all Rust allocation, indexing included",
                    mb(committed),
                    mb(peak_committed)
                ),
                "WebKit / Objective-C / C only; blind to the Rust heap above",
                "mimalloc tags its arenas with VM tag 100, which macOS names VM_MEMORY_IOACCELERATOR, so `IOAccelerator` rows ARE this Rust heap, not GPU memory",
            ),
            cmdr_fs::process_memory::RustHeap::System { in_use, reserved } => (
                format!(
                    "{:.0} MB in use, {:.0} MB reserved — the default malloc zone, OUR global allocator: all Rust allocation, indexing included, plus Objective-C and C",
                    mb(in_use),
                    mb(reserved)
                ),
                "the other zones: WebKit and framework allocations, excluding the Rust heap above",
                "the Rust heap is in the `Malloc *` rows, shared with Objective-C and C",
            ),
        };
        format!(
            "  phys_footprint:  {:.2} GB{} — the metric macOS keys memory pressure and jetsam on (Activity Monitor's \"Memory\"); the thresholds key on this\n\
             \x20 resident_size:   {:.2} GB (max {:.2} GB) — RSS; counts graphics and shared mappings phys_footprint excludes\n\
             \x20 Rust heap:       {}\n\
             \x20 system malloc:   {:.0} MB in use, {:.0} MB reserved across {} zone(s){} — {}\n\
             \x20 untracked:       {:.0} MB — phys_footprint minus both readings: graphics surfaces, mapped files, thread stacks\n\
             \x20 verdict:         {}\n\
             \x20 live FSEvents:   {} processed\n\
             \x20 Reading vmmap next? {}.",
            gb(self.phys_footprint),
            peak,
            gb(self.resident_size),
            gb(self.resident_size_max),
            rust_heap,
            mb(self.system_malloc_in_use),
            mb(self.system_malloc_reserved),
            self.zone_count,
            largest,
            other_zones,
            mb(self.untracked()),
            self.attribution().explanation(),
            grouped(self.live_event_count),
            vmmap_hint,
        )
    }

    /// Build the memory-warning report. Falls back to whatever the caller already
    /// knows (`phys_footprint`) if the full snapshot couldn't be gathered.
    pub(super) fn memory_warning_event(
        snapshot: Option<&MemorySnapshot>,
        phys_footprint: u64,
        action: crate::MemoryWatchdogAction,
    ) -> crate::IndexEvent {
        crate::IndexEvent::MemoryWarning {
            phys_footprint_bytes: phys_footprint,
            resident_bytes: snapshot.map_or(phys_footprint, |s| s.resident_size),
            global_allocator: cmdr_fs::process_memory::GLOBAL_ALLOCATOR,
            rust_heap_bytes: snapshot.map_or(0, |s| s.rust_heap.held_bytes()),
            system_malloc_bytes: snapshot.map_or(0, |s| s.system_malloc_in_use),
            untracked_bytes: snapshot.map_or(0, MemorySnapshot::untracked),
            action,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn snapshot_captures_and_reports_key_fields() {
        let snapshot = MemorySnapshot::capture().expect("snapshot should capture on macOS");
        assert!(snapshot.phys_footprint > 0, "phys_footprint should be positive");
        assert!(snapshot.resident_size > 0, "resident_size should be positive");
        assert!(snapshot.rust_heap.held_bytes() > 0, "the Rust heap should be positive");
        assert_eq!(
            snapshot.rust_heap.allocator(),
            cmdr_fs::process_memory::GLOBAL_ALLOCATOR,
            "the heap is read from the allocator the build installs"
        );

        let report = snapshot.report();
        for needle in [
            "phys_footprint",
            "resident_size",
            "Rust heap",
            "system malloc",
            "untracked",
            "verdict",
            "live FSEvents",
        ] {
            assert!(
                report.contains(needle),
                "report should mention {needle}; got:\n{report}"
            );
        }
    }

    // ── Attribution ──────────────────────────────────────────────────

    /// The 2026-07 runaway, as the watchdog would have seen it: a 16.5 GB
    /// footprint that was almost entirely the Rust heap, with `resident` equal
    /// to `phys_footprint` (so a zero graphics delta).
    fn incident_snapshot() -> MemorySnapshot {
        MemorySnapshot {
            phys_footprint: 16 * GB + GB / 2,
            phys_footprint_peak: Some(16 * GB + GB / 2),
            resident_size: 16 * GB + GB / 2,
            resident_size_max: 16 * GB + GB / 2,
            rust_heap: cmdr_fs::process_memory::RustHeap::Mimalloc {
                committed: 15 * GB,
                peak_committed: 15 * GB,
            },
            system_malloc_in_use: GB + GB / 2,
            system_malloc_reserved: 2 * GB,
            zone_count: 4,
            largest_zone: Some(("DefaultMallocZone".to_string(), GB)),
            live_event_count: 1_234_567,
        }
    }

    #[test]
    fn the_runaway_that_was_the_rust_heap_is_attributed_to_the_rust_heap() {
        // Pre-fix, this exact shape was logged as "likely WebView/GPU memory
        // (IOAccelerator), NOT the indexing heap" — off a resident−phys delta
        // that was 0.00 GB. Three investigations chased the frontend for it.
        let snapshot = incident_snapshot();
        assert_eq!(snapshot.attribution(), MemoryAttribution::RustHeap);
        assert_eq!(
            snapshot.resident_size, snapshot.phys_footprint,
            "the incident had no graphics delta at all"
        );

        let report = snapshot.report();
        assert!(
            !report.contains("NOT the indexing heap"),
            "the report must not deny the heap it just measured; got:\n{report}"
        );
        assert!(
            report.contains("IS backend memory"),
            "the verdict should name the Rust heap; got:\n{report}"
        );
    }

    /// The same runaway under the system allocator: the heap is the default zone, so the
    /// report sends a `vmmap` reader to the `Malloc *` rows, never to `IOAccelerator`.
    #[test]
    fn a_system_allocator_runaway_points_vmmap_at_the_malloc_rows() {
        let snapshot = MemorySnapshot {
            rust_heap: cmdr_fs::process_memory::RustHeap::System {
                in_use: 14 * GB,
                reserved: 15 * GB,
            },
            ..incident_snapshot()
        };
        assert_eq!(snapshot.attribution(), MemoryAttribution::RustHeap);

        let report = snapshot.report();
        assert!(report.contains("default malloc zone"), "names the zone; got:\n{report}");
        assert!(
            report.contains("`Malloc *` rows"),
            "and where vmmap shows it; got:\n{report}"
        );
        assert!(
            !report.contains("IOAccelerator"),
            "tag 100 isn't the heap in this build; got:\n{report}"
        );
    }

    #[test]
    fn a_webkit_heavy_footprint_is_attributed_to_system_malloc() {
        assert_eq!(
            MemoryAttribution::classify(5 * GB, GB / 4, 4 * GB),
            MemoryAttribution::SystemMalloc
        );
    }

    #[test]
    fn memory_neither_allocator_claims_is_unattributed() {
        // Real graphics/mapped-file territory: both allocators are small.
        assert_eq!(
            MemoryAttribution::classify(8 * GB, GB / 2, GB / 2),
            MemoryAttribution::Unattributed
        );
    }

    #[test]
    fn a_footprint_with_no_majority_holder_is_mixed() {
        // 4 + 3.5 + 2.5 of 10: nobody owns half, so don't pretend to know.
        assert_eq!(
            MemoryAttribution::classify(10 * GB, 4 * GB, 3 * GB + GB / 2),
            MemoryAttribution::Mixed
        );
    }

    #[test]
    fn untracked_bytes_never_underflow_when_the_allocators_overshoot() {
        // An allocator can hold pages phys_footprint no longer counts.
        assert_eq!(untracked_bytes(4 * GB, 5 * GB, GB), 0);
        assert_eq!(untracked_bytes(0, 0, 0), 0);
        assert_eq!(MemoryAttribution::classify(0, 0, 0), MemoryAttribution::Mixed);
    }
}
