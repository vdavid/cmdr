//! Single source of truth for MCP tools.
//!
//! Each tool is authored exactly once in the `mcp_tools!` table in `table.rs`, bundling its name,
//! description, JSON input schema, consumer exposure, access class, and handler. The macro
//! expands that one table into every consumer, so the facets can't drift:
//!
//! - [`get_all_tools`](super::get_all_tools) — the AI-client `tools/list` payload (entries whose `consumers` include
//!   [`Consumer::AiClient`]; non-generic; server + tests read it).
//! - [`agent_tool_view`](super::agent_tool_view) — the in-process agent's tool set (entries whose `consumers` include
//!   [`Consumer::Agent`]): the read (and, once authored, propose) families the chat agent dispatches.
//! - [`execute_tool`](super::execute_tool) — the `tools/call` dispatch (generic over `Runtime`), gated to the caller's
//!   consumer view: a name outside the caller's view is refused before dispatch.
//! - [`tool_consumers`] / [`tool_access`](super::tool_access) — the two new dimensions, read by the structural tests.
//! - [`ToolId`](super::ToolId) — the typed name of every entry, with `KNOWN`, the wire-name
//!   conversions, and serde. `agent::llm::types` re-exports it.
//! - [`SubjectSpec`] — what a call's tool line names (`agent/line/`).
//!
//! Adding a tool means adding one entry: you can't add it without supplying an id, a brief, a schema,
//! a subject, consumers, an access class, and a handler, and you can't add a handler the dispatch
//! doesn't know about. No enum, no `KNOWN`, and no `agent/line/` arm unless it says
//! [`SubjectSpec::Custom`]. The count and coverage tests are then cheap guards over a property that's
//! true by construction.
//!
//! **Two view dimensions, why both (agent decisions D49/D59):** one authored registry feeds two
//! consumers. `consumers` is the exposure axis — the agent's dispatch view physically excludes
//! every tool not tagged `[agent]`, so its write path is absent by construction, not policy.
//! `access` classifies what a tool can do independently of transport authentication. The
//! structural tests pin the agent view to exactly its authored `[agent]` entries AND require
//! every one to be [`Access::Read`], [`Access::ReadContents`], [`Access::Propose`],
//! [`Access::Memory`], or [`Access::Session`]. **The agent can propose; only the user can
//! approve** — no tool approves a proposal, the only thing it writes is its own memory folder,
//! and the only session it opens is to a place the user saved.
//!
//! Wire output must stay byte-identical: each schema is the exact `json!` block (hoisted into
//! [`schemas`](super::schemas) verbatim), and the tool order is the historical category concatenation. The
//! `tool_snapshot_tests` fixture pins it. Schema keys serialize in authored order: genai turns on
//! serde_json's `preserve_order` for the whole build, so reordering a `json!` block's keys changes the
//! bytes (and the snapshot). Anything a model reads as a key LIST sorts explicitly (`params.rs`).
//!
//! Split: this file is the mechanism (the two view dimensions, the params gate, and the
//! `mcp_tools!` macro); `table.rs` is the data the macro expands. Layering: the table depends on the
//! `actions` handlers and on `schemas`; `auth` depends on this module. Neither may depend on
//! `server` or `auth` (that would cycle).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::params;
use super::{ToolError, tool_consumers, tool_schema};

/// A tool definition for MCP: the registry's output type, one `tools/list` element (serde
/// `camelCase`, so `input_schema` serializes as `inputSchema`). Every generated view returns
/// `Vec<Tool>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Which AI consumer a tool is exposed to. One authored registry, per-consumer views (D49):
/// the MCP HTTP server dispatches the [`AiClient`](Consumer::AiClient) view, the in-process agent
/// runtime dispatches the [`Agent`](Consumer::Agent) view, and neither can reach the other's
/// tools ([`execute_tool`](super::execute_tool) refuses a name outside the caller's view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumer {
    /// External MCP clients over the HTTP transport (dev tooling, Claude Code, E2E).
    AiClient,
    /// The in-process Ask Cmdr agent runtime, which dispatches the agent view via
    /// [`execute_tool`](super::execute_tool) with this identity (`crate::agent::tool_gate`).
    Agent,
}

