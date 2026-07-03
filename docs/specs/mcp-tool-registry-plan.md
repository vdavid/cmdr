# MCP tool registry: collapse the 4-way hand-synced bookkeeping into one source

Status: planning (review round 1 incorporated). Worktree: `.claude/worktrees/mcp-tool-registry`, branch
`worktree-mcp-tool-registry`.

## Problem

An MCP tool's contract is currently spread across four hand-synced places that nothing forces to agree beyond
count-asserting tests:

1. **Schema literals** in `mcp/tools.rs::get_all_tools()` — one `json!({...})` input-schema per tool, grouped into
   `get_*_tools()` category functions.
2. **Dispatch** in `mcp/executor/mod.rs::execute_tool()` — a `match name { … }` mapping each tool name to its handler.
3. **Auth classification** in `mcp/auth.rs::tool_call_requires_token()` — a `match name { … }` string list re-deriving
   which tools/params bypass the user's confirmation dialog and therefore need the bearer token. This is a live security
   footgun: add a destructive auto-confirm tool and forget this list, and it ships without a token gate.
4. **The TS validate-parse layer** in `apps/desktop/src/routes/(main)/mcp-listeners.ts` — per-event parsers
   (`parsePane`, `parseSortColumn`, …) that whitelist-check each emitted event payload before dispatching on the typed
   command bus.

Consistency across 1–3 is enforced only by `test_total_tool_count` / the per-category `*_count` tests: they catch "you
added a tool and forgot a place" only if the counts happen to diverge, and never catch "schema says param X, handler
reads param Y" or "you added a destructive tool and forgot the auth list."

## What we're NOT changing (hard invariants)

Read `mcp/CLAUDE.md`, `mcp/DETAILS.md`, and `mcp/executor/CLAUDE.md` before touching anything. The following are
deliberate designs, not debt:

- **The emit-and-ack UI-driving execution model stays byte-for-byte.** Every handler keeps its exact event name, payload
  shape, ack signal, timeout budget, and bespoke error messages. This is the "security via parity" design (agents drive
  the same UI a user does) plus the ack contract (`OK` means "the FE accepted the dispatched action"). We are collapsing
  the *definition / dispatch / auth bookkeeping*, not the execution semantics. (Task steer, and `AGENTS.md` §
  Principles.)
- **Handlers keep taking `&Value` and doing their own extraction/validation.** Several handlers do far more than parse:
  `move_cursor` normalizes `index` XOR `filename` → a single `to`, `select` has multi-mode range/names/all logic,
  `select_volume`/`tab` validate against live `PaneStateStore`, and every handler emits agent-facing error strings we
  must not regress (`"Provide either 'index' or 'filename', not both"`). See § Schema-derivation decision for why we do
  NOT introduce a serde params struct per tool.
- **Server ↔ auth ↔ executor layering.** `server.rs` consumes exactly two generic entry points — `get_all_tools()`
  (tools/list, server.rs:726) and `execute_tool(app, name, args)` (tools/call, server.rs:770) — plus the non-generic
  auth predicate `tool_call_requires_token(method, params)` (server.rs:584). `server` depends on `auth`, never the
  reverse. Don't disturb this. The registry must keep the auth predicate callable *without* a `Runtime` type parameter
  (it runs before dispatch, where no handler `R` is in scope — see § The generic/non-generic split).
- **Wire behavior is byte-identical.** Same tool names, same schemas (field names, types, enums, required-ness, key
  order), same auth decisions, same JSON-RPC error codes. Proven by a tools/list snapshot test (§ M0) plus the existing
  auth-decision tests kept green.

## The four surfaces, precisely (verified against current code)

- **Entry points into the MCP request path (server.rs):**
  - `tools/list` → `get_all_tools() -> Vec<Tool>` where `Tool { name: String, description: String, input_schema: Value }`
    (serde `rename_all = "camelCase"`, so `input_schema` serializes as `inputSchema`).
  - `tools/call` → `execute_tool<R: Runtime>(&AppHandle<R>, name: &str, params: &Value) -> ToolResult`.
  - auth gate → `tool_call_requires_token(method: &str, params: &Value) -> bool` (params = the JSON-RPC `params` object
    holding `name` + `arguments`). NON-generic.
