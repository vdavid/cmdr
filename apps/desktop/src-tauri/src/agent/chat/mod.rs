//! The chat runtime and its pure context-assembly core.
//!
//! - [`cancel`]: the one registry of in-flight turns, keyed by conversation. Below
//!   `commands/` because a wake registers in it too.
//! - [`budget`]: the per-model prompt budget table plus the one token-size estimator the
//!   whole agent shares (including each tool's self-cap). Pure data + arithmetic.
//! - [`context`]: the pure core — values in, prompt out, no I/O and no clock. The
//!   stable byte-identical prefix, elide-only history compaction, the context envelope
//!   on the latest user turn only, and budget enforcement. Every test here runs with
//!   no tokio runtime.
//! - [`system_prompt`]: the stable identity + rules the model reads (part of the
//!   cached prefix).
//! - [`session`]: what a turn needs resolved from live app state before it can run (the LLM
//!   slot, the prompt budget, the context envelope). Shared by the rail and by a wake.
//! - [`stream`]: the one conversation-keyed transport every turn's progress streams over,
//!   shared by the rail's sends and by wakes.
//! - [`runtime`]: the chat runtime that drives one user message to an answer —
//!   single-flight per thread, per-message budgets, cancellation, typed errors, and the
//!   crash-safe persistence model. It emits typed progress events through a channel seam
//!   the IPC layer subscribes to.
//!
//! See `CLAUDE.md` for the must-knows (prefix stability, snapshot-at-send, the crash
//! cases) and `DETAILS.md` for the anatomy-of-one-call reference and the constants
//! table.

pub mod budget;
pub mod cancel;
pub mod context;
pub mod runtime;
pub mod session;
pub mod stream;
pub mod system_prompt;
