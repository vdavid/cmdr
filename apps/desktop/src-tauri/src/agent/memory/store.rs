//! The store itself: the two caps, the write, the edit, and the slice a turn carries.
//!
//! Pure of Tauri and of app state — a root path is the whole of its world — so the rules here
//! are unit-tested against a `tempdir` rather than inferred from a handler nobody can run.

use std::path::{Path, PathBuf};

use crate::agent::memory::jail;
use crate::agent::memory::refusal::MemoryRefusal;

const LOG_TARGET: &str = "agent::memory";

/// The hub file. It is auto-fed into every turn's prefix, and for now it is the only file the
/// agent has any reason to write, which is why there is no read or list tool: every schema
/// rides in the cached prefix of every turn, the rail's included.
pub const HUB_FILE: &str = "AGENTS.md";

/// The most the whole memory folder may hold, across every `.md` in it.
///
/// ⚠️ **This cap protects DISK, and it is not the one that protects the prompt.** A turn feeds
/// a slice sized as a share of the resolved prompt budget (`chat::budget::memory_slice_bytes`),
/// which is a different number for a different reason. Conflating them is how a byte cap ends
/// up sized for a 16k window and starving a 60k one, or the other way round.
pub const MEMORY_DIR_MAX_BYTES: u64 = 64 * 1024;

/// What the two TOOLS may claim of [`MEMORY_DIR_MAX_BYTES`]: whatever the decision ring
/// reserves is held back from them.
///
/// ⚠️ **A reserve, not just a second cap.** `outcomes.md` is written mechanically, once per
/// decided proposal, with no model turn to hand a [`MemoryRefusal::DirectoryFull`] to
/// (`outcomes.rs`). If the model's own notes could fill the folder, the first thing to stop
/// working would be the channel that teaches it what the user actually wants — silently, and
/// worst for the user who uses the agent most.
pub const MEMORY_MODEL_MAX_BYTES: u64 = MEMORY_DIR_MAX_BYTES - super::outcomes::OUTCOMES_MAX_BYTES as u64;

/// How much of a turn's memory slice the decision ring may claim, as a divisor. The rest is the
/// hub's: what the agent worked out about the person outranks the raw log of what they clicked.
const OUTCOMES_PROMPT_SHARE: usize = 4;

/// What a cut slice says about itself, so the model knows it is reading the head of a file
/// rather than the whole of what it once wrote.
const TRUNCATION_NOTE: &str =
    "\n\n[Cut off here: memory is longer than this turn can carry. Prune it with memory_edit so the rest fits.]";

/// What a landed write or edit reports back, so the model can see it worked and how much room
/// is left before it has to prune.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryWritten {
    /// The path as the model asked for it.
    pub path: String,
    /// What the file holds now.
    pub bytes: usize,
    /// What the folder has left against [`MEMORY_MODEL_MAX_BYTES`].
    pub remaining_bytes: u64,
}

/// The agent's memory folder, rooted at `<data-dir>/ai/memory/`.
pub struct MemoryStore {
    root: PathBuf,
}