- **32 tools** across 13 category functions in `tools.rs`; dispatch match arms in `executor/mod.rs` (grouped by
  category, delegating to `app.rs` / `view.rs` / `nav.rs` / `file_ops.rs` / `dialogs.rs` / `async_tools.rs` /
  `search.rs` / `downloads.rs`).
- **Auth cases** (the entire current classification): token required iff `method == "tools/call"` AND one of:
  `delete`/`move`/`copy` with `arguments.autoConfirm == true`; `dialog` with `arguments.action == "confirm"`;
  `set_setting` (whole tool).
- **Tests that read the surface:** `tools.rs` inline `#[cfg(test)]` (per-category counts + schema shape),
  `mcp/tests/protocol_tests.rs` (`test_total_tool_count` = 32, schema validity, no dup names),
  `mcp/tests/tool_category_tests.rs` (per-category existence + specific schemas), `mcp/tests/security_tests.rs` (name
  charset, bounded descriptions, no `fs.`/`shell.` tools), `mcp/tests/pane_state_tests.rs:161`, and the auth-decision
  tests in `auth.rs` (lines ~369–438).

## Target shape

One authored table where each tool is declared **exactly once**, bundling all facets. Adding a tool means adding one
entry; you cannot add an entry without supplying a schema, an auth classification, and a handler (they're required
fields / macro arms), and you cannot add a handler the dispatch doesn't know about. The count tests become redundant
guards over a property that's now true by construction.

### Recommended: a `macro_rules!` registry that generates all three Rust consumers

Author each tool once in a declarative table; the macro expands to the list, the dispatch, and the auth lookup, so the
three cannot drift. Sketch (final form decided in execution, reviewer-approved):

```rust
// mcp/tool_registry.rs  (new file; `tools.rs` becomes a thin re-export shim or is absorbed)

/// How a tool relates to the bearer-token gate. Pure, non-generic, unit-testable.
/// Reproduces `tool_call_requires_token` exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenGate {
    /// No token needed (reads, nav, search, prompting destructive ops).
    Open,
    /// Always gated (config mutation with no user confirmation): `set_setting`.
    Always,
    /// Gated iff `arguments.autoConfirm == true`: `copy`/`move`/`delete`.
    IfAutoConfirm,
    /// Gated iff `arguments.action == "confirm"`: `dialog`.
    IfConfirmAction,
}

impl TokenGate {
    pub fn requires_token(self, arguments: Option<&Value>) -> bool { /* mirrors current match */ }
}

mcp_tools! {
    //  name              schema-expr (verbatim json! moved from tools.rs)   gate            handler
    "nav_to_path"    => { schema: nav_to_path_schema(),  gate: Open,          run: |app, p| nav::execute_nav_command_with_params(app, "nav_to_path", p) },
    "copy"           => { schema: copy_schema(),         gate: IfAutoConfirm, run: |app, p| file_ops::execute_copy(app, p) },
    "set_setting"    => { schema: set_setting_schema(),  gate: Always,        run: |app, p| async_tools::execute_set_setting(app, p) },
    "quit"           => { schema: no_params_schema(),    gate: Open,          run: |app, _p| async move { app::execute_quit(app) } },
    // … all 32, grouped by category comments …
}
```

**Macro form — the one "does it compile" question (spike this FIRST in M1).** The `run:` arm must be treated as an
**ident-list + expression template inlined into each match arm**, NOT as a real closure that the macro constructs and
calls. A closure returning a future that borrows its `&AppHandle<R>` / `&Value` arguments hits Rust's
higher-ranked-closure-return limitation (`for<'a> Fn(&'a T) -> impl Future + 'a` is inexpressible → "cannot infer an
appropriate lifetime" / "hidden type captures a lifetime"). So the macro generates arms like
`"copy" => { file_ops::execute_copy(app, params).await }`, binding the author's `run:` idents (`app`, `p`) to the
dispatch scope's `app`/`params` by macro substitution (hygiene: the idents come from the invocation, so they resolve to
the generated `execute_tool` params). **All match arms must unify to `ToolResult` after `.await`**, so the table must
handle three handler shapes explicitly, not just the async-with-app common case:
- Async with app+params (most): `run: |app, p| file_ops::execute_copy(app, p)` → arm `execute_copy(app, params).await`.
- Async with params only (`search`, `ai_search` take no `app`): `run: |_app, p| search::execute_search(p)`.
- Sync returning `ToolResult` directly (`quit`, `switch_pane`, `swap_panes`, `remove_manual_server`): wrap so the arm
  still `.await`s — `run: |app, _p| async move { app::execute_quit(app) }`. Spell out all four sync tools in the table;
  don't discover them one compile error at a time.
