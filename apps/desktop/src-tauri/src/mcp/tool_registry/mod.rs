//! Single source of truth for MCP tools.
//!
//! Each tool is authored exactly once in the `mcp_tools!` table in `table.rs`, bundling its name,
//! description, JSON input schema, bearer-token gate, consumer exposure, access class, and
//! handler. The macro expands that one table into every consumer, so the facets can't drift:
//!
//! - [`get_all_tools`] — the AI-client `tools/list` payload (entries whose `consumers` include
//!   [`Consumer::AiClient`]; non-generic; server + tests read it).
//! - [`agent_tool_view`] — the in-process agent's tool set (entries whose `consumers` include
//!   [`Consumer::Agent`]): the read (and, once authored, propose) families the chat agent dispatches.
//! - [`execute_tool`] — the `tools/call` dispatch (generic over `Runtime`), gated to the caller's
//!   consumer view: a name outside the caller's view is refused before dispatch.
//! - [`tool_gate`] + [`TokenGate`] — the auth classification `auth.rs` reads.
//! - [`tool_consumers`] / [`tool_access`] — the two new dimensions, read by the structural tests.
//!
//! Adding a tool means adding one entry: you can't add it without supplying a schema, a gate,
//! consumers, an access class, and a handler, and you can't add a handler the dispatch doesn't
//! know about. The count and coverage tests are then cheap guards over a property that's true by
//! construction.
//!
//! **Two view dimensions, why both (agent-spec D49/D59):** one authored registry feeds two
//! consumers. `consumers` is the exposure axis — the agent's dispatch view physically excludes
//! every tool not tagged `[agent]`, so its write path is absent by construction, not policy.
//! `access` is a stronger guarantee than the gate can give: [`TokenGate::Open`] covers
//! destructive-but-prompting ops (`copy`/`move`/`delete` with `autoConfirm` absent carry
//! `IfAutoConfirm`, effectively open), so a gate-based agent filter would let a destructive tool
//! into the agent's view. The structural tests pin the agent view to exactly its authored
//! `[agent]` entries AND require every one to be [`Access::Read`], [`Access::Propose`], or
//! [`Access::Memory`], never [`Access::Write`]. **The agent can propose; only the user can
//! approve** — no tool approves a proposal, and the only thing it writes is its own memory
//! folder.
//!
//! Wire output must stay byte-identical: each schema is the exact `json!` block (hoisted into
//! [`schemas`] verbatim), and the tool order is the historical category concatenation. The
//! `tool_snapshot_tests` fixture pins it. Schema keys serialize alphabetically (serde_json `Map`
//! is a `BTreeMap`; `preserve_order` is off), so authored key order never affects the bytes.
//!
//! Split: this file is the mechanism (the two view dimensions, the params gate, and the
//! `mcp_tools!` macro); `table.rs` is the data the macro expands. Layering: the table depends on the
//! `executor` handlers and on `schemas`; `auth` depends on this module. Neither may depend on
//! `server` or `auth` (that would cycle).

mod gate;
pub mod params;
mod schemas;

pub use gate::TokenGate;

use serde_json::Value;

use super::executor::ToolError;

/// Which AI consumer a tool is exposed to. One authored registry, per-consumer views (D49):
/// the MCP HTTP server dispatches the [`AiClient`](Consumer::AiClient) view, the in-process agent
/// runtime dispatches the [`Agent`](Consumer::Agent) view, and neither can reach the other's
/// tools ([`execute_tool`] refuses a name outside the caller's view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumer {
    /// External MCP clients over the HTTP transport (dev tooling, Claude Code, E2E).
    AiClient,
    /// The in-process Ask Cmdr agent runtime, which dispatches the agent view via
    /// [`execute_tool`] with this identity (`crate::agent::tools`).
    Agent,
}

