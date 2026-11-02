# AI tools details

Every AI-callable tool: the one authored table, the agent's read families, the `Propose` tier over them, and the two
`Memory` tools. Handlers shared with outside clients: `actions/DETAILS.md`. Must-knows: `CLAUDE.md`.

## The rule

**The agent can start anything. A person finishes anything sensitive.** There is ONE authored tool table (`table.rs`,
expanded by `mcp_tools!` from `registry.rs`), and each entry declares only what the tool does (`access`). `admit(caller,
access)` in `policy.rs` is pure, is the only thing deciding, and answers Allow, Ask, or Refuse:

- Agent, on a turn the user started: `Sensitive` asks, `DevOnly` refuses, everything else allows.
- Agent, on a turn Cmdr started (a wake, a follow-up): `Read`, `ReadContents`, `Propose`, and `Memory` allow; `DevOnly`
  refuses; the rest refuse `refusedInBackground`.
- Outside client, direct: everything allows.
- Outside client, not direct: `Sensitive` asks; `ReadContents`, `Memory`, and `DevOnly` refuse `directModeOff` naming
  `developer.mcpDirectMode`; the rest allow.

**There are no per-caller lists.** Every tool is reachable by both callers, or its entry carries a reason (`DevOnly`,
`Load::Meta`), pinned by `every_tool_is_reachable_by_both_callers_or_carries_a_reason`. `execute_tool` asks `admit` before
the schema check, a question, or a handler; `../agent/tool_gate.rs` asks it too, to shape a refusal as a tool result and
because `propose_rename_plan` goes around `execute_tool`. It's pure, so the two can't disagree.

The agent's declarations are every tool `admit` doesn't refuse on a user-started turn, whatever turn the call serves: a user
turn and a follow-up can share a thread, and those bytes are the provider's cache key. So a wake sees `copy` and hears
`refusedInBackground` if it calls it.

`tools/list` shows an outside client the whole table but `Meta` entries, each with annotations generated from `access`
(`Access::hints`): `Read` and `ReadContents` `readOnlyHint: true`; `Propose`, `Memory`, `Session`, `AppState`, and
`LightWrite` `destructiveHint: false`; `Initiate`, `Sensitive`, and `DevOnly` `destructiveHint: true`.

**Decision: a module, not a crate.** Crates here carry no `tauri`, and handlers need `AppHandle`, events, and pane stores,
so a crate would need a host abstraction over nearly every handler for little gain.