If the spike shows the macro fights Rust more than it earns, take the § Fallback data-table route immediately.

The macro expands to:

- `pub fn get_all_tools() -> Vec<Tool>` — non-generic; maps each entry to `Tool { name.into(), description.into(),
  (schema_expr) }`. Descriptions: keep them in the table too (one `desc:` field per entry) so name+desc+schema+gate+handler
  all live on one line-group. **Preserve the exact current tool order** (category concatenation nav → cursor → selection
  → file_op → view → tab → dialog → app → search → settings → network → await → downloads, and the within-category
  order), since `get_all_tools()` order is serialized into the tools/list `Vec` and the M0 snapshot pins it.
- `pub fn tool_gate(name: &str) -> Option<TokenGate>` — non-generic; the single source auth reads.
- `pub async fn execute_tool<R: Runtime>(app, name, params) -> ToolResult` — generic; the generated dispatch (unknown
  name → the existing `ToolError::invalid_params("Unknown tool: …")`).

`tool_call_requires_token` in `auth.rs` becomes:

```rust
pub(super) fn tool_call_requires_token(method: &str, params: &Value) -> bool {
    if method != "tools/call" { return false; }
    let Some(name) = params.get("name").and_then(|v| v.as_str()) else { return false; };
    crate::mcp::tool_registry::tool_gate(name)
        .is_some_and(|gate| gate.requires_token(params.get("arguments")))
}
```

Layering stays intact: `auth` depends on `tool_registry` (a sibling data module), `tool_registry` depends on nothing in
`server`/`auth`. The generic/non-generic split is handled by the macro emitting a non-generic metadata path
(`get_all_tools`, `tool_gate`) and a separate generic dispatch (`execute_tool`) — see § The generic/non-generic split.

### Fallback if the macro proves unwieldy: paired data table + consistency test

If (during execution) the macro hurts readability more than it helps, fall back to:

- A non-generic `const`/fn table of `ToolMeta { name, description, schema: fn() -> Value, gate: TokenGate }` — collapses
  surfaces #1 and #3 into one authored list; `get_all_tools` and `tool_gate` read it.
- Dispatch stays a hand `match` in `execute_tool`, BUT add a test that asserts the dispatch and the meta table cover the
  exact same name set (every meta name dispatches to something other than the `Unknown tool` arm; every dispatchable
  name is in the table). This keeps #2 a second list but makes drift a hard test failure, not a silent count mismatch.

Prefer the macro (true single source, "ideal over cheap"); the fallback is the safety valve, not the goal. Decide with
the reviewer and at first sign of macro pain.

## Schema-derivation decision (deliberate: NO serde struct / schemars)

The task floated "typed serde params struct, JSON schema derived from the params (schemars or hand-rolled derive)." We
consciously do **not** do this. Rationale, to be challenged by the reviewer:

- **No `schemars` in the tree** (verified: absent from `Cargo.lock`). Adding it is a new dep (license-check per the
  `dependencies` rule) whose derived schemas are **not** byte-identical to today's: schemars emits `format: "int64"` /
  `minimum: 0` on integers, may hoist enums into `$defs`/`oneOf`, adds `$schema`, and orders keys its own way. That
  violates the byte-identical-wire goal and can change how agents read the tools, for a purity win.
  `test_select_tool_schema` even asserts `count` is a plain `"integer"`, *not* a `oneOf` — schemars would break it.
- **Handlers can't collapse to serde deserialization without regressing wire behavior.** The bespoke cross-field
  validation and agent-facing error strings (`move_cursor` index-XOR-filename, `select` modes, `select_volume`/`tab`
  live-state validation) are deliberate UX. serde's deserialize errors are not those messages, and much of the logic
  isn't "parse a struct" at all. Rewriting them is the execution-semantics change the task explicitly scopes out.
