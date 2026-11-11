//! The portable payload a memory-watchdog trip reports, and the unit it's logged in.
//!
//! A leaf both sides of the per-OS seam import: the watchdog reads a `Breakdown`, and the
//! macOS arm builds one from its snapshot. ❌ Don't move it back into `platform/` or the
//! watchdog: the snapshot would then import the module that imports it.

/// A trip's breakdown: where the bytes are, for the log, the error report, and the warning.
pub(crate) struct Breakdown {
    /// The multi-line log breakdown, verdict included.
    pub(crate) report: String,
    /// Resident set size, which counts mappings the footprint leaves out.
    pub(crate) resident_bytes: u64,
    /// What the global allocator holds for the Rust heap.
    pub(crate) rust_heap_bytes: u64,
    /// What the other malloc zones hold, disjoint from the heap.
    pub(crate) system_malloc_bytes: u64,
    /// The footprint neither allocator accounts for.
    pub(crate) untracked_bytes: u64,
}

/// Bytes as gibibytes, for log formatting.
pub(super) fn gb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}
