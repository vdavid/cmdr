//! macOS: the full snapshot (`task_info`, the Rust heap, the malloc zones).

use super::super::breakdown::Breakdown;
use super::super::memory_snapshot::MemorySnapshot;

pub(crate) fn breakdown() -> Option<Breakdown> {
    MemorySnapshot::capture().map(|snapshot| snapshot.breakdown())
}
