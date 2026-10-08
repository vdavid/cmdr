//! Which global allocator the app runs on, decided in one place.
//!
//! macOS defaults to the system allocator and Linux to mimalloc; the `mimalloc` feature
//! puts macOS on mimalloc too. `build.rs` folds that into the `cmdr_mimalloc` cfg, and
//! everything allocator-specific asks it: the type `main.rs` installs, the constant a
//! reader reports, and which heap reader runs. Why the platforms differ:
//! `crates/cmdr-fs/DETAILS.md` § "Which global allocator".

/// The allocator behind every Rust allocation in the shipped app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum GlobalAllocator {
    /// mimalloc. Its arenas sit under VM tag 100 (`IOAccelerator`), outside every malloc
    /// zone, so the zone APIs can't see the Rust heap.
    Mimalloc,
    /// The platform's `malloc`. On macOS the Rust heap shares the default malloc zone with
    /// Objective-C and C code, and shows as the `MALLOC_*` VM tags.
    System,
}

/// The allocator this build installs as its global one.
#[cfg(cmdr_mimalloc)]
pub const GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator::Mimalloc;
/// The allocator this build installs as its global one.
#[cfg(not(cmdr_mimalloc))]
pub const GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator::System;

/// The type the app's `main.rs` installs with `#[global_allocator]`.
#[cfg(cmdr_mimalloc)]
pub type GlobalAlloc = mimalloc::MiMalloc;
/// The type the app's `main.rs` installs with `#[global_allocator]`.
#[cfg(not(cmdr_mimalloc))]
pub type GlobalAlloc = std::alloc::System;

/// The value the app's `main.rs` installs with `#[global_allocator]`. Only a binary that
/// installs it runs on [`GLOBAL_ALLOCATOR`]: test binaries install their own counting
/// allocator over `System`.
#[cfg(cmdr_mimalloc)]
pub const GLOBAL_ALLOC: GlobalAlloc = mimalloc::MiMalloc;
/// The value the app's `main.rs` installs with `#[global_allocator]`. Only a binary that
/// installs it runs on [`GLOBAL_ALLOCATOR`]: test binaries install their own counting
/// allocator over `System`.
#[cfg(not(cmdr_mimalloc))]
pub const GLOBAL_ALLOC: GlobalAlloc = std::alloc::System;