- **The footguns the task actually names are fully solved without it.** Schema/handler/auth drift and count-only
  enforcement die the moment name+schema+gate+handler are one authored entry. Typed param structs would buy marginal
  extra safety at the cost of wire drift, a new dep, and rewriting every handler — a bad trade here (`ideal-over-cheap`
  is about the ideal *end state*, and the ideal end state keeps wire + error quality intact).

So: **schemas move verbatim** (the exact `json!({...})` blocks relocate from `tools.rs` into the registry entries,
unchanged), giving truly byte-identical output. Optional, only if byte-identical is preserved and the reviewer likes
it: a tiny `pane_enum()` helper for the `{"type":"string","enum":["left","right"],"description":…}` fragment repeated
~15×. Descriptions differ per tool, so the helper would take the description — evaluate whether it actually reduces
noise without risking drift; if in doubt, leave the literals.

**`preserve_order` is NOT enabled** (verified: `serde_json` in the root `Cargo.lock` pulls no `indexmap`/`preserve_order`
feature), so `serde_json::Map` is a `BTreeMap` — schema keys serialize alphabetically at every nesting level,
deterministic and independent of authored key order. This makes byte-identical even stronger than "move verbatim": even
a `pane_enum()` helper or any source-level key reordering can't change the wire bytes, and the snapshot still catches
real drift (added/removed/renamed keys or enum members). Down-rank the "key-order drift" risk accordingly — but keep the
snapshot as the guard. (`Tool` is a struct, not a Map, so serde serializes its fields in *declaration* order — keep
`name`, `description`, `input_schema` in that order if `Tool` moves into `tool_registry.rs`.)

## The generic/non-generic split (the one real Rust subtlety)

`tool_call_requires_token` runs at server.rs:584 before dispatch, with no handler `R` in scope, so the auth path must be
non-generic. `execute_tool` is generic over `R: Runtime`. The registry must serve both:

- Metadata (name, description, schema, gate) is **non-generic** — `get_all_tools()` and `tool_gate()` need no `R`, and
  the existing tests call `get_all_tools()` bare (no runtime). Keep it that way.
- The handler is **generic** — stored/dispatched via the macro's generated `execute_tool<R>` match. Handlers stay `fn
  execute_x<R: Runtime>(&AppHandle<R>, &Value) -> impl Future`. The macro's `run:` arm wraps each into the match arm; no
  boxed async trait objects, no generic statics, no per-call registry allocation. (If the fallback data-table route is
  taken, the same split holds: `ToolMeta` non-generic, dispatch match generic.)

This is why a single generic `Vec<ToolDef<R>>` holding both metadata and boxed handlers is rejected: it would force
`get_all_tools`/`tool_gate` to name an `R` they don't have. The macro sidesteps it by generating two code paths from
one authored table.

## Milestones

Sequential, one agent. Commit per milestone with an impact-first message (no `Co-Authored-By`, no `Claude-Session`
footer). Send a one-paragraph progress note to `main` after each. Run `pnpm check --fast` while iterating; scope Rust
checks with `pnpm check rust` / `pnpm check clippy`.

### M0 — Characterization tests first (real red where it can be)

Goal: lock current wire behavior before refactoring, TDD-style for the security-sensitive auth bit.

- **tools/list snapshot.** Add a test (in `mcp/tests/protocol_tests.rs` or a new `tool_snapshot_tests.rs`) that
  serializes `json!({"tools": get_all_tools()})` and compares against a committed fixture string captured from *current*
  code. Generate the fixture from HEAD before any registry change (write a throwaway `#[test]` that prints it, or
  `serde_json::to_string_pretty`, capture, commit as the expected value). This is the byte-identical guard for M1.
  - Real red→green isn't meaningful here (it's a snapshot of existing behavior), so this is a characterization test,
    written-and-green. Note it as such (not fake TDD).
