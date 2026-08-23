//! Why a memory operation was turned down.
//!
//! Its own module so the jail and the store can both name it without depending on each other.

/// Why a memory operation was turned down.
///
/// Typed, and every variant reaches the model as a machine-readable token beside its sentence:
/// a refusal it has to parse out of prose is a refusal it will misread on the first copy edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryRefusal {
    /// The path pointed outside the memory folder: absolute, a `..`, or a symlink.
    OutsideMemory,
    /// Memory holds Markdown and nothing else.
    NotMarkdown,
    /// The path was blank, or named nothing after the `.` segments came out.
    NoPath,
    /// The folder is at its cap. ❌ Never a silent failure: the model is told to prune.
    DirectoryFull { used: u64, cap: u64, wanted: u64 },
    /// `memory_edit` on a file that isn't there.
    NoSuchFile,
    /// `memory_edit` whose `oldString` appears nowhere in the file.
    NoMatch,
    /// `memory_edit` whose `oldString` appears more than once. Guessing which one the model
    /// meant would rewrite a line it never looked at, in a file that rides every later turn.
    NotUnique { matches: usize },
    /// The disk said no. The detail is for the model to relay, never to branch on.
    Unwritable(String),
}

impl MemoryRefusal {
    /// The stable token the tool result carries. What anything downstream matches on.
    pub fn token(&self) -> &'static str {
        match self {
            MemoryRefusal::OutsideMemory => "outsideMemory",
            MemoryRefusal::NotMarkdown => "notMarkdown",
            MemoryRefusal::NoPath => "noPath",
            MemoryRefusal::DirectoryFull { .. } => "memoryFull",
            MemoryRefusal::NoSuchFile => "noSuchFile",
            MemoryRefusal::NoMatch => "noMatch",
            MemoryRefusal::NotUnique { .. } => "notUnique",
            MemoryRefusal::Unwritable(_) => "unwritable",
        }
    }
}
