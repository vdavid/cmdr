//! The chat runtime and its pure context-assembly core.
//!
//! - [`context`]: the pure core — values in, prompt out, no I/O and no clock. The
//!   stable byte-identical prefix, elide-only history compaction, the context envelope
//!   on the latest user turn only, and budget enforcement. Every test here runs with
//!   no tokio runtime.
//! - [`system_prompt`]: the stable identity + rules the model reads (part of the
//!   cached prefix).
//! - [`runtime`]: the chat runtime that drives one user message to an answer —
//!   single-flight per thread, per-message budgets, cancellation, typed errors, and the
//!   crash-safe persistence model. It emits typed progress events through a channel seam
//!   the IPC layer (M6) subscribes to.
//!
//! See `CLAUDE.md` for the must-knows (prefix stability, snapshot-at-send, the crash
//! cases) and `DETAILS.md` for the anatomy-of-one-call reference and the constants
//! table.

pub mod context;
pub mod runtime;
pub mod system_prompt;