- **Auth-decision parity + structural anti-footgun (this is the TDD part).** The existing `auth.rs` tests
  (`requires_token_*` / `no_token_*`) already pin every decision; they must stay green through M3 — that's the parity
  proof. Add, RED-FIRST against the not-yet-existing registry API:
  - `tool_gate("copy") == Some(IfAutoConfirm)`, `tool_gate("set_setting") == Some(Always)`,
    `tool_gate("dialog") == Some(IfConfirmAction)`, `tool_gate("nav_to_path") == Some(Open)`, `tool_gate("bogus") ==
    None` — fail to compile / fail (no `tool_gate` yet) → green after M1/M3.
  - **Structural:** for every tool in `get_all_tools()`, if its schema's `properties` contains `autoConfirm`, then
    `tool_gate(name)` must be `IfAutoConfirm` (never `Open`). This is the "add a destructive tool, can't forget the
    gate" backstop. Red first (no `tool_gate`), green after wiring.
  - **Full-table expectation (must assert set-equality, not just per-name):** a test listing the expected `TokenGate`
    for all 32 tool names, asserting both that `tool_gate(name)` matches for each AND that the set of names in
    `get_all_tools()` equals the expected map's keys. Set-equality is load-bearing: it's the only thing that forces a
    conscious auth review for a *new always-gated* tool or a *new confirm-action* tool wrongly left `Open` (the
    autoConfirm structural test above only catches the auto-confirm class). Without the completeness assert, a 33rd tool
    left `gate: Open` would slip through. This is the residual footgun the full-table test closes.
- Docs: none yet.
- Checks: `pnpm check rust` (new tests compile; snapshot + parity green, structural/gate tests RED until M1).
- Commit: "MCP: characterize tools/list wire output and auth decisions before the registry refactor".

### M1 — Introduce the registry; re-implement `get_all_tools` + `tool_gate` over it

Goal: one authored table is the source for listing and auth metadata. Dispatch and `tool_call_requires_token` may still
be the old code at the end of M1 as long as everything is green — but prefer to land the macro generating all three at
once if clean (then M2/M3 become "delete the old code + prove parity").

- New `mcp/tool_registry.rs` (add `mod tool_registry;` to `mcp/mod.rs`). **Keep `tools.rs` as a thin re-export shim
  through M1** (`pub use tool_registry::get_all_tools;` and the `Tool` type), so server.rs:37 `use super::tools::…` and
  the four test files importing `crate::mcp::tools::get_all_tools` (`pane_state_tests`, `protocol_tests`,
  `tool_category_tests`, `security_tests`) need zero edits — minimizes churn and keeps the snapshot honest. Defer any
  file deletion / import-site rewrite to M4 (see open question 3). Define `TokenGate` + `mcp_tools!` (or the fallback
  table). Move every `json!` schema block verbatim.
- Re-point `get_all_tools()` to the registry. `tool_gate()` new.
- **The inline `#[cfg(test)]` tests in `tools.rs` MUST be rewritten, not just moved** — they call per-category functions
  (`get_nav_tools()`, `get_selection_tools()`, `get_view_tools()`, …) that the flat registry eliminates, so they won't
  compile via re-export. Rewrite each to look up tools by name in `get_all_tools()` (e.g.
  `test_select_tool_schema` → `get_all_tools().iter().find(|t| t.name == "select")`), and drop the now-redundant
  per-category `*_count` tests (the registry makes count-by-construction; `test_total_tool_count` in `protocol_tests.rs`
  stays as the single cheap guard). The external `tool_category_tests.rs` already only calls `get_all_tools()`, so it
  needs no change.
- **Verify:** M0 snapshot test green (byte-identical), all `tools.rs`/`protocol`/`tool_category`/`security` tests green,
  M0 structural/gate tests now green.
- Docs: update `mcp/CLAUDE.md` module-map line (`tools.rs` → registry) and `mcp/DETAILS.md` § Tools; hold the big
  "adding a tool" rewrite for M4.
- Checks: `pnpm check rust`.
- Commit: "MCP: single-source the tool list and auth classification through one registry".

### M2 — Collapse dispatch through the registry

Goal: kill the hand `match name` in `executor/mod.rs`; dispatch flows from the same registry entries.

