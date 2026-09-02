# Agent subsystem details

Pull-tier docs for `src-tauri/src/agent/`. Must-knows live in `CLAUDE.md`.

The agent is the app's AI agent (agent-spec: `docs/specs/later/ai/agent-spec.md`). Its first shipped slice is **Ask Cmdr**:
a read-only chat rail where the user talks to a BYO-key LLM that can see what Cmdr already knows (the drive index,
importance, the operation log, live app state) and answers questions about their files. It deliberately ships ahead of
the agent's proactive machinery (wake loop, proposals, notifications) — the wow reaches beta users cheaply while the
risky proactivity bakes.

## Why "agent", not "ask-cmdr"

The persistent entity is "the agent" (agent-spec D44); "Ask Cmdr" is the user-facing name of this one read-only slice.
Naming the subsystem after the entity means the later proactive surfaces (proposals, notifications) grow inside `agent/`
rather than forcing a rename. `name-internals-after-the-UI` still applies to the surfaces (`ask-cmdr/` on the frontend).

## Module layout

The backend modules:

- `llm/`: the `AgentLlm` trait, its genai-backed impl over `crate::ai::AiBackend`, the deterministic fake,
  and the typed message-part model. This is the seam the whole runtime and UI test against. Depth:
  `llm/DETAILS.md`.
- `store/`: the `main.db` durable store — a forward-migration ladder (mirroring `operation_log/store/`),
  FTS5 over message text, a per-day cost meter, and the durable proposal spine in `store/proposals/`.
  `agent::start(app)` (open the DB, register the `AgentDb` handle, run the interrupted-proposal sweep once) lands here,
  modeled on `operation_log::start`. Depth: `store/DETAILS.md`, `store/proposals/DETAILS.md`.
- `suggested_ops/`: the service over the spine — resolving a selector to a frozen op list against the drive index,
  wrapping the store's claim, and the acceptance-rate metric. Depth: `suggested_ops/DETAILS.md`.
- `tools/`: the in-process toolset — the read families authored as `consumers: [Agent]` entries in the consolidated
  registry (agent-spec D49, extend-don't-fork), their handlers/result shapes that reuse the shipped cores (drive index,
  importance, operation log, volumes, app state, the file viewer), the propose and memory tiers, and the gated dispatch
  that refuses any non-view name before `execute_tool`. Depth: `tools/DETAILS.md`.
- `chat/`: the chat runtime (single-flight per thread, per-message budgets, cancellation, typed errors,
  crash-safe persistence, the `AgentChatEvent` seam) and the pure, TDD-heavy context-assembly core (stable prefix,
  elide-only compaction, the fresh context envelope on the latest user turn only). `chat/session.rs` is what a turn
  needs resolved from live app state (the LLM slot, the prompt budget, the envelope), shared by the rail's command and
  by a wake — it sits here rather than in `commands/agent/`, which is ABOVE `agent/` and so unreachable from a wake.
  Depth: `chat/DETAILS.md`.
- `memory/`: the Markdown folder the agent writes about the user (`<data-dir>/ai/memory/`, `AGENTS.md` the hub) —
  a pure `MemoryStore` holding the jail, the two caps, the write, and the edit, plus eight lines of `AppHandle` path
  resolution. Depth: `memory/DETAILS.md`.
- `wake/`: the proactive half — the pure noticing pipeline (coalesce → interest → compact → inbox) plus the loop that
  drives it. `agent::start` brings up one thread owning the `Inbox`, a long-lived write connection, and the timer; the
  indexer's tap reaches it through a process-global channel and a prepared wake runs on its own thread, so neither the
  live loop nor the inbox is ever held across a model call. Depth: `wake/DETAILS.md`.

## The agent can propose; only the user can approve

A staged rename proposal is one group on the durable proposal spine, addressed by opaque id; the tool can stage one, but
no agent path can approve or apply it.

**The invariant.** The agent can propose. Only the user can approve. Approval originates in the frontend as a user
action. There is no tool, and never will be a tool, that approves a proposal. Without that, `Propose` is `Write` with
extra steps.

The agent can look, speak, ask, and write its own notes (spec §2.1): no tool in its dispatch view touches the user's
files. Names, paths, and metadata reach the provider on every turn; file contents reach it only on request, through
three read tools whose egress the consent copy names item by item: `search_photos` and `image_facts` (image-derived
text) and `inspect_file` (bounded text windows, `find` lines, PDF pages plus title and author, one level of archive
entry names, EXIF including GPS). No tool can return bytes: every result DTO is text-only by construction, each pinned
by a test. This is the privacy line and it is structural, not a runtime guard. The registry's `consumers` + `access`
dimensions pin the agent's view to exactly its authored `[agent]` entries, every one `Access::Read`,
`Access::Propose`, or `Access::Memory`, never `Access::Write`; the runtime's `ToolId` parse step is the runtime choke
point (an unrecognized name resolves to `ToolId::Unrecognized`, which is never in the agent view, so dispatch refuses
it). A new KIND of content egress (a new tool, or a new field on an existing one) is a consent-copy change plus a
`CONSENT_COPY_VERSION` bump, never a silent widening; `docs/security.md` § Ask Cmdr agent egress, the user-facing
account, has to move with it.

