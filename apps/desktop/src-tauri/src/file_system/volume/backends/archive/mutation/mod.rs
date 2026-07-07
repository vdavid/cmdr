//! The zip write side: safe-overwrite (temp+rename) archive mutation, decoupled
//! from the `Volume` trait and the manager just like the [read core](super::read).
//! The write-ops `ArchiveEditOperation` driver wraps [`mutator`] with the real
//! event sink, pause gate, and cancel intent; nothing calls `ArchiveVolume`'s own
//! mutation methods (they stay `NotSupported`).
//!
//! Must-knows in [`CLAUDE.md`](CLAUDE.md); design rationale (why temp+rename and
//! never append-in-place, the encrypted-entry refusal, metadata preservation, the
//! leftover policy) in [`DETAILS.md`](DETAILS.md).

pub(crate) mod mutator;
