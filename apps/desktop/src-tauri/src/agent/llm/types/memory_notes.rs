//! The payload `AgentPart::MemoryNotes` carries, as plain data. A leaf: the diff and the fenced
//! rendering live with prompt assembly (`chat/context/memory_notes.rs`), which depends on this
//! module and never the other way round.

use serde::{Deserialize, Serialize};

/// The agent's memory as one user turn carries it: each file whose text differs from the
/// thread's newest earlier copy, as it stands now.
///
/// ⚠️ **Memory rides the transcript, ❌ never the system string.** The system string and the
/// tool declarations lead every cached prefix, so a note saved mid-chat would re-bill the whole
/// thread at full price. Carried here, a write moves only what comes after the last call.
/// `chat/DETAILS.md` § The memory block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNotes {
    pub files: Vec<MemoryNote>,
}

/// One memory file at one moment. Empty `text` says the file is gone or emptied since the copy
/// the thread carried, so the model stops relying on that copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNote {
    /// The file's name inside the memory folder, as the memory tools take it (`AGENTS.md`).
    pub file: String,
    /// What this turn carries of it, already cut to the turn's share of the budget.
    pub text: String,
}
