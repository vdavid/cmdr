# Ask Cmdr: implementation plan

Status: plan ready, execution pending. 2026-07-12. Owner: David.

Contract: `ask-cmdr-spec.md` (behavior + settled decisions) and its parent `later/agent-spec.md` (principles, decision
log D1–D60). The genai capability spike (spec §3 step 0, milestone M0) is complete: `ask-cmdr-genai-spike.md`. This plan
owns construction: module layout, DDL, the `AgentLlm` trait shape, the IPC surface, milestones, and the resolutions to
spec §7. It does not reopen the spec's decisions. Single-source rule: where the spec owns a behavior, this plan points
at it rather than restating it.

## 1. Intent

Ask Cmdr is a read-only chat rail in the main window where the user talks to a BYO-key LLM that can see what Cmdr
already knows — the drive index (sizes, listings, recency), the importance subsystem, the operation log, and live app
state — and answers questions about their files. It is the first LLM-powered slice of the full agent and ships ahead of
the agent's proactive machinery (wake loop, proposals, notifications) because the wow ("talk to your file manager")
reaches beta users cheaply while the risky proactivity bakes. Everything it needs already exists and is deterministic;
the LLM only adds judgment and language on top of typed tool data. It is read-only **by construction** — no write tool
exists in its dispatch view — which is also the privacy line: only names, paths, and metadata ever reach the provider,
never file contents. This slice also forces the interaction-surface design (the rail, the session history, the focus
model) that the agent's later surfaces inherit.

## 2. Settled-decisions digest

The spec and agent-spec own these; this plan constructs to them. Pointers, not restatements:

- Read-only by construction; privacy line (names/paths/metadata only): spec §2.1, agent-spec D26.
- Deterministic bottom, LLM top; no LLM in any hot path: spec §2.2.
- Continuity through DB state; every call is cold and self-assembled: spec §2.3, §5.
- Honesty about coverage caveats is load-bearing: spec §2.4.
- Radical transparency (every tool call visible; threads are plain DB rows): spec §2.5.
- Typed everything across IPC/DB (no matched strings): spec §2.6, `.claude/rules/no-string-matching.md`.
- `AgentLlm` trait with opaque per-message provider state over the shipped `genai`: agent-spec D41, spec §3.
- Two model slots (bulk vs. interactive); v1 uses the interactive slot: agent-spec D43.
- Registry is agent-first, consumed in-process; MCP is the second consumer: agent-spec D49.
- Registry is one authored source with per-consumer views (extend, don't fork): agent-spec D49. Consumer gating is
  structural (agent view excludes write tools, pinned by a set-equality test): agent-spec D59 — **strengthened below**
  (§11 open-question 2 and M3) with an `access: Read | Write` dimension, because `TokenGate::Open` is not read-only.
- `main.db` is the durable peer DB beside `operation-log.db`; agent state lives there, settings stays preferences:
  agent-spec D1/D3/D56, spec §3.
- Tier-1 providers to certify: Anthropic, OpenAI, Gemini, local: agent-spec D40; local allowed with honest labeling and
  graceful degradation: D53.
- The rail (over window/palette/drawer), with its four-places menu wiring, focus model, sessions, search: spec §3 (UI).

## 3. Milestone 0 outcome (the genai spike — done)

Full report: `ask-cmdr-genai-spike.md`. Load-bearing outcomes this plan encodes:

- The tool-loop plumbing is solid on all four adapters (multi-step loops, streaming-with-tools, stop-reason/usage
  normalization). OpenAI chat-completions is live-verified end-to-end against Cmdr's own local llama-server; the cloud
  adapters are source-verified (keys were dead).
- **Reasoning-state round-trip splits hard**: Gemini round-trips `thoughtSignature` correctly; the **Anthropic** adapter
  drops the thinking signature on parse and never re-serializes thinking on replay (upstream issue #213), and the
  **OpenAI Responses** adapter never re-serializes reasoning items (`store=false` is the right privacy posture). So a
  reasoning-on multi-step loop breaks on Anthropic and degrades/400s on OpenAI-Responses.
- Consequences baked into this plan: (a) `AgentLlm` carries messages as **typed parts** — text, tool-call with attached
  opaque provider state, reasoning blob — never flattened content+reasoning strings (M1). (b) v1 ships with reasoning
  **off/minimal on the Anthropic and OpenAI-Responses paths**, with graceful degradation and honest UI (M1, M8). (c) A
  scoped local `[patch.crates.io]` genai patch (or upstream PR) for Anthropic thinking capture+replay is a named
  follow-up, needed before Tier-1-certifying Anthropic with thinking on, since newer Claude models default thinking on
  (§13 follow-ups). (d) Five provider-side live checks are pending API keys — the pre-ship certification step in M8 (§11
  open-question 7).

## 4. Module layout

### Backend — `apps/desktop/src-tauri/src/agent/`

New subsystem, named after the UI surface where it makes sense (`name-internals-after-the-UI`). The persistent entity is
"the agent" (agent-spec D44); the user-facing slice is "Ask Cmdr".

- `agent/mod.rs` — subsystem entry: `start(app)` (open `main.db`, register runtime state), re-exports. Modeled on
  `operation_log::start`.
- `agent/llm/` — the `AgentLlm` trait, its genai-backed impl over `crate::ai::AiBackend`, the deterministic fake, and
  the typed message-part model (M1). `agent/llm/genai_impl.rs`, `agent/llm/fake.rs`, `agent/llm/types.rs`,
  `agent/llm/error.rs`.
- `agent/store/` — the `main.db` durable store: migration ladder, `token_enum!` types, connection/pragmas, conversation
  - message + FTS + cost-meter queries (M2). Mirrors `operation_log/store/` structure: `store/mod.rs`,
    `store/migrations.rs`, `store/connection.rs`, `store/query.rs`, `agent/types.rs`.
- `agent/tools/` — the in-process read-only toolset: the agent's registry view + the concrete tool implementations that
  call the underlying cores (M4). `tools/mod.rs`, `tools/view.rs` (the gated dispatch view), `tools/read/*.rs` (one file
  per tool family: state, listing, importance, operations, volumes).
- `agent/chat/` — the chat runtime + the pure context-assembly core (M5). `chat/runtime.rs` (single-flight, budgets,
  cancellation, typed errors), `chat/context.rs` (pure: prefix, elision, envelope, budget — the TDD core),
  `chat/system_prompt.rs`.
- Each new area gets its sibling `CLAUDE.md` (must-knows only) + `DETAILS.md` (depth), per `AGENTS.md` § Docs and
  `claude-md-details-sibling`. IPC commands live in `src-tauri/src/commands/agent.rs` (thin pass-throughs), registered
  in `src-tauri/src/ipc.rs` and `src-tauri/src/ipc_collectors.rs` (both required, per the operation-log precedent).

### Registry refactor — `apps/desktop/src-tauri/src/mcp/tool_registry.rs` (M3)

Split into a directory module (see M3) and grow a `consumers` / `access` dimension. Stays under `mcp/` because it is
still one authored source for every AI-callable tool (agent-spec D49: extend, don't fork).

### Frontend — `apps/desktop/src/lib/ask-cmdr/`

Named after the surface. Modeled on `src/lib/operation-log/` (the sessions UI template).

- `AskCmdrRail.svelte` — the toggleable right rail: header (title + ALPHA badge), thread view, composer, streaming
  render, stop button, tool-call lines. Hosted by `src/routes/(main)/+page.svelte` beside `DualPaneExplorer.svelte`.
- `ask-cmdr-trigger.svelte.ts` — reactive open/close + focus + active-conversation state, the paging state machine
  (offset = entries.length, page const), modeled on `operation-log-trigger.svelte.ts`.
- `AskCmdrSessions.svelte` — thread list (recent first), new/rename/archive, cross-thread search box.
- `ask-cmdr-labels.ts` — typed enum → localized string maps (role, stop reason, error kind, tool id), never a
  backend-rendered string.
- `MessageBlocks.svelte`, `ToolCallLine.svelte`, `Composer.svelte`, attachment chips.
- Wrappers in `src/lib/tauri-commands/ask-cmdr.ts` (barrel-exported; delegate to generated `commands.*`, never a raw
  bindings import — `cmdr/no-raw-bindings-import`).
- Settings section `src/lib/settings/sections/AskCmdrSection.svelte`.
- i18n catalog `src/lib/intl/messages/en/askCmdr.json` (`askCmdr.*` keys).

## 5. `main.db` v1 DDL

Discipline copied from `operation_log/store/`: a `meta` anchor table outside the ladder, a forward-only migration ladder
(one transaction per step, refuse downgrade, delete-and-recreate only on the typed `NotADatabase`/`DatabaseCorrupt`
error code), WAL + incremental auto_vacuum pragmas, snake_case DB tokens via `token_enum!`, precomputed values in Rust
(no custom collation, `sqlite3`-inspectable). File lives at `resolved_app_data_dir(app).join("main.db")` — peer to
`operation-log.db` (agent-spec D1/D3).

**FTS5 is net-new.** The operation log does not use FTS5 and the `rusqlite` dependency does not enable the `fts5`
feature today. M2 must add `fts5` to the `rusqlite` features in `src-tauri/Cargo.toml` (the `bundled` SQLite already
contains the FTS5 module; it's a feature-flag flip, not a new crate — still run `cargo deny check` per the dependency
rule). There is no in-tree trigger-based FTS sync pattern to copy (the op log keeps a folded column in sync in Rust, not
via SQL triggers), so the external-content FTS5 table + its sync triggers below are authored fresh.

```sql
-- meta: bootstrap anchor, outside the ladder; holds schema_version. (copied verbatim discipline)
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;

-- v1 migration body:
CREATE TABLE conversations (
    id           INTEGER PRIMARY KEY,
    title        TEXT NOT NULL,               -- generated from first message; user-renamable
    created_at   INTEGER NOT NULL,            -- unix secs
    updated_at   INTEGER NOT NULL,
    archived     INTEGER NOT NULL DEFAULT 0,  -- 0/1 flag + filter, no delete in v1
    origin       TEXT                          -- nullable snake_case token; NULL = user-started.
);                                             -- cheap insurance: a future notification-spawned thread
                                               -- is a column value, not a migration (spec §3).
CREATE INDEX conversations_updated ON conversations (archived, updated_at DESC, id DESC);

CREATE TABLE messages (
    id                INTEGER PRIMARY KEY,
    conversation_id   INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    seq               INTEGER NOT NULL,        -- per-conversation ordinal
    role              TEXT NOT NULL,           -- token_enum: system|user|assistant|tool
    content_blocks    TEXT NOT NULL,           -- JSON: ordered typed parts (text, tool_call, tool_result,
                                               -- reasoning{provider,blob}); the opaque provider state rides here
                                               -- and NEVER crosses to the frontend (backend-only column).
    text_for_search   TEXT NOT NULL DEFAULT '',-- plain user+assistant text extracted at insert, for FTS
    prompt_tokens     INTEGER,                 -- nullable; assistant turns only
    completion_tokens INTEGER,
    created_at        INTEGER NOT NULL
);
CREATE UNIQUE INDEX messages_conv_seq ON messages (conversation_id, seq);

-- external-content FTS5 over message text (net-new; needs the fts5 rusqlite feature)
CREATE VIRTUAL TABLE messages_fts USING fts5 (
    text_for_search,
    content='messages',
    content_rowid='id'
);
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, text_for_search) VALUES (new.id, new.text_for_search);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text_for_search) VALUES('delete', old.id, old.text_for_search);
END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text_for_search) VALUES('delete', old.id, old.text_for_search);
    INSERT INTO messages_fts(rowid, text_for_search) VALUES (new.id, new.text_for_search);
END;

-- per-day, per-thread, per-model token + cost rollup (spec §3 cost_meter, §Cost visibility)
CREATE TABLE cost_meter (
    day               TEXT NOT NULL,           -- YYYY-MM-DD, local day
    conversation_id   INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL,           -- token_enum ProviderTag
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    cost_micros       INTEGER NOT NULL DEFAULT 0,  -- integer micro-USD; est. from a per-model price table, honest "est."
    priced            INTEGER NOT NULL DEFAULT 1,  -- 0 when the model wasn't in the price table at metering time.
                                               -- Miss-path (spec §2.4 honesty): local model ⇒ cost_micros 0, shown
                                               -- "free/on-device"; an unpriced cloud model ⇒ priced 0, so the rollup
                                               -- shows token counts with cost "unknown", NEVER a silent $0. Kept NOT
                                               -- NULL so the ON CONFLICT accumulation arithmetic never hits NULL.
    PRIMARY KEY (day, conversation_id, provider, model)
);
-- conversation_id is NOT NULL: SQLite treats NULLs as distinct in a PK/UNIQUE, so a nullable column inside
-- the PK breaks ON CONFLICT DO UPDATE (every write inserts a duplicate instead of upserting). One row per real
-- thread; the per-day cross-thread rollup is computed at query time (SUM ... GROUP BY day) in ask_cmdr_cost_summary.
```

No auto-retention in v1 (transcripts are small; spec §3). The retention _scaffold_ (`operation_log/retention.rs` +
`PruneRequest`) is the template when real sizes exist — a follow-up, not built now.

## 6. The `AgentLlm` trait sketch

Rust signatures (the implementer refines). The non-negotiable shape from the spike: an assistant turn is an ordered list
of **typed parts**, and opaque reasoning state is **provider-tagged and rides on the part that owns it**, never
flattened to `content: String + reasoning: String` (that lossy shape is exactly what breaks on step 3 — spike Gaps A/B).
The frontend never receives the reasoning blob.

```rust
// agent/llm/types.rs
pub enum AgentRole { System, User, Assistant, Tool }

pub enum AgentPart {
    Text(String),
    ToolCall(AgentToolCall),
    ToolResult(AgentToolResult),
    Reasoning(ReasoningState),           // opaque; persisted + replayed untouched
}

pub struct ReasoningState {
    pub provider: ProviderTag,           // typed enum, never a matched string
    pub blob: serde_json::Value,         // shape owned by the provider adapter; opaque to everything else
}

pub struct AgentToolCall {
    pub call_id: String,
    pub tool: ToolId,                    // typed enum over the read-only toolset
    pub arguments: serde_json::Value,
    pub reasoning: Option<ReasoningState>,   // e.g. Gemini per-functionCall thoughtSignature
}
pub struct AgentToolResult { pub call_id: String, pub content: serde_json::Value, pub elided: bool }

pub struct AgentMessage {
    pub role: AgentRole,
    pub parts: Vec<AgentPart>,
    pub at: i64,                         // unix secs; every message carries its timestamp (spec §5)
}

pub struct ToolDeclaration { pub name: ToolId, pub description: String, pub schema: serde_json::Value } // never strict:true (Gap D)

pub enum AgentStopReason { Completed, ToolCall, MaxTokens, ContentFilter, StopSequence, Other(String) }
pub struct AgentUsage { pub prompt_tokens: u32, pub completion_tokens: u32 }

pub enum AgentDelta {
    Text(String),
    ReasoningTick,                       // opaque; UI shows "thinking…", content never surfaced
    ToolCallStarted { call_id: String, tool: ToolId },
    End { stop: AgentStopReason, usage: AgentUsage, message: AgentMessage },  // final message carries opaque state
}

pub enum AgentLlmError { NoKey, NotConfigured, Unavailable, Timeout, AuthFailed, RateLimited, BudgetExhausted, Provider(String) }

// agent/llm/mod.rs
#[async_trait::async_trait]
pub trait AgentLlm: Send + Sync {
    /// One cold, self-contained call. `messages` is the fully-assembled prompt (stable prefix + elided
    /// history + envelope on the latest user turn). Streams deltas; cancel drops the stream (reqwest body
    /// closes, billing stops — reuse the ai::stream_registry CancellationToken pattern).
    async fn respond(
        &self,
        system: &str,
        tools: &[ToolDeclaration],
        messages: &[AgentMessage],
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<futures::stream::BoxStream<'static, Result<AgentDelta, AgentLlmError>>, AgentLlmError>;
}
```

The genai-backed impl (`agent/llm/genai_impl.rs`) resolves the interactive model slot (M8) into an
`crate::ai::AiBackend` and maps `AgentPart` ⇄ genai `ContentPart` (`ToolCall`, `ToolResponse`, `ThoughtSignature`,
`ReasoningContent`, `ToolCall.thought_signatures`), reusing `map_genai_error` for the typed error surface
(429→RateLimited, 401/403→AuthFailed — status-based, satisfies `no-string-matching`). Reasoning stays off/minimal for
Anthropic and OpenAI-Responses in v1 (spike verdict).

**The read-only enforcement gate is the `ToolId` parse step.** A provider returns each tool call's name as a raw string
(genai `ToolCall.fn_name`). The runtime parses that string into an agent `ToolId` **before dispatch**, and a name that
isn't an `agent_tool_view()` entry (a hallucinated `"delete"`/`"copy"`, or a typo) fails to parse and yields a typed
"tool not available" tool-result fed back to the model — it never reaches `execute_tool`, which would otherwise
string-match and dispatch any table name. So read-only-by-construction holds at runtime, not just structurally: the
parse is the choke point, tested by M4's negative test.

The deterministic fake (`agent/llm/fake.rs`) — the whole runtime + UI is testable with zero network:

```rust
pub enum ScriptedTurn {
    Say(Vec<String>),                    // final answer, streamed as these text chunks
    CallTools(Vec<(ToolId, serde_json::Value)>),  // emit these tool calls, then await results and continue
    CallRawTool(String, serde_json::Value),  // emit an arbitrary raw name (e.g. "delete") to exercise the parse gate
    Fail(AgentLlmError),
}
pub struct FakeAgentLlm { /* scripted turns + a recording of every messages slice it was called with */ }
impl FakeAgentLlm {
    pub fn script(turns: Vec<ScriptedTurn>) -> Self;
    pub fn calls_seen(&self) -> Vec<Vec<AgentMessage>>;   // assert exact assembled prompts (prefix stability, elision, envelope)
}
```

## 7. IPC surface

Thin pass-throughs in `commands/agent.rs`, typed via tauri-specta (`ipc_collectors.rs`), wrapped in
`tauri-commands/ask-cmdr.ts`. Streaming follows the shipped `suggestions.rs` pattern: `send` takes a per-message Tauri
`Channel<AskCmdrStreamEvent>`; cancellation drops the channel + trips the stream-cancel token.

Commands (names are the wire contract; typed `Result<T, String>` unwrapped via `throwIpcError`):

- `ask_cmdr_list_conversations(limit, offset, include_archived) -> Vec<ConversationRow>`
- `ask_cmdr_get_conversation(id, msg_limit, msg_offset) -> Option<ConversationDetail>`
- `ask_cmdr_search_conversations(query, limit, offset) -> Vec<ConversationSearchHit>` (FTS5 over `messages_fts`)
- `ask_cmdr_send_message(conversation_id: Option<i64>, text, attachments: Vec<AttachmentRef>, channel: Channel<AskCmdrStreamEvent>) -> i64`
  — `None` conversation_id lazily creates one; returns the (possibly new) conversation id. Single-flight per thread.
- `ask_cmdr_cancel(conversation_id) -> ()`
- `ask_cmdr_rename_conversation(id, title) -> ()`
- `ask_cmdr_archive_conversation(id, archived: bool) -> ()`
- `ask_cmdr_cost_summary() -> CostSummary` (per-day rollup from `cost_meter`)

Streaming events (`AskCmdrStreamEvent`, typed enum; the frontend gets display parts only — no reasoning blob crosses):

- `Queued` — a send arrived while a loop runs; shows the "working… stop?" affordance.
- `UserPersisted { message_id, seq }`, `AssistantStarted { message_id, seq }`
- `TextDelta { text }`, `ReasoningTick`
- `ToolCallStarted { call_id, tool: ToolId, summary }` — the collapsible "looked at X" line (summary is a localized
  label built frontend-side from `tool` + args, never a backend string)
- `ToolCallFinished { call_id, ok: bool }`
- `Done { stop: AgentStopReason, usage: AgentUsage }`
- `Failed { kind: AgentErrorKind }` — typed; rendered honestly, never with "error"/"failed" words

Wire types: `ConversationRow`, `ConversationDetail` (header + a page of `MessageView`), `MessageView` (display-only
blocks: text, tool-call summary, tool-result stub — **no** provider blob), `ConversationSearchHit`,
`AttachmentRef { path, kind }`, `CostSummary`. All `#[serde(rename_all = "camelCase")]` + `specta::Type`; DB tokens
snake_case via `token_enum!`.

## 8. Milestones

Sequential by default. M1 and M2 are parallel-safe (independent surfaces: the trait vs. the DB, no shared files). M3
touches shared registry code (`tool_registry.rs`) that parallel main-branch sessions also touch — keep it sequential,
rebase before landing. Each milestone finishes by running the stated `pnpm check` scope (never bare cargo; the
COW-cloned worktree `target/` false-greens — `touch` a source when verifying compile freshness).

### M0 — genai capability spike (DONE)

Outcome pointer: §3 above / `ask-cmdr-genai-spike.md`.

### M1 — `AgentLlm` trait + deterministic fake

- **Scope**: the typed message-part model (§6), the trait, the genai-backed impl over `crate::ai::AiBackend`, the fake,
  the typed error surface. Reasoning off/minimal on Anthropic + OpenAI-Responses paths.
- **Intention**: lock the seam that the entire runtime and UI test against, with the opaque-provider-state shape the
  spike proved is mandatory. Getting the part model right here is what keeps multi-step loops from silently degrading
  later; everything downstream depends on this contract, so it is worth over-investing in.
- **TDD-first** (real red→green): part-model ⇄ genai mapping round-trip (a tool-call-with-reasoning survives
  serialize→parse without losing the blob); `map_genai_error` status→typed-kind mapping; the fake's scripted-turn
  sequencing and `calls_seen` recording.
- **Test-after**: one gated live smoke per Tier-1 cloud provider (skipped without keys, never in CI's critical path);
  the local-slot smoke against llama-server.
- **Docs**: `agent/llm/CLAUDE.md` + `DETAILS.md` (the part-model invariant, the reasoning-off-in-v1 decision, the genai
  gaps A/B pointer).
- **Checks**: `pnpm check rust clippy --fast` while iterating; `pnpm check rust` at milestone end.

### M2 — `main.db` durable store

- **Scope**: the migration ladder + `token_enum!` types + connection/pragmas (mirroring `operation_log/store/`), the v1
  DDL (§5), the FTS5 feature enablement + triggers, the conversation/message/cost-meter query layer, `agent::start`
  wiring at app setup, and a pure **FTS5 query-sanitizer** function (used by M7's cross-thread search). Raw user input
  fed straight into `... MATCH ?` throws an fts5 syntax error on ordinary filename fragments (`report(v2)`, `foo:bar`, a
  bareword `AND`/`OR`/`NOT`, an unbalanced `"`) — parameter binding does not help, and there's no in-tree pattern to
  copy. The sanitizer tokenizes the input, wraps each token as an FTS5 **string literal** (doubling embedded quotes),
  and appends `*` for prefix search, so any user text is a safe, prefix-matching query.
- **Intention**: a second consumer that proves the operation-log durable-DB template generalizes (agent-spec D3). Reuse
  the ladder discipline exactly — refuse downgrade, delete-and-recreate only on the typed corrupt-DB error code, never
  on a string.
- **TDD-first**: migration ladder bootstrap (fresh DB → v1) + downgrade-refusal + corrupt-recreate (reuse the op-log
  test shape); `token_enum!` round-trip + uniqueness; FTS5 insert/update/delete trigger sync (a message edit re-indexes;
  a delete de-indexes); a search query returns the right rows and ranks recent-first; **the FTS5 query-sanitizer** (real
  red→green: empty query, punctuation like `report(v2)`, an embedded quote, a bareword `AND`/`OR`/`NOT`, and a prefix
  match all produce a valid non-throwing query with the expected hits).
- **TDD-first (cost meter)**: an upsert against an existing `(day, conversation_id, provider, model)` row
  **accumulates** (`ON CONFLICT DO UPDATE prompt_tokens = prompt_tokens + excluded.prompt_tokens`, etc.) rather than
  inserting a duplicate — the guard against the NULL-in-PK regression; and the per-day cross-thread rollup
  (`SUM ... GROUP BY day`) sums across threads correctly.
- **Test-after**: large-transcript paging.
- **Docs**: `agent/store/CLAUDE.md` + `DETAILS.md` (the FTS5-is-net-new note, the DDL rationale, the no-retention-in-v1
  decision + retention scaffold pointer).
- **Checks**: `pnpm check rust`; `pnpm check cargo-deny` after the `fts5` feature flip.

### M3 — Registry: consumer + access dimensions, file split

- **Scope**: honor agent-spec D49 ("extend the consolidated registry, don't fork") and D59 (structural consumer gating)
  by growing the one authored `mcp_tools!` registry, not by standing up a parallel agent-only dispatch table. Three
  things in one refactor (they share `tool_registry.rs`):
  1. Grow each authored entry with a **`consumers`** dimension (`[ai_client]`, `[agent]`, or both) — D59's actual
     mechanism — and the **`access: Read | Write`** dimension. Generate `tool_consumers(name)`, `tool_access(name)`, and
     an `agent_tool_view()` (the set of entries whose `consumers` includes `agent`) next to `tool_gate`. Existing
     entries stay `consumers: [ai_client]` unless deliberately shared. **Mechanism (load-bearing):** the macro currently
     emits a `Tool` for every entry unconditionally, so agent-only entries do NOT auto-drop from the wire —
     `get_all_tools()` must be reworked to APPLY a consumer filter, returning only entries whose `consumers` includes
     `ai_client`. Under that filter `get_all_tools()` IS the ai_client view, and it stays byte-identical for the entries
     it already contained (the new `consumers`/`access` dimensions are registry metadata, not fields on the emitted
     `Tool` struct), so `tool_snapshot_tests.rs` plus `EXPECTED_TOOL_NAMES` / count / gate-table tests stay green
     **unmodified** — but only because the M4 agent-only entries are filtered out of this view. Symmetrically,
     **`execute_tool` gains a consumer identity and dispatches only its own view**: an agent-only tool name arriving
     over MCP is refused, and an `ai_client`-only name arriving through the agent runtime is refused. "Callable but not
     listed" is exactly the drift D59 exists to prevent, so no transport dispatches a name outside its consumer view.
  2. **Split** `tool_registry.rs` (currently 1104 lines, at allowlist parity — adding two fields per entry plus the M4
     agent entries would push it well over) into a directory module: `tool_registry/mod.rs` (macro + table + accessors),
     `tool_registry/gate.rs` (`TokenGate`), `tool_registry/schemas/*.rs` (the per-category `json!` schema blocks hoisted
     into `fn <tool>_schema() -> Value`, which dominate the line count and serialize identically). This resolves the
     standing length warn by trimming, not by touching the allowlist (run `pnpm check file-length` and commit the
     shrink-wrap).
- **Intention**: give the agent a **read-only-by-construction** dispatch view (the agent runtime in M5 dispatches ONLY
  through `agent_tool_view()`) and a structural test that keeps it that way. This strengthens D59: D59's stated test is
  "every non-`Open`-gated tool is absent from the agent view", which is necessary but **not sufficient here**, because
  `TokenGate::Open` is not "read-only" — its doc comment covers "destructive ops that still prompt the user", and the
  actual file-mutating ops (`copy`/`move`/`delete`) carry `IfAutoConfirm` (effectively open when `autoConfirm` is
  absent). A gate-based filter would let a destructive tool sit in a read-only agent's view. So the explicit
  `access: Read | Write` dimension is the correct structural guarantee: the agent view must equal exactly its authored
  `consumers:[agent]` entries **and** every one of them must be `access: Read`.
- **TDD-first** (the structural gate — real red→green): (a) **set-equality** — `agent_tool_view()` equals exactly the
  authored set of `consumers:[agent]` entry names (a hand-listed expected set, like `EXPECTED_TOOL_NAMES`); (b) every
  entry in `agent_tool_view()` has `access: Read`. In the red step, add a `consumers:[agent], access: Write` entry and
  watch (b) fail before the view/adapters exist to make it pass.
- **Test-after**: the existing `EXPECTED_TOOL_NAMES` / gate-table / wire-snapshot tests still pass (update consciously
  only for the new accessors; if you deliberately share an entry into `[ai_client, agent]`, update the structural tests
  in the same conscious step — never loosen them).
- **Docs**: update `mcp/CLAUDE.md` + `DETAILS.md` (the consumer + access dimensions, the "extend don't fork" record, the
  two-view model, why access strengthens the gate-based D59 test, the split map).
- **Checks**: `pnpm check rust file-length`; confirm the ai_client wire snapshot is byte-identical.

### M4 — Agent read-only tool entries + in-process handlers

- **Scope**: author the concrete v1 tool families (spec §3) as **new registry entries tagged
  `consumers:[agent], access: Read`**, each with a handler that calls the underlying core the Q2 research verified. This
  is the "extend the registry" work D49 mandates — the agent gets first-class registry tools, not a side dispatch path.
  Handlers run in-process and may take `&AppHandle<R>` (the agent runs in-process with a handle; the transport-agnostic
  bar is met — `execute_tool` needs only `AppHandle` + JSON, no HTTP server or auth). The five families and their cores
  (see §11 Q2 for the corrected signatures + the visibility bumps/shims each needs — **budget these as small wiring
  tasks, not literal drop-ins**):
  - app-state snapshot — reuse `mcp::resources` state building (`build_state_yaml` is private + takes `AppHandle<R>`
    plus opts; either call it through a small in-`mcp` shim or read `PaneStateStore` + `snapshot_volumes` directly).
  - directory listing + stats — `indexing::read::queries::get_dir_stats*` +
    `EntryRow::list_children_on(parent_id: i64, conn: &Connection)` (a method on `EntryRow` in
    `indexing/store/entries.rs`, taking a resolved `parent_id` + a read connection, NOT a path-based `IndexStore` call).
    So the listing tool first resolves the path to a `parent_id` (`indexing::store::resolve_path`) and opens a read
    connection; budget that wiring. **The top-N-by-size tool batches `get_dir_stats` over candidate dirs and sorts
    itself; no index query does this.**
  - importance — `ImportanceIndex::{top_n, lookup, top_above_threshold, explain(path, now_secs: u64)}` (already
    `&Path`-based, offline). `explain` needs a `now_secs` clock arg (recency contributions are computed relative to
    now); pass the same wall-clock unix-secs source the envelope timestamp uses, so a tool call and its explanation
    agree on "now".
  - operation-log search + detail — `operation_log::query::{search_operations, get_operation}`, which take a
    `&Connection` (open one via `operation_log::store::open_read_connection(&db_path)`); consider sharing the existing
    `operations_list`/`operations_get` entries as `consumers:[ai_client, agent]` since their cores fit unchanged —
    recommended, with the structural test updated consciously in M3.
  - volume list —
    `mcp::resources::volumes::{snapshot_volumes (async, `pub(crate)` → needs a visibility bump), build_volumes_yaml(&[VolumeSummary])}`.
  - Every tool voices coverage honestly in its typed result: index `Freshness` (`fresh`/`scanning`/`stale`,
    `is_authoritative()`), `DirStats.recursive_size_complete/_stale/_pending`, importance `as_of_generation` vs
    `recompute_generation`, unmounted/unindexed volumes.
- **Intention**: one authored registry, two consumer views (D49); reuse the shipped cores, don't re-derive
  listing/importance logic. The honesty caveats are load-bearing (spec §2.4): a tool returning a stale or lower-bound
  number must say so in its typed result so the system prompt can require the model to voice it.
- **TDD-first**:
  - the M3 set-equality + all-Read tests now cover the concrete agent entries: M3 establishes the machinery with an
    empty agent view (the tests pass vacuously); M4 populates the entries and extends the expected set, going red→green
    on the populated assertions.
  - **the read-only NEGATIVE / enforcement test** (the point of the whole guarantee): when the fake `AgentLlm` emits a
    tool call naming a write or non-view tool — a hallucinated `"delete"`, `"copy"`, or an unknown name — the agent
    dispatch **refuses it** (the incoming name fails to parse into a valid agent `ToolId`, yielding a typed "tool not
    available" tool-result) and **never reaches `execute_tool`** (which dispatches every table name by string match, so
    the parse step, not `execute_tool`, is the gate). Red step: point the fake at `"delete"` and assert dispatch refuses
    without touching `execute_tool`.
  - **`ToolId` ↔ `agent_tool_view()` 1:1 structural test**: the typed `ToolId` enum and the registry's agent-consumer
    view are authored in two places with nothing tying them; a test asserts the `ToolId` variants map exactly onto the
    `agent_tool_view()` entry names (no orphan variant, no unmapped view entry). Joins the M3/M4 structural suite.
  - each tool's core returns the right typed shape against a fixture DB; the coverage flags surface correctly (a stale
    index reads `stale`, an unmounted-but-scored volume still returns importance offline, an unindexed volume returns
    "no index" not a wrong zero).
- **Test-after**: the agent toolset emits `ToolDeclaration`s with no `strict:true`; a fake-driven loop dispatches a tool
  through `agent_tool_view()` and gets a well-formed result.
- **Docs**: `agent/tools/CLAUDE.md` + `DETAILS.md` (the tool catalog, the "author as registry entries / reuse the core"
  rule, the top-N-by-size gap note, the shim/visibility-bump list).
- **Checks**: `pnpm check rust`.

### M5 — Chat runtime + context assembly

- **Scope**: the pure context-assembly core (`chat/context.rs`) — stable byte-identical prefix (system prompt + tool
  declarations + `~/.cmdr/CMDR.md` if present), elide-only history compaction, the fresh context envelope on the latest
  user turn only, budget enforcement — plus the runtime (`chat/runtime.rs`): single-flight per thread with a visible
  queue, per-message budgets (max tool turns, max wall time), cancellation at tool boundaries + stream-cancel, the typed
  error surface.
- **Intention**: this is the spec's TDD-heavy pure core (values in, prompt out, no I/O). Prefix byte-stability is what
  buys provider prompt caching; the envelope must live only on the latest user turn so caching survives. A runaway loop
  must be impossible by construction, not by hope.
- **Envelope is snapshot-at-send.** The context envelope (§9) is captured **once** at message-send time and held
  constant across that turn's entire tool loop. The user navigating a pane or changing the selection mid-loop must not
  shift the model's ground truth mid-turn; the next user message captures a fresh envelope.
- **Crash / mid-stream persistence semantics.** Continuity is through DB state (spec §2.3), so partial state must be
  unambiguous. A message's `content_blocks` are written only on that call's `End`, so:
  - (a) assistant text streamed before a non-`End` termination (provider drop, app crash, cancel) is **discarded** from
    the DB — no partial assistant row is persisted — and the UI shows a typed, honest notice ("the reply didn't finish —
    try again?", never "error"/"failed").
  - (b) a user message whose **first** `respond` call never reached `End` is a **valid, resumable** state — nothing
    about the failed attempt is recorded, so re-sending assembles byte-identically the same prompt.
  - (c) **interrupted multi-turn loop**: when a crash lands mid-loop after one or more turns completed (e.g.
    `user → assistant(tool_call) → tool(result) → [crash streaming turn 2]`), the completed turns and their tool results
    **stay persisted** (each was written on its own `End`). "Try again" issues a **fresh** `respond` assembled from the
    current persisted transcript (including those completed turns) — NOT a byte-identical re-send of the original user
    message.
  - (d) cost is metered per **completed** `respond` call (each stream `End` carrying usage), so tool-loop turns that
    completed before a crash are still counted, never double-counted, and never lost for completed calls.
- **TDD-first** (mandatory real red→green, spec §8): prefix is byte-identical across calls; a tool result older than
  `ELIDE_TOOL_RESULTS_AFTER_TURNS` collapses to a typed stub while assistant prose survives verbatim; the envelope
  carries exactly the §9 field set and appears only on the latest user turn; **two `respond` calls within one turn's
  tool loop see byte-identical envelopes** (the snapshot-at-send guarantee); the assembled **system prompt contains the
  coverage-honesty rule and the read-only self-description** (a string-contains guard on our OWN prompt asset — this is
  not error/state classification, so no `no-string-matching` conflict; note that in the test comment); the token budget
  is enforced (assembly stays within `CONTEXT_TOKEN_BUDGET`); `MAX_TOOL_TURNS` and `MAX_WALL_TIME` halt a loop;
  single-flight queues a second send and the "working… stop?" state is emitted; cancellation mid-loop stops cleanly at a
  tool boundary; **a stream dropped mid-text persists no assistant row and emits the typed unfinished-reply notice**;
  **an interrupted loop — crash after turn 1 completes — persists turn 1's rows, and the retry's assembled prompt
  includes them** (fresh `respond` from the persisted transcript, not a byte-identical re-send).
- **Test-after**: end-to-end fake-driven multi-tool turn; typed errors (no key, provider down, rate-limited, budget
  exhausted) render without the words "error"/"failed".
- **Docs**: `agent/chat/CLAUDE.md` + `DETAILS.md` (the anatomy-of-one-call reference from spec §5, the named constants
  table §10, the prefix-stability invariant).
- **Checks**: `pnpm check rust clippy`.

### M6 — Sidebar rail UI + streaming + menu wiring

- **Scope**: the right rail (`AskCmdrRail.svelte`) hosted in `+page.svelte` beside `DualPaneExplorer`; streaming render
  with a stop button; collapsible tool-call lines; markdown-lite via `snarkdown` **with mandatory entity-escaping of
  untrusted model/user text** (the `errors/markdown-escape.ts` pattern) before `{@html}`; screen-reader support for the
  live stream — streamed assistant text renders into a polite `aria-live` region and tool-call status uses `aria-busy` /
  `role=status` (reuse the existing pattern from `Spinner.svelte`, `ToastContainer.svelte`, `QueryResults.svelte`); the
  focus model (the rail as a third focus region — add a parallel region flag rather than widening the binary
  `'left'|'right'` union in `explorer-state.svelte.ts`; toggle focuses the composer, Esc returns focus to the active
  pane; pane min-widths hold; layout persists via `app-status-store.ts`); the full four-places menu wiring for the
  toggle (command registry + command id, Rust `command_map.rs` + `macos.rs`/`linux.rs` View submenu,
  `shortcuts-store.ts` `menuCommands`, `showInPalette`); ALPHA badge via `feature-status.json` (`id: "ask-cmdr"`) +
  `getBadgeStatus` + `StatusBadge.svelte`; the thread-length soft-cap nudge ("this chat is getting long — start a fresh
  one?", honest UI copy, no hard cut) shown when a thread crosses `THREAD_SOFT_CAP_MESSAGES` (§10), with a one-click
  new-chat action.
- **Intention**: the killer context is what the user is looking at, so the chat lives in the main window next to the
  panes. The focus model must be deliberate — the rail is the future convergence surface for notifications and proposal
  reviews, so entering/leaving must feel intentional now.
- **TDD-first**: none pure here; behavior is UI.
- **Test-after** (Vitest + Playwright): the registered toggle resolves for JS dispatch — `getEffectiveShortcuts` returns
  `['⌘⌥A']` (command-first), the guard against re-introducing the Apple-display-order string that only fires in the
  native menu; rail toggles via shortcut and via View menu; send-and-render streams against the fake backend; the stop
  button cancels; tool-call lines expand/collapse; focus enters the composer on open and returns to the pane on Esc;
  layout width persists; markdown escaping neutralizes an injection-shaped model string; the soft-cap nudge appears once
  a thread crosses `THREAD_SOFT_CAP_MESSAGES` and its new-chat action opens a fresh thread.
- **Docs**: `src/lib/ask-cmdr/CLAUDE.md` + `DETAILS.md`.
- **Checks**: `pnpm check svelte desktop`; `pnpm check --include-slow` for the E2E specs.

### M7 — Sessions, cross-thread search, attachments

- **Scope**: the thread list (recent first, `AskCmdrSessions.svelte`), new-chat/rename/archive (flag + filter), the
  offset-`= entries.length` paging pattern, cross-thread FTS5 search, attach-by-drag-from-a-pane and "ask about
  selection" → reference chips resolving to path + metadata (never contents).
- **Intention**: reuse the operation-log dialog's list/paging/search template. Attachments are by reference only — the
  chip resolves to path + metadata in the envelope, structurally never file contents (the read-only privacy line).
- **Test-after**: paging never overlaps/desyncs (offset from `entries.length`); search returns the right threads;
  archive filters correctly; an attachment chip injects path + metadata into the envelope and nothing more.
- **Docs**: extend `src/lib/ask-cmdr/DETAILS.md`.
- **Checks**: `pnpm check svelte desktop`.

### M8 — Consent, settings, cost visibility, i18n, a11y, certification

- **Scope**: the opt-in consent screen (exact copy in §12) stating what leaves the machine; settings section (enable
  toggle, provider/model for the **interactive slot** — a new concept: today `ai/` has a single `cloud_model`, so M8
  introduces the interactive-slot resolution over the existing `ai/` provider config; spend display from `cost_meter`);
  record consent; per-thread token/cost in the thread footer + per-day rollup in settings; i18n across all 10 locales
  (`askCmdr.*` keys, `pnpm intl:keys`, parity test); a11y (tier-3 test, focus trap, AA contrast); the website
  privacy-copy touchpoint noted for ship; the pending live provider checks (§11 open-question 7) run with real keys,
  re-verifying current model ids from each provider's models endpoint at that time — including the **thinking-off
  ≥3-step Anthropic loop** that certifies the v1 Anthropic happy path, and pinning the v1 Anthropic model to the newest
  thinking-disablable id (always-on-thinking Claude models stay out of v1 cert until the genai patch lands).
- **Intention**: the consent screen is the trust contract — it must be exact and honest, and it is a David-reviewed
  human-facing string (principle 6). The interactive slot is where the bulk slot later slots in (agent-spec D43).
- **TDD-first**: none pure.
- **Test-after**: consent gates first use; settings round-trip; cost footer reflects `cost_meter`; the cost miss-path
  (local ⇒ "free/on-device"; unpriced model ⇒ tokens shown, cost "unknown", never a silent $0); i18n parity green; a11y
  tier-3 passes, including the streaming `aria-live` region announcing appended assistant text and tool-call
  `role=status`; E2E open-via-menu + open-via-shortcut + send-and-render.
- **Docs**: `AskCmdrSection` settings doc; update `feature-status.json`; note the website privacy touchpoint.
- **Checks**: `pnpm check` (full) then `pnpm check --include-slow`; `pnpm check intl`.

### M9 — LLM call logging (the observability gateway)

- **Requirement (David, verbatim intent)**: "what is it exactly that we sent to this LLM? Was it set up for success? And
  what did it respond?" A structural, no-call-can-bypass-it log of every LLM request and response to local disk.
- **Choke point**: all LLM traffic flows through `AiBackend` (`ai/client.rs`) — the agent (via M1's `AgentLlm` genai
  impl) AND the existing one-shot features (folder suggestions, translate, NL search). The logger taps at the
  `AiBackend`/genai boundary so interception is structural: both the M5 runtime's calls and the legacy prompt-helpers
  get logged through one seam, and no code path can skip it.
- **Capture fidelity — verbatim wire body is available and cheap (researched).** genai `=0.6.0-beta.19` exposes
  `AdapterDispatcher::to_web_request_data(target, service_type, chat_req, options_set) -> WebRequestData { url, headers, payload: Value }`
  as a **public** function, and `payload` is the **exact per-adapter wire JSON** genai sends — it is the very function
  `exec_chat` / `exec_chat_stream` call internally (`client_impl.rs`), so reproducing it for logging yields the
  byte-identical body (post-transform: strict-schema injection, thinking config, tool-call formatting) with **no network
  call and no proxy**. So v1 logs the verbatim wire `payload`, and records `fidelity: "wire"` in metadata. (The
  documented fallback — serialize the genai `ChatRequest` to JSON — stays noted for any future adapter where
  `to_web_request_data` can't be reached, marked `fidelity: "request_struct"`; it still carries the full assembled
  prompt: system, tools, history, envelope.) **Redact the auth header**: `WebRequestData.headers` carries the user's API
  key — strip/redact it before writing; the `payload` body itself contains no secret. Responses: log the full raw
  response body (genai's non-stream path parses `WebResponse { status, body: Value }`); for streams, log the assembled
  final response plus, optionally, the raw delta log.
- **File layout** (David's design):
  `{app data dir}/llm-logs/{session-or-thread-id}/{NNN}_{request|response}_{slug}.json` — `NNN` a three-digit
  zero-padded per-session counter; `slug` is **deterministic** (job type + a few sanitized words of the latest user
  message), never LLM-generated. JSON files (chat APIs are JSON), with metadata embedded: ISO timestamp, provider,
  model, adapter kind, token counts (when the provider returns usage), latency, stop reason, and the `fidelity` marker.
  Borrow OpenTelemetry GenAI semantic-convention field names where they fit (`gen_ai.request.model`-style) so external
  tooling can consume the files later, but **add no OTel dependency**.
- **Setting**: `logLlmCalls` in Settings › Advanced; **default ON in dev builds, OFF in prod/release**;
  runtime-toggleable without restart (re-read the setting per call, like the AI config's read-fresh pattern).
  **Failure-isolated**: a logging error (disk full, permission) never breaks or delays the LLM call — log-and-continue,
  the call result is unaffected.
- **Privacy**: logs contain everything the provider saw (names, paths, envelope) — **local only, never transmitted**, in
  the app data dir. One line in the consent/settings docs points at the folder.
- **Intention**: answer David's three questions for both the agent and the legacy AI features, and give the M8 live
  certification runs their natural debugging companion (dev-default-ON).
- **TDD-first** (real red→green, the pure parts): path/slug/counter generation (deterministic slug from job type +
  message; the counter increments and zero-pads; the session dir is derived correctly); metadata assembly; a
  redaction-free round-trip of a captured payload (write→read yields the same JSON), plus the **auth-header redaction**
  (the written request file carries no API key).
- **Test-after** (integration, against the fake LLM): one fake turn produces exactly one request + one response file
  with correct sequence numbers; the setting OFF produces zero files; a deliberately failing logger (unwritable dir)
  does not fail or delay the call.
- **Docs**: a new `agent/llm/DETAILS.md` section (or the logger's own colocated doc) on the wire-capture approach,
  fidelity markers, the file layout, and the failure-isolation contract; the settings/consent privacy line.
- **Checks**: `pnpm check rust`; `pnpm check svelte desktop` for the setting UI; `pnpm check --include-slow` before the
  wrap.
- **Placement**: after M8, independent of David's product calls. Because the setting is dev-default-ON, M8 execution may
  pull M9 earlier to instrument the live certification runs — an allowed resequencing, record the rationale if done.

## 9. Context envelope field set (resolves spec §7 Q4)

A single tagged block opening the latest user turn only (never the prefix, so prompt caching survives). Fields, in
order, all from verified sources:

```
[Sat 2026-07-12 21:30 · focused: ~/Documents/taxes · cursor: 2024/ · 2 selected · volumes: Macintosh HD (fresh), NAS-home (stale, direct)]
```

- Local weekday + ISO date + time (so the model can reason about "this morning"; every historical message also carries
  its own timestamp).
- Focused pane path — `PaneStateStore::get_focused_pane` returns the pane **side** (a bare `String`,
  `"left"`/`"right"`), not a path; resolve that side's path from the state snapshot (the pane's current directory).
- Cursor item name, or `—` if none.
- Selection count.
- Mounted volumes, each with an index-freshness token (`freshness_token`: fresh/scanning/stale/off) and, for SMB
  volumes, a connectivity token (`SmbConnectionState`: `direct`/`os_mount`/`disconnected`) — from `snapshot_volumes()`.

Consent wording for the envelope is folded into the consent copy (§12): "the app-state envelope (what you're looking at:
current folder, cursor, selection, and your connected drives)".

## 10. Named constants (resolves spec §7 Q5)

Initial values; tune with use (comment each as such at the definition site).

- `MAX_TOOL_TURNS = 8` — per user message; a loop that wants a 9th tool turn stops and answers with what it has.
- `MAX_WALL_TIME = Duration::from_secs(60)` — per user message.
- `CONTEXT_TOKEN_BUDGET = 8_000` — target assembled-prompt size per call (spec's 6–10k band).
- `ELIDE_TOOL_RESULTS_AFTER_TURNS = 3` — tool results older than this collapse to a typed stub; assistant prose always
  survives verbatim.
- `THREAD_SOFT_CAP_MESSAGES = 40` — past this, show the honest "this chat is getting long — start a fresh one?" nudge
  (no hard cut; summarize-on-overflow is deferred).
- `TOOL_RESULT_STUB_TOKENS_HINT` — the stub records the elided result's approximate token size for the "[tool result
  elided: top-20 dir listing, ~3.1k tokens]" copy.

## 11. Resolutions to spec §7 open questions

**Q1 — everything the spike answers.** Done (M0, §3). The reasoning-round-trip split (Gemini works; Anthropic +
OpenAI-Responses broken) drives the reasoning-off-in-v1 posture and the genai-patch follow-up.

**Q2 — which MCP executor cores are consumable in-process as-is vs. need hoisting.** Verified answer: **all five v1 read
surfaces are callable in-process** given the app's `AppHandle`; none needs hoisting out of a transport, but several need
small visibility bumps or shims (so M4 budgets them as wiring, not literal drop-ins). The research clarified two
"transports" often conflated: (a) the MCP/HTTP/JSON-RPC wire — the registry core is already fully decoupled from it
(`execute_tool` needs only an `AppHandle<R>` + JSON, no server/auth); (b) the Tauri **frontend round-trip**
(`executor::mcp_round_trip`, emit/listen on `mcp-response`) — this welds the _mutating/action_ tools to the frontend,
but **none of the read surfaces touch it** (they read Rust-side stores and SQLite directly). The cores and their exact
call shapes (corrected against the tree):

- `operation_log::query::{search_operations, get_operation}` take a **`&Connection`** (not a `&Path`); open one via
  `operation_log::store::open_read_connection(&db_path)`.
- `ImportanceIndex` is `&Path`-based already (offline-capable). Note `explain(path, now_secs: u64)` takes a wall-clock
  arg (recency is relative to now); the other query methods (`top_n`, `lookup`, `top_above_threshold`) don't.
- drive index: `indexing::read::queries::get_dir_stats*` (globals) +
  `EntryRow::list_children_on(parent_id: i64, conn: &Connection)` (a method on `EntryRow` in
  `indexing/store/entries.rs`, not a path-based `IndexStore` call), so listing wires up path→`parent_id` resolution
  (`indexing::store::resolve_path`) + a read connection first.
- `mcp::resources::volumes::snapshot_volumes()` is **`async` + `pub(crate)`** (needs a visibility bump);
  `build_volumes_yaml(&[VolumeSummary])` **takes an arg**.
- `mcp::resources::build_state_yaml` is **private** and takes **`AppHandle<R>` + opts** — reach it via a small in-`mcp`
  shim, or read `PaneStateStore` + `snapshot_volumes` directly for the envelope's needs.

The one real caveat, which reshapes M3: **`TokenGate::Open` ≠ read-only.** Its doc comment covers "destructive ops that
still prompt the user", and the file-mutating ops (`copy`/`move`/`delete`) carry **`IfAutoConfirm`** (effectively open
when `autoConfirm` is absent), so a gate-based agent-view test (D59's phrasing) is insufficient. The registry therefore
grows a `consumers` dimension (D59's actual mechanism) plus an explicit `access: Read | Write` dimension, and the agent
view is pinned to exactly its `consumers:[agent]` entries, each `access: Read` (M3). And **there is no top-N-by-size
index query** — that tool batches `get_dir_stats` over candidate dirs and sorts itself.

**Q3 — markdown-lite: hand-rolled vs. in-tree renderer.** Reuse the in-tree `snarkdown` lib (already a dependency,
already used by the What's-new dialog and the error-explanation pane). Do **not** hand-roll. Rationale: it exists, it's
tiny, and matching the app's existing markdown look is free. The one requirement it adds: both current uses feed
**trusted** input; streaming LLM output is untrusted-shaped, so every model/user string must be entity-escaped (the
`errors/markdown-escape.ts` HTML-numeric-entity helper, since snarkdown ignores CommonMark `\` escapes) before
`{@html}`. snarkdown lacks tables and code-fence highlighting; that's acceptable for markdown-lite (paragraphs, lists,
inline code, bold — spec §3). **Filesystem-derived names are a third untrusted source** (an SMB share's file names are
attacker-controlled): tool-call summary lines (M6) and attachment chips (M7) render path/name text as escaped plain
text, never through `{@html}`; any styling routes through the same markdown-escape helper.

**Q4 — envelope field set + consent wording.** §9 above (fields) and §12 (consent).

**Q5 — thread-length soft cap + elision thresholds.** §10 above (named constants, tune with use).

**Q6 — sidebar min-width at small window sizes.** Rail default width 340px, min 280px, max 520px; each pane keeps its
existing min-width. Persist rail width + open flag in `app-status-store.ts` (same pattern as `leftPaneWidthPercent`).
When the window is too narrow to hold both panes at min-width plus the rail (below ~900px effective), the rail
**overlays the right pane as a floating panel** rather than compressing panes below their minimum — the panes never
break their min-width contract (spec UI requirement). All widths are tunable constants.

**Q7 — which Tier-1 models to certify at ship time.** Anthropic, OpenAI, Gemini, local (agent-spec D40). Do **not** pin
model ids from training data — re-verify current ids from each provider's models endpoint at implementation time (M8).

The **v1 Anthropic certified model is "the newest Anthropic model where thinking can be disabled"**, id verified from
the provider's models endpoint at M8 — because v1 runs reasoning-off on Anthropic (the round-trip is broken, spike Gap
A). **Always-on-thinking Claude models (Fable/Mythos-class, which cannot disable thinking) are explicitly OUT of v1
Anthropic certification** until the genai Anthropic thinking capture+replay patch (§13 follow-up) lands. This is the
single most important thing to state plainly to a user picking an Anthropic model in v1.

The pending live checks (from the spike §5, run with real keys in M8) are the certification gate:

1. **Anthropic, thinking OFF + a ≥3-step tool loop — confirm it COMPLETES.** This is the check that actually certifies
   the v1 Anthropic posture (the spike harness has it staged: `./target/debug/spike sequential <model>` without
   `REASON=1`); nothing in the original list proved the happy path works, only that thinking-ON breaks.
2. Anthropic, thinking ON + tool loop — confirm it breaks (expected 400), so the reasoning-off posture is justified.
3. OpenAI Responses, reasoning + tool loop — confirm degrade-vs-400 when reasoning items drop.
4. Gemini parallel calls (spike Gap C: `functionResponse.name` = synthetic call_id) — confirm no mis-pairing.
5. Gemini `thoughtSignature` across a real ≥3-step loop.
6. OpenAI-direct strict schema with an optional prop (Gap D) — confirm the 400 that lenient providers masked (mitigated
   by never setting `strict:true`).

## 12. Product calls for David

The plan proceeds on these recommendations, but David confirms them **before the i18n pass (M8)** and before building UI
copy on top of them.

**1. Feature name — recommend "Ask Cmdr".** Menu item, rail title, `feature-status` id `ask-cmdr`. It's a
product-identity call (spec §4); the internal subsystem is `agent/` regardless (agent-spec D44).

**2. Shortcut default — register `⌘⌥A` (command-first, "A for Ask").** Collision check (verified against the shortcut
registry): the combo Cmd+Option+A is **free** — no Cmdr binding uses it, and the only A-key bindings are `⌘A`
(select-all) and `⌘⇧A`, neither of which conflicts. **Modifier order is load-bearing, not cosmetic:** register it as
`⌘⌥A` (command before option), NOT `⌥⌘A`. `formatKeyCombo` emits Command-then-Option, and
`operation-log-shortcut.test.ts` is the regression guard proving Apple-display-order strings (`⌥⌘…`) resolve as
native-menu-only and **never fire via JS dispatch** — which is exactly why the operation log registered `⌘⌥L`, not
`⌥⌘L`. macOS still _renders_ the combo as ⌥⌘A in menus (Apple's display order); the registered string is `⌘⌥A`. Existing
⌥⌘-family combos in use: ⌥⌘H (hide-others, macOS-native), `⌘⌥L` (operation log), the paste-special and show-in-Finder
ops (so **don't** reuse show-in-Finder's combo). No known macOS system-global reservation on Cmd+Option+A (unlike ⌘⇧A =
Finder Applications). Worth one manual runtime check on the target OS. If it ever collides, `⌘⌥K`/`⌘⌥J`/`⌘⌥I` are also
free.

**3. Consent-screen copy — draft (David reviews; human-facing per principle 6).** Follows the style guide: active voice,
friendly, sentence case, no "just/simple", never "error"/"failed", and the spec §3 privacy line verbatim in spirit.

> **Talk to Cmdr about your files**
>
> Ask Cmdr sends your questions to the AI provider you choose, using your own API key. Here's exactly what leaves your
> Mac when you chat:
>
> - your messages
> - the names and paths of files and folders Cmdr looks at to answer you
> - their sizes and dates
> - the app-state envelope: what you're looking at right now — current folder, cursor, selection, and your connected
>   drives
>
> Cmdr never sends the contents of your files. It can't — in this version there's no tool that reads them. Ask Cmdr only
> looks and speaks; it never changes anything.
>
> Your chats stay on your Mac, in a local database you can open and read. You pick the provider and model, and you can
> see what each conversation costs.
>
> [Turn on Ask Cmdr] [Not now]

**4. Sidebar side — recommend right.** The rail is the future convergence surface for notifications and proposal reviews
(spec §3); right keeps it out of the left-to-right reading path into the panes. This is a recommendation, not a decree
(spec §8).

## 13. Execution gotchas + follow-ups

### Gotchas (spec §6 carried forward, plus discovered)

- The on-open formatter hook reflows `docs/specs/*.md`; the final diff must contain only `ask-cmdr-plan.md` and
  `index.md`. Revert any other churn (`git checkout -- <file>`); never commit it. ~12 docs are oxfmt-unclean and reflow
  on every check run — keep them out of commits.
- Use `pnpm check`, never bare cargo (the COW worktree `target/` false-greens; `touch` a source before verifying compile
  freshness). `--fast` iterating, full per milestone, `--include-slow` before the wrap. Never truncate checker output;
  never bump `file-length`/`claude-md-length` allowlists — surface warns to David.
- New IPC/DB enums: camelCase over the wire, snake_case tokens in DB, one owning `types.rs` via `token_enum!`.
- The structural MCP registry tests (`EXPECTED_TOOL_NAMES`, counts, gate table, wire-bytes snapshot) fail on any
  registry change by design — update consciously, never loosen. The new `access`-dimension set-equality test joins them.
- Register keyboard shortcuts **command-modifier-first** (`⌘⌥A`, not `⌥⌘A`): `formatKeyCombo` emits Command-then-Option,
  and an Apple-display-order string (`⌥⌘…`) resolves as native-menu-only and **never fires via JS dispatch**
  (`operation-log-shortcut.test.ts` guards this; it's why the op log is `⌘⌥L`). macOS still renders it ⌥⌘A in menus.
- A new IPC command must be registered in **both** `ipc.rs` and `ipc_collectors.rs` (the tauri-specta collector that
  regenerates `bindings.ts`), then get a `tauri-commands/` wrapper (never a raw bindings import).
- The reasoning provider-state blob is a **backend-only** DB column; it must never cross to the frontend (`MessageView`
  carries display parts only). This is both a privacy and a typing boundary.
- Folder and file names entering the context envelope and tool results are a **prompt-injection vector** (a crafted
  filename can carry instructions). Accepted as a bounded risk in v1 because the agent is read-only by construction with
  no content-read and no fetch channel — the blast radius is misleading text in the reply only, not action or
  exfiltration. Revisit when the first write or content-read tool arrives.
- `~/.cmdr/CMDR.md` is read-only in v1 (rules/memory machinery stays with the full agent); it goes in the stable prefix
  if present.
- Run the app via `pnpm dev --worktree ask-cmdr`; an FF-merge leaves the worktree dev data dir behind to delete by hand.
- Local `main` moves mid-effort; rebase before FF-merge, compile-gate each replayed commit, expect union conflicts where
  a feature threads a parameter through a shared seam.

### Follow-ups ledger

- **genai Anthropic thinking capture+replay** — a scoped local `[patch.crates.io]` patch (or upstream PR to issue #213):
  capture `{thinking, signature}` into a `ContentPart` and re-serialize a `thinking` block before `tool_use` on the
  assistant turn. Needed **before Tier-1-certifying Anthropic with thinking on** (newer Claude models default thinking
  on, so "just disable it" won't hold long-term). ~0.5–1 day; track upstream #213.
- **genai OpenAI-Responses reasoning round-trip** — parse `type:"reasoning"` items (incl. `encrypted_content`) and emit
  them back into `input` on replay with `include:["reasoning.encrypted_content"]`, `store=false`. Alternative pragmatic
  v1 stance: route OpenAI reasoning models via chat-completions and accept reasoning loss. Decide at certification.
- **Five live provider certification checks** — run with real keys in M8 (§11 Q7), re-verifying model ids at that time.
  M9's LLM call logging (dev-default-ON) is the natural companion for these runs — the on-disk request/response files
  make "what did we send, what came back" directly inspectable, so M8 may pull M9 forward to instrument them.
- **FTS5-as-net-new** — M2 adds the `fts5` rusqlite feature + trigger sync; there's no in-tree trigger pattern to copy,
  so review the trigger sync carefully.
- **Auto-retention** — deferred (transcripts small); the `operation_log/retention.rs` + `PruneRequest` scaffold is the
  template when real sizes exist.
- **Bulk model slot, summarize-on-overflow compaction, local slot beyond drop-in, content reading, external MCP servers,
  memory writes** — all explicitly deferred by spec §3; design the tool layer so external mounts could slot in later
  behind per-server consent and the same access gating, but build none of it now. **The interactive slot shipped (M8)**
  as `askCmdr.interactiveModel` layered over the shared `ai/` provider config; the bulk slot slots in beside it as its
  own additive key (`askCmdr.bulkModel`), no migration.
- **getcmdr.com needs two human-written Ask Cmdr additions before ship (website copy, principle 6 — NOT edited here).**
  (1) The privacy page needs a paragraph matching the app's consent screen (names/paths/metadata + the app-state
  envelope + attachments by reference, never file contents; chats local; optional local-only LLM logs). (2) The features
  page (`apps/website/src/pages/features.astro`) needs an `ask-cmdr` bento entry: `feature-status.json` has carried the
  `ask-cmdr` id since M6, so `website-build` / `analytics-injection` FAIL ("features page out of sync … Missing:
  [ask-cmdr]") until a human writes the title + description. This is a PRE-EXISTING red since M6, surfaced by the full
  check; left for David per the do-not-edit-website instruction. Re-translation surface if David changes the name or the
  consent copy: the `askCmdr.consent.*` keys + `askCmdr.title` + `commands.askCmdrToggle.*` +
  `settings.section.askCmdr`.
- **Verify the per-model price table at release** (`agent/pricing.rs`). The Tier-1 prices (USD per million tokens) are
  provisional and drift; re-check each provider's pricing page when certifying, and treat any model not in the table as
  honestly unpriced (tokens shown, cost "unknown", never a silent $0) rather than padding the table with stale guesses.
- **Five live cloud certification checks + the local-slot cert** — see §11 Q7. The local (llama-server) slot is
  keys-free and is the one cert runnable now; the five cloud checks (Anthropic thinking-off ≥3-step loop, Anthropic
  thinking-on 400, OpenAI-Responses reasoning degrade, Gemini parallel-call pairing, Gemini thoughtSignature loop,
  OpenAI-direct strict-schema 400) stay pending real API keys. M9's on-disk `llm-logs/` are the inspection companion for
  those runs.

## 14. Top risks for the reviewer to attack

1. **The read-only structural guarantee (approach settled; execution is the risk).** The lead chose Option A: extend the
   one authored registry with `consumers` + `access` dimensions (D49/D59), agent tools as `consumers:[agent]` entries,
   agent runtime dispatching only through `agent_tool_view()`, pinned by the set-equality + all-`Read` tests (M3/M4).
   The residual risks are execution details: (a) getting the wire-snapshot invariant right when agent-only entries are
   added and when the `operations_list`/`operations_get` pair is shared into `[ai_client, agent]`; (b) the `access`
   classification of the app-state snapshot path if it's reached through an `mcp` shim rather than an authored entry —
   the shim must stay read-only. M3/M4 is where a mistake would silently weaken the guarantee.
2. **FTS5 trigger sync correctness.** It's net-new (no in-tree pattern), and external-content FTS5 tables are easy to
   desync (a missed `'delete'` on update leaves orphan index rows). The M2 TDD list covers insert/update/delete, but
   this is the DB area most likely to hide a subtle bug.
3. **Reasoning-off degradation honesty end-to-end.** v1 runs Anthropic + OpenAI-Responses with reasoning off/minimal.
   The risk is the UI silently implying full reasoning, or a provider 400 leaking as an ugly error instead of a graceful
   typed notice. The genai gaps are source-verified but three of the five cloud live checks are still pending keys — the
   real-world break/degrade behavior (M8 certification) could still surprise.
