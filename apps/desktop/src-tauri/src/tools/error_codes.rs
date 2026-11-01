//! JSON-RPC 2.0 standard error codes: the `code` a [`ToolError`](super::ToolError) carries, and
//! the codes the transport answers with. They live here so tools don't depend on the transport.

#[allow(dead_code, reason = "Standard JSON-RPC error code, kept for completeness")]
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
