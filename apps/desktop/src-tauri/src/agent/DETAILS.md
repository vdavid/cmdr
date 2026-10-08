# Agent subsystem details

Pull-tier docs for `src-tauri/src/agent/`. Must-knows live in `CLAUDE.md`.

The agent is the app's AI agent. Its design principles and its numbered decision log are at the end of this file
(§ Principles, § Decision log); what's designed and unbuilt is in [GitHub issues labelled `ai`](https://github.com/vdavid/cmdr/issues?q=is%3Aopen+label%3Aai). Its first
shipped slice is **Ask Cmdr**:
a read-only chat rail where the user talks to a BYO-key LLM that can see what Cmdr already knows (the drive index,
importance, the operation log, live app state) and answers questions about their files. It deliberately ships ahead of
the agent's proactive machinery (wake loop, proposals, notifications) — the wow reaches beta users cheaply while the
risky proactivity bakes.

## Why "agent", not "ask-cmdr"

The persistent entity is "the agent" (agent decision D44); "Ask Cmdr" is the user-facing name of this one read-only slice.
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
  registry (agent decision D49, extend-don't-fork), their handlers/result shapes that reuse the shipped cores (drive index,
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

The agent can look, speak, ask, and write its own notes (principle 3): no tool in its dispatch view touches the user's
files. Names, paths, and metadata reach the provider on every turn; file contents reach it only on request, through
three read tools whose egress the cloud AI disclosure names item by item: `search_photos` and `image_facts` (image-derived
text) and `inspect_file` (bounded text windows, `find` lines, PDF pages plus title and author, one level of archive
entry names, EXIF including GPS). No tool can return bytes: every result DTO is text-only by construction, each pinned
by a test.

`inspect_file`'s text window reads through the viewer's backends, so what it receives are ROWS, not physical lines
(`src-tauri/src/file_viewer/CLAUDE.md`). ❗ A row whose `continues` is true was broken by Cmdr at a segment boundary,
so `window_from_chunk` owes it NO separator: joining those two rows with a `\n` would hand the model a line break the
file does not contain, and the model would reason about a file shape that isn't there. That is why the loop carries a
`separator_owed` flag rather than testing `returned > 0`. The same rule, and the reason behind it, is in
`file_viewer/DETAILS.md` § "Rows, not lines"; this note exists because the agent path is the one that looks like it
could ignore it. This is the privacy line and it is structural, not a runtime guard. The registry's `consumers` + `access`
dimensions pin the agent's view to exactly its authored `[agent]` entries, every one `Access::Read`,
`Access::Propose`, or `Access::Memory`, never `Access::Write`; the runtime's `ToolId` parse step is the runtime choke
point (an unrecognized name resolves to `ToolId::Unrecognized`, which is never in the agent view, so dispatch refuses
it). A new KIND of content egress (a new tool, or a new field on an existing one) is a disclosure-copy change plus a
`CLOUD_AI_CONSENT_VERSION` bump (`ai::cloud_consent`), never a silent widening; `docs/security.md` § Cloud AI egress,
the user-facing account, has to move with it.

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
`mcp/tests/tool_registry_tests/access.rs`) rather than something inferred — adding one is a deliberate act a human signs off,
having read the handler. It holds two names: `propose_rename_plan` and `propose_suggestions`.

**Consent is unaffected.** Proposals flow agent → user, never to the provider. `Propose` adds no egress, so the
provider-egress question and `CLOUD_AI_CONSENT_VERSION` are unchanged by this tier. Don't re-litigate it: only a change to
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
8. **No new egress category without a `CLOUD_AI_CONSENT_VERSION` bump**, and no bump without revisiting the whole
   disclosure (`ai::cloud_consent`, `../ai/DETAILS.md` § Cloud AI consent). `CLAUDE.md`.
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

## Principles

These govern anything the decision log below doesn't answer. `(principle 3)` citations resolve here.

1. **Deterministic bottom, LLM top.** Cheap, testable Rust handles everything with an obviously correct answer (event
   coalescing, importance, staleness, digest compaction, proposal validation). The model is for judgment and language.
   ❌ Never put a model in a per-event hot path.
2. **The agent costs about nothing when nothing interesting happens.** No idle wakes, no heartbeat calls. Noise is
   absorbed deterministically and reaches the model only as one digest line at the next real wake.
3. **Propose, never act.** No tool reaches the user's files; the only path to them is a proposal the user approves,
   executed by the ordinary file-op pipeline. This is also the structural prompt-injection defense: the worst a
   malicious file can do is produce a weird suggestion in a review queue. The one bounded exception is the agent's own
   memory folder (§ The agent can propose above).