/// What a tool does, whoever calls it. The agent's gate (`crate::agent::tool_gate`) reads it through
/// [`tool_access`](super::tool_access) as a runtime backstop, and admits only [`Access::Read`],
/// [`Access::ReadContents`], [`Access::Propose`], [`Access::Memory`], and [`Access::Session`], so
/// "the agent can't act" holds even against a mis-tagged entry. It's registry metadata, not a field
/// on the emitted `Tool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Reads state and mutates nothing.
    Read,
    /// Reads inside file contents, or image-derived text. Exactly [`Access::Read`] for the agent.
    /// Authored by hand into `EXPECTED_READ_CONTENTS_TOOL_NAMES`, because it's the class that decides
    /// what an outside client can read through Cmdr's Full Disk Access.
    ReadContents,
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
    /// Opens a session to a place the user saved, with credentials the user stored. Adds or
    /// edits no file, server, account, credential, pin, or setting; the saved place's own
    /// connection bookkeeping updates the way a user's connect updates it.
    ///
    /// ⚠️ **A deliberate widening, like [`Access::Memory`]**, authored by hand into
    /// `EXPECTED_SESSION_TOOL_NAMES` for the same reason: no structural check can prove a handler
    /// dials only a saved place. The agent's gate also admits it only on a turn the user started
    /// (`crate::agent::tool_gate`), since a background connect can wake a sleeping NAS for a
    /// digest nobody asked for. Agent invariant 7 (`agent/DETAILS.md`) is the promise.
    Session,
    /// Changes what's on screen, never data: navigation, cursor, selection, tabs, view, and queue
    /// steering. Refused by the agent's gate for now.
    AppState,
    /// Opens a dialog or editor a person must confirm. Refused by the agent's gate for now.
    /// Hand-allowlisted (`EXPECTED_INITIATE_TOOL_NAMES`).
    Initiate,
    /// Writes with no confirmation, the ONE written exception to "nothing writes without a person":
    /// `mkdir` and `mkfile` with a name, and `tag`, which has no undo. Refused by the agent's gate for
    /// now. Hand-allowlisted (`EXPECTED_LIGHT_WRITE_TOOL_NAMES`).
    LightWrite,
    /// Runs only after a person confirms an Ask. Refused by the agent's gate for now.
    /// Hand-allowlisted (`EXPECTED_SENSITIVE_TOOL_NAMES`).
    Sensitive,
    /// Never the agent, and an outside client only in direct mode
    /// ([`policy::direct_mode`](super::policy::direct_mode)). The string is the one-line reason, so
    /// the table carries it.
    DevOnly(&'static str),
}

