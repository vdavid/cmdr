//! The agent subsystem: the in-app AI agent whose first user-facing slice is
//! "Ask Cmdr", a read-only chat rail (spec: `docs/specs/ask-cmdr-spec.md`).
//!
//! The subsystem is named after the persistent entity ("the agent"), not the
//! surface, so later proactive slices (proposals, notifications) grow here too.
//! It builds out over the milestones in `docs/specs/ask-cmdr-plan.md`:
//!
//! - `llm` (M1): the `AgentLlm` seam — the provider-agnostic trait, its
//!   genai-backed impl, the deterministic fake, and the typed message-part model.
//! - `store` (M2): the `main.db` durable store; `start(app)` lands here.
//! - `tools` (M4): the in-process read-only toolset (the agent's registry view).
//! - `chat` (M5): the chat runtime and the pure context-assembly core.
//!
//! See `CLAUDE.md` for must-knows and `DETAILS.md` for the map.

// The agent seam is built ahead of its consumers: M1 lands the `AgentLlm` trait,
// its impls, and the typed part model, but the runtime that drives them and the
// IPC that reaches them arrive in M5/M6. Until a non-test consumer wires the
// subsystem in, its items are legitimately unreferenced from a release build, so
// allow dead_code here (a justified exception to `#![deny(unused)]`). Remove this
// once M5 wires `AgentLlm` into the chat runtime.
#![allow(dead_code, reason = "M1 seam; consumers (runtime, IPC) arrive in M5/M6")]

pub mod llm;