/// Whether a tool reads, asks, remembers, or mutates. The agent view admits `Read`, `Propose`,
/// and `Memory`, and must contain zero `Write` tools — this is the guarantee [`TokenGate`] alone
/// can't give (see the module docs).
///
/// The agent dispatch (`crate::agent::tools`) reads [`tool_access`] as a runtime backstop: it
/// refuses to execute any tool classified `Write`, so "the agent can't act" holds even against a
/// mis-tagged entry. It's registry metadata, not a field on the emitted `Tool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Reads state and mutates nothing.
    Read,
    /// Stages a proposal and opens a review surface for the user. Mutates nothing: no filesystem
    /// write, no silent config change. **The agent can propose; only the user can approve** —
    /// approval originates in the frontend as a user action, and there is no tool that approves a
    /// proposal. A `Propose` tool is authored by hand into the test allowlist
    /// (`EXPECTED_PROPOSE_TOOL_NAMES`), because no structural check can prove a handler doesn't
    /// mutate.
    Propose,
    /// Writes inside the agent's OWN memory folder (`<data-dir>/ai/memory/`), and nowhere else.
    ///
    /// ⚠️ **A deliberate widening of the app's central agent-safety invariant.** "The agent
    /// never changes anything" becomes "the agent writes only into its memory folder" — still
    /// structural, but it is a different promise, so it is authored by hand into
    /// `EXPECTED_MEMORY_TOOL_NAMES` for the same reason [`Access::Propose`] is: no structural
    /// check can prove a handler stays in the jail, so a human has to put the name there having
    /// read it. ❌ It must never be acquired as a side effect of editing a registry line.
    ///
    /// The containment itself is `agent::memory`'s jail, not this tag.
    Memory,
    /// Mutates the filesystem OR app state (nav, cursor, selection, tabs, dialogs, settings,
    /// connect/eject, file ops, rollback-cancel); when in doubt a tool is `Write`. Never reachable
    /// from the agent view.
    Write,
}

/// Whether `name` is listable/dispatchable by `consumer` — its authored `consumers` set includes
/// it. The choke point [`execute_tool`] consults before dispatch, and the invariant the
/// structural tests pin ("no transport dispatches a name outside its consumer view"). The
/// decision is on the typed [`Consumer`] set, never a string.
pub fn tool_available_to(name: &str, consumer: Consumer) -> bool {
    tool_consumers(name).is_some_and(|cs| cs.contains(&consumer))
}

/// Refuse a params object its tool's own declared schema doesn't allow, before a handler
/// reads a single field off it.
///
/// Handlers pluck fields off the raw value, so without this an undeclared property is
/// swallowed in silence and the call runs as something the caller never asked for. What the
/// check does and (just as deliberately) doesn't look at: [`params`]. An unknown name passes
/// through untouched — the dispatch paths refuse it on their own, with their own wording.
pub fn validate_params(name: &str, params: &Value) -> Result<(), ToolError> {
    match tool_schema(name) {
        Some(schema) => params::gate(name, &schema, params),
        None => Ok(()),
    }
}