- With the macro: `execute_tool<R>` is generated — **actually delete** the hand-written match in `executor/mod.rs` (keep
  the module's shared types/helpers: `ToolResult`, `ToolError`, `mcp_round_trip`, `user_path_param`, etc.; those stay).
  Don't leave the old match as a dead duplicate — the deletion is the point of this commit. (If M1 already landed the
  macro generating dispatch, M2 IS this deletion + the dispatch-coverage test.) With the fallback: keep the match but add
  the coverage-consistency test from § Fallback.
- Handlers stay in their category files, unchanged.
- **Verify:** every tool still dispatches (add/keep a test that calls `execute_tool` with each name against a
  `MockRuntime` and asserts it's not the `Unknown tool` error — or that the registry name set == a hardcoded 32-name
  set). The MCP has 13 test files; keep all green.
- Docs: `mcp/executor/CLAUDE.md` (the `execute_tool` dispatcher line + "Adding new tools" section point to the
  registry).
- Checks: `pnpm check rust`.
- Commit: "MCP: dispatch tool calls from the registry, removing the hand-synced match".

### M3 — Auth by construction

Goal: `tool_call_requires_token` reads `tool_gate`; delete the string list from `auth.rs`.

- Re-implement `tool_call_requires_token` as the 4-line registry lookup (§ Target shape). **Actually delete** the old
  `match name` body (not leave it dead). `TokenGate::requires_token` carries the per-arg logic. Note:
  `TokenGate::IfConfirmAction` exact-matching `arguments.action == "confirm"` is NOT a `no-string-matching` violation —
  that rule targets classifying errors/state from *message/stderr* substrings; here we read the tool's own typed input
  enum (schema enum `open|focus|close|confirm`), the same class as existing `mode == "brief"` reads, and lifting it into
  a typed `TokenGate` variant improves typing. No opt-out comment needed.
- **Verify (security-critical, re-run yourself per `verify-delegated-work`):** the full `auth.rs` `requires_token_*` /
  `no_token_*` suite green (parity), plus M0's structural + full-table gate tests green. This is the moment the footgun
  closes: a new destructive tool's gate is a required field on its registry entry, and the structural test fails if it's
  left `Open`.
- Docs: `mcp/CLAUDE.md` Auth must-know + `mcp/DETAILS.md` § Authentication — describe the gate as sourced from the
  registry `TokenGate`, not a separate list. Update the `file_ops.rs` comments that reference `tool_call_requires_token`
  as "the string list" if wording drifts (they reference it as the gate location — keep accurate).
- Checks: `pnpm check rust`.
- Commit: "MCP: derive the bearer-token gate from the registry, closing the add-a-destructive-tool footgun".

### M4 — Docs, TS decision, cleanup

