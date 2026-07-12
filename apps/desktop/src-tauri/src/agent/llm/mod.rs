//! The `AgentLlm` seam: the provider-agnostic boundary the entire chat runtime
//! and UI test against.
//!
//! One method, [`AgentLlm::respond`], makes one cold, self-contained streaming
//! call. Provider types never cross this boundary — the genai coupling is sealed
//! inside [`genai_impl`], and the deterministic [`fake`] lets the whole runtime run
//! with zero network. The typed message-part model lives in [`types`]; its
//! non-negotiable shape (opaque provider state rides on the part that owns it) is
//! documented there and in `CLAUDE.md`.

pub mod fake;
pub mod genai_impl;
pub mod types;

#[cfg(test)]
mod live_smoke_test;

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use tokio_util::sync::CancellationToken;

pub use types::{AgentDelta, AgentLlmError, AgentMessage, ToolDeclaration};

/// The stream of increments a single [`AgentLlm::respond`] call produces. Always
/// `'static` (it owns its data), so it survives the borrow of the call arguments.
pub type AgentDeltaStream = BoxStream<'static, Result<AgentDelta, AgentLlmError>>;

/// A provider-agnostic streaming LLM the chat runtime drives.
///
/// The one method makes a single cold call: `messages` is the fully-assembled
/// prompt (stable prefix + elided history + the envelope on the latest user turn —
/// context assembly is the runtime's job, M5). The returned stream yields
/// [`AgentDelta`]s and, on `End`, the fully-assembled final [`AgentMessage`]
/// carrying any opaque provider state for persistence and replay.
///
/// Cancellation: fire `cancel` to drop the stream (the reqwest body closes, billing
/// stops — the genai impl reuses `crate::ai`'s stream-cancel model).
///
/// Written as a boxed-future return rather than `async fn` so the trait stays
/// object-safe (`Box<dyn AgentLlm>`) without pulling in `async-trait`.
pub trait AgentLlm: Send + Sync {
    fn respond<'a>(
        &'a self,
        system: &'a str,
        tools: &'a [ToolDeclaration],
        messages: &'a [AgentMessage],
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>>;
}
