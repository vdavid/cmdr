//! What a memory-watchdog trip can say beyond the footprint, per OS: macOS reads the full
//! allocator breakdown (`memory_snapshot.rs`); elsewhere the footprint alone is reported.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

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

const _: fn() -> Option<Breakdown> = breakdown;

/// The breakdown right now, or `None` where this OS gives none (or its read failed).
pub(super) use imp::breakdown;