- **`docs/guides/mcp-development.md`:** rewrite "Adding a new tool" — it's now ONE registry entry (name, description,
  schema, gate, handler) plus a handler fn in the right category file, instead of the current 5-step
  edit-tools.rs-then-executor.rs-then-count dance. Fix the stale bits: it lists `executor.rs` / `tests.rs` as single
  files (they're directories now), uses a wrong `43 → 45` tool-count example (actual is 32), and its "STDIO bridge"
  section describes a `cmdr-mcp-stdio` binary that **does not exist** in the tree (the only `[[bin]]` is `Cmdr`) — remove
  or correct that section. Show the before/after of adding a tool. (`docs/architecture.md` doesn't reference `tools.rs`
  and `docs/tooling/mcp.md` doesn't cover adding tools, so neither needs updating — confirmed.)
- **TS surface (#4): consciously deferred — document why.** `mcp-listeners.ts` is a separate transport adapter on the
  other side of the IPC boundary; its parsers validate *event payloads* (which stay byte-identical — we changed nothing
  emitted). Collapsing it into the Rust registry would mean generating TS from Rust (a codegen pipeline), which is a
  much larger, separate effort and out of scope. The event-payload contracts are unchanged, so the two sides stay in
  sync exactly as before. Record this in the plan's § Descopes and in `mcp/DETAILS.md` (a short "why the TS parse layer
  isn't in the registry" note) so a future reader doesn't think it was missed. (If, while in M1–M3, schema-deriving
  turns out to make a cheap TS tightening obvious, take it — but don't force it.)
- **Strip milestone tags** from touched code/docs per `execute.md`: grep the touched files for
  `\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b` and replace with descriptive references. Leave pre-existing
  unrelated ones (e.g. `dialogs.rs` "plan §5.7", `mcp-listeners.ts` "plan §3.11", "L12" — those cite *other* plans;
  don't churn them unless they're ours).
- **Update the count-test comments** (`test_total_tool_count`, the per-category `*_count` tests) to note the count is
  now a cheap guard over a by-construction property.
- Update `docs/specs/index.md` (add this plan under "In progress" while active; it gets wiped on ship per the specs
  README).
- Checks: full `pnpm check` (not `--fast`); then `pnpm check desktop-e2e-playwright` scoped to the MCP spec if possible
  (the `mcp-agent-tools` E2E spec) — see `test/e2e-playwright/CLAUDE.md` for single-spec iteration.
- Commit: "MCP: document the one-entry tool workflow and record the TS parse-layer deferral".

## Verification (lead-owned, per `verify-delegated-work`)

- Re-run the security- and data-safety-critical tests personally: the `auth.rs` token suite, the M0 structural/gate
  tests, and the tools/list snapshot.
- Read the actual diffs: confirm no handler's event/payload/ack/error changed, schemas moved verbatim, and the auth
  decisions are identical.
- Run the `mcp-agent-tools` Playwright E2E at the end (drives real tools over MCP) to confirm end-to-end parity.
- Rebase onto current local `main` before the FF-merge (do NOT merge/push — the task forbids it; leave worktree +
  branch in place and report).

## Byte-identical proof obligations (the checklist reviewers/verifier check)

- tools/list snapshot equal before/after (M0 fixture).
- Every tool name unchanged (dup-name + count tests).
- Every schema unchanged (schemas moved verbatim; snapshot covers it).
- Every auth decision unchanged (`auth.rs` suite green; full-table gate test).
- No handler behavior change (diff review: category files' logic untouched).
- JSON-RPC error codes unchanged (unknown-tool still `INVALID_PARAMS`).

## Descopes

- **TS `mcp-listeners.ts` parse layer** — deferred (see M4). Event payloads unchanged, so no drift introduced.
- **No `schemars` / typed serde param structs** — rejected (see § Schema-derivation decision).
- **No change to resources, transport, auth token lifecycle, ack contract, or any handler's semantics.**

## Risks

- **Macro won't compile** (the `run:`-as-closure lifetime trap). This is the top risk. Mitigation: the inline-expression
  macro form + the M1 spike-first note (§ Recommended); fall back to the data-table route at first sign of pain.
- **Macro readability.** Mitigation: the fallback data-table route; decide early with the reviewer.
- **Key-order / formatting drift in schemas.** Down-ranked: `preserve_order` is off (BTreeMap → alphabetical,
  deterministic), and `json!` blocks move verbatim; the M0 snapshot catches any real drift immediately.
- **Import churn** (`use super::tools::get_all_tools` at server.rs:37; `execute_tool` at :33; four test files import
  `crate::mcp::tools::get_all_tools`). Mitigation: keep `tools.rs` as a re-export shim through M1 (zero edits to server
  + tests); defer any deletion/import rewrite to M4.
- **Inline `tools.rs` tests won't compile after the category fns vanish.** Mitigation: rewrite them by-name in M1 (§ M1),
  not a re-export.
- **Generic/non-generic split** getting tangled. Mitigation: § The generic/non-generic split — metadata non-generic,
  dispatch generic, no boxed async, no generic statics.

## Open questions for the reviewer

1. Macro registry vs paired-table-plus-consistency-test — is the macro's single-source win worth its indirection here,
   given a 32-entry table? (Recommendation: macro.)
2. Is skipping typed serde param structs / schemars the right call given the byte-identical + zero-dep + custom-error
   constraints? (Recommendation: yes; documented rationale above.)
3. Should `tool_registry.rs` fully absorb `tools.rs` (delete the file, move the `Tool` struct) or should `tools.rs` stay
   as the `Tool` type + a re-export shim? **Resolved (reviewer):** keep `tools.rs` as a re-export shim through M1 for
   zero churn and an honest snapshot; if a clean full absorption (delete `tools.rs`, move `Tool`, rewrite the ~5 import
   sites in server.rs + four test files) is worth it, do it in M4 as an explicit final-state tidy — not mid-refactor.
```
