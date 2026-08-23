//! What the agent remembers between threads: one small folder of Markdown it writes itself.
//!
//! [`MemoryStore`] is pure — parameterized on a root path and touching nothing else — so every
//! rule it holds is unit-testable against a `tempdir`. The only part that needs an `AppHandle`
//! is [`store_for`], eight lines of path resolution. There is no Tauri mock runtime in the
//! tree, so that split is what makes the jail and the caps testable at all.
//!
//! Depth, the two caps, and the injection surface: `DETAILS.md`.

mod jail;
mod refusal;
mod store;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Runtime};

pub use jail::MEMORY_EXTENSION;
pub use refusal::MemoryRefusal;
pub use store::{HUB_FILE, MEMORY_DIR_MAX_BYTES, MemoryStore, MemoryWritten};

/// Where memory lives under the app data dir.
///
/// ⚠️ The app data dir, ❌ not `~/.cmdr/`: this is app-managed state rather than user config,
/// `app_data_dir()` is already the canonical per-OS path on all three platforms, and it
/// inherits `CMDR_DATA_DIR` isolation for free. Sharing the home dotfile would mean an E2E run
/// writing personal facts into the developer's real agent memory.
pub fn memory_root(data_dir: &Path) -> PathBuf {
    data_dir.join("ai").join("memory")
}

/// This app instance's memory store, or `None` when the data dir can't be resolved (the same
/// non-fatal path `agent::start` takes: the app runs without the agent store rather than
/// refusing to launch).
pub fn store_for<R: Runtime>(app: &AppHandle<R>) -> Option<MemoryStore> {
    match crate::config::resolved_app_data_dir(app) {
        Ok(data_dir) => Some(MemoryStore::new(memory_root(&data_dir))),
        Err(e) => {
            log::warn!(target: "agent::memory", "memory is unavailable this session: {e}");
            None
        }
    }
}

/// What one turn carries of the agent's memory: the hub file, cut to this turn's share of the
/// resolved prompt budget (`chat::budget::memory_slice_bytes`), with a note when it was cut.
pub fn read_for_turn<R: Runtime>(app: &AppHandle<R>, prompt_budget: usize) -> Option<String> {
    store_for(app)?.read_for_prompt(crate::agent::chat::budget::memory_slice_bytes(prompt_budget))
}