4. **Continuity through state, not transcript.** A wake gets a fresh, budgeted context assembled from the database and
   memory; only chat threads keep (bounded) transcripts.
5. **Radical transparency, applied to the agent itself.** Every decision, proposal, and file read should be visible to
   the user with its reason. This is the least-paid principle today: no activity log exists yet.
6. **Derived data lives in the database; beliefs and rules live in Markdown** the user can open, edit, and delete.
7. **Events are liveness hints; state is truth.** The event stream has gaps (app closed, volume gone, cache purged);
   recovery reconciles against indexed state and stored fingerprints, never replays events.
8. **Don't gamble the user's trust.** Anti-noise etiquette is policy: caps on proactive surfacing, a user-chosen
   proactivity level, per-folder mute, no repeat after a rejection.

## Decision log

The agent's design decisions, numbered. **The numbers are load-bearing**: code comments and docs cite them as
`agent decision D49`, so a number is never reused or renumbered. Each entry: the decision, its why, and its state today
(built, superseded, or unbuilt). Unbuilt work lives in [GitHub issues labelled `ai`](https://github.com/vdavid/cmdr/issues?q=is%3Aopen+label%3Aai).

Vocabulary: "the agent" is this feature; external MCP consumers are "AI clients"; any future sub-entity is a
"subagent". "AI" stays the umbrella for capabilities (the settings section, provider config, one-shot features).

- **D1**: Two DB families: the per-volume drive index (a regenerable cache) and `main.db` (the durable catch-all). Why:
  regenerable versus valuable data, separate writers, different backup policies. Built; the mutation journal is a
  third, peer durable DB (`operation-log.db`), because a multi-GB append-heavy journal would bloat `main.db` and has its
  own write cadence and retention.
- **D2**: The drive-index files live in `~/Library/Caches/<bundle id>/drive-index/`. Why: the platform-native
  "purgeable, don't back up"; Time Machine skips Caches, and a purge takes the same path as a full reindex. Built, with
  two changes from the plan: a `drive-index/` folder of their own (WebKit and Core ML share that cache dir), so the
  files keep their `index-{volume_id}.db` names rather than becoming `drive-index-{volume_id}.db`. Canonical:
  `crates/cmdr-index/DETAILS.md` § "Where the stores live".
- **D3**: `main.db` is a generic catch-all, not agent-specialized. Why: future durable state lands there too. Built.
- **D4**: No custom collation in `main.db`. Why: it stays inspectable with plain `sqlite3`; the index DB's
  `platform_case` collation forced a custom query tool. Built.
- **D5**: Everything keys by `(volume_id, rel_path)`, and a `volumes` table ships early. Why: NAS, S3, and FTP need
  arrives soon, and retrofitting keys is brutal. Partly built: agent rows key by volume; no `volumes` table, no stable
  volume identity. Reuse the `Location` vocabulary (`src/location.rs`) rather than minting a parallel pair type.
- **D6**: Only the local volume is active at first; SMB, MTP, and S3 knowledge deferred. Why: staleness and reconnect
  semantics differ per volume type; don't block the spine on them.
- **D7**: Staleness is per volume and first-class; the agent caveats answers ("as of May 28"). Why: it makes answering
  about unmounted volumes possible (an offline index of the NAS). Unbuilt.
- **D8**: A deterministic importance scorer gates summaries and event interest, and informs the model. Why: fast, free,
  testable. Built, superseded in placement: a neutral subsystem (`crates/cmdr-index/src/importance/`) with its own
  per-volume `importance.db`, not a column in the drive index.
- **D9**: Folder summaries cover the whole drive at a system-decided depth: deterministic prune, an importance
  threshold, and a `children_worth_descending` list riding each summarize call. Why: one pass; the model refines depth
  only as a byproduct of calls already paid for. Unbuilt.
- **D10**: Summaries feed from the drive index, not the filesystem. Why: the listing tier needs zero extra I/O.
  Unbuilt.
- **D11**: Two summary tiers: listing-only bulk versus content-aware deep. Why: a 10–100x cost cliff; content is for hot
  folders and on-demand asks. Unbuilt.
- **D12**: A cloud model is the summarization default; the local model stays an option. Why: opt-in plus BYO key plus
  the value justifies the cost; "nothing leaves the Mac" stays available. Unbuilt.
- **D13**: Batch APIs rejected for the initial pass. Why: their ~24 h async window conflicts with "summaries ready right
  after indexing".
- **D14**: Hot folders (Downloads, Desktop, Documents, project roots) summarize in parallel with indexing. Why: their
  paths are known up front. Unbuilt.
- **D15**: A preflight shows folder count, cost estimate, and privacy disclosure before any tokens are spent; the walk
  is resumable. Why: transparency, and it settles "how many folders matter" empirically. Unbuilt.
- **D16**: FTS5 over summaries first; embeddings deferred. Why: cheap and good enough for "where do invoices live";
  vectors are regenerable, so adding them later is reversible. Unbuilt.
- **D17**: The agent receives digests, never raw events: a deadline-scheduled inbox, drained whole on any wake. Why:
  bounded context, and a MAX(interest) wake policy falls out for free. Built (`wake/`).
- **D18**: No idle or heartbeat model calls; noise is absorbed deterministically. Why: about zero cost when nothing
  happens. Built.
- **D19**: The digest has a hard token budget and the deterministic aggregator decides granularity. Why: the model
  never sees unbounded input. Built (`wake/compact.rs`).
- **D20**: Restart: a roll-forward produces a normal digest; after a full rescan the agent recovers by asking which
  stored beliefs are stale (`list_stale_summaries`). Why: events are hints, state is truth; this also covers a purged
  cache. The roll-forward half is built (`wake/DETAILS.md`); the stale-belief half waits on summaries.
- **D21**: User actions inside Cmdr are first-class agent input. Why: they carry intent (a manual move, a rejection).
  The mutation half is built (the operation log, the outcome ring); the navigation-intent stream is unbuilt.
- **D22**: Beliefs and rules are Markdown; operational data is SQLite. Why: a human-auditable agent "mind". Built.
- **D23**: A global profile (`~/.cmdr/CMDR.md`) plus `~/.cmdr/rules/*.md` scoped by `applies_to` globs. Why:
  folder-scoped rules without putting files in folders. The profile is built; scoped rules are unbuilt.
- **D24**: Folder-level `CMDR.md` files are cut. Why: a cloned repo or a downloaded zip can carry one, so it's an
  injection vector with authority; `applies_to` covers the need. If ever reintroduced, a folder-level file is
  information about the folder, never authority, unless under a user-marked trusted root.
- **D25**: Agent memory is separate from the user's rules, capped, and auditable. Why: "the user told me" and "I
  inferred" never blur. Built, at `<data-dir>/ai/memory/` rather than the planned `~/.cmdr/memory/`;
  `memory/DETAILS.md`.
- **D26**: No direct write tools; proposals are the only write path, shared by every AI consumer. Why: safety by
  construction, a structural injection defense, one consent surface. Built, and refined: the agent also writes its own
  memory folder (`Access::Memory`), so the invariant is "nothing changes outside that folder".
- **D27**: A proposal freezes at creation: a pattern resolves to a concrete op list, and the pattern survives only as
  display text. Why: no drift between what was shown and what runs. Built (`store/proposals/DETAILS.md`).
- **D28**: Per-op rows with their own statuses, so partial apply is reportable. Why: "apply 11, skip 3 stale" beats
  all-or-nothing. Built.
- **D29**: Drift detection via a per-op `(inode, size, mtime)` snapshot, rechecked at apply. Why: creation and apply
  can be days apart. Built.
- **D30**: Trash over delete by default. Built. Its other halves (op caps per batch, proposal expiry) were decided
  against: a suggestion waits until the user acts, and a 60,000-op group is legitimate (`store/proposals/DETAILS.md`
  § DDL notes).
- **D31**: Standing rules (live patterns that keep applying) are deferred, with their own consent UX. Why: a pattern
  that stays live indefinitely is a different, more dangerous feature than a one-shot proposal.
- **D32**: No `priority` column; the producing model is provenance only. Why: YAGNI; no logic on a model's
  "authoritativeness". Built.
- **D33**: Apply rides the shipped `OperationManager` and op pipeline. Why: zero new write paths; preflight, conflicts,
  progress, and rollback for free. Built (`suggested_ops/` bridge).
- **D34**: No subagents; one agent with job types (wake, chat, planner, summarizer), each with its own prompt, context
  recipe, and model slot. Why: one brain; a hierarchy is unearned. Wake and chat are built; planner and summarizer are
  unbuilt.
- **D35**: A "librarian" is a tool, not an agent. Why: querying summaries is an FTS SELECT; a model in between is
  overhead.
- **D36**: Wake context is fresh each time; continuity lives in the DB and memory. Why: the defining difference between
  an agentic app and a chat app. Built.
- **D37**: Single-flight: chat has priority, wakes queue and their digests merge, and a hot bundle that waits keeps its
  priority. Why: no self-conflicting concurrent writes; late is fine, dropped is not. Built.
- **D38**: Per-wake budgets (tool turns, wall time, reads) plus the house cancellation pattern, including the `ai/`
  layer's stream cancel for an in-flight HTTP call. Why: a runaway loop is impossible by construction. Built.
- **D39**: The proactivity level is chosen at onboarding from named policy bundles and never self-adjusts; after several
  dismissals the agent may ASK "want me to pipe down?". Why: no silent default, no creepy auto-tuning. Unbuilt (an
  on/off toggle and a cadence slider ship instead).
- **D40**: Tier 1 providers: Anthropic, OpenAI, Gemini, and the local model; Tier 2: any OpenAI-compatible endpoint.
  OpenRouter carries the long tail as a user choice, never a default, since it's a middleman in the privacy path. Why:
  a bounded certification surface.
- **D41**: An own `AgentLlm` trait carrying opaque per-message provider state, over the shipped `genai` integration.
  Why: round-tripping thinking state is make-or-break for tool loops, and the trait is the asset. ❌ Never a parallel
  provider layer. Built (`llm/`).
- **D42**: Pinned default models per provider, an "untested" badge on overrides, and evals as the regression suite.
  Why: new-model churn becomes a button press. Pinning is built; the regression suite is unbuilt.
- **D43**: Two model slots: bulk (summarizer) and interactive (wake, chat, planner). Why: different cost and quality
  needs. The interactive slot is built (`askCmdr.interactiveModel`); the bulk slot is reserved as `askCmdr.bulkModel`.
- **D44**: The name is "agent", user-facing and internal. Why: name internals after the UI; honest and specific. Built.
- **D45**: Prompts as Markdown templates with frontmatter, `minijinja` only where needed, dev hot-reload, and a
  `prompt-lint` check. Why: fast iteration, template drift caught in CI. Unbuilt (prompts are Rust source).
- **D46**: Proposal acceptance rate is the north-star metric. Why: it measures suggestion quality directly. Built
  (`suggested_ops/analytics.rs`).
- **D47**: The data-dir rename is decoupled from agent work. Why: a cosmetic change with plugin and migration risk
  mustn't block the agent.
- **D48**: A navigation-intent log is local-only, opt-out, with ~90-day retention. Why: high-signal input needs a
  privacy posture. Unbuilt.
- **D49**: One tool registry, agent-first; AI clients get the same surface. Why: the interface stays natural for its
  primary consumer, with one write path for all AI. Built: the agent extends the consolidated `mcp_tools!` registry
  rather than forking a parallel table.
- **D50**: The index relocation migrates by moving files when cheap; a full rescan is the acceptable fallback. Why:
  kind to existing users without heavy migration code. Unbuilt (see D2).
- **D51**: The agent is enabled from the onboarding AI step, gated on a working key, after the FDA step. Why: it meets
  users where AI setup already happens, and the FDA gate composes. Built (`StepAi`).
- **D52**: Background jobs never materialize dataless cloud files; synced content is readable; dataless content only on
  an explicit ask. Why: no surprise downloads.
- **D53**: The local model is allowed in both slots, labeled honestly, and degrades gracefully: it does less and never
  hard-fails, and after repeated loop failures the agent says politely that a cloud model would do better. Why: local
  is a headline option.
- **D54**: The background-refresh budget defaults to ~$10/month, visible and adjustable. Why: real utility over
  penny-pinching, with the user in control. Unbuilt (waits on summaries).
- **D55**: The execution queue is a separate prerequisite effort. Built (`OperationManager`).
- **D56**: Agent operational state lives in `main.db`; the settings store holds preferences only. Why: backend-written
  state needs a backend home. Built.
- **D57**: Late wakes keep full priority; a notification cap counts only notifications actually shown. Why: the cap
  protects attention, which is only spent on screen. The first half is built; the cap waits on D39.
- **D58**: Every agent tunable is exposed in Settings. Why: better too many dials than hidden behavior. Flagged for
  revisit: the main UI may keep ~3 dials with the tail in an advanced section.
- **D59**: Consumer gating is structural: each registry entry declares its consumers and access, and the agent's
  dispatch view is pinned by set-equality tests. Why: "holds the token but doesn't use it" is policy, not construction.
  Built (`mcp/tool_registry/`).
- **D60**: User-granted autonomy is an auto-apply policy on the proposal pipeline's apply step (a Settings toggle,
  default off), never raw tool exposure; the toggle is changeable only in the Settings UI, never via `set_setting` or
  the agent. A CLI flag was rejected: it's invisible to whoever reviews what the app may do. Why: drop only the review
  click, keep the audit and safety machinery, and make self-enabled autonomy impossible. Unbuilt.