**What's left of the cycle.** `tools` still imports `agent::{store, suggested_ops, memory, chat::budget, types,
llm::types, AgentDb}`, because proposal and memory handlers write the agent's stores and the size contract shares the one
token estimator. `agent` imports `tools`, and `mcp` imports `tools` but no `agent`. The `tools-module-boundary` check pins
exactly that list.

## The table

Every AI-callable tool is authored once in the `mcp_tools!` table in `table.rs` — name, `id`, `brief`, description, JSON
schema, `subject`, `access`, `load`, and handler per entry. That one table generates `get_all_tools()` (the whole table but
`Meta` entries: `tools/list`), `execute_tool()` (the dispatch), `tool_access()`, and `ToolId` (the enum, `KNOWN`, the
wire-name conversions, and serde), so the facets can't drift and adding a tool is a single entry. `registry.rs` also
defines `Tool`, the `tools/list` element. Read the entries
for the exact schemas (don't transcribe them here); the sections below summarize behavior. Wire output is byte-identical
and pinned by `tests/tool_snapshot_tests.rs`.

**The per-entry columns that aren't dispatch.**

- `id`: the entry's `ToolId` variant. `KNOWN` is every entry, so a name parsing as known says nothing about who may run
  it: that's `admit`'s call.
- `load`: how the agent gets the schema. `Core` is declared in full on every call: the tools the everyday flows and the
  system prompt name, capped by a test. `Deferred` is one line in the prompt and the schema on request. `Meta` is a
  tool about tools, for the in-process agent only. § Lazy loading.
- `brief`: one line, at most 100 characters (test-capped), for a tool a model hasn't loaded in full. It describes what
  the tool does for anyone holding it, so it never names a parameter.
- `subject`: what the tool line names (`../agent/line/DETAILS.md` § The table's subject column). `Custom` is the only
  value that needs code outside the table.

**Directory split (`tools/`).** The table's `json!` schema blocks dominate the line count and serialize
identically wherever they live, so they're hoisted into `schemas/*.rs` (one `fn <tool>_schema() -> Value` per tool,
grouped by category) and the entries call them (`schema: schemas::copy_schema()`). `registry.rs` holds the mechanism (the
macro, `Access`, `Load`, `SubjectSpec`, the params gate), `mod.rs` re-exports what the macro generates, and `table.rs`
holds nothing but the authored entries, so the file that grows per tool is pure data. The
splits changed no wire bytes — the snapshot passes unmodified.

## Lazy loading (`meta.rs`)

Every provider call pays for every schema it declares, and no cross-provider standard defers one (MCP's `tools/list`
returns them whole, and `defer_loading` is Anthropic's and OpenAI's alone, absent from `genai` and local models). So the
agent declares the `Core` entries plus `tool_details` and `use_tool`, and the system prompt's `{{more_tools}}` gets one
generated line per other tool it can reach (`deferred_tool_lines`). An outside client keeps the full `tools/list`, and
`execute_tool` refuses the pair for it as an unknown name.

- **`tool_details(names)`** answers `{ tools: [{ name, description, inputSchema }], returned, total, truncated }`, under
  `MAX_TOOL_RESULT_TOKENS`, in the order asked, plus `unknown` for a name outside the table and `unavailable` for a tool
  `admit` never lets the agent run. It's an ordinary result, so a thread pays only for what it uses.
- **`use_tool(name, params)`** never runs. `AgentToolCall::effective` turns a well-formed one into the tool it names under
  the same `call_id`, and `run_turn` uses that before anything reads the call, so `admit`, the schema gate, Stop's class,
  the repeat breaker, `moves_the_turn_on`, the wake's `WakeToolWatch`, the evidence ledgers, and the rail see only the
  tool. `thread.rs` and `context.rs` read stored calls through it too.
- **The stored call stays what the provider sent.** The row and every replay keep `use_tool`, or a provider that signed it
  rejects the thread. `effective()` is derived and never persisted.
- **A malformed `use_tool` stays itself**, reaches `execute_use_tool`, and hears a typed problem: no `name`, a `name` that's
  one of the pair (nesting would hide the call from everything above), `params` that isn't an object. A key beside the two
  stays too, so the closed schema refuses it by name rather than the call acting on less than was asked. A name outside
  the table unwraps and refuses like any unknown tool.
- **A deferred tool called directly works**: it parses like any provider name, so "call image_facts again" in a stub holds.
- **Decision: `params` is `type: ["object", "null"]`.** Why: a property with no `type` makes llama-server refuse the whole
  request, so Cmdr's own local model answered nothing (`meta.rs` has the evidence). `set_setting`'s `value` still has none,
  and that's safe only while it's deferred: `every_declared_property_has_a_type_a_local_server_can_read`.
- **Decision: `use_tool`'s description names a few deferred tools.** Why: weak models read declarations and skim the
  system prompt. Without the names, `gpt-4.1-mini` reached `sort` on one try of two and Ministral 3B on none of three,
  making do with `list_dir`'s `sortBy` or wandering through `search` and `dialog`, then saying they'd sorted. With them,
  `gpt-4.1-mini` reached one eight times of eight, and Ministral 3B three times of four (verified on those models, live
  smokes, 2026-11-02). An `enum` of every name on `name` and the list at the end of the prompt didn't help.
- **Weak models skip `tool_details` and guess, so a refusal teaches.** `claude-haiku-5-5` calls `tool_details`;
  `gpt-4.1-mini` (the OpenAI preset's default) never did across eight rail threads, and Ministral 3B seldom does. So what a
  refusal says is what they read about the tool, and for a deferred tool (`meta::schema_for_a_refusal`, never a core one,
  whose schema went out with the call) it adds `tool: { name, description, inputSchema }`, the entry `tool_details`
  answers, capped at `MAX_REFUSAL_SCHEMA_TOKENS`:
  - the schema gate's refusal, by whichever road the call came (`use_tool`, flattened beside `name`, `params` that isn't an
    object, the wrapper skipped);
  - a handler's `INVALID_PARAMS` with no typed `data`, like `Unknown tab action: open`, since the gate reads keys and never
    a value. Typed `data` means something else stopped it (a person's no, a dialog in the way), so nothing's added.
- **A guessed name is pointed at the tool it wanted.** `meta::closest_tools` compares a guess's words to each tool's name
  and brief: `new_tab` lands on `tab`, `read_logs` on `read_resource`. `unknown_tool` answers `closest` plus the best one's
  schema, and `dialog`'s `type: tab` refusal names `tab`, since "open" reads as a dialog to Ministral 3B.
- **Decision: `tab` and `read_resource` stay deferred.** Why: three sentences do the job for 201 tokens a call, where the two
  in the core cost about 500. Two say where the plain request goes (the system prompt's logs and new-tab lines), and `use_tool`'s
  description names both with a gloss. Measured with `smoke_*_plain_requests` (real prefix, canned tool results), runs that
  reached the tool with arguments that do the job, before and after (verified 2026-11-02):
  - `gpt-4.1-mini`: new tab 3 of 3 before and 4 of 4 after, now on the first call. "Check Cmdr's logs: any warning?" 1 of 3
    to 4 of 4, and "check the logs" 0 of 3 to 4 of 4. Before, it searched files for "warning" or read `operations_list`
    and said the logs were clean.
  - Ministral 3B: new tab 1 of 3 to 5 of 5, after one wrong `dialog` call. The logs 1 of 3 to 5 of 5, and 0 of 3 to 5 of 5.
    Before, it searched the pane's folder and said it found no warnings.
  - In the running app, real handlers and the loop guard on: `gpt-4.1-mini` opened a tab three of three (one on the first
    call, two after the refusal handed it `pane`), read the logs five of five, and sorted once after `sortBy` corrected
    itself. Ministral 3B opened a tab after opening the About dialog on the way and one `dialog type: tab`, and read the
    logs two of two.
- **`cmdr://logs`'s `filter` is case-sensitive and levels read `WARN`,** so `filter=warning` finds nothing and a model reads
  that as clean. A model sent exactly that until the prompt line named `filter=WARN`.
- **What a sentence can't fix:** a model that answers in words without a tool call. The prompt's "never say you read
  something unless a tool result shows it" lowers it and doesn't end it; Ministral 3B also invents details around a real
  result.

## The schema gate (`params.rs`)

`validate_params(name, params)` checks a call against the tool's OWN declared schema before a handler reads a field off
it, at both dispatch entries: `execute_tool` for everything routed through the registry, and `agent/tool_gate.rs`
ahead of its branch, because `propose_rename_plan`, `propose_suggestions`, and `propose_server` need the thread's scope and never reach
`execute_tool`. Both run for an agent call; one small object checked twice costs nothing against a provider round trip,
and neither path has to know what the other does.

It reads ONE level of ONE object schema and answers two questions: is every `required` property present, and (only when
the schema closed itself with `additionalProperties: false`) does every property present appear in `properties`. The
refusal names each offender AND lists what the tool accepts, so one turn is enough to self-correct, and carries the
same facts typed on the error's `data` member.

**Why it exists.** Handlers pluck fields off the raw `Value` (`params.get("path")`), and providers don't enforce
schemas. A model called `list_dir` with `name: "penguin"` and `nameMatch: "prefix"`; both keys vanished, an ordinary
listing came back, and the agent told the user four times over that it had searched and found nothing. Fabricating a
zero is what `CLAUDE.md`'s honesty contract exists to prevent.

**Why it stops there.** Types and enums stay unchecked: a wrong type already gets an honest, specific refusal from the
handler that reads the field, so nothing was silent there. Nesting stays unchecked: `propose_rename_plan` and
`propose_suggestions` deserialize their rows into serde structs carrying `deny_unknown_fields`, which refuses an
unknown field and names it. What those structs can't do is name the ROW, which is why `check_object` is public and the
rename boundary points it at one row at a time (`propose/DETAILS.md`).

Every schema in the table closes itself, all the way down, pinned by
`every_tool_in_the_table_closes_its_schema_all_the_way_down`: nobody reads a call before it runs, its answer reaches a
person as a fact, and an old client's `rollback: true` must refuse rather than act on less than was asked. The gate runs
before the ask, so a stray argument on a `Sensitive` tool refuses before it reaches a person. A nested object the gate
can't reach checks itself: `inspect_file`'s `parse_find` runs `gate` on `find`, since a guessed `regexp: true` would
otherwise search literally and report a zero.

## The access classes

`access` says what the tool does, whoever calls it (decision D49: **one registry, no parallel table**):

  - `Read`: reads state.
  - `ReadContents`: reads inside files, or image-derived text (`inspect_file`, `image_facts`, `search_photos`).
    Exactly `Read` for the agent.
  - `Propose`: stages a proposal, described below.
  - `Memory`: writes only the agent's memory folder.
  - `Session`: opens a session to a place the user saved, with stored credentials (agent invariant 7, § The `Session`
    tier below).
  - `AppState`: changes what's on screen, never data.
  - `Initiate`: opens a dialog or editor a person confirms.
  - `LightWrite`: writes with no confirmation, the ONE written exception (`tag`, and `mkdir` and `mkfile` with a name).
  - `Sensitive`: runs after a person confirms an Ask.
  - `DevOnly(reason)`: never the agent, and an outside client only in direct mode (§ Direct mode).

  Every class except `Read` and `AppState` is hand-allowlisted (`EXPECTED_*_TOOL_NAMES`), and `DevOnly` is pinned by name.

⚠️ **`Access::Memory` was a deliberate widening of the app's central agent-safety invariant.** "The agent never changes
anything" became "the agent writes only into `<data-dir>/ai/memory/`". Three things hold the narrowed promise: the jail
in `../agent/memory/` (relative `.md` paths only, no `..`, no symlink along the chain, containment re-checked against a
canonicalized parent); the hand-authored `EXPECTED_MEMORY_TOOL_NAMES` allowlist, which exists for the same reason the
`Propose` one does (no structural check can prove a handler stays in a jail, so a human puts each name there having
read it); and `admit`, which keeps an outside client out off direct mode, since nobody's agreed to a filesystem write over
HTTP. Depth: `../agent/memory/DETAILS.md`.

## Who's calling (`Caller`, `ToolCtx`)

`policy.rs`'s `Caller` is `Agent { origin }` or `External { direct }`. Dispatch builds a `ToolCtx { caller, cancel,
conversation_id }` for each call: `agent/tool_gate.rs` from the turn, `mcp/handler.rs` from `ToolCtx::external` with the
client's disconnect as its cancel. A handler that needs it takes the table's `ctx_params` shape.

- `Caller::initiator` is who the operation log credits: `Agent` → `agent`, `External` → `aiClient`, whoever confirms it.
- `Caller::guarded` is everything but `External { direct: true }`. What a guarded caller opens must arm before a keystroke
  can touch it (`apps/desktop/src/lib/ui/DETAILS.md` § The started-elsewhere guard), since someone may be typing elsewhere
  as it appears. Direct mode is the way out: a person said this client may go past them.
- `ToolCtx::stamp` puts both on an event's payload as `initiator` and `guard`. Every opener of a dialog or editor uses it,
  and so does a direct create. `mcp-listeners.ts` reads them failing closed: anything but `agent` is `aiClient`, ❌ never
  `user`, and only a plain `false` turns the guard off.
- A guarded `dialog open onboarding` refuses (`onboardingNeedsPerson`): the wizard isn't a `ModalDialog`, so nothing arms
  it, and it holds the AI setup. A guarded `open_search_dialog` always opens in `filename` mode and says so: its default
  is `ai` with `autoRun`, the nested LLM call `ai_search` is `DevOnly` for.
- **The write commands take a required `initiator`**: `create_directory`, `create_file`, `rename_file`, `move_to_trash`,
  `move_files`, `delete_files`, `trash_files`, `copy_between_volumes`, `move_between_volumes`, `rename_by_move`,
  `compress_files`, and `rollback_operation`. A missing one was recorded as `user`, so provenance failed open.
  `cancel_write_operation` has none on purpose: `apps/desktop/src/lib/file-operations/DETAILS.md` § Rollback asks first.

## The human gate (`ask.rs`, `ask_plans.rs`)

A `Sensitive` tool's table entry names an `ask:` plan, and `execute_tool` runs it through `ask::gate` after the schema
gate and before the handler, only when `admit` answers Ask, so direct mode never reaches it. A test pins plan and
`Sensitive` to each other, both ways.

- **A plan only reads, and runs twice**: once to word the question, and again after a yes. Different facts, or a plan
  that no longer asks, refuse with `data.refusal: "changedWhileAsking"`. So every `Sensitive` tool gets the re-check for
  free: `eject`'s volume, `set_setting`'s old value, `open_under_cursor`'s path.
- **`AskFacts` is typed, and every string in it is `fact_text`**: no control or bidirectional-override characters, since
  `cod\u{202E}fdp.command` would read as a PDF, and bounded by dropping the middle, since a name's end says what it does.
  `OpenFile.path` is the one exact string, kept for comparing; `shownPath` is what a person reads.
- **Answers**: `declined`, `expired` (five minutes), `cancelled` (the turn's Stop, or the client going away), and
  `noWindow`, each in `data.refusal`; another dialog in the way is `data.blockingDialog`. One question at a time, in
  `ToolAskRegistry`: a second is `blockingDialog: "tool-ask"`. A reload wakes the wait (`register_known_dialogs`), and
  the drop guard takes the question down however the wait ends, a dropped future included.
- **Decision: raw `emit_to` for the two events**, with the specta structs naming the wire and the types. The specta
  helper panics when events aren't mounted, and one of these sends runs in a destructor.
- **Decision: `open_under_cursor`'s last check is in the frontend**, in `entry-activation.ts::openForTool`, at the moment
  it opens. The plan's re-read leaves a gap a concurrent `move_cursor` could use, so the yes crosses as `expectedPath` off
  `ToolCtx::asked`. With no question asked, a guarded caller sends `mayLaunch: false`: a folder under the cursor at plan
  time must not become a `.command` at open time, launched with nobody asked. `resolveActivation` is the one decision
  chain, shared with Enter, so "what would this do" can't drift from what it does. A document package set to Browse is
  still a file and launches, which is why the plan reads the activation and never the policy.
- **Decision: `set_setting` asks unless the registry says otherwise.** `mcp-describe-setting` reports the facts and
  `setting_plan` decides, so a setting outside the registry refuses, a `mcpAskFree` one needs nobody, and a write that
  changes nothing needs nobody. `mcpSettable` and managed locks refuse first, ask-free or not, and they stay in
  `mcp-main-bridge.ts` too, because direct mode skips the plan.
- **Stop**: `stop_may_abandon` keeps `Sensitive`, `Initiate`, and `LightWrite` to their own bounded end. Stop while the
  question is up answers through the token; Stop after a yes lets the handler finish.
- `dialog_confirm tool-ask` is a direct client's yes, and `dialog close tool-ask` is a no. The dialog's half:
  `apps/desktop/src/lib/tool-ask/DETAILS.md`.

## Direct mode

The switch that lets an outside client use the tools that go past a person (`policy.rs`). The agent never gets it.

- **On** when an E2E or capture run happens in a build that honors it (`E2E_HONORED`: debug or `playwright-e2e`), or when
  `developer.mcpDirectMode` is on. A release build ignores the environment, since `CMDR_E2E_MODE` is readable in every
  build.
- **Off**, a `DevOnly`, `ReadContents`, or `Memory` tool, and `multi_rename`'s media fields, refuse an outside client with
  `data: { reason: "directModeOff", setting: "developer.mcpDirectMode" }` (`admit`). `tools/list` still lists them.
  `mcp/handler.rs` reads an atomic per request, so a toggle applies to the next call.
- **On**, an outside client's successful content read lands in the security log, by tool and never by path
  (`McpContentRead`).
- **Decision: the JSON value never turns it on by itself.** A same-user process with no Full Disk Access can write
  `settings.json` or launch Cmdr with a crafted `CMDR_DATA_DIR`, then read `~/Documents` through Cmdr's Full Disk Access.
  So launch seeds the atomic only when the JSON says on AND `secrets`'s `mcp.directMode` item reads through
  `store().vouching()`. It reads the item only when the JSON says on, so nobody else gets a Keychain touch at launch. A
  fallback file store vouches for nothing, so there the toggle lasts the session and the row says so.
- **Gaps in this check are tracked privately** and reviewed before every release. The check itself:
  `../secrets/DETAILS.md` § Vouching. The user-facing account: `docs/security.md` § MCP direct mode.
- **The Settings row is the only writer** (`McpServerSection.svelte`, `set_mcp_direct_mode`). The setting is
  `mcpSettable: false`, and the row shows what `get_mcp_direct_mode` answers, so a JSON value alone doesn't read as on.
  Each real change, and a launch that comes up on, lands in the security log.
- **A reset takes the item with it.** The row's reset reaches `set_mcp_direct_mode(false)` through
  `McpServerSection`'s listener, and a recovered `settings.json` clears it at launch (`seed_direct_mode_at_launch`).
- **A launch that comes up on tells the person** (`AccessNotice::McpDirectModeOnAtLaunch`), so direct mode is never on
  quietly. It's raised in `setup()`, so the main window drains it through `take_security_notices_from_launch`.

## The `Session` tier

`upgrade_smb_to_direct` moves a share that's already mounted to a direct session, and it's an outside client's tool.
`connect_volume` connects a saved place without taking a pane over; `select_volume` on one (a pinned server or share in
`cmdr://state`) switches a pane to it, and the pane brings it to life itself (`place-connect.svelte.ts`, through
`servers/connect-flow.ts`), with the sign-in sheet for anything missing. Handler and rules: § Connecting a saved place.

## The `Propose` tier

**The invariant: the agent can propose; only the user can approve.** Approval originates in the frontend as a user
action. There is no tool, and never will be a tool, that approves a proposal. Without that, `Propose` is `Write` with
extra steps. A `Propose` tool stages a proposal and opens a review surface; it mutates nothing — no filesystem write, no
silent config mutation. That is its entire power.

Three things make it hold rather than merely be intended:

- **A hand-authored allowlist, not inference.** No structural check can prove a handler doesn't mutate, so
  `EXPECTED_PROPOSE_TOOL_NAMES` (in `tests/tool_registry_tests/access.rs`) lists every `Propose` tool by name, and
  `test_propose_tools_are_an_explicit_allowlist` fails if a registry entry in EITHER view is tagged `Propose` without
  being listed: the agent's `propose_rename_plan`, `propose_suggestions`, and `propose_server`, and the ai-client's
  `multi_rename`.
  Widening that power remains a deliberate act a human signs off, having read the handler.
- **No proposal reaches the confirmation bypass.** `autoConfirm` and `queue`'s `rollback` let an external MCP client
  skip the user's confirmation dialog — exactly the approval a proposal must never grant itself.
  `test_no_agent_tool_reaches_the_confirmation_bypass` asserts every agent-view tool declares no bypass parameter in
  its schema.
- **Payloads are capped, per tool.** A `Propose` payload must be bounded the way `image_facts` caps at 200 paths: a
  proposal the user can't actually review is one they can only rubber-stamp, which dissolves the invariant. The cap
  can't be enforced generically (payload shapes differ), so each `Propose` tool honours it explicitly and pins it with
  its own test. `multi_rename` caps a proposal at 10,000 renames: a pattern rename is reviewable by its rule plus a
  paged sample, which hand-picked names aren't.

**Consent is unaffected.** Proposals flow agent → user, never to the provider. `Propose` adds no egress, so the
provider-egress question and `CLOUD_AI_CONSENT_VERSION` are unchanged by this tier.

**Param naming is camelCase** (`tabId`, `timeoutSeconds`, `sizeMin`, `autoConfirm`). Tool names stay snake_case. Don't add snake_case params; agents pattern-match across tools and every inconsistency is a guessed-wrong call.

## Why the TS parse layer stays separate from the registry


The `mcp_tools!` registry single-sources the three **Rust** consumers (list, dispatch, auth gate). The frontend's
per-event validate-parse layer in `apps/desktop/src/routes/(main)/mcp-listeners.ts` is deliberately NOT folded in. It's
a transport adapter on the other side of the IPC boundary: it validates the *event payloads* the handlers emit, not the
tool schemas, and those payloads are unchanged. Folding it into the registry would require generating TypeScript from
the Rust table (a Rust→TS codegen pipeline) — a much larger, separate effort, out of scope here. The event-payload
contracts stay in sync across the boundary exactly as they did before the registry existed, so nothing was lost by
leaving this side hand-written.

## Key decisions about the surface

### Why agent-centric API?

The original design mirrored keyboard shortcuts (43 tools like `nav_up`, `nav_down`). This forced agents to make dozens of calls to find a file. The agent-centric redesign consolidated to semantic tools (`move_cursor(index=42)`, `nav_to_path("/Users")`). This reduced round-trips from 6+ reads to 1 (`cmdr://state` resource).


Tool results and resource content are consumed by LLMs, not parsed by code. Output doesn't need to be JSON, YAML, or any structured format. Anything that reads well to a human and is concise works. Tool results are plain text (`"OK: Navigated to /Users"`, aligned columns for search results), resources use YAML or plain text. Errors are still JSON-RPC error objects, but the `content` field is plain text. Optimize for readability and token efficiency, not parseability.

### Why stateful architecture?

Without state, resources would need to query the frontend on every read (slow, async). Storing state in Rust allows synchronous reads. The frontend syncs state after meaningful changes (file load, cursor move, selection).

### Why no file system access?

Security via parity: agents can only do what users can do. Giving agents `fs.read`/`fs.write` would violate this. Agents navigate the UI just like users, using `move_cursor`, `open_under_cursor`, etc.

### No `open_select_dialog` twin

**Decision.** We expose `open_search_dialog` but NOT an `open_select_dialog` counterpart for the AI-selection dialog.

**Why.** Coverage: the `select` tool already selects by names, index range, or all, and `ai_search` already runs a natural-language query — together they cover what the Selection dialog does programmatically, so a dialog-opener adds little. The Selection dialog IS registered now (`selection-add` / `selection-remove`), so `dialog close` reaches it and an opener would ack on `SoftDialogAppeared` the same way `open_search_dialog` does; what's left to build is an FE prefill listener plus the tool/handler/tests. Add it when an agent actually needs to hand a human a pre-filled selection to review, not for symmetry.

### No `duplicate` tool

**Decision.** The Duplicate command (⌘D, the context menu, the File menu) gets no MCP tool of its own.

**Why.** Coverage again: `copy` into the folder the items already live in IS a duplicate, and an agent gets there by pointing the other pane at that folder, which it can already do with `nav_to_path`. A dedicated tool would add a second name for one operation and a second path into the same engine, for no capability the surface lacks. The human command exists because a person shouldn't have to arrange two panes to duplicate a file; an agent has no such trouble.

### Why no file-content read tool (`read_file`)

**Decision.** No tool or resource returns a file's raw bytes. `inspect_file`, `image_facts`, `search_photos`, and `multi_rename`'s media fields are the bounded ways in, and an outside client gets them only in direct mode (§ The rule).

**Why.** File-content access is the privacy + prompt-injection surface that needs real guardrails: size caps, per-read budgets, a sensitive-path denylist, per-read logging, and explicit consent. The denylist is § The sensitive-content gate. Don't add a `read_file` tool or a `cmdr://file?path=` resource.

## Tests (`tests/`)

Directory module split by test category:

- `tool_registry_tests/`: the `mcp_tools!` table's structural tests, one file per property pinned: `schemas.rs` (the tool
  set and each declared schema), `access.rs` (the access classes and what `admit` makes of them across the table), `schema_gate.rs`
  (`validate_params`). Kept out of `table.rs` so the authored table stays lean
- `tool_snapshot_tests.rs`: the exact `tools/list` bytes against `fixtures/tools_list_snapshot.json`
- `tool_category_tests.rs`: tool existence by category, schema checks
- `ack_system_tests.rs`: the primitives the ack contract relies on
- `resource_tests.rs`: resource URI validation, count, mime types (the public `get_all_resources` surface)
- `resource_state_tests.rs`: `cmdr://state` builder: URI/query parsing, pane/tab/file formatting, and exact
  compatibility-policy bytes for recent listing failures
- `resource_log_tests.rs`: `cmdr://logs` builder: option parsing, line selection, `since` filter, and exact
  compatibility-policy bytes
- `resource_indexing_tests.rs`: `cmdr://indexing` builder: duration/number formatting helpers
- `resource_operations_tests.rs`, `resource_operations_tree_tests.rs`: `cmdr://state` `operations:` builder

## The tool catalog

Each handler is `async fn(&AppHandle<R>, &Value) -> ToolResult` (the `app_params` macro shape; `search` is the only one here
authored `params_only`, since it needs no `AppHandle`), reuses a shipped core, and returns a typed serde shape as the
tool-result JSON the model reads.

- **`app_state`** (`read/state.rs`) — both panes (path, cursor item, selection count, view/sort) plus the volume list.
  Built from `PaneStateStore` (`get_focused_pane` returns the SIDE; the path comes from that side's state) +
  `snapshot_volumes`. Not the private `build_state_yaml` — typed data, not parsed YAML. `selectedEntries` lists the exact
  selection only when it's complete AND fits one result; otherwise it's absent with a typed
  `selectedEntriesOmitted` (`outsideWindow` / `tooMany`), never a half list that would read as the whole selection.
  `selectedCount` is always honest, and `list_pane_files` pages the names.
- **`list_pane_files`** (`read/pane_listing.rs`) — up to 200 compact rows from the focused pane's existing Rust
  listing cache. It uses the current selection when present, otherwise the whole folder, and returns the folder as a
  `location` (the exact volume ID plus the one shared parent path `propose_rename_plan` needs). It never queries the
  index or starts a filesystem listing.
- **`list_dir`** (`read/listing.rs`) — takes `path` + optional `volumeId`, resolved
  once (§ The location contract), then reads THAT volume's index: its immediate children (`Index::list_children`) plus
  its own recursive size stats (`Index::dir_stats`) and a `Coverage` block naming `source: index`. The result echoes
  `location`. A folder no index lists (an unindexed share, or ground the index hasn't reached) is read live once
  instead, `source: live` (§ The live source); a resolved volume that isn't connected answers from the index its last
  walk left on disk, `source: offlineIndex` (§ The honesty contract), or `volumeNotConnected` when that index doesn't
  list the folder, and an unplaceable one the resolver's miss. Ordered by `sortBy` (`name` / `size` / `modified`) and paged by `limit` / `offset`;
  `type` narrows to files or folders. **`sortBy: "size"` is the disk-usage answer for ONE folder's children**;
  `search` with the same sort ranks a whole drive — see § One tool, both questions.
  Every number also arrives spoken (`sizeHuman`, `modifiedHuman`, `recursiveSizeHuman`, `totalHuman` /
  `availableHuman`) and a paged answer carries a `remainder` — see § Numbers arrive already spoken.
- **`search`** (`tools/actions/search.rs`) — find files across ONE whole drive by name
  pattern (glob or regex), size range, modified range, or type, over the same live path a person's search takes
  (`search::run_live_collected`: the index where it covers the scope, a walk for the rest, so an unindexed drive still
  answers). `sortBy` is relevance / size / modified, `limit` defaults to 30 and clamps to 200, and `countOnly` answers
  with the counts and coverage alone. The typed result shape and every rule about its counts: `tools/actions/DETAILS.md`
  § The search result.
  - **Names and metadata only, ❌ never contents.** The chain for a contents question is `search` (or `search_photos`)
    to narrow, then `inspect_file` with `find` over the hits. A `modified` is when a file last CHANGED, never when it
    was saved or opened, which the model has to voice rather than round off. Names, paths, sizes, and dates are
    already on the consent screen, so `search` widened no egress and needed no `CONSENT_COPY_VERSION` bump.
  - **One drive per call, and the model loops.** `resolve_target` refuses a scope spanning volumes
    (`ScopeError::SpansMultipleVolumes`), because a fan-out is the only way a search can silently omit a drive. The
    default is the boot volume; another drive is named by its `volumeId` (no scope covers its whole `root`), which
    also reaches a volume that isn't connected, from its index (§ The location contract). The description obliges
    the model to say WHICH drive it covered before offering the others.
  - **What the coverage block obliges the model to say**, field by field: `stillWalking` and `abandonedGround` make the
    list a lower bound (❗ never "no matches"), `permissionDenied` names refused folders and offers Full Disk Access
    only where it would help, `declined` explains rather than offers (snapshot trees are nothing to fix),
    `stillCovering` says those results arrive later rather than being lost, `unresolvedScopes` is ❌ never "that folder
    doesn't exist" (Cmdr can't tell a typo from unwalked ground), and `hiddenByExcludes` says the count is filtered.
    The default system/cache/build tier is right for "find my invoice" and exactly wrong for "where is my disk space
    going", where the hidden folders ARE the answer. The system prompt's § Coverage carries the rule; the authored
    sentences ride in `notes` beside the flags.
  - **❌ No `offset`, by decision.** Ranking is top-k, and an offset over a re-ranked run would skip and double-count
    rows. The model narrows instead (a tighter pattern, a smaller scope, a date range), and the description says so.
  - **❌ `ai_search` is deliberately NOT in the agent view.** It spends an LLM call turning prose into a structured
    query, and Ask Cmdr is already an LLM holding the user's prose: exposing it would nest a second model call, bill
    two providers, and hide a translation the agent can't see or correct, for a schema that rides every turn. The agent
    writes the structured query itself.
  - **❌ No drive-wide content index**, until something measures the gap the `search` → `inspect_file` chain leaves. It
    would be a second index with its own enrichment pass, storage budget, and incremental reconcile, on the scale of
    `media_index`.
- **`important_folders`** (`read/importance.rs`) — top-N or above-threshold across scored volumes, reusing
  `tools::resources::importance::{snapshot_top, snapshot_threshold, snapshot_overview}` (which read offline volumes too).
  Every row is a `location` on its volume's root (§ The location contract), with a terse `why` (its top three signals)
  and `folded` (near-duplicate descendants merged into it). `volumeId` scopes to one volume; `under` (a folder, on
  `volumeId` when given, else on the live volume holding it) narrows to one subtree, whose own children then get rows,
  and echoes back as `under`. The overview carries each volume's current generation for staleness, and `unplaced: true`
  for a store scored under an id no volume carries (the Bonjour-addressed share): its rows have no root to stand on, so
  the read skips that store (`RankedScope.among`), ❌ never a row with an invented root. Folding, `why`, and `under` live
  in the snapshot layer: `tools/resources/DETAILS.md` § `cmdr://importance`. An empty ranking carries `empty`
  (`EmptyReason`): no scored volume, a kind never scored (MTP, SFTP, ...), no drive index, still on its first pass (no
  rows AND generation 0; generation alone lies, since only a full pass stamps it), `under` floors, nothing reaches
  `threshold`, or nothing scored there. A named volume with no store answers this way too, ❌ never an `invalid_params`.
- **`folder_importance`** (`read/importance.rs`) — `path` + optional `volumeId`; the resolver picks ONE volume and only
  its store answers (`snapshot_path_on`), so `/papers` on one share never answers for another's. Scored (score +
  `Explanation` breakdown + `stale` from asOf vs the volume's current `recompute_generation`), Floored (with reason), or
  Unscored, each with its `location`. Offline-capable: a named saved share resolves and its store still answers.
- **`inspect_file`** (`read/inspect/`) — "what's in this file?" for up to 200 paths in one call. Each row: metadata
  (`sizeBytes` + `sizeHuman`, `modified` + `modifiedHuman`), the extension's `mime` beside `content.kind` (so a lying
  extension shows), and a typed `content` per kind. Text is a line window (`startLine` + `maxLines`, default 200, max
  2,000, capped at 16,000 chars and 2,000 chars a line) read through the viewer's own backends, with `encoding`,
  `totalLines` when known, `lineNumbersApproximate` on the ByteSeek fallback, and `truncated` / `linesCut` on the
  window, plus `scanIncomplete` + `bytesScanned` when the private-key check's deadline cut it. With `find: { query, regex?, caseSensitive? }` the window is replaced by `find`: the matching lines (up to
  50, each `{ line, matches, text }` with `text` a 300-char snippet around the first match), `totalMatches`,
  `matchesCapped` (the viewer's 10,000-match cap), `returnedLines` / `truncated`, and `scanIncomplete` with
  `bytesScanned` / `totalBytes` when the deadline stopped the scan. One `find` applies to every text and PDF path in
  the call. A PDF gives `version`, the exact `pageCount`, the Info dictionary's `title` and `author`, and `pages`: a
  page window (`pageStart` + `maxPages`, default 3, max 20; `{ page, text, truncated, unparseable? }` per page, each
  page trimmed and cut at 8,000 chars, whole pages only up to the row's 16,000, with `returnedPages` / `truncated`, and
  `scanIncomplete` + `pagesScanned` when the private-key check's deadline cut it),
  or with `find` the matching lines each carrying `page` (`line` counts within the page; `scanIncomplete` +
  `pagesScanned` when the deadline left pages unchecked). `hasTextLayer` is `false` when every
  decoded page was whitespace (a scan), absent when none decoded; `textUnavailable` says `encrypted` (no strings read
  either), `tooLarge` (over 64 MiB, never parsed), or `unparseable` (the parser refused or panicked, contained), with
  the header `version` still answered. Images give `format` + dimensions, the camera's `exif` when the container carries a block (`dateTaken`,
  `cameraMake`, `cameraModel`, `lens`, `orientation { value, spoken }`, `exposureTime`, `fNumber`, `iso`,
  `focalLength`, and `gps { latitude, longitude }` in decimal degrees: a photo's coordinates are a home address, and
  they egress on request), and point at `image_facts`; an archive (`.zip`, tar, 7z), or a directory inside
  one, lists its immediate children (`format`, `inner`, up to 200 `entries` with `isDir`, `size` + `sizeHuman`,
  `modified` + `modifiedHuman`, `encrypted`, plus `total` / `returned` / `truncated` and `hasEncryptedEntries`), and a
  FILE inside an archive is read as its own kind through the viewer's bounded temp; empty and binary carry metadata
  only. Per-path statuses: `ok` / `folder` / `missing` / `noVolumeAtPath` /
  `unreadable { permission | io | encrypted | corrupt | unsupported | tooLargeToExtract }` / `unreachable` /
  `unsupportedVolume` / `protected { reason, note }` (§ The sensitive-content gate), and every row echoes the
  `volumeId` it's on (§ The location contract). The call reports
  `total` / `returned` / `truncated` and names every path with no row in `unanswered`. One of two disk readers among the handlers (the other is `list_dir`'s live source); how it reads and how it times out: § Reading a
  file the way the viewer does.
- **`list_volumes`** (`read/volumes.rs`) — the map a model plans from: `{ volumes, favorites }`. Every volume with
  `root`, `indexStatus` (`fresh`/`scanning`/`stale`/`off`) and, for a remote one, `connectionState`
  (`direct`/`os_mount`/`disconnected`/`needs_sign_in`/`needs_host_key_approval`/`saved`), the tokens straight from
  `snapshot_volumes_from` so they can't drift. A DEVICE row carries `deviceReadiness`
  (`ready`/`waiting_for_authorization`/`unavailable_offline`/`unavailable_no_permissions`) instead: ❗ a phone waiting
  for its "Allow USB debugging?" tap is listed, so without it the row reads as browsable and every call against it is a
  refusal you can't explain. Presence is not a session, which is why it isn't on `connectionState`.
  Space rides along as `totalBytes` / `availableBytes` plus `totalHuman` / `availableHuman`, each pair present exactly
  when the poller has a reading (the same pair `cmdr://state`'s `volumes:` renders; see `resources/DETAILS.md`).
  - **`root`** is the resolver's own (`VolumeFact::root`, the one root rule below), so a row and a `list_dir` on that
    volume can't disagree about where its paths start: a mount point, a scheme root (`sftp://…`, `mtp://…`), or for a
    volume that isn't connected the root it last had (its `connectionState` says so). Absent only on a row no location
    lives on (the `Network` hub). It's also the scope a `search` of that drive names. ❌ No `mountPath` here: a saved
    share's LAST mount point read as "where it is" is what sent the model to a dead `/Volumes/naspi`. `cmdr://state`'s
    YAML keeps `mountPath` (an ai-client contract).
  - **The offline vocabulary.** A volume that isn't live, whose index database is on disk with a completed scan,
    reads `indexStatus: stale` plus `asOf` (that scan's date, spoken), the same pair `list_dir`'s offline coverage
    voices; with no database it stays `off`. The date comes off the index's read-only opener through the facts shell
    (`IndexSources::stored_as_of`), never by registering the volume. A live volume's status is its index's own.
  - **Rows of one share sit together**, most recently connected first (`known-shares.json`'s `lastConnectedAt`, rows
    matched by share name ignoring case), so the address that worked last is the one a model meets first. Ordering
    only: it claims nothing about the rows being one share.
  - **`favorites`** are folders, not volumes, so they left `volumes` (where they read as `kind: local`, the phone one
    included). Each is `{ name, path, location?, reachable? }`: the saved `path` always, a `location` when it resolves
    as a bare path to a connected volume, and `reachable: false` when nothing connected holds it.
  - **Two sources, one per kind of fact.** `VolumeSummary` is shared with `cmdr://state` and the envelope; `root`,
    `asOf`, and the favorites' locations come from `VolumeFacts` in the pure `to_volume_map`, so a field added for this
    tool can't reach the YAML by accident. `app_state` embeds the same map's `volumes`.
- **`operations_list` / `operations_get`** — the shipped executors (`tools/actions/operation_log.rs`), shared into the
  agent view unchanged (their schemas + coverage flags already fit an agent reader).
- **`search_photos`** (`tools/actions/photos.rs`) — photo search by description (CLIP),
  in-image text (OCR), or Vision tag. Shapes the `media_index` read API into a TEXT-ONLY DTO (path + volume + typed
  `matchKind` + optional score + optional OCR snippet / no image bytes), reuses `media_index`'s own `volume_state` for
  per-volume coverage honesty, and returns a typed status when indexing is off, still building, or the CLIP model isn't
  installed. Privacy: the OCR snippet + tags it returns are image-derived text that egresses to the provider — named in
  the cloud AI disclosure (see `tools/actions/photos.rs` and `docs/security.md`). **Watch**: when it answers
  "image indexing is off" and the contents half is what the user asked for, the model should offer to turn indexing
  on. The transcript that prompted `search` had it answer "off" eight times and then invent name-search arguments. If
  transcripts show it still doesn't offer, the fix is `search_photos`'s and `search`'s descriptions plus one prompt
  line, ❌ not a third tool.
- **`image_facts`** (`tools/actions/image_facts.rs`) — the lookup direction of the same
  index: given paths the agent already has, the FULL stored OCR text (capped at 2,000 characters per file, a cut
  flagged) plus the Vision tags for each. It accepts up to 200 paths but answers as many as fit one result (see § The
  size contract), reporting `total` / `returned` / `truncated` so the caller batches the rest. Backs naming/describing files the user is looking at. Same text-only DTO, same coverage
  honesty (it reuses `photos.rs`'s helpers), and a typed per-path `indexed` / `notIndexed` so a not-yet-enriched file
  is never read as an empty one. Privacy: this is the widest derived-content egress the agent has (full recognized
  text, not a snippet) — same cloud consent gate, same disclosure.

- **`list_suggestions` / `get_suggestion_group` / `propose_suggestions`** (`suggestions/`) — the suggested-ops surface
  over the proposal spine: what the agent has already put in front of the user (summaries and counts, never op rows),
  one group's ops paged, and the one tool that stages a sweep or amends a pending group. Access classes, the
  resolve-check-write order, the selector schema, and the last-opened gap: `suggestions/DETAILS.md`.
- **`propose_server`** (`suggestions/server.rs`, `Access::Propose`) — stages a server to save, or a change
  to a saved one, as a zero-op group on the same spine. No secret, no contact with any server, only on a turn the user
  started, and an edit can't move a server or change its account. `suggestions/DETAILS.md` § Proposing a server.
- **`nothing_to_suggest`** (`quiet.rs`) — one argument, a short `reason`, and no effect at all. See § A tool that is
  only a signal.
- **`memory_write` / `memory_edit`** (`memory.rs`) — the two `Access::Memory` tools. See § The two tools that write.
- **`connect_volume`** (`connect.rs`, `Access::Session`) — brings a place the user saved to
  life by `volumeId`. See § Connecting a saved place.

## Connecting a saved place (`connect_volume`)

Turns "the NAS isn't connected" from a dead end into one call, without any power an injected file name would want: the
schema is `{ volumeId }` and nothing else, so it can't add a host, take a password, or approve a key. The gate admits
it only on a turn the user started (§ The dispatch gate); the promise is agent invariant 7 in `../agent/DETAILS.md`.

**One move per standing, the frontend's own** (`apps/desktop/src/lib/servers/DETAILS.md` § The three arms), because
the wrong move is silent: re-dialing a registered volume registers a SECOND one, and dialing one the backoff loop owns
races it.

- Serving (or a local disk) → `alreadyConnected`, no dial.
- Registered and `disconnected` → no dial. It emits `VolumeReconnectRequested`, which the frontend's
  `smbReconnectManager` answers with the same `startCycle` a pane landing there makes, and waits up to `RECONNECT_WAIT`
  (10 s) on `file_system::volume::manager::connection_news()` (subscribed BEFORE the emit) for the outcome:
  `connected`, `needsCredentials`, `needsApproval`, `rejected` (a refused guest session), or `stillReconnecting`, whose
  detail tells the model not to poll. With no window listening, the wait ends `stillReconnecting`, which is true.
- Registered and `needs_sign_in` / a host key to approve → `needsCredentials` / `needsApproval`, no dial.
- Not registered (a `saved` row) → the only dial: `saved_servers::connect_saved_place` with no secret and no
  account, so a missing Keychain password answers `needsCredentials` before anything mounts, and every NetFS mount is
  `NoUI` anyway (`network/CLAUDE.md`). A Keychain access prompt on a user-started turn is acceptable (invariant 7).
  `connected` echoes the root it landed on, which may have moved (`naspi-2`).

**One share, one connect** (`location/same_share.rs::connect_target`, on the same `redirectable` proof every read
uses): a member whose same-account sibling is serving answers `alreadyConnected` with the sibling's `location` and
`via` naming the volume asked; with none serving, the dial goes to the member connected most recently
(`known-shares.json`'s `lastConnectedAt`), with `via` when that isn't the one named. Recency is the one signal for "the
address that works from here"; nothing sniffs networks.

**Shared with the pane's dial.** Every saved-place dial is one flight per place (`network/dial_flights.rs`): a rail
call during a pane's Enter shares that dial's outcome instead of registering a second volume. Stop drops the call,
which leaves the flight; the dial is called off once nobody waits on it, so Stop never ends a dial the pane still
wants. The handler races the turn's token itself, so a stop reads `outcome: cancelled`.

**The result is always an answer**, `{ outcome, location?, via?, reason?, detail }`: `reason` is the dial's own outcome
name for `rejected` / `unreachable` / `needsApproval`, mapped exhaustively in `connect.rs::classify` (no `_ =>`, so a
new `ServerConnectOutcome` is a compile error there). `refusedInBackground` is the gate's, never the handler's.

**Not shared with AI clients.** `select_volume` on a saved place already brings it to life for them: the pane lands on
it and `place-connect.svelte.ts` dials, with the sign-in sheet for anything missing. `apps/desktop/src-tauri/src/tools/DETAILS.md` § The `Session`
tier.

## A tool that is only a signal (`nothing_to_suggest`)

Every other entry here answers a question or stages a proposal. This one exists so a wake can say "I looked, and none
of it is worth your attention" in a form the code can read.

**Why typed rather than phrased.** A wake that finds nothing must leave no thread behind, and deciding that from the
model's wording would classify control flow by text — what `error-string-match` forbids, and what breaks on the first
copy edit or non-English reply. The call resolves to `ToolId::NothingToSuggest`, and that is what the wake path matches
on.

**Why the handler is inert.** A wake and the rail declare the same tools, so the rail sees this tool too. A handler that
deleted the conversation wouldn't be `Read`,
and it would delete a USER's thread the moment a rail turn called it. So the handler acknowledges and returns; the
delete lives on the wake path, after the turn (`agent/wake/`), and a rail turn calling this changes nothing —
`wake/tests/job.rs` pins that.

**Why the `reason` never reaches a log.** It exists for the agent's own memory and is trimmed to
`MAX_REASON_CHARS`. ❌ It must never be logged verbatim: `cmdr.log` ships inside error reports, including the
auto-dispatched ones the user never previews, and `redact::RedactionContext::redact_line` is path-shaped, so it does
nothing to a sentence about which of the user's folders were boring. Log that a wake was quiet, never what it said.

**What it costs everyone else.** The schema is prefix, so all 21 declarations are paid on every rail turn: this one is
97 tokens of the fixed overhead (`agent/chat/DETAILS.md` § What the budgets buy). That's the price of the wake
being able to stay silent, and it's why the description is two sentences.

## The two tools that write (`memory_write`, `memory_edit`)

Everything either tool DECIDES lives in `../agent/memory/` — the jail, the two caps, the edit's uniqueness rule — so this
file holds only root resolution (`memory::store_for`, which reads the app data dir) and result shaping. That split is
not tidiness: there is no Tauri mock runtime in the tree and every registry handler takes an `AppHandle`, so a rule
placed here is a rule no test can reach. `../agent/memory/DETAILS.md` § Testing shape.

**A refusal is an `Ok` carrying a typed token, ❌ never a `ToolError`.** `view::dispatch` flattens a `ToolError` to
`{ "problem": <sentence> }`, and a model that has to read prose to learn its memory is full will keep writing into a
folder that is saving nothing. So the result is `{ saved: false, refused: <token>, detail: <sentence> }`, where
`refused` is `MemoryRefusal::token()` and the sentence names a next move — a refusal with no next move gets retried
verbatim. Same rule as `error-string-match`, pointed at the model instead of at our own code.

A landed write answers `{ saved: true, path, bytes, remainingBytes }`. `remainingBytes` is what the folder has left
against its 64 KB disk cap, so the model can see pruning coming rather than discovering it at a refusal.

⚠️ **Both are callable from the RAIL, not only from a wake.** "Remember that I keep invoices by year" is what the
folder is for. It is also the mechanism behind the injection risk the prompt fences against
(`../agent/memory/DETAILS.md` § The injection surface), so it is stated rather than implied.

## Reading a file the way the viewer does (`inspect_file`)

The tool re-derives nothing the viewer already ships. Per behavior, the symbol it calls (`read/inspect/mod.rs`,
`text.rs`, `pdf.rs`, `archive.rs`):

- **Head**: one 64 KB read per file. `encoding::detect_from_head` takes all of it; the classifier takes the first
  `content_kind::CLASSIFY_HEAD_LEN` bytes of the same buffer.
- **Kind**: `file_viewer::content_kind::classify_viewer_content(head, None, true)`. `ext = None` keeps SVG on the text
  path (its markup says more to a model than "an image"); `is_local = true` because the row is already a local file.
  `Image` → `content_kind::media_mime` for the format and `media::read_image_dimensions` for the size (`None` for HEIC).
  `Pdf` → `pdf.rs` (below).
- **EXIF** (`exif.rs`): for a JPEG / TIFF / HEIC / PNG / WebP row (by the format above; GIF and BMP carry none and are
  never opened for it), a second open through `exif::Reader::read_from_container` on a `BufReader<File>`
  (`kamadak-exif` needs a seekable reader: HEIF boxes point across the file), then the pure `exif_facts`. `dateTaken`
  is `DateTimeOriginal`, else `DateTime`, the camera's own `YYYY:MM:DD HH:MM:SS` with no time zone invented (a blank
  or malformed date is absent); the strings come from the raw ASCII value, not `display_value` (which quotes and
  escapes); the numbers are `display_value().with_unit()` ("1/250 s", "f/2.8", "400", "50 mm"); `orientation`
  is the 1–8 code plus a spoken twin from a fixed table (a code outside it is absent); `gps` needs BOTH coordinates
  (three rationals each, signed by the `Ref`, a zero denominator or an out-of-range value drops it) and rounds to six
  decimals so the JSON carries no float noise. No block, a block that doesn't parse, and a block with none of these
  fields are all "no `exif` key", never an error row. Inside an archive the same branch runs on the extracted
  temp, so a photo in a zip carries its EXIF too.
- **Text vs binary**: `content_kind::looks_binary(head, encoding)`, the seam the viewer doesn't need (it leaves the
  warning to the FE's extension list). UTF-16 is never binary; a NUL or a control-byte share over 5% is.
- **Encoding**: `encoding::detect_from_head` → `FileEncoding::label()` is the string the row carries. Never
  `String::from_utf8_lossy` on the raw bytes: that read every UTF-16 file as binary once.
- **The backend**: `file_viewer::headless::open_text_backend(path, encoding, cancel)`: FullLoad up to 1 MB, else a
  LineIndex built under the cancel flag, falling back to ByteSeek with `line_numbers_exact = false` when the deadline
  flips the flag (`file_viewer/DETAILS.md` § Headless reads). No session, no watcher, nothing to tear down.
- **The window**: `backend.get_lines(Line(startLine - 1), maxLines + 1)`, under a fresh cancel flag (the path's own
  is often already set by the deadline that stopped the line index; `find`'s per-hit line reads do the same). The extra line says exactly whether more
  exist, on every backend, without leaning on `total_lines`. `window_from_chunk` (pure) joins with `\n`, strips one
  trailing `\r` per line (the backends keep it on CRLF files), cuts a line at `MAX_LINE_CHARS` (`linesCut`), stops at
  `MAX_WINDOW_CHARS` (`truncated`), and answers a past-the-end `startLine` with an empty, un-truncated window (the
  exact backends clamp such a target to the last line; the shaper must not present that line as line 50). `totalLines`
  counts the trailing empty line after a final newline, as the viewer's line numbers do, so "line 812" means the same
  thing in both places.
- **`find`** (`find.rs`): `file_viewer::Matcher::build(query, SearchMode { use_regex, case_sensitive })` once per call
  at param time (a `MatcherBuildError`, invalid or cross-line regex, is `INVALID_PARAMS` carrying the matcher's own
  text), shared by every path through `TextAsk::Find(Arc<Matcher>)`. Per text row: `headless::open_scan_backend`
  (FullLoad, or ByteSeek with no index: a scan streams from byte 0 and numbers lines exactly, so an index would only
  read the file twice), then `backend.search(matcher, cancel, matches, progress)`, the viewer's own loop, capped at
  `MAX_SEARCH_MATCHES`. Matches are grouped by line in arrival order, the first `MAX_FIND_LINES` (50) lines are fetched
  by `SeekTarget::ByteOffset(match.byte_offset)` (exact on every backend; `Row(n)` is a guess on ByteSeek), `\r`
  stripped, and cut by `snippet_around` to `FIND_SNIPPET_CHARS` (300) around the first match, a third before it, with
  `…` at each cut end. The match column is UTF-16 (the viewer's JS-facing unit) and goes through
  `range_read::clamp_utf16_offset_to_byte`, the one UTF-16→byte conversion in the tree; read as a char index it lands
  twice as far along a line of emoji. `matchesCapped` is `totalMatches ≥ 10,000`; `scanIncomplete` is a flag-stopped
  scan that didn't reach the cap or the end (`bytesScanned` / `totalBytes` say where, spoken twins beside them). A
  `find` row never sets `lineNumbersApproximate` (nothing in it is estimated) and has `totalLines` only for a FullLoad
  file. Non-text rows are untouched by `find`.
- **PDF** (`pdf.rs`): the one kind the viewer doesn't decode for us, so it rides `pdf-extract` 0.12.0 (which
  re-exports `lopdf` 0.42, so page tree, header, and Info dict come through the same crate) with every parser call
  inside `crash_reporter::contain_panics` (`crash_reporter/DETAILS.md` § The one exemption): the closures wrap the
  foreign calls only, never our shapers. Order: `header_version` over the classifier's head bytes (ours, so the version
  survives a refused file), the 64 MiB `MAX_PDF_BYTES` gate (over it, `tooLarge` and no read), `std::fs::read`,
  `Document::load_mem`, `get_pages()` (its length is the exact count; a tree that panics the parser is `unparseable`),
  `is_encrypted()`
  (→ `encrypted`, page count kept, Info strings not read: they're ciphertext), then `Title` / `Author` through
  `doc.dereference` + `decode_text_string` (PDFDocEncoding or UTF-16, trimmed, blank is absent). Page text is
  `output_doc_page(&doc, &mut PlainTextOutput::new(&mut buf), n)`, one page at a time, under the deadline, every page up
  front for the private-key check and kept in memory so the answer decodes nothing twice; a refusal or a contained panic
  marks that page `unparseable` and the loop continues. ❗ So does a page whose
  `Parent` chain loops (`parent_chain_ends`, checked before the parser sees the page): `pdf-extract` resolves inherited
  `Resources` / `MediaBox` by recursing up `Parent` unguarded, a stack overflow `catch_unwind` can't contain (found by
  the `pdf` fuzz target, `fuzz/DETAILS.md`). `window_from_pages` and
  `find_in_pages` are pure over an `extract(page)` closure (tests inject page texts): the window trims each page, cuts
  at `MAX_PAGE_CHARS` (8,000: two dense pages per row; a whole page the model can re-ask for by number beats a slice
  it can't, since there is no offset inside a page), carries whole pages until the next would break
  `MAX_WINDOW_CHARS` (the first always fits); `find` runs the call's `Matcher` per
  trimmed line of each page in order, groups by line with the first match's byte offset feeding `snippet_around_byte`,
  and carries the first `MAX_FIND_LINES`; `matchesCapped` at the viewer's `MAX_SEARCH_MATCHES`
  is its own reason to stop, as on text. Both read only the pages `prescan_pages` checked, and `answer_from_pages` says
  so through `scanIncomplete` + `pagesScanned` when the deadline left some unchecked. `hasTextLayer` is a verdict over decoded pages only. The whole file is read
  into memory (bounded by the gate); nothing is cached between calls.
- **Connected remote files**: `VolumeManager::mount_id_for_path` identifies the registered owner before a
  scheme-prefixed path can be refused. A virtual path with no owner → `unsupportedVolume`; a connected owner whose
  `paths_are_os_visible()` is false streams through `materialize_for_inspect` into the shared bounded temp, then runs
  the ordinary per-kind pipeline. Mounted direct SMB stays on `std::fs` even though `supports_local_fs_access()` is
  false; once its mount disappears, the same volume streams through SMB. The 256 MiB cap refuses from the source stat
  before pulling, and the runner's cancel flag stops a pull between chunks and removes its partial temp. A materialized
  row carries no `modified`: the temp's fresh mtime would be a lie about the source.
- **An OOXML document inspects as an ARCHIVE, deliberately.** `.docx` / `.xlsx` / `.pptx` / `.jar` / `.apk` are
  browsable archive suffixes, so `inspect_file` on a Word document returns its PARTS (`[Content_Types].xml`, `word/`,
  `docProps/`) rather than treating it as one opaque file. **Decision/Why**: there is no Office-document reader
  anywhere in the agent or viewer, so the alternative answer is "binary, unreadable" — the part listing is strictly more
  useful, `word/document.xml` is genuinely where the text lives, and the whole path is read-only. It follows from the
  suffix table rather than from anything here, and it is NOT gated on the user's `behavior.archiveEnter.ooxml` setting:
  that setting steers the Enter KEY, while this is the tool's own read. Egress is unchanged — entry names, as for any
  archive. If an Office reader ever lands, revisit this branch first.
- **Archives** (`archive.rs`): the pane's own routing, before any `std::fs`. A path with an archive-named component
  (`cmdr_fs::archive_format::archive_boundary_candidate`, a pure string check) goes through `VolumeManager::resolve(volume_id,
  path)` (`block_on` from the blocking thread, as `materialize` does): the shared boundary detector confirms the
  format by name and magic bytes and hands back the on-demand `ArchiveVolume`, or a passthrough for a mislabeled
  `.zip`, which then reads as text or binary. The row is then built from the archive's cached index
  (`ArchiveVolume::index()`, the seam the volume opened for this: `FileEntry` has no `encrypted` field, and
  `list_directory` is a thin map over the same nodes; the key is the inner path the boundary candidate split off): a
  directory node (the root, or one inside) →
  `Content::Archive` from `index.list(inner)`, cut at `MAX_ARCHIVE_ENTRIES` (200), dirs first as the pane lists them,
  format from `ArchiveFormat::label()`; the archive root's row metadata is the `.zip` file's own `std::fs` stat, an
  inner directory's `sizeBytes` is absent (never a zero). A file node → refused `unreadable { encrypted }` from the
  node's flag BEFORE extraction (the tool has no password path), else
  `materialize::extract_if_routed_for_inspect(path, volume_id, cancel)` streams it to the shared bounded temp (the same
  256 MiB refuse-before-extract cap; `TooLargeToPreview` → `tooLargeToExtract`, `ViewerError::Archive { failure:
  Unsupported }` → `unsupported`, every other archive failure → `corrupt`),
  `read_content` runs the normal per-kind pipeline on `temp_file` (so `find` and the window work inside a zip), and
  `TempCleanup` removes `cleanup_dir` in `Drop`, so an early return or a panic can't leak it. A zip inside a zip is
  `binary`: the boundary is the leftmost archive component, as in the pane. The parse errors map typed:
  `NeedsPassword` (a header-encrypted 7z) → `encrypted`, `IoError` (a damaged structure) → `corrupt`, `NotSupported`
  (the archive layer's unsupported-codec / non-archive / over-cap collapse) → `unsupported`: an unsupported codec is
  not a damaged file. `materialize` preserves that same distinction when the codec refusal appears only at EXTRACT
  time. The extract step is injected (`ExtractFn`) so the tests shrink the cap and watch the temp dir.
- **Every other route** (`inspect_routed_path` in `mod.rs`): a file in a repo's virtual `.git` trees has no inode to
  `stat` either, so once the archive branch declines, `volume::manager::path_routes_over_its_parent` gates the same
  cancellable route extraction and the row is built from the temp with the ordinary per-kind pipeline. It reports no
  `modified`: the temp was written a moment ago, and quoting its mtime would date a years-old commit as today. A path
  the confirm rejects (a mislabeled `.zip`, a `.git` that isn't a repository) falls through to the plain `std::fs`
  pipeline, and so do the REAL files under `.git/`, which are the parent volume's and keep their own mtime. Snapshot
  contents egress like any other file contents, which the cloud AI disclosure already covers.
- **Statuses from I/O**: `NotFound` / `NotADirectory` → `missing`; `PermissionDenied` → `unreadable { permission }`
  (EACCES and a Full Disk Access refusal are one kind of `std::io::Error`, so the enum doesn't pretend to tell them
  apart); anything else, including a read that panicked → `unreadable { io }`.

**The runner (`runner.rs`).** Every path is its own `spawn_blocking`, `PATH_CONCURRENCY` (4) in flight
(`buffer_unordered`, slots re-sorted into request order). A path's budget is `PATH_TIMEOUT` (5 s) in two phases: the
cooperative window, then the deadline flips the path's `AtomicBool` and waits `CANCEL_GRACE` (1 s) for a partial,
flagged answer (a LineIndex still scanning falls back to ByteSeek in milliseconds), then the row is `unreachable` and
the task is dropped, which detaches it. A thread stuck in a kernel call cannot be cancelled: the tool ABANDONS it,
holding a blocking-pool thread until the syscall returns (the same posture as `commands/file_viewer.rs`'s
`blocking_viewer_op`), so `unreachable` never means "we stopped reading". `CALL_TIMEOUT` (20 s) bounds the call: past
it no new path starts, and each unlaunched path is an empty slot. The policy is a `RunnerConfig` value and the per-path
work an injected `InspectFn`, so the tests drive both phases with millisecond budgets and no hung mount.

**The `unanswered` contract.** `shape_ok` runs `fit_to_result_budget` over the rows that exist, then names every
requested path with no row in the kept prefix: cut by the size ceiling, never launched, or abandoned without a row.
`total` is the paths asked, `returned` the rows carried, `truncated` is `returned < total`. The model joins rows back
by `path` and asks again for exactly `unanswered`.

**Text-only by construction**: no field on any row can hold bytes; `tests::every_row_shape_is_text_only_no_byte_fields`
walks the serialized result and requires every leaf to be a string, a number, or a flag.

## The sensitive-content gate

`sensitive.rs` is the one gate for every path that yields file contents or image-derived text, for every caller. A
prompt-injected file name could otherwise get `~/.ssh/id_rsa` or Cmdr's `mcp.token` read and sent to the provider. It
blocks contents only: names in these folders may still flow. A new tool that yields contents goes through it.

**The rules** live in `RULES` and `CREDENTIAL_FILE_NAMES`: case-folded, NFC-normalized component runs at any depth, so a
NAS backup of a home folder counts. File names match the last component alone, since a `.env` FOLDER is a Python venv.
Browsers stay anchored under `Library/Application Support` or `.config`, because a bare `Notes`, `Chrome`, or `Arc`,
case-folded at any depth, would refuse `~/Documents/notes/*`. `com.veszelovszki.cmdr*` matches every instance's data dir,
so a dev build can't read the prod `mcp.token`.

**Three entry points, by what the caller can afford:**

- `classify(path)`: pure. The whole check for `mtp://`, routed, and `.git` portal paths, and for the DB-only photo tools.
- `path_gate(path)`: also checks `config::standalone_app_data_dir()` (a custom `CMDR_DATA_DIR` has no telling component)
  and the path after following symlinks. It resolves the longest prefix that exists, which is what makes
  `~/link.zip/inner` and `~/link-to-ssh/gone` resolve at all. Blocking, so it runs only under a deadline.
- `path_gate_within(path, budget)`: the same on a thread it abandons at `budget`, failing closed, for a caller with no
  deadline of its own. `multi_rename`'s loader joins its reads, so a hung resolve there would wedge the call.

**Where each tool checks:**

- `inspect_file`: `inspect_path_with` runs `path_gate` first, on the runner's blocking thread, before anything opens the
  file or materializes an archive entry. `foo.zip/.ssh/id_rsa` is caught on the virtual path. A hung mount makes the row
  `unreachable`, which reads nothing either.
- `image_facts`, `search_photos`: `classify` on stored paths. They run DB-only and promise a dead NAS can't hang them, and
  that's enough: `facts_for_paths` matches paths exactly, so a symlink's path only finds what the index stored under that
  same path. `image_facts` answers a `protected` row. `search_photos` drops the hit and reports `protectedHidden`, since a
  snippet in a hit would leak, and widens its read when hiding left a volume short: the read API's own limit would
  otherwise let a hidden hit eat the answer.
- `multi_rename`: `gated` wraps the loader's reader, so media fields (`tag.comment`, `tag.title`, EXIF) are gated where
  the file is read, on wakes too since `Propose` runs there. A refusal is `ReadError::NotSupported`, not `Io`: a folder of
  refused files mustn't read as a dying volume. The row reads `unreadable`, and `protectedFiles` says why.
  - **Gotcha/Why:** taking an open sheet's answers (`adopt_where`) runs no reader, so it would walk past the gate. The
    tool only takes answers when the folder passes the gate and the file is neither protected nor a symlink.

**The private-key check** (`refuse_private_key_text`, both armor lines; `cmdr://logs` reads it too) catches a copy in `~/Desktop/backup` that no path
rule can see. A hit refuses the whole file, and neither `startLine` nor a `find` needle can skip it. The deadline bounds
the check, so "whole file" holds up to where it stopped, and nothing past that comes back:

- **Text, window and `find`** (`text.rs`, `find.rs`): `prescan_for_private_key` reads the whole file first, through the
  backend's decoder, with its own matcher: `backend.search` returns matches only, so there's no seam to watch the user's
  scan. A head scan isn't enough (it misses a key past 64 KB, and a UTF-16 key slips through raw bytes), and neither is a
  look behind a window (an armored block's body can run past any fixed number of lines). If the deadline stops the scan
  (`StoppedAt`), hits and window rows past its offset are dropped, because a key's body always follows its `BEGIN`, and
  the row says `scanIncomplete`. So on a slow volume or a huge file, a window past what the deadline covered comes back
  empty. Cost: a large file is read twice, and a window that spends the deadline on the scan falls back to approximate
  line numbers.
  - **Gotcha/Why:** the scan reads rows, so armor straddling a segment break (a line past `SEGMENT_BYTES`) is invisible to
    it, as to ⌘F. The window's own check joins rows and covers that inside the window.
- **PDF** (`pdf.rs`): `prescan_pages` decodes every page, whole, under the deadline, and the answer reads only those
  pages. The title and author pass too. A page the parser can't read has no text to check.
- **OCR**: `image_facts` reads the full stored text before its cap. `search_photos` reads it behind every text hit, because
  a query matching a key's body gets a snippet with no armor in it.

**Refusals** are typed, never a false "not found", and reach the security log as tool and reason, with no path
(`note_refusal`, coalesced per tool and reason: an injected turn can ask for a thousand files).

## One tool, both questions (`list_dir`)

"What's in this folder" and "where is my disk space going" are one query with two orderings, so they're one tool. A
second by-size tool would have overlapped it on every axis but the sort, and an agent facing two near-identical listing
tools guesses.

**Where `search` draws the line.** `list_dir` ranks the children of a folder the caller names; `search` with
`sortBy: "size"` ranks a whole drive, which is what surfaces a 900 GB VM image eight levels down that no
folder-by-folder walk would reach. Both tool descriptions state that split, and ❌ nothing else does: the `path`
property deliberately carries no disk-space advice, because two places saying it is two places to drift.

Three properties make the size ordering an honest disk-usage answer:

- **Files and folders rank together.** A folder's `size` is its RECURSIVE total from `dir_stats`, not its inode size, so
  it's comparable with a file's. A folders-only ranking hides the case that motivated this: a single ~900 GB sparse
  VM disk image outweighing every folder on the volume.
- **Unknown sorts last, both directions.** A folder with no `dir_stats` row is unknown, not empty. Leading a `desc`
  ranking with it claims "biggest"; leading `asc` claims "smallest". Both are claims the index can't back, so
  `compare_unknown_last` pins it to the tail either way.
- **The enrichment order follows the sort.** Ranking by size needs every child folder's size BEFORE the sort, so the
  `dir_stats_batch` covers the whole folder; any other order pages first and enriches only the surviving rows, so
  browsing a 20k-entry folder costs one small batch. Identical rows either way — only the lookup count differs.

`offset`/`limit` paging rides on a total order (the sort key, ties broken by name), which is what makes "resume with
`offset + returned`" safe: an unstable order would silently skip or double-count rows, and double-counting a folder is
double-counting its bytes.

## The location contract

Every tool that takes a place speaks one shape, placed by one resolver (`apps/desktop/src-tauri/src/location/resolve.rs`, pure over
`VolumeFacts`; table tests in `resolve_tests.rs`, the prod story in `transcript_179_tests.rs`). `list_dir`,
`important_folders`, `folder_importance`, `search`, and `inspect_file` speak it, `list_pane_files` echoes its folder
as a `location`, and `propose_suggestions` places every destination, selector root, and hand-listed source through it,
refusing the call in the miss shape and re-addressing a write only onto a redirectable sibling
(`suggestions/DETAILS.md` § Where a proposal may point).

**Shapes.** An INPUT is the flat pair `path` + optional `volumeId` (`LocationInput`): flat so an AI client's schema stays
two strings, `volumeId` optional so `~/Downloads` or `/Volumes/naspi-1/papers` simply works. An OUTPUT carries
`location`, the wire shape of `crate::location::Location`, `{ "volumeId", "path" }`, so a result's two fields are a
valid next input. `path` is always the APP path: the string the panes, `search` rows, `inspect_file`, and
`propose_suggestions` use. ❌ Never a share-relative path on the wire; translating into a store's own path space
(index-relative, importance-relative) happens inside the tool.

**Resolution.**

- A leading `~` expands to this machine's home (app paths start with `/` or a scheme, so `~` can mean nothing else).
- **Named** (`volumeId` given): an unknown id is `unknownVolume`. A path under the volume's root resolves, connected or
  not (`live: false` for a saved share: whether it answers is the TOOL's call). A path under a root the volume held
  earlier is rebased onto the current one and answers with `rebasedFrom`; earlier roots are the saved share's last mount
  path plus every root it held this session (`facts.rs::RootHistory`, fed by `on_volume_arrival` and by every facts
  read). Anything else is `pathOutsideVolume` with the volume's `root`.
- **Bare**: the deepest LIVE root that holds the path wins. A path inside a mount area (`/Volumes/<name>` on macOS) is
  never the boot disk's, so with nothing live mounted there it's `noVolumeAtPath`. ❌ No saved-root tier: two saved
  shares can share one last root, and a reused mount point would resolve to the wrong one. Only an explicit `volumeId`
  reaches a not-connected volume.
- **One root rule** (`volume_root`): the live mount (or scheme) root when registered, else the index's `volume_path`
  meta, else the saved row's last mount path. For a volume that isn't live, the facts shell reads the meta (and the
  last scan's date) off its index database through `Index::status`, the read-only opener: nothing is registered or
  rebuilt to answer. `search`'s loader (`search/volumes.rs::read_mount_root`) takes its root from the same function.
- **Facts are in-memory** (`facts.rs`): the registry's live set, the saved-server and saved-share stores, the cached
  device list, and the index's own state. ❌ No `statfs`, mount-table read, or full volume listing, so a resolve can't
  wait on a wedged mount. That's also why the resolver doesn't reuse `resolve_location`'s filesystem arm: its `statfs`
  answers `root` for a dead mount point, exactly the lie this contract exists to stop. The scheme arms read the same
  stores the facts do, with the same deepest-root rule (`app_paths::path_is_under`).
- **A volume is not always its own index.** `ResolvedLocation.index.volume_id` names the index holding the rows: a cloud
  drive in `~/Library/CloudStorage` is its own volume but `root`'s index walks it. A live volume's index is wherever
  the index routes its ROOT; a volume that isn't live has only its own (routing a dead mount point answers `root`).
  `location` keeps the volume's id; index reads use the index's.
- **An importance store keeps its index's paths** (volume-relative, absolute only on the boot disk), so the importance
  tools translate at one seam (`apps/desktop/src-tauri/src/location/store_path.rs`, shared with the visit recorder): strip the resolved volume's root on
  the way in, join it back on the way out. A cloud drive's store is relative to the drive's OWN root although `root`'s
  index walks it, so the seam always takes the importance volume's root, ❌ never the index's routing.

**Misses** (`apps/desktop/src-tauri/src/location/miss.rs`) are an `Ok` result, the memory tools' refusal pattern (§ The two tools that
write): `{ miss, location, detail, root?, nearby?, sameShareAs? }`. `location` echoes what was asked (tilde-expanded),
`detail` names a next move. Tokens: `unknownVolume`, `pathOutsideVolume` (carries `root`), `noVolumeAtPath`, and
`volumeNotConnected`, which a tool answers when the volume resolved but none of its sources can (`volume_not_connected`,
shared), plus the live source's own (§ The live source). ❌ Never an empty listing, ❌ never `authoritative: true` for ground nothing read.

**`nearby` and `candidate`.** Every not-connected miss carries `nearby` (possibly empty) and `sameShareAs` (the volume's
proven siblings, § One share, many ids; empty for a bare path, which names no volume). Each `nearby` entry is `{ volumeId, name, kind, connectionState?, root, candidate }`,
where `candidate` is the requested place re-rooted onto that volume (`/Volumes/naspi/papers` →
`{ smb-192-…, /Volumes/naspi-1/papers }`), so the next call is a copy, not arithmetic. Picked in order: volumes whose
mount name shares the requested `/Volumes/<name>` stem (macOS appends `-N` to a second mount), else volumes whose root
holds the path, else every remote volume; live first, at most 8, never the volume the miss is about.

**One share, many ids** (`apps/desktop/src-tauri/src/location/same_share.rs`). One NAS reached by its LAN IP and by
Tailscale is two volumes, two indexes, and two importance stores (transcript 179: a 220-folder offline index beside an
unindexed live share). Two rules, both proof-based:

- **Same share**: both record the same server GUID AND the same share name, ignoring case. The GUID names the BOX, so it
  never groups alone, and an all-zero GUID is no GUID (`apps/desktop/src-tauri/src/network/server_guid.rs`). Where the GUIDs come from, and why a
  probe's only fills: `apps/desktop/src-tauri/src/network/DETAILS.md` § "Which server a share is on". ❌ No fuzzy "possibly the same". It shows as
  `sameShareAs: [volumeId]` on `list_volumes` rows, `important_folders`' overview, and not-connected misses.
- **Redirectable**: the same share AND the same account (a guest is no account) AND both GUIDs recorded by a session
  that signed in as that account (`serverGuidProof: authenticated` on both rows, or the live session's own proof).
  Another account can see different files on the same share, and a GUID the probe or a guest session filled only says
  what answered a saved address (a hotel network can hand out the NAS's LAN IP), so either sibling stays a
  `sameShareAs` hint and never answers in its place, for reads (`readers`, `preferred_member`, `redirect_group_ids`)
  and for anything that re-addresses a write.
- **Preference is per read** (`list_dir`, `read/listing/readers.rs`): across the volume asked and its redirectable
  siblings, the first member and source in order live index → live readdir → freshest index on disk answers, the
  volume asked first within each tier. A sibling's answer carries its own `location` plus `via: { volumeId }` naming the
  volume ASKED, misses included.
- **`important_folders` re-addresses** a member's rows onto the member a read prefers (`preferred_member`: live with an
  index, then live, then the freshest index on disk, the scoring volume on a tie), with `via` naming the store that
  scored it, and dedupes by `location`, keeping the higher score. A `volumeId` scope spans the volume's redirectable
  group, since one share's stores hold share-relative paths.

**The two tools that can't take the pair as is.**

- **`search`** (`apps/desktop/src-tauri/src/tools/actions/search/placement.rs`) takes `volumeId` beside its `scope` string, shared with AI clients,
  so it adds rather than reinterprets: with no `volumeId` the scope routes the run as it always did, and the resolver
  only catches an include path under a mount area with nothing mounted there (`noVolumeAtPath`, which would otherwise
  route to the boot disk and walk a folder that isn't the user's drive). ❌ No miss for any other bare scope: a phone's
  or server's scheme path routes by its own spelling, connected or not. With `volumeId`, every include path resolves
  onto that volume (a miss answers the call whole), no scope means the volume's root, and the run reads the RESOLVED
  index (`run_live_collected`'s `on_volume`), because a not-connected share's last root routes to the boot disk. A
  not-connected volume with an index on disk answers from it, leading its `notes` with "isn't connected … as of";
  one with none is `volumeNotConnected`.
- **`inspect_file`** keeps `paths: string[]` (200 location objects would be a large call for no gain), and every row
  echoes `volumeId` from the volume registry (`read/inspect/place.rs`). A `missing` row under a mount area with nothing
  mounted there is `noVolumeAtPath` instead, and the call carries the resolver's `nearby` ONCE, for the first such row
  (each `candidate` is that path on another volume; the others swap the same prefix). Only `missing` rows read volume
  facts, so a call that found every file pays nothing for placement.

### The live source

`list_dir` on a CONNECTED volume whose index doesn't list the folder (no index at all, like the LAN share in
transcript 179, or ground the index hasn't reached), and whose share has no live sibling index listing it either
(§ One share, many ids), answers from one readdir through the volume's provider
(`read/listing/live.rs`), so the share the user has connected is always listable. `coverage.source: live`, the
volume's own `indexStatus` beside it, `authoritative: true` unless the cap cut it, and a note that folder sizes need an
index: folder rows and the folder's own `size` are ABSENT, never the inode's 4 KB or a `0 B`.

- **Gate first, on the target links resolve to.** For a volume whose paths are `std::fs` paths, `gate_local` resolves
  the target one component at a time and checks each place against the Full Disk Access gate BEFORE `lstat` touches it
  (`fda_gate::is_fda_pending_runtime` + `tcc_paths::is_potentially_tcc_restricted`). ❌ Never `std::fs::canonicalize`
  first: it stats a link's target to resolve it, which for a link into `~/Documents` (or the first touch of a network
  mount) is the very TCC prompt the gate exists to prevent. Then the resolved target must sit under the volume's
  resolved root with no deeper live root holding it (a link off the volume is refused, naming where it lands, ❌ never
  followed). The folder listed is the PHYSICAL one, spelled under the volume's own root.
- **Never ground the index skipped on purpose.** Every folder from the root down to the target, the leaf included, is
  asked `Index::scan_skips` (the scanner's own `should_exclude`: junk basenames like `.Spotlight-V100` on any volume, a
  root pseudo-filesystem, the boot disk's system prefixes), so a live read and the index answer the same way for one
  folder. ❌ Not search's default-hidden tier (`SYSTEM_DIR_EXCLUDES`): the index lists `node_modules`, so a live read
  must too. A provider-addressed volume (direct SMB, MTP) reaches no local folder, so only this check applies: links are
  the server's. ⚠️ The scanner's pseudo-filesystem probe stats a `proc` / `sys` / `dev` at a volume root's parent, so
  the check runs on the read's blocking thread, under its deadline.
- **One bounded read** (`run_bounded`): its own task on the app's shared runtime (a turn's runtime is single-threaded
  and ends with the turn, and an abandoned read must still finish), `LIVE_READ` = 10 s with a 1 s grace: at 9 s its
  token flips (SMB and MTP stop between round trips), at 10 s it's ABANDONED as `unreachable`. One in flight per
  volume: the slot frees only when the task really returns, so a second call behind a hung mount answers `unreachable`
  at once instead of parking another thread.
- **Stop reaches it.** The turn races every call against Stop (`agent/chat/DETAILS.md` § Stop reaches every tool
  call); `view::dispatch` also hands `list_dir` the turn's token through `execute_list_dir_cancellable`, so the read
  stops itself and answers `cancelled` with its `location` before the turn's generic answer would. The abandoned task
  frees its slot when it returns. An AI client's call gets a fresh token, and dropping it (the client hanging up)
  flips the read's token too.
- **The cap**: `LIVE_ENTRY_CAP` = 10,000 entries kept, the first ones in the provider's order (folders first, then
  names), so a capped folder pages over a stable set; the requested sort applies within it, with `truncated: true`,
  `totalIsLowerBound: true`, and `authoritative: false`. An `offset` at or past the cap is refused before reading,
  pointing at a subfolder or `search`. ⚠️ The cap bounds what's KEPT, not what the provider reads: no `Volume` lists
  in pages, so a 200,000-entry folder still materializes once inside the provider, as it does for a pane.
- **The listing is the pane's open** (`cmdr_listing::entries_as_stored`), so a typed path in another Unicode form (an NFD
  `Rymdskottkärra AB` against an NFC share) finds the stored spelling.

Its misses ride the same shape as the resolver's (`LocationMissResult::on`, `location` naming the volume):
`needsDiskAccess` (the FDA question is open, or macOS privacy refused the read; the `detail` says which),
`unreachable` (past the deadline, another read on the volume still stuck, or the volume dropping mid-read), `notFound`
(nothing there, a link loop, or a link off the volume, whose `detail` names the volume and path to ask instead),
`excluded` (names the folder the index skips), `permissionDenied`
(the folder's own permissions: Full Disk Access won't help), and `cancelled`.

**A dropped session points at `connect_volume`, not at the user.** When the volume is a session Cmdr owns
(`connectionState` `direct` or `disconnected`, `live.rs::reconnects`), a mid-read drop's `detail` says Cmdr is
reconnecting it and to call `connect_volume` with its `volumeId` (which waits out the reconnect, § Connecting a saved
place), or wait a few seconds where that tool isn't offered (AI clients), then read again. An OS mount or a local disk
keeps "tell the user". Why: a stale smb2 session failed one read with "Tell the user it may have disconnected", the
agent wrote the NAS off for the whole answer, and the backoff loop had it back nine seconds later (verified on a dev
build, `main.db` conversation 689 plus `cmdr.log` `ConnectionLost` then `session: established`, 2026-10-28). A location
miss carries no `problem` key, so the repeat breaker never refuses the retry.

## The honesty (coverage) contract

`read/listing/source.rs::coverage` is the single builder for index freshness honesty: it reuses `status_token` +
`Freshness::is_authoritative` (never re-derives the tokens) and attaches a plain-language note when a read isn't
authoritative or the path isn't indexed. `authoritative` needs both a `fresh` index AND a row for the folder: a fresh
volume vouches for nothing it doesn't list (a model once read `authoritative: true` beside "isn't in the drive index"
and trusted the bool). `SizeStats::from_dir_stats` carries the exact-vs-lower-bound / stale /
has-symlinks flags verbatim from `DirStats`. Importance staleness is `asOfGeneration < recomputeGeneration`. These are
the flags spec §2.4 makes load-bearing; the system prompt requires the model to voice them.

**`sizeIsUpdating` is the one flag `DirStats` can't answer alone.** A folder's total also moves while a walk is on it,
above it, or below it (the roll-up repairs ancestors), and `DirStats.recursive_size_pending` only knows about that
folder's own draining writes. So `execute_list_dir` resolves the volume's walked ground once and
`IndexStatusResponse::walk_affects` answers per row — the folder's own total and each child folder's, since a walker
deep inside one child leaves its siblings settled. ❌ Reading the pending flag alone calls a folder settled through the
whole walk that's rewriting it, which is when its number is furthest from the truth.

That ground travels as one field of `VolumeView`, beside `enabled`, `freshness`, the `VolumeBlock` the model sees, and
`reach` (which copy of the index answers, below). All are read from the same volume at the same instant, so bundling
them is what stops a `walked_roots` resolved for one volume from sitting beside an `enabled` read from another and
quietly mis-reporting every size on the page.

**An offline answer is dated, stale, and never authoritative.** A resolved volume that isn't connected
(`ResolvedLocation::live == false`) answers through `answer_offline`: the same `Index::list_children` / `dir_stats` /
`dir_stats_batch` cores, which read a volume the host doesn't serve from its database on disk, read-only, with nothing
stood up (`crates/cmdr-index/src/indexing/read/DETAILS.md` § A stopped volume's database answers reads). Its coverage
(`offline_coverage`) is `source: offlineIndex`, `indexStatus: stale`, `authoritative: false`, and `asOf` (the last
completed walk, spoken; absent when it never finished), the one offline vocabulary `list_volumes` speaks too, plus a
note to connect for a live answer. Sizes keep their own flags. Nothing walks a volume with no instance, so nothing is
`sizeIsUpdating`; space is the poller's cache, as for any volume. A folder that index doesn't list (or a volume with no
index on disk) is `volumeNotConnected` with its `nearby`, ❌ never an empty listing. The index maps the app path by the
root its last walk recorded (`volume_path`), which is what the one root rule answers for a volume that isn't live; a
database that predates that meta can't place a path and so answers nothing.

Aggregation (the 12–19 s after a first index's walk, when the drive's folder totals are computed) is deliberately NOT
in that answer: the walker has released its ground by then, and nothing queryable per volume reports it. The `coverage`
block already carries that window (`indexStatus` isn't `fresh`, `authoritative` is false), which is the same warning at
a coarser grain. Revisit if a per-volume aggregation read ever exists.

**A caveat that lives only in a sibling flag is a caveat the model can shed.** A flag is honest only while the reader
carries it alongside the number, and an agent restating "1.8 TB" has already dropped `sizeIsLowerBound: true`. So the
qualifier rides INSIDE the human string (next section), and the flag stays for anything that branches on it.

## Numbers arrive already spoken

The Ask Cmdr agent can't run a script, so every number it might state out loud is formatted before it crosses the wire:
raw bytes and epochs make a model do arithmetic it does unreliably. Each raw field keeps a formatted twin — never one
instead of the other, since the raw value is what anything downstream computes with:

- `ChildEntry.sizeHuman` / `modifiedHuman`, present exactly when `size` / `modified` are. An unknown size gets NO
  string: a `"0 B"` would read as an empty folder, which is the wrong-zero the whole coverage contract exists to
  prevent.
- `SizeStats.recursiveSizeHuman` (always present, the folder's own total).
- `VolumeBlock.totalHuman` / `availableHuman`, present exactly when their byte counterparts are.
- `ListDirResult.remainder` (below).

**Uncertainty is inside the string.** `≥ 1.8 TB` when the number can only be higher (a settled lower bound), `~ 40 GB`
when the error runs in both directions. `human_size` / `size_qualifier` / `qualified_size` (`read/listing.rs`) are the
one place that decides; `ChildEntry::new` / `set_size` are the only ways to set a size, so the number and its string
can't drift apart.

Both take the number and its two caveats as ONE `SizeClaim`, which is that contract as a type: `size_human` is derived
from all three at once, and no caller can set a size while forgetting which uncertainty it carries. The flags mean
opposite things (`≥` up only, `~` either way) and used to sit adjacent in an argument list, where swapping them was
silent. A row's `RowKind` (`File` / `Folder` / `SymlinkToFile` / `SymlinkToFolder`) does the same for the other pair:
the wire still carries `isDirectory` + `isSymlink`, but only `RowKind::Folder` earns a recursive total, since following
a link can leave the tree or loop.

**Motion outranks coverage** (`size_qualifier`): a size that is BOTH a lower bound and still updating reads `~`, never
`≥`. `≥` is derived from unscanned subtrees alone, so it can't express the opposite error — an index entry for a
subtree that's already gone, where the truth is far lower — and that's exactly what a running walk is correcting. A
folder read `≥422 GB` on its way down to 56 KB (2026-09-07). Both booleans still ship, so a caller that wants to know
which uncertainty it has can look.

**One formatter, `search::format_size` + `format_timestamp`.** ❌ Never a second one: two would round differently and
the same folder would read two sizes across two surfaces. Like the `search` results table, this path does NOT consult
the user's SI-vs-binary units setting; MCP/agent output stays internally consistent instead of tracking a UI preference.
It's base 1024, so it writes IEC symbols (`KiB`, `MiB`, `GiB`, `TiB`). Its inverse, `search::parse_size`, lets the
symbol pick the base (`MiB` = 1,048,576, `MB` = 1,000,000, a bare `KB` = 1,000), and every typed or model-written
size goes through it: MCP's `parse_human_size` and the search AI's `>50mb` / `>2gib` bounds. The selection AI writes
raw bytes, so its prompt teaches the same split.

### The remainder: what the page didn't show

A paged listing can say what the rows it left out add up to: `{ count, bytes, human, isApproximate }`, where `count` is
`total - returned` and `bytes` is the folder's own `recursiveSize` minus the sizes on this page (saturating). Without
it, "the other 3,000 files" is a number the model can only invent.

`isApproximate`, deliberately not `isLowerBound`: the bounds run in BOTH directions. An understated folder total pulls
the remainder down, an understated child size pushes it up, and both can be in play at once, so naming a direction
would be false precision. Its string wears `~`, never `≥`.

**Omitted rather than guessed** (`remainder()`), because the model would state a wrong one as fact:

- `count == 0` — this page is the whole folder; a zero would only invite interpretation.
- Any returned child has an unknown size. It's missing from the subtraction, so the difference silently absorbs it.
- No `dir_stats` total for the folder to subtract from.
- A `type` filter is active: `count` would be "folders not shown" while the recursive total still counts every loose
  file, so the pair would describe two populations in one sentence ("the other 3 folders come to 40 GB", where 38 GB of
  it is files that were never in the running).

Rejected: a `summary` sentence field. A model reuses such a string verbatim, which makes it user-facing copy needing
review in every state (scanning, unindexed, lower-bound, empty) — and Cmdr's rule is that human-facing text is
human-written (`AGENTS.md` § Principles, "Humans to humans").

## The size contract: a result never outgrows the caller's context

Every result that carries a LIST is cut to `agent::chat::budget::MAX_TOOL_RESULT_TOKENS` through
`tools::actions::fit_to_result_budget`, and reports `total` / `returned` / `truncated` so the model can say what it saw
and ask for the rest. It applies to `list_dir` (children, under the caller's own `limit`), `list_pane_files` (entries, on
top of its 200-row cap), `image_facts` (per-path rows, on top of the 2,000-char per-file text cap),
`search_photos` (hits), `search` (entries, on top of its own 200-row cap), `inspect_file` (per-path rows, on top of the
16,000-char per-row window cap, with the rows it drops named in `unanswered`), the `operations_*` pages, and the
suggested-ops reads (group summaries from `list_suggestions`, the op page from `get_suggestion_group`).

⚠️ **`search` reports `matchCount`, which is NOT the `total` the rest of this list reports.** Everywhere else `total` is
what the page was cut from, so `returned == total` means "you saw everything". A search's engine stops emitting rows at
the row cap while the count keeps rising, so `matchCount` can exceed `returned` by orders of magnitude with nothing
wrong; `coverage.capped` says which, and `matchCountHuman` wears a `≥` so the caveat can't be shed when the number is
restated.

**Why a size cut on top of the row caps:** a row cap can't bound a payload. `image_facts` at 200 paths × 2,000
characters is ~100k estimated tokens, and a `list_dir` on a 20k-entry folder had no cap at all. A result that doesn't
fit doesn't just get itself dropped — it pushes the rest of the turn out of the prompt, which is how a bulk-rename turn
lost the very evidence it was reasoning from (`agent/chat/DETAILS.md` § Budget enforcement). The ceiling is derived from
the CONSERVATIVE default prompt budget, not the resolved model, because a handler doesn't know the model and may be
answering an external MCP client.

The cut is never silent (`image_facts`'s founding principle, now the rule for every tool): the counts cross the wire,
and the system prompt requires the model to disclose "returned of total" whenever `truncated: true`.

## The dispatch gate

`../agent/tool_gate.rs::refuse_unavailable(call_id, tool, origin)` turns `admit`'s answer into a tool result the model can
read, BEFORE `execute_tool`:

- A name outside the table ⇒ `{ available: false, requested, reason }`.
- A call `admit` refuses ⇒ the same, plus the `refusal` token: `refusedInBackground` for a class that waits for a turn the
  user started, `devOnly` for a tool that's never the agent's. A background connect can spin up a sleeping NAS's disks for
  a digest nobody asked for (agent invariant 7). "User-started" follows `docs/design-principles.md` ("An approved action
  is the user's own"): an action the user approved counts as theirs, while a turn the app opens on its own (a wake, the
  follow-up a rejection earns) doesn't. Today no approval opens a turn, so `TurnOrigin` has no approved-action origin; a
  feature that runs an approved card's follow-up through the agent adds one that `user_started()` answers `true`.
- Otherwise `None` ⇒ `dispatch` calls `execute_tool` with a `Caller::Agent` context, which asks `admit` again.
  `propose_rename_plan` alone goes around it, for the proposal snapshot its result carries.

A `ToolError` reaches the model with its `data` (`error_content`), since that's the contract and the message only the human
half. Two read differently on purpose: `outcome_unknown` is `{ outcomeUnknown: true, detail }` with no `problem` key, since
the action may still be running and a problem would trip the repeat breaker; and a question Stop took down is
`{ miss: "cancelled", detail }`, the words of any call Stop ended.

**The origin is the turn's, stated by its caller.** `TurnParams::origin` (`agent::types::TurnOrigin`) is set by
`send_message` (`text`) and by the wake runner (`wake` / `outcomes`), and reaches every call in `CallContext` beside the
turn's Stop. ❌ Never derive it from the opener: a follow-up rides as `UserTurn::Outcomes` today, and the gate must hold
if that wrapping ever changes.

Every tool × every origin is tested against `admit` (`../agent/tool_gate.rs`), and one scripted turn per class runs through the
real gate (`../agent/chat/runtime/class_smoke_tests.rs`). What the agent can do and what it can't finish alone are proven through
whole turns with a fake frontend (`../agent/chat/runtime/capability_tests.rs`), called directly and through `use_tool`, and against the
real webview (`apps/desktop/test/e2e-playwright/ask-cmdr-capability.spec.ts`).

A refusal's `reason` says what's possible instead. ⚠️ Keep it accurate as the tiers grow: a model told a false limit either
refuses the question or invents the answer.

### A propose result always answers `readyForReview`

`true` when it staged, `false` when it didn't. `../agent/tool_gate.rs::ensure_review_verdict` holds it at `dispatch`'s single exit:
`dispatch` is a two-line wrapper over `route_call`, so every branch funnels through one stamp.

**It is a choke point rather than a stamp at each refusal site** because four sites already have to agree — the schema
gate, `execute_tool`'s flattened `ToolError`, `propose_in_thread`'s, and the rename boundary's own typed refusals — and
the fifth nobody has written yet is the one that would go missing. Two of those four had no verdict when this was
found: a model read a bare `{ problem }` from the gate and told the user its rename plan was waiting in the suggestions
panel while the store held nothing.

Both decisions are typed, never the tool's name or the refusal's wording: the registry's `Access` says which tools owe
a verdict, and `AgentToolResult::reports_a_problem` (on the type, shared with `chat/runtime`'s `dispatch_ok`) tells a
problem from an answer. It fails closed but never overwrites — a result that already answered keeps its own verdict, so
a plan that DID stage can't be reported as staging nothing, which would be the same dishonesty pointed the other way.

`chat/runtime/repeats.rs` keeps a failed result's content whole, so a repeat inherits the verdict too.

### What a gate refusal says

The schema gate (`tools/params.rs`) runs inside `route_call`, ahead of the branch, so it covers every agent
call. It warns on `agent::tools`, carrying the typed `data` (which properties were wrong) beside the sentence. Before
that line, a plan that never reached the proposal store was invisible: the log held the provider round trips and the
repeat breaker's warn, and characterizing the report meant reading `main.db`'s conversation rows.

The refusal SENTENCE matters as much as the shape, and its rules live with the gate, not here: a per-row property sent
at the top level is a `Misplaced` violation naming the row that takes it, because "propose_rename_plan has no volumeId
parameter. It takes renames." is a true sentence a model cannot act on — it reads as "your rows were fine".

`dispatch` routes one tool around `execute_tool`: `propose_rename_plan`, which needs the evidence scope and returns the
proposal snapshot. Every handler that needs the caller, the thread, or the turn's Stop reads them off `ToolCtx`.

`dispatch` also feeds `propose::evidence::ImageFactsLedger` from every non-elided `image_facts` result. That's what makes
a rename plan's content claim checkable; the guardrail, its refusal shape, and the revocation seam are in
`propose/DETAILS.md`.

## Cross-module symbols the toolset reuses

- `Index::list_children` / `dir_stats` / `dir_stats_batch` — volume-keyed: the caller names the volume the resolver
  placed, so a path can't pick a second index (`crates/cmdr-index/src/indexing/read/DETAILS.md`).
- `crate::location` — the resolver every location-taking tool shares (§ The location contract).
- `tools::resources::volumes::VolumeKind::token` — `pub(crate)` so the volume mapper reuses the one kind→token mapping.
- `tools` re-exports `Access`, `execute_tool`, `tool_access`, `ToolError`, `ToolResult` as `pub(crate)` for the agent
  runtime and the transport. `snapshot_volumes` and the `importance::snapshot_*` functions are `pub(crate)` too.

## Not covered here (the runtime harness)

A full fake-driven dispatch of a REAL agent tool (success path through `execute_tool`) needs a Tauri app with managed
state (`PaneStateStore`, the index registry, a data dir). That app harness is the chat runtime's concern, so this layer
covers the success path with per-tool pure-shaper tests (fixtures for the coverage flags) and the refusal path in full
(no app needed). The dispatch entry point the runtime calls is `agent::tool_gate::dispatch`; the declaration API is
`tools::agent_tool_declarations`.