**`Access::Memory`: what the widening cost, and what holds it.** The agent's promise used to be "it never changes
anything". `memory_write` and `memory_edit` made that false, so the promise narrowed to "it writes only into its own
memory folder" — still structural, and held by three things rather than one. First, `memory/`'s jail: relative `.md`
paths only, no `..`, no symlink anywhere along the chain, containment re-checked against a canonicalized parent.
Second, a hand-authored allowlist (`EXPECTED_MEMORY_TOOL_NAMES`), for the same reason `Propose` has one: no
structural check can prove a handler stays in the jail, so a human puts each name there having read it. Third, the
folder is unreachable from the external MCP transport, whose own security story is "no filesystem access".

⚠️ The widening also opened an injection surface, because the write path is reachable from text the agent read
(`image_facts` OCR, file names off disk) and what it writes rides the prefix of every later turn. The defences are in
`memory/DETAILS.md` § The injection surface; don't weaken the fence in `chat/context.rs` or the placement of memory
before the rules without reading it.

**Where a proposal LIVES.** `store/proposals/`, the durable spine in `main.db`, for every verb including rename. Its
claim transaction binds an approval to a server-owned acceptance record rather than to the client's word. Proposals have
no expiry; the one thing deliberately held in memory instead is a rename's ACCEPTED preflight, so a restart forces a
fresh one (`tools/propose/DETAILS.md`).

**What a `Propose` tool may do.** Stage a proposal and open a review surface. That is its entire power: no filesystem
write, no silent config mutation, no self-approval. Because no structural check can prove a handler doesn't mutate,
`Propose` tools are an explicit hand-authored allowlist (`EXPECTED_PROPOSE_TOOL_NAMES` in
`mcp/tests/tool_registry_tests.rs`) rather than something inferred — adding one is a deliberate act a human signs off,
having read the handler. It holds two names: `propose_rename_plan` and `propose_suggestions`.

**Consent is unaffected.** Proposals flow agent → user, never to the provider. `Propose` adds no egress, so the
provider-egress question and `CONSENT_COPY_VERSION` are unchanged by this tier. Don't re-litigate it: only a change to
what reaches the provider touches consent.

**Bounding is the tool's contract.** A `Propose` payload must be capped the way `image_facts` caps at 200 paths. A
proposal the user can't actually review is a proposal they can only rubber-stamp, which quietly dissolves the invariant
above. The cap can't be enforced generically (each tool's payload shape differs), so the first `Propose` tool has to
honour it explicitly and pin it with a test.

## The invariants register

Twelve numbered invariants span the agent's context core, its proposal path, and the write engine it hands work to.
**The numbers are load-bearing**: roughly twenty code sites and doc lines cite them bare (`(invariant 6)`,
`(invariant 10)`), so this list is where those citations resolve. Numbers are permanent: a retired entry keeps its
number and says it retired, because renumbering silently repoints every one of those citations.

Each line is a pointer, not a restatement: the mechanism lives in the doc named beside it.

1. **The current turn's tool results are never elided.** Handed a stub instead of the facts it was told to name files
   by, a model invents. `chat/CLAUDE.md`, `chat/DETAILS.md` § Budget enforcement.
2. **The pure context core stays pure**: no clock, no I/O, no app state, no per-tool knowledge. New inputs arrive as
   values. `chat/CLAUDE.md`, `chat/context/digest.rs`.
3. **The prefix is byte-identical across a thread's calls**, which is what buys prompt caching. `chat/CLAUDE.md`.
4. **The envelope rides the latest user turn only**, snapshot-at-send. `chat/CLAUDE.md`.
5. **Assistant prose is never modified.** A tool CALL may be collapsed when it is a rename plan the store no longer
   holds live; prose never. Nothing collapses calls today, so the narrowed form is a contract for whoever builds it.
6. **A content claim needs a delivery the ledger recorded, in that thread.** Digests, envelopes, patterns, and
   summaries describe deliveries and are never deliveries. `tools/propose/CLAUDE.md`, `tools/propose/DETAILS.md`
   § Evidence.
7. **The agent proposes; only the user approves.** Approval originates in the frontend as a user action, and there is
   no tool that approves. The agent's one write is its own memory folder, jailed and hand-allowlisted. `CLAUDE.md`,
   § The agent can propose above, `memory/DETAILS.md`.
8. **No new egress category, and no consent bump** without revisiting the whole consent story. `CLAUDE.md`.
9. **Every cut, cap, or trim is visible** in the result, the log, and (where the user could be misled) the UI.
   `chat/CLAUDE.md`, `chat/DETAILS.md` § Reporting what a turn cost.
10. **A user-edited name needs no evidence, never claims any, and never inherits the model's**, and it invalidates the
    accepted preflight so no name reaches the filesystem unchecked. `tools/propose/CLAUDE.md`,
    `tools/propose/DETAILS.md` § Revising one row.
11. **Every row that reaches the filesystem was preflighted with a fingerprint the writer rechecks.** No path around the
    proposal may skip it. `store/proposals/CLAUDE.md`, `suggested_ops/DETAILS.md` § The approval bridge. Compress is the
    documented single exception, and that `DETAILS.md` says what would make it stop being safe.
12. **Evidence validation proves the model READ something, never that the name is right.** So the review surface must
    show how thin a match is, and the offline eval targets the genuine-quote-wrong-name case rather than asserting a
    refusal that is impossible without understanding the image. `tools/propose/DETAILS.md` § Coverage,
    § The name-quality eval.