/// Declarative tool table → the consumers (`get_all_tools`, `agent_tool_view`, `execute_tool`,
/// `tool_gate`, `tool_consumers`, `tool_access`).
///
/// Entry form:
/// `"name" => { desc, schema, gate, consumers: &[..], access: .., run: <shape> <handler-path> }`.
///
/// The `run` shape tag selects how the generated dispatch calls the handler, sidestepping
/// `macro_rules!` hygiene (call-site idents from the table can't bind to the def-site fn params,
/// so the shape helper passes the macro's own `app`/`params`/`name` positionally):
///
/// - `app_params` — `handler(app, params).await` (async; most tools).
/// - `app_only` — `handler(app).await` (async; no params: `toggle_hidden`, `mkdir`, …).
/// - `params_only` — `handler(params).await` (async; no `app`: `search`, `ai_search`).
/// - `sync_app` — `handler(app)` (sync; `quit`, `switch_pane`, `swap_panes`).
/// - `sync_app_params` — `handler(app, params)` (sync; `remove_manual_server`).
/// - `nav` / `nav_params` — `handler(app, name)` / `handler(app, name, params)` for the nav
///   family, which routes several tools through one handler by passing the tool name as a literal.
///
/// Sync arms deliberately don't `.await` (the handlers are sync), matching the hand-written
/// dispatch this replaced.
macro_rules! mcp_tools {
    ( $( $name:literal => {
        desc: $desc:expr,
        schema: $schema:expr,
        gate: $gate:expr,
        consumers: $consumers:expr,
        access: $access:expr,
        run: $shape:tt $path:path
    } ),* $(,)? ) => {
        /// The AI-client `tools/list` payload: every `[ai_client]` tool in wire order. Agent-only
        /// entries are filtered out, so this stays byte-identical to the pre-dimension output for
        /// the tools it already contained.
        pub fn get_all_tools() -> Vec<Tool> {
            let mut tools = Vec::new();
            $(
                if $consumers.contains(&Consumer::AiClient) {
                    tools.push(Tool { name: $name.into(), description: $desc.into(), input_schema: $schema });
                }
            )*
            tools
        }

        /// The in-process agent's tool set: every `[agent]` tool in table order. The agent
        /// runtime (`crate::agent::tools`) turns these into `ToolDeclaration`s and dispatches
        /// them; the structural set-equality + all-`Read` tests pin the set.
        pub fn agent_tool_view() -> Vec<Tool> {
            let mut tools = Vec::new();
            $(
                if $consumers.contains(&Consumer::Agent) {
                    tools.push(Tool { name: $name.into(), description: $desc.into(), input_schema: $schema });
                }
            )*
            tools
        }

        /// The bearer-token classification for a tool, or `None` for an unknown name. The
        /// single source `auth::tool_call_requires_token` reads.
        pub fn tool_gate(name: &str) -> Option<TokenGate> {
            match name {
                $( $name => Some($gate), )*
                _ => None,
            }
        }

        /// The consumer exposure for a tool, or `None` for an unknown name.
        pub fn tool_consumers(name: &str) -> Option<&'static [Consumer]> {
            match name {
                $( $name => Some($consumers), )*
                _ => None,
            }
        }

        /// The access class for a tool, or `None` for an unknown name. Read by the structural
        /// tests and by the agent dispatch's runtime read-only backstop (`crate::agent::tools`).
        pub fn tool_access(name: &str) -> Option<Access> {
            match name {
                $( $name => Some($access), )*
                _ => None,
            }
        }

        /// The input schema a tool declares, or `None` for an unknown name. The same value
        /// both views publish, so [`validate_params`] gates a call against exactly what the
        /// caller was told the tool takes.
        pub fn tool_schema(name: &str) -> Option<Value> {
            match name {
                $( $name => Some($schema), )*
                _ => None,
            }
        }

        /// The `tools/call` dispatch, gated to `consumer`'s view. A name outside that view (an
        /// agent-only name over MCP, an `ai_client`-only name through the agent runtime, or an
        /// unknown name) is refused before dispatch with the same `INVALID_PARAMS` "Unknown tool"
        /// error — the refusal is on the typed [`Consumer`] set, not a string. Generic over
        /// `Runtime`.
        pub async fn execute_tool<R: tauri::Runtime>(
            app: &tauri::AppHandle<R>,
            consumer: Consumer,
            name: &str,
            params: &Value,
        ) -> ToolResult {
            if !tool_available_to(name, consumer) {
                return Err(ToolError::invalid_params(format!("Unknown tool: {name}")));
            }
            validate_params(name, params)?;
            match name {
                $( $name => mcp_tools!(@call $shape $path, $name, app, params), )*
                _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
            }
        }
    };

    // Handler-shape arms: each evaluates to a `ToolResult`. Sync handlers deliberately
    // don't `.await`. `app`/`params`/`name` are passed positionally from the generated
    // dispatch (the macro's own body context) so `macro_rules!` hygiene never bites.
    (@call app_params      $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $params).await };
    (@call app_only        $p:path, $name:literal, $app:ident, $params:ident) => { $p($app).await };
    (@call params_only     $p:path, $name:literal, $app:ident, $params:ident) => { $p($params).await };
    (@call sync_app        $p:path, $name:literal, $app:ident, $params:ident) => { $p($app) };
    (@call sync_app_params $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $params) };
    (@call nav             $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $name).await };
    (@call nav_params      $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $name, $params).await };
}

mod table;

pub use table::{agent_tool_view, execute_tool, get_all_tools, tool_access, tool_consumers, tool_gate, tool_schema};