/// What a call's tool line names, read off its arguments by `agent/line/`. The strings are argument
/// keys. A tool here flows through the line's one generic arm: its subject comes from this, its
/// outcome reads as an answer unless its result carries a typed refusal, and its preview has no rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectSpec {
    /// Nothing to name: the tool's own words say it all.
    None,
    /// A place, at this argument.
    Path(&'static str),
    /// A volume id, at this argument.
    Volume(&'static str),
    /// An operation id, at this argument.
    Operation(&'static str),
    /// The pane the `pane` argument names; an absent one means the focused pane.
    Pane,
    /// The setting the `id` argument names.
    Setting,
    /// A rich tool: `agent/line/` has hand arms for its subject, outcome, and preview.
    Custom,
}

/// Whether `name` is listable/dispatchable by `consumer` — its authored `consumers` set includes
/// it. The choke point [`execute_tool`](super::execute_tool) consults before dispatch, and the invariant the
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
/// `tool_consumers`, `tool_access`).
///
/// Entry form:
/// `"name" => { id: <Variant>, brief, desc, schema, subject: .., consumers: &[..], access: .., run: <shape> <handler-path> }`.
///
/// `ask` names a [`Sensitive`](Access::Sensitive) tool's plan: an `async fn(app, ctx, params) -> AskPlan` the
/// dispatch runs ahead of the handler, and again after a yes (`ask.rs`).
///
/// `id` becomes the entry's [`ToolId`](super::ToolId) variant. `brief` is the one line (at most 100
/// characters, test-capped) a model reads for a tool it hasn't loaded in full. `desc` is what it reads
/// once it has.
///
/// The `run` shape tag selects how the generated dispatch calls the handler, sidestepping
/// `macro_rules!` hygiene (call-site idents from the table can't bind to the def-site fn params,
/// so the shape helper passes the macro's own `app`/`params`/`name` positionally):
///
/// - `app_params` — `handler(app, params).await` (async; most tools).
/// - `app_only` — `handler(app).await` (async; no params: `toggle_hidden`, `mkdir`, …).
/// - `params_only` — `handler(params).await` (async; no `app`: `search`, `favorites`, …).
/// - `sync_app` — `handler(app)` (sync; `quit`, `switch_pane`, `swap_panes`).
/// - `sync_app_params` — `handler(app, params)` (sync; `remove_manual_server`).
/// - `nav` / `nav_params` — `handler(app, name)` / `handler(app, name, params)` for the nav
///   family, which routes several tools through one handler by passing the tool name as a literal.
/// - `ctx_params` — `handler(app, ctx, params).await`, for a tool that needs to know who's calling:
///   the [`ToolCtx`](super::policy::ToolCtx) with its caller, cancel token, and thread.
///
/// Sync arms deliberately don't `.await` (the handlers are sync), matching the hand-written
/// dispatch this replaced.
macro_rules! mcp_tools {
    ( $( $name:literal => {
        id: $id:ident,
        brief: $brief:expr,
        desc: $desc:expr,
        schema: $schema:expr,
        subject: $subject:expr,
        consumers: $consumers:expr,
        access: $access:expr,
        $( ask: $ask:path, )?
        run: $shape:tt $path:path
    } ),* $(,)? ) => {
        /// A tool's typed name, one variant per table entry.
        ///
        /// [`ToolId::Unrecognized`] holds any name outside the table: a provider returns each tool
        /// call's name as a raw string, and a hallucinated or mistyped one has to stay representable
        /// for the transparent UI and the typed refusal. It carries no dispatch path. A name in the
        /// table parses as known whether or not the agent may run it: that's the gate's call
        /// (`crate::agent::tool_gate`), never this parse's.
        ///
        /// Serializes transparently as its wire name (a bare string), so the DB token, the genai
        /// `fn_name`, and the IPC form are one identical value. A stored name reloads through the
        /// same parse, so a thread that once asked for a tool keeps that tool's line.
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum ToolId {
            $( $id, )*
            /// A name outside the table. Always refused by dispatch.
            Unrecognized(String),
        }

        impl ToolId {
            /// Every table entry, in wire order. Excludes [`ToolId::Unrecognized`] by design: it's
            /// the refusal case, never an entry.
            pub const KNOWN: [ToolId; <[&str]>::len(&[$( $name ),*])] = [ $( ToolId::$id ),* ];

            /// The wire name for this tool: the genai `fn_name`, the DB token, and the IPC
            /// string, all one value.
            pub fn as_wire_name(&self) -> &str {
                match self {
                    $( ToolId::$id => $name, )*
                    ToolId::Unrecognized(name) => name.as_str(),
                }
            }

            /// Resolves a raw provider-supplied tool name to a typed [`ToolId`]. Total: a name
            /// outside the table resolves to [`ToolId::Unrecognized`] rather than failing.
            pub fn from_wire_name(name: &str) -> Self {
                match name {
                    $( $name => ToolId::$id, )*
                    other => ToolId::Unrecognized(other.to_string()),
                }
            }

            /// True for a table entry. Says nothing about who may run it.
            pub fn is_known(&self) -> bool {
                !matches!(self, ToolId::Unrecognized(_))
            }

            /// True when the provider named no function at all. A nameless call is never a real
            /// invocation: nothing can dispatch it, and providers that validate the field reject
            /// it on replay, so both directions of `genai_impl` drop it rather than carry it.
            pub fn is_nameless(&self) -> bool {
                self.as_wire_name().trim().is_empty()
            }

            /// What this tool's line names, or `None` for a name outside the table.
            pub fn subject_spec(&self) -> Option<SubjectSpec> {
                match self {
                    $( ToolId::$id => Some($subject), )*
                    ToolId::Unrecognized(_) => None,
                }
            }

            /// This tool's one-line summary, or `None` for a name outside the table.
            pub fn brief(&self) -> Option<&'static str> {
                match self {
                    $( ToolId::$id => Some($brief), )*
                    ToolId::Unrecognized(_) => None,
                }
            }
        }

        impl serde::Serialize for ToolId {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_wire_name())
            }
        }

        impl<'de> serde::Deserialize<'de> for ToolId {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let name = <String as serde::Deserialize>::deserialize(deserializer)?;
                Ok(ToolId::from_wire_name(&name))
            }
        }

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
        /// runtime (`crate::agent::tool_gate`) turns these into `ToolDeclaration`s and dispatches
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

        /// The consumer exposure for a tool, or `None` for an unknown name.
        pub fn tool_consumers(name: &str) -> Option<&'static [Consumer]> {
            match name {
                $( $name => Some($consumers), )*
                _ => None,
            }
        }

        /// The access class for a tool, or `None` for an unknown name. Read by the structural
        /// tests and by the agent dispatch's runtime read-only backstop (`crate::agent::tool_gate`).
        pub fn tool_access(name: &str) -> Option<Access> {
            match name {
                $( $name => Some($access), )*
                _ => None,
            }
        }

        /// Whether this tool names an `ask` plan. A test pins it to `Sensitive`, both ways.
        #[cfg_attr(not(test), expect(dead_code, reason = "only the test that pins the table reads it"))]
        pub fn tool_has_ask_plan(name: &str) -> bool {
            match name {
                $( $name => mcp_tools!(@has_plan $( $ask )?), )*
                _ => false,
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

        /// The `tools/call` dispatch, gated to the caller's view. A name outside that view (an
        /// agent-only name over MCP, an `ai_client`-only name through the agent runtime, or an
        /// unknown name) is refused before dispatch with the same `INVALID_PARAMS` "Unknown tool"
        /// error — the refusal is on the typed [`Consumer`] set, not a string. Generic over
        /// `Runtime`.
        pub async fn execute_tool<R: tauri::Runtime>(
            app: &tauri::AppHandle<R>,
            ctx: &ToolCtx,
            name: &str,
            params: &Value,
        ) -> ToolResult {
            if !tool_available_to(name, ctx.caller.consumer()) {
                return Err(ToolError::invalid_params(format!("Unknown tool: {name}")));
            }
            if let Some(refusal) = super::policy::dev_only_refusal(ctx.caller, name, tool_access(name)) {
                return Err(refusal);
            }
            validate_params(name, params)?;
            // Ahead of the handler and after the schema, so a malformed call never reaches a person.
            let asked = match name {
                $( $name => mcp_tools!(@ask [$( $ask )?] app, ctx, params), )*
                _ => None,
            };
            let confirmed;
            let ctx = if asked.is_some() {
                confirmed = ToolCtx { asked, ..ctx.clone() };
                &confirmed
            } else {
                ctx
            };
            match name {
                $( $name => mcp_tools!(@call $shape $path, $name, app, ctx, params), )*
                _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
            }
        }
    };

    (@has_plan) => { false };
    (@has_plan $p:path) => { true };
    (@ask [] $app:ident, $ctx:ident, $params:ident) => { None };
    (@ask [$p:path] $app:ident, $ctx:ident, $params:ident) => {
        super::ask::gate($app, $ctx, || $p($app, $ctx, $params)).await?
    };

    // Handler-shape arms: each evaluates to a `ToolResult`. Sync handlers deliberately
    // don't `.await`. `app`/`ctx`/`params`/`name` are passed positionally from the generated
    // dispatch (the macro's own body context) so `macro_rules!` hygiene never bites.
    (@call app_params      $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app, $params).await };
    (@call app_only        $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app).await };
    (@call params_only     $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($params).await };
    (@call sync_app        $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app) };
    (@call sync_app_params $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app, $params) };
    (@call nav             $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app, $name).await };
    (@call nav_params      $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app, $name, $params).await };
    (@call ctx_params      $p:path, $name:literal, $app:ident, $ctx:ident, $params:ident) => { $p($app, $ctx, $params).await };
}