impl MemoryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where this store writes. The settings window's "Open memory folder" needs it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// What one turn carries of memory: the hub file and the decision ring, each cut to its
    /// own share of `max_bytes`. `None` when there is nothing worth prefixing a turn with.
    ///
    /// The ring takes its bounded share FIRST and the hub gets everything left, so a busy week
    /// of approvals can never displace what the agent worked out about the person.
    pub fn read_for_prompt(&self, max_bytes: usize) -> Option<String> {
        let outcomes = self.read_outcomes_for_prompt(max_bytes / OUTCOMES_PROMPT_SHARE);
        let spent = outcomes.as_deref().map_or(0, str::len);
        match (self.read_hub_for_prompt(max_bytes.saturating_sub(spent)), outcomes) {
            (None, None) => None,
            (Some(hub), None) => Some(hub),
            (None, Some(outcomes)) => Some(outcomes),
            (Some(hub), Some(outcomes)) => Some(format!("{hub}\n\n{outcomes}")),
        }
    }

    /// The hub file's text for a turn's prefix, cut to `max_bytes` with a note when it had to
    /// be. `None` when there is nothing worth prefixing a turn with.
    ///
    /// ⚠️ **Unreadable is not the same as absent.** Under a bare `read_to_string(..).ok()` a
    /// non-UTF8 or permission-denied file leaves the agent believing it has never remembered
    /// anything, so it starts the user over with no sign that anything went wrong. Both cases
    /// are logged.
    fn read_hub_for_prompt(&self, max_bytes: usize) -> Option<String> {
        let path = self.root.join(HUB_FILE);
        let mut text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                log::warn!(
                    target: LOG_TARGET,
                    "memory is on disk but this turn carries none of it: {e}"
                );
                return None;
            }
        };
        if text.trim().is_empty() {
            return None;
        }
        if text.len() > max_bytes {
            // The cut is a byte count over text, so it can land inside a character. Walking
            // back to the nearest boundary keeps a non-ASCII memory readable instead of
            // dropping it whole.
            let mut cut = max_bytes;
            while cut > 0 && !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
            text.push_str(TRUNCATION_NOTE);
        }
        Some(text)
    }

    /// Create or fully replace one memory file.
    pub fn write(&self, requested: &str, content: &str) -> Result<MemoryWritten, MemoryRefusal> {
        let path = jail::resolve(&self.root, requested)?;
        self.store(requested, &path, content)
    }

    /// Land a file priced against a cap of the caller's choosing, jailed exactly as a tool
    /// write is. Only the decision ring passes anything but [`MEMORY_MODEL_MAX_BYTES`], and
    /// only because it is what reserved the difference.
    pub(super) fn store_capped(
        &self,
        requested: &str,
        content: &str,
        cap: u64,
    ) -> Result<MemoryWritten, MemoryRefusal> {
        let path = jail::resolve(&self.root, requested)?;
        self.store_against(requested, &path, content, cap)
    }

    /// Replace one exact, unique occurrence inside one memory file.
    pub fn edit(&self, requested: &str, old: &str, new: &str) -> Result<MemoryWritten, MemoryRefusal> {
        let path = jail::resolve(&self.root, requested)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(MemoryRefusal::NoSuchFile),
            Err(e) => return Err(MemoryRefusal::Unwritable(e.to_string())),
        };
        let matches = if old.is_empty() { 0 } else { text.matches(old).count() };
        match matches {
            0 => return Err(MemoryRefusal::NoMatch),
            1 => {}
            matches => return Err(MemoryRefusal::NotUnique { matches }),
        }
        self.store(requested, &path, &text.replacen(old, new, 1))
    }

    /// The shared tail of both tools: price the write against the folder cap, then land it
    /// durably.
    fn store(&self, requested: &str, path: &Path, content: &str) -> Result<MemoryWritten, MemoryRefusal> {
        self.store_against(requested, path, content, MEMORY_MODEL_MAX_BYTES)
    }

    fn store_against(
        &self,
        requested: &str,
        path: &Path,
        content: &str,
        cap: u64,
    ) -> Result<MemoryWritten, MemoryRefusal> {
        let used = self.used_bytes();
        let replaced = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        let wanted = used.saturating_sub(replaced).saturating_add(content.len() as u64);
        if wanted > cap {
            return Err(MemoryRefusal::DirectoryFull { used, cap, wanted });
        }

        // ⚠️ Durably. The user is invited into this folder by a settings control while the
        // agent may be writing in it, so a torn file is reachable rather than theoretical.
        let temp = temp_beside(path);
        crate::config::durable_write_json(path, &temp, content).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            MemoryRefusal::Unwritable(e.to_string())
        })?;

        Ok(MemoryWritten {
            path: requested.to_string(),
            bytes: content.len(),
            remaining_bytes: cap.saturating_sub(wanted),
        })
    }

    /// Delete every note the agent has written and report how many went. What the settings
    /// window's "Forget everything" does.
    ///
    /// ⚠️ **`.md` files only, and the folders stay.** The same reasoning [`used_bytes`] applies
    /// to the cap applies here: anything else under the root is something the agent could never
    /// have written, so it is the user's and not ours to delete. Keeping the root itself means
    /// the next write lands without the jail having to recreate it, and the user is not left
    /// looking at a pane for a directory that vanished under them.
    ///
    /// [`used_bytes`]: MemoryStore::used_bytes
    pub fn forget_all(&self) -> Result<usize, MemoryRefusal> {
        let mut forgotten = 0;
        let mut folders = vec![self.root.clone()];
        while let Some(folder) = folders.pop() {
            let entries = match std::fs::read_dir(&folder) {
                Ok(entries) => entries,
                // An absent root is nothing to forget, which is a success: the button exists
                // before the agent has ever written anything.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(MemoryRefusal::Unwritable(e.to_string())),
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else { continue };
                let path = entry.path();
                if kind.is_dir() {
                    folders.push(path);
                } else if kind.is_file() && is_markdown(&path) {
                    std::fs::remove_file(&path).map_err(|e| MemoryRefusal::Unwritable(e.to_string()))?;
                    forgotten += 1;
                }
            }
        }
        log::info!(target: LOG_TARGET, "the user cleared the agent's memory: {forgotten} note(s) deleted");
        Ok(forgotten)
    }

    /// What every `.md` under the root adds up to. Anything else in the folder (a stale temp,
    /// something the user dropped in) is deliberately not counted: the cap exists to bound
    /// what the AGENT can write, and counting a stray file would jam it with no way out.
    fn used_bytes(&self) -> u64 {
        let mut total = 0;
        let mut folders = vec![self.root.clone()];
        while let Some(folder) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    folders.push(entry.path());
                } else if kind.is_file() && is_markdown(&entry.path()) {
                    total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                }
            }
        }
        total
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(jail::MEMORY_EXTENSION))
}

/// The temp file a durable write renames from: a hidden sibling, so it lands on the same
/// filesystem as its target and so `used_bytes` skips it.
fn temp_beside(path: &Path) -> PathBuf {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned());
    let temp = format!(".{}.tmp", name.as_deref().unwrap_or(HUB_FILE));
    path.parent().unwrap_or(Path::new(".")).join(temp)
}
