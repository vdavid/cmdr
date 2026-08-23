//! What the agent remembers between threads: one small folder of Markdown it writes itself.
//!
//! [`MemoryStore`] is pure — parameterized on a root path and touching nothing else — so every
//! rule below is unit-testable against a `tempdir`. Resolving the real root out of the app's
//! data dir is the tool handler's job (`agent/tools/memory.rs`), the only part that needs an
//! `AppHandle`. There is no Tauri mock runtime in the tree, so that split is what makes these
//! rules testable at all.
//!
//! Depth, the two caps, and why each one exists: `DETAILS.md`.

mod jail;
mod store;

#[cfg(test)]
mod tests;

pub use jail::MEMORY_EXTENSION;
pub use store::{HUB_FILE, MEMORY_DIR_MAX_BYTES, MemoryRefusal, MemoryStore, MemoryWritten};
