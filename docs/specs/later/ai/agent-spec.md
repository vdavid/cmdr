# Agent: v1.0 spec (with v1.5+ outlook)

Status: **partially implemented**. The reactive half shipped as **Ask Cmdr** (a read-only chat rail); the proactive half
(summaries, event pipeline, wake loop, proposal store, memory, notifications) is not started. Written 2026-06-04;
codebase claims reconciled against the live tree 2026-07-08 and again 2026-08-18. Section §0 is the current status map
and is the first thing to read; the rest of the spec still states the v1.0 target, so where §0 and a later section
disagree about the tree, §0 wins.

This spec captures a full design session between David and an AI agent. It is written so that a fresh agent (or human)
can pick it up with no other context. Decisions below are settled unless they appear in §18 (open questions); intentions
and principles (§2) govern anything this spec doesn't explicitly answer. §19 is the decision log with rationale, kept as
a second angle on the same material for future planning and implementing agents. Code paths in this spec are relative to
`apps/desktop/src-tauri/` unless noted.

## 0. Status map (reconciled 2026-08-18)

What the tree holds today, section by section. "Shipped" means in `main` with tests and colocated docs.

**Shipped:**

- **The agent subsystem exists**: `src/agent/` (~15k lines), named after the entity per D44, with `CLAUDE.md` +
  `DETAILS.md`. Its first user-facing slice is Ask Cmdr, a read-only chat rail. Frontend: `src/lib/ask-cmdr/`.
- **§4 storage, the durable half**: `main.db` ships with a forward-migration ladder mirroring `operation_log/store/`,
  WAL pragmas, no custom collation, and FTS5 (no rusqlite feature needed; the bundled SQLite compiles it in). Tables
  today: `meta`, `conversations`, `messages`, `cost_meter`. `operation-log.db` ships as the peer durable journal, and
  its `Initiator` enum already carries `Agent` and `AgentEdited` (the spec's "reserved for later" is now built).
- **§5.1 importance scorer**: shipped as its own neutral subsystem, not under `agent/`
  (`crates/cmdr-index/src/importance/`, per-volume `importance.db`, explain call, offline reads, an evals corpus, and a
  scoped incremental rescore at ~100 µs per origin). This supersedes D8's "cached in the drive index"; see §18.15.
- **§9.2/9.3 context assembly and §9.4 runtime discipline**: `agent/chat/` ships the stable prefix, elide-only
  compaction, a user-set chat-memory size, single-flight per thread, per-message budgets, cancellation, crash-safe
  persistence, and a typed `AgentChatEvent` seam.
- **§9.6 IPC**: `commands/agent/` ships the chat, conversation CRUD + FTS, attachment, consent, and cost families over
  typed bindings, with streaming on a Tauri `Channel`.
- **§10.2 provider layer**: `agent/llm/` ships the `AgentLlm` seam (trait, genai impl over `crate::ai::AiBackend`, a
  deterministic fake, a typed message-part model carrying the opaque provider reasoning blob). D41 is built.
- **§11.1 consumer gating**: D59 is built. `mcp/tool_registry/` carries both the `Consumer` and `Access` dimensions; the
  agent's dispatch view is pinned by structural tests to its authored `[agent]` entries, every one `Read` or `Propose`,
  never `Write`. `Propose` additionally needs a hand-authored name in `EXPECTED_PROPOSE_TOOL_NAMES`.
- **§11.2, part of the toolset**: five read families (`app_state`, `list_pane_files`, `list_dir`, `important_folders` +
  `folder_importance`, `list_volumes`) plus shared `operations_list` / `operations_get` and the photo pair
  (`search_photos`, `image_facts`). One `Propose` tool is authored: `propose_rename_plan`.
- **§7, the profile only**: `~/.cmdr/CMDR.md` is read into the stable prefix when present (read-only).
- **§12 consent and cost**: a backend-enforced consent gate (`agent/consent.rs`, `CONSENT_COPY_VERSION`, record in
  `main.db`'s `meta`, fails closed) plus a per-day, per-thread cost meter with an honest unpriced path.
- **§12.1 enable flow, the wizard half**: the onboarding wizard ships `StepFda` and `StepAi`.
- **§16, part of settings**: `askCmdr.interactiveModel` (the interactive slot layered over shared `ai/` provider config,
  so the bulk slot slots in with no migration) and `askCmdr.chatMemorySize`.

**Not started:**

- **§4.1 index relocation**: the per-volume `index-{volume_id}.db` files still live in the app data dir. Nothing moved
  to `~/Library/Caches/<bundle id>/`, nothing was renamed to `drive-index-{volume_id}.db`.
- **§4.2, the rest of the schema**: no `volumes`, `folder_summaries`, `proposals`, `proposal_ops`, `agent_log`,
  `agent_inbox`, `walk_state`, or `user_action_log` tables, and no retention pruning in `main.db`.
- **§5.2, §5.3 summarization**: no summarizer, no walk, no preflight, no summary FTS. The agent's knowledge today is the
  drive index, importance, the operation log, live app state, and image-derived text.
- **§6 event pipeline**: no coalescer, interest scorer, inbox, deadline scheduling, digest compaction, or restart
  reconciliation for the agent. §18.14's tap point is still undesigned.
- **§8 proposals as durable state**: `propose_rename_plan` stages into an in-memory `RenameProposalStore` keyed by
  opaque id, reviewed in `BulkRenameReviewDialog` and undoable after apply. There is no `proposals` / `proposal_ops`
  table, no freeze-at-creation snapshot, no drift detection, no expiry, no invalidation, and no `OperationManager` batch
  apply. §8.5 auto-apply does not exist.
- **§7, rules and memory**: no `~/.cmdr/rules/*.md` with `applies_to`, no `~/.cmdr/memory/`, no memory writes.
- **§9.1 job types**: only chat exists. No wake, planner, or summarizer job.
- **§9.5 notifications**: no `notify_user` tool, no proactivity dial, no per-folder mute or snooze.
- **§10.4 bulk slot**: only the interactive slot is settable.
- **§12 activity log**: no `agent_log` table and no activity or read-log surface. Transparency today is the rail's
  per-tool lines.
- **§14 prompts as repo assets**: prompts are Rust source, not markdown templates. No `prompt-lint` check.
- **§15 evals**: seeded only (`agent/tools/propose/name_quality_eval.rs`, plus the importance evals corpus). No
  synthetic-home fixture generator, no summarizer or planner scoring harness.

**Changed since the spec was written, and the spec text below is stale where it says otherwise:**

- **genai is pinned `=0.6.5`**, not `=0.6.0-beta.19`, and it is no longer a beta. §18.1's supply risk is reduced but not
  gone (still a solo-maintainer crate carrying the whole provider layer).
- **The `read_file` tool of §11.3 was not built, and the privacy line moved with it.** The agent has no content-read
  tool at all: only names, paths, and metadata reach the provider, plus one deliberate exception, the image-derived TEXT
  of `search_photos` and `image_facts` (OCR snippets and Vision tags, never image bytes), which the consent copy names.
  §11.3's guardrails describe a tool that does not exist; adding it means revisiting the whole consent story, not
  implementing a designed feature.
- **§11.1's "factor a transport-agnostic core first" did not happen and was not needed.** `execute_tool` is still
  generic over the Tauri `Runtime` and handlers still take `&AppHandle<R>` plus a `serde_json::Value`. The in-process
  agent consumes that shape directly. The bounded refactor named there is not a prerequisite for anything left.
- **A `Propose` tool exists.** `agent/DETAILS.md` still says the allowlist "is empty today"; it holds
  `propose_rename_plan`. Its bounding contract (cap the payload, pin it with a test) is live, not hypothetical.
- **Evidence grounding is a new invariant the spec never anticipated.** A proposal citing file contents must prove the
  model actually received them: `propose::evidence::ImageFactsLedger` is scoped per chat thread, and
  `propose_rename_plan` refuses a plan citing content the ledger has no delivery for. This exists because 12 real files
  got fabricated names. Any future `Propose` tool inherits the obligation. See
  `apps/desktop/src-tauri/src/agent/tools/propose/DETAILS.md` § Evidence, and invariant 6 in
  `apps/desktop/src-tauri/src/agent/DETAILS.md`.

## 1. What this is

A persistent agent that lives inside Cmdr. It builds and maintains compressed knowledge of what lives where in the
user's file system, watches file system events and user actions, and proactively suggests file operations (tidy
Downloads, unpack that dataset into the right project folder, name those screenshots), which the user reviews and
applies. It also answers questions ("where do we usually store invoices?") through a chat surface.

It is opt-in, BYO API key, and it never touches files directly: it can only propose. The user applies proposals through
a review dialog, and execution runs through Cmdr's existing, hardened file operation pipeline.

UI is intentionally out of scope here beyond naming the surfaces: a review dialog for proposals, a chat surface, and
notifications. This spec is about agent behavior, inputs, outputs, storage, context, and memory.

## 2. Principles (these govern everything below)

1. **Deterministic bottom, LLM top.** Cheap, fast, testable Rust handles everything with an obviously correct answer
   (event coalescing, importance scoring, staleness, digest compaction, proposal validation). The LLM is reserved for
   judgment and language. Never put a model in a per-event hot path.
2. **The agent costs ~zero when nothing interesting happens.** No idle wakes, no heartbeat LLM calls. Noise is absorbed
   deterministically (counters and staleness marks); it reaches the model only as one digest line the next time the
   agent wakes for a real reason.
3. **Propose, never act.** The agent has no write tools. Its only write path is the proposal queue, gated by user
   review, executed by the existing file-op pipeline (preflight, conflicts, progress, rollback, trash). This is also the
   structural prompt-injection defense: file contents are an untrusted input, and the worst a malicious file can achieve
   is a weird suggestion sitting in a review queue.
4. **Continuity through state, not transcript.** The agent does not carry its life story in its context window. Durable
   knowledge lives in the database and in markdown memory; each wake gets a fresh, budgeted context assembled from
   state. Only chat threads keep (bounded) transcripts.
5. **Radical transparency, applied to the agent itself.** Every decision, every proposal, and every file the agent reads
   is logged with a rationale and visible to the user. "The agent read these 3 files, here's why" does more for trust
   than any copy.
6. **Derived data lives in the database; beliefs and rules live in markdown.** Summaries, proposals, and logs are
   operational state (SQLite). What the user tells the agent and what the agent infers about the user are
   human-auditable markdown files the user can open, edit, and delete.
7. **Events are liveness hints; state is truth.** The event stream will have gaps (app closed, volume disconnected,
   cache purged). Reconciliation against indexed state and stored fingerprints is the recovery mechanism, never event
   replay.
8. **Don't gamble the user's trust.** Anti-noise etiquette is policy, not vibes: hard caps on proactive notifications, a
   user-chosen proactivity level, per-folder mute, and no repeats after a rejection.

## 3. Scope

### v1.0

- Storage split: the per-volume drive index (cache) and `main.db` (durable), schemas in §4.
- Multi-volume **keying** everywhere from day one; only the local main volume is active in v1.
- Deterministic importance scorer with weights cached in the drive index.
- Folder summaries: hot folders first, then importance-gated whole-drive pass; FTS search over summaries; preflight cost
  estimate; resumable.
- Event pipeline: coalescer, interest scoring, inbox with deadline scheduling, token-budgeted digests, restart
  reconciliation.
- The four LLM call types (wake, chat, planner, summarizer) with the context anatomy in §9.
- Proposals: batches with per-op rows, freeze at creation, drift detection, review and apply, invalidation, expiry.
- `~/.cmdr/` profile, scoped rules, and agent memory files.
- Tool registry serving the agent as first consumer (external AI clients are the second).
- Provider layer: Tier 1 = Anthropic, OpenAI, Gemini, plus the local model option; Tier 2 = any OpenAI-compatible
  endpoint.
- Activity log, read log, cost meter with per-job attribution, proactivity dial, degraded modes.
- Unit tests for all deterministic parts; a v0 eval harness with synthetic fixture folders.

### v1.5 (named and deferred)

- Multi-volume summaries opt-in (NAS/SMB first), with per-volume staleness and reconciliation.
- Memory mining from implicit signals (rejections, manual moves) into proposed memory entries.
- Natural-language search integration: the search box's AI path uses `search_summaries` as a tool.
- Archive ops in proposals (extract/compress). No longer blocked: zip browse + edit shipped, with read-only tar/7z and
  remote-hosted zips (`file_system/write_operations/archive_edit/`, `ArchiveVolume`). The proposal `op` enum grows the
  archive verbs and the executor routes them through the existing archive-edit driver.
- Eval harness v1 doubling as the provider/model regression suite with pinned certified models.

### v2 / later

- Standing rules (live patterns that keep applying), with their own consent UX. Explicitly NOT in v1: a pattern that
  stays live indefinitely is a different, more dangerous feature than a one-shot proposal.
- Subagents (if ever). v1 has job types, not agent hierarchies; see §9.1.
- Folder-level `CMDR.md` files, cut from v1. If reintroduced, they need trust tiers: a folder-level file is information
  about the folder, never authority, unless under a user-marked trusted root (a cloned repo or downloaded zip can
  contain one; injection vector).
- Claude-skills-format support in `~/.cmdr/` (revisit when the plugins roadmap item lands).
- S3/FTP/WebDAV summaries (the keying supports them from day one).
- Local-only daemon mode (agent running while the app is closed).

## 4. Storage

### 4.1 Two databases

| DB                                        | Location                               | Nature                                                                                              | Backup                            |
| ----------------------------------------- | -------------------------------------- | --------------------------------------------------------------------------------------------------- | --------------------------------- |
| `drive-index-{volume_id}.db` (per volume) | `~/Library/Caches/<bundle id>/`        | Regenerable cache: the drive index (per-volume `index-{volume_id}.db` files, renamed and relocated) | None; Time Machine skips Caches   |
| `main.db`                                 | `<app data dir>` (Application Support) | Durable catch-all: summaries, proposals, logs, conversations, action history                        | Time Machine picks it up normally |

The existing drive index is already **per-volume** (`index-{volume_id}.db`, per `indexing/CLAUDE.md`), not a single
file. The relocation is therefore N files (or a `drive-index/` subdirectory), the naming keeps the volume id, and it
composes with the multi-volume keying in §4.3 rather than colliding with it. Migration for existing installs (decided):
prefer moving the files, a same-volume rename expected to be cheap and nicer for the existing testers; if it turns out
nontrivial, a one-time full rescan on upgrade is acceptable. Note the Caches path uses the **current** bundle id (a
separate effort may rename these directories to friendlier names; it is independent of this work). Hereafter "the drive
index" means this per-volume DB family.

Rationale: regenerable vs. valuable data, different lifecycles, different backup policies, and it splits the writers
(the indexer hammering the cache never contends with agent writes). Putting the cache in `~/Library/Caches/` is the
platform-native way to say "purgeable, don't back up": macOS may purge it under disk pressure, and that is acceptable
(it triggers the same reconciliation path as a full reindex, §6.4).

`main.db` is deliberately a catch-all, not specialized: future durable state lands here too. **Exception — the
file-mutation journal is NOT a `main.db` table.** It ships as its own durable `operation-log.db` (the operation log): a
multi-GB append-heavy per-mutation journal would bloat `main.db`'s backups and defeat its small-inspectable-catch-all
nature, and the two have different write cadences (per-mutation vs agent-episodic) and different retention. `main.db`
stays the agent's durable catch-all; the mutation journal is a peer durable DB. The split serves D3's "generic
catch-all" intent better (keeps `main.db` small) than a giant table would. The two DBs share the same discipline (no
collation, forward migrations, retention), so `operation-log.db` is the built, working reference implementation of that
discipline for `main.db` to follow.

Constraints:

- **No custom collation in `main.db`.** It must stay inspectable with plain `sqlite3`. (The index DB's `platform_case`
  collation forced the `index-query` tool; don't repeat that.)
- `schema_version` table plus forward migrations from day one. This DB lives for years.
- Retention policy per log-like table (prune by age and row cap on startup). `main.db` must not grow unboundedly.
- Note: `main.db` is itself sensitive (it is a map of the user's life). It stays local; nothing in it is ever
  transmitted.

### 4.2 `main.db` schema (v1 shape)

Names are indicative; the implementing agent owns the final DDL.

- `volumes`: `volume_id` PK, `kind` (local | smb | mtp | s3 | ftp | ...), `stable_identity`, `display_name`,
  `index_opt_in`, `summary_opt_in`, `last_reconciled_at`.
- `folder_summaries`: `(volume_id, rel_path)` PK, `summary` TEXT, `generated_at`, `model`, `listing_fingerprint` (what
  the summary was computed from, for staleness), `stale` flag, `interest_weight` (denormalized copy; the authoritative
  cached weights live in the drive index). Plus an **FTS5** index over `summary` and `rel_path`. Embeddings are
  explicitly deferred: FTS first, add vectors later only if FTS disappoints (they are regenerable, so this is
  reversible).
- `proposals` (batch level): `batch_id` PK, `created_at`, `op_display_name` (friendly name, may include the originating
  pattern as display text, e.g. "All installers you've already opened (~/Downloads/\*.dmg with last-open > creation)"),
  `rationale`, `status`, `created_by_model`, `expires_at`.
- `proposal_ops` (op level): `op_id` PK, `batch_id` FK, `op` (move | rename | trash | mkdir; extract later),
  `source_volume_id`, `source_rel_path`, `dest_volume_id`, `dest_rel_path`, `status` (per-op, enabling partial apply),
  `snapshot_inode`, `snapshot_size`, `snapshot_mtime` (drift detection), `executed_at`, `result`.
- `agent_log`: `ts`, `source` (detector | wake | planner | chat | summarizer), `kind` (proposal | notify | memory*write
  | file_read | observation | error), `target`, `rationale`, `model`, `tokens_in`, `tokens_out`, `latency_ms`. This
  feeds the activity UI and is also context input (recent rejections, §9.2). Terminology mapping, since the spec uses
  several names: the user-facing **"activity log"** IS this table; the **"read log"** is its `kind = file_read` filtered
  view, not a separate table; `user_action_log` (below) is separate because its writer is the user, not the agent. **No
  term collision with the operation log**: that journal is the **"operation log"** (its rows are operations), this
  `agent_log` is the agent's **decision** log, and **"action"** stays reserved for the `user_action_log`
  navigation/intent stream. Their future \_merged UI surface* may be labelled **"Activity"** then (a UI-copy call, not
  an entity/table name) — so a future planner reads the three surfaces correctly and doesn't re-collide the terms.
- `conversations` + `messages`: chat threads. A notification the user replies to becomes a thread carrying the
  originating wake's context.
- `agent_inbox`: pending event bundles (persisted so a crash loses nothing): `bundle_id`, `volume_id`, `rel_path`,
  `counters` (JSON), `interest`, `deliver_by`, `created_at`.
- `cost_meter`: per-day, per-job-type (initial_index | refresh | wake | chat | planner) token and cost accounting.
  Powers the spend display and the budget caps, and makes "why did this cost $4" answerable.
- `user_action_log`: **navigation/intent only** (a high-signal intent source, §6.1). Local only, opt-out setting,
  default retention ~90 days. **The operations half of this table's original mandate moved out**: file mutations (copy /
  move / trash / rename / compress) are now journaled by the durable `operation-log.db` (the operation log), which is
  richer than this table's sketch (per-item rows, snapshots, rollback linkage). So this table is navigation/intent
  signals, NOT mutations. The three-way boundary, stated so no future effort builds a second operations recorder:
  **mutations → `operation-log.db`; navigation counts + recency → `importance.db`'s `record_visit` today (folding into
  the agent's intent stream later); this `user_action_log` → navigation/intent events.** The operation log records
  mutations only, never navigation.
- `walk_state`: resumable summarization-walk bookkeeping.

### 4.3 Multi-volume identity

Everything keys by `(volume_id, rel_path)`, never absolute paths. The `Volume` trait grows a `stable_identity()` used by
both the drive index and the agent: APFS UUID for local volumes, server+share for SMB, device serial for MTP,
endpoint+bucket for S3. The need for non-local volumes arrives within weeks of v1 (NAS indexing is a personal target),
so the keying ships in v1 even though only the local volume is active.

Navigation now has a first-class `Location` type (`src/location.rs`, a `(volume_id, path)` pair with a
`resolve_location` resolver; the `location-type-nav` effort that kills bare-path navigation is in progress). The agent's
`(volume_id, rel_path)` keying should reuse that vocabulary rather than mint a parallel pair type, so navigation and the
agent name the same thing the same way.

Per-volume staleness is a first-class property, not an error: summaries carry their `as_of` fingerprint, volumes carry
`last_reconciled_at`, and the agent caveats answers ("as of May 28").

**Headline consequence worth building toward:** the agent can answer questions about volumes that are not currently
mounted ("where's that 2024 photo backup?" answered from NAS summaries while the NAS is off). Summaries become an
offline index of unmounted drives. Nobody else has this.

Volume-type notes for v1.5:

- SMB: events are surprisingly reliable while connected, but a disconnect leaves no backlog, so reconnect means "mark
  volume needs-reconciliation, importance-gated rescan, diff into a digest" (the same mechanism as app restart, §6.4).
- MTP: scanning is expensive and devices detach quickly; summaries are on-demand only, never background.

## 5. The knowledge layer

### 5.1 Deterministic importance scorer

A fast, pure-Rust algorithm assigning each folder an interest weight. Inputs (hardcoded heuristics, tunable):

- Known-unimportant names (`node_modules`, caches, build artifacts, `.git` internals); hidden dirs; system vs. user
  ownership.
- File extensions present, and the **diversity** of extensions (monoculture folders like logs score low).
- Modification recency; last-opened recency where available (macOS: `kMDItemLastUsedDate` via Spotlight metadata;
  per-item MDItem queries are slow, so sample rather than sweep; atime is unreliable).
- Cmdr's own navigation signals: folders the user actually visits (tab history, last-used paths).
- A `.git` root (or similar project marker) raises the subtree: projects are important.
- Path class priors: Downloads, Desktop, Documents, project roots high; `~/Library`, caches low.

**This section shipped**, as its own neutral subsystem rather than agent-owned code:
`crates/cmdr-index/src/importance/`, with weights in a separate per-volume `importance.db` (a regenerable cache), an
explain call, offline reads for unmounted volumes, an evals corpus, and a scoped incremental rescore costing O(touched)
rather than O(dirs). This supersedes D8's "cached in the drive index"; the plan is
`docs/specs/later/importance-subsystem-plan.md` and the durable intent lives in the `importance/` and `indexing/`
colocated docs. The list above stays the requirements source. The weight serves three consumers: summary generation
gating, event-bundle interest (§6.2), and as an input the LLM sees when reasoning about folders. The agent already reads
it through the `important_folders` and `folder_importance` tools.

Still open here: signal-weight tuning (§18.3) and the `kMDItemLastUsedDate` sampling cost (§18.4).

Expectation check: David expects a typical user to have only dozens to a few hundred genuinely important folders. The
design does not depend on that guess being right: the pre-scan counts before anything costs money, and budgets cap the
tail (§5.3).

### 5.2 Summarization

One agent-generated summary per folder, at a depth the system decides. The naive version of "the agent decides the
depth" is an LLM call per directory node; the actual design keeps it one pass:

- **Deterministic pruning first.** The importance scorer excludes the obvious (a `node_modules` gets, at most, one line
  in its parent's summary), and an importance threshold gates which folders get summarize calls at all.
- **The descend decision rides the call you already paid for.** Each summarizer call returns
  `{summary, children_worth_descending}`. The LLM refines the walk only inside the ambiguous band, and each refinement
  is a byproduct of a summary you wanted anyway. Walk top-down in importance order.
- **Feed from the drive index, not the filesystem.** Names, sizes, and mtimes for the listing-only tier come from the
  drive index with zero extra disk I/O.
- **Two tiers with a cost cliff between them.** Listing-only summaries (metadata in, summary out) are the cheap bulk
  tier. Content-aware summaries (file heads/samples included) cost 10-100x and are reserved for hot folders and
  on-demand requests.
- **Pack siblings.** Many small folders go into one call; never call-per-folder.
- **Hot folders run early and in parallel with drive indexing.** Downloads, Desktop, Documents, and detected project
  roots are known a priori and don't need to wait for the full index.

Model choice: **cloud by default** (the feature is opt-in, BYO key, and the value justifies the upfront cost; deliberate
decision over local-first). The **local model remains a supported option** (see §10.4) for users who accept the quality
tradeoff.

Refresh policy needs a budget or drift quietly re-burns the drive: refresh on access, event-driven for hot folders, and
a monthly token cap for everything else. Staleness detection via `listing_fingerprint` (hash over child names, sizes,
and mtimes; exact definition is an implementation detail to pin down). Summaries do NOT regenerate on model switch; the
`model` column is provenance, and refreshes happen opportunistically.

### 5.3 Preflight (first run)

Enabling the agent runs the free, deterministic pre-scan and shows a preflight before any tokens are spent:

> I found ~N folders worth reading. Initial read with {model}: ~$X, roughly N minutes. File and folder names will be
> sent to {provider}; file contents only where you allow it.

Cancelable, resumable (`walk_state` in `main.db`), with progress. Batch APIs were considered for the initial pass (~50%
off) and rejected: their async ~24h window doesn't suit "summaries should exist right after indexing finishes." Rate
limits still require a concurrency-limited drip with retry/backoff.

## 6. Inputs: events and digests

### 6.1 Sources

- **File system events**: consume the indexer's existing event stream, not a parallel raw FSEvents subscription. The
  indexer already coalesces, dedups, and batches FS events (its own flush window, replay vs. live loops, verifier
  corrections, per `indexing/`); the agent's coalescer (§6.2) is a second, interest-oriented stage over that
  already-corrected stream (subscribe, don't poll; don't duplicate dedup machinery). Exact tap point, and how this
  relates to the standalone `downloads/` watcher (already shipped, so this is integration with an existing subsystem,
  not co-design), is an open question (§18.14). Index roll-forward on startup feeds the same path (§6.4).
- **User actions inside Cmdr**: operations and navigation, logged to `user_action_log`. These are the highest-signal
  events because they carry intent: a user manually moving three PDFs from Downloads to `~/Dropbox/invoices` is a
  preference worth learning; a rejected proposal is implicit feedback. Actions done outside Cmdr (Finder) appear only as
  their FS-event results; that's acceptable, they're just lower-signal.

### 6.2 Pipeline

```
FS events + user actions
   → coalescer (per-folder counters in a window)
   → interest scorer (deterministic, §5.1)
   → inbox bundles with deliver_by deadlines
   → WAKE → drain the whole inbox into one digest
```

The agent never receives raw events. It receives a digest of everything since its last wake; the pipeline's only real
output decision is **when to wake it**:

- Each bundle gets `deliver_by = now + f(interest)`: hot (new file in Downloads, new file in a folder with matching
  rules) ~2-5s; warm ~1-5 min; cold ~1h. Exact tier values need tuning (§18).
- Any wake, for any reason (a hot deadline, a user question, a proposal invalidation), drains the entire inbox, so cold
  bundles ride along for free. A `MAX(interest)` policy falls out implicitly.
- **No minimum wake frequency.** If only uninteresting things happen, the agent simply does not run. Noise (the 10,000
  log-file changes) is fully absorbed by the deterministic layer: counters incremented, summaries marked stale, zero LLM
  involvement. The noise becomes one line of situational awareness in the next real digest.

### 6.3 Digest compaction

The digest has a **hard token budget** (~2-4k; tune later) and the deterministic aggregator fills it in importance order
with hierarchical compaction: "5M changes in /tmp/log" is one line; the interesting tail gets per-folder granularity
until the budget is full. The aggregator decides granularity, never the LLM.

Testability seams (per the project's design-for-testability rule, name them at write time): the coalescer is a pure
`coalesce(events, window) -> bundles`, and the compactor is a pure `compact(bundles, token_budget) -> digest`. Both take
values in and return values out, no I/O, so the §15 unit tests construct inputs directly.

### 6.4 Restart, gaps, and reconciliation

On app restart, the indexer either rolls forward the backlog of FS events (up to ~10M) or, beyond that, performs a full
rescan. Correspondingly:

- **Roll-forward path**: the coalescer runs over the rolled-forward changes and produces a normal (budgeted, compacted)
  digest: "5M changes in /tmp/log, 450k in node_modules, and these 200 interesting changes in detail."
- **Full-rescan path** (also: macOS purged the Caches DB): the diff is lost. The digest then says so: "the app was
  closed between X and Y; a full reindex happened; the diff is unknown." The agent recovers via a deterministic tool,
  `list_stale_summaries(min_interest)`, which diffs current index state against the stored summary fingerprints. "We
  don't know what changed" becomes "ask the DB which of your beliefs are stale."

Both paths express principle 7: events for freshness, state for truth, reconciliation over replay.

### 6.5 Degraded modes

No API key, provider down, rate-limited, offline: the agent silently downgrades to deterministic-only operation (absorb
events, mark staleness, queue work) and never affects the file manager. A subtle status indicator, no error spam.
**Pile-up is bounded on both sides**: the digest token budget bounds the context side, and queued work folds into
reconciliation rather than replaying, bounding the work side.

## 7. Rules, profile, and memory

Layout (user-authored content lives in a friendly dotdir, machine state in the app data dir):

- `~/.cmdr/CMDR.md`: the global profile, always loaded into every agent context. Personal info, preferences, standing
  guidance.
- `~/.cmdr/rules/*.md`: modular rules with optional YAML frontmatter `applies_to: <glob>` patterns. This gives
  folder-scoped rules WITHOUT placing files in folders.
- `~/.cmdr/memory/*.md`: agent-written takeaways. Deliberately a separate directory from `rules/` so "what the user told
  me" and "what I inferred" never blur. User-auditable: open, edit, delete. Size-capped, deduplicated, and every write
  is logged to the activity log.

Folder-level `CMDR.md` files are cut from v1 entirely (see §3 later-scope for the trust-tier design if they return). The
`applies_to` mechanism covers folder-specific rules without the pollution or the injection surface.

DRY references work because the agent has a read-file tool: a user can write "see `~/.claude/CLAUDE.md` for my profile"
and the agent follows it (within the read-tool guardrails, §11.3).

The markdown/DB line (principle 6): summaries, proposals, and logs are DB; beliefs and rules are markdown.

## 8. Proposals

### 8.1 The contract

The agent's only write path, shared by every AI consumer (§11.1): the internal agent, and any external AI client. One
review surface gates "an AI wants to touch your files," regardless of which AI.

### 8.2 Freeze at creation

The agent may _think_ in patterns ("delete `~/Downloads/*.dmg` that you've already opened"), but the proposal tool
resolves the pattern to a **concrete op list at creation time**. The pattern survives only as display text in
`op_display_name`. The review dialog shows the friendly name and expands to the exact file list.

Because creation and apply can be days apart, frozen lists carry drift detection: each op snapshots
`(inode, size, mtime)` at creation, and the executor re-verifies at apply. A mismatch flips that op to `invalidated`
rather than operating on a changed file.

### 8.3 Lifecycle

```
proposed → accepted (= user clicked apply) → executing → executed | failed
        → rejected
        → expired      (proposals auto-expire after days; stale suggestions are worse than none)
        → invalidated  (drift detected, or an FS event touched a source)
```

Per-op statuses enable **partial apply**: if 3 of 14 ops went stale, the dialog applies 11 and reports 3 skipped, user's
choice, never all-or-nothing.

Invalidation plumbing: an incoming FS event affecting any op's source marks the op (deterministic), revalidates cheaply
where possible, and queues an inbox bundle so the agent learns its earlier suggestion was affected.

### 8.4 Execution

"Approve" means "apply". Applied batches run through the existing `write_operations` pipeline: preflight, conflict
handling, progress, cancellation, rollback. Destructive ops default to **trash**, not delete. Batches are capped (a few
hundred ops) to keep review usable; large cleanups chunk into multiple batches.

The queue this depends on has shipped: `file_system/write_operations/manager.rs` (`OperationManager`) is a lane-based
queue with a transfer-queue window, copy/move/delete spawn via `spawn_managed`, and rename/mkdir/mkfile run as managed
instant ops via `run_instant`. Crucially the pipeline is headless-callable: writes emit through the `OperationEventSink`
trait (`event_sinks.rs`), built only at the IPC edge and injected in (production `TauriEventSink`, test
`CollectorEventSink`), so the managed write path no longer needs Tauri. That is what a proposal executor needs. The
remaining work is therefore a **fit-check against the shipped manager, not a prerequisite effort**: whether it accepts a
batch of ops as one unit with per-op statuses and partial apply (the `proposal_ops` table keys apply on the op subset),
and whether it reports a per-op result the `proposal_ops` table can consume. Design the apply call against
`OperationManager`'s API, and file any batch-semantics gaps as small extensions to it.

Because applied proposals execute through the managed pipeline, **the operation log journals every applied proposal
batch for free** (the operation log hooks that pipeline). A _rejected_ proposal never becomes an operation, so it never
appears in that journal — the `proposals` / `proposal_ops` tables above hold the "agent-suggested / accepted / rejected"
pre-operation states and reference `operations.op_id` for the ops that were accepted and executed. Applied ops are
tagged `initiator = agent` in the journal; that value is **built, not reserved** (`operation_log::types::Initiator`
ships `User`, `AiClient`, `Agent`, and `AgentEdited`, the last for an agent proposal the user edited before applying).
So the journal is the execution record; the proposal tables are the decision record — don't smear proposal states into
the operations journal.

Dropped from the earlier sketch, deliberately: a `priority` column (YAGNI) and any logic on model "authoritativeness"
(`created_by_model` is kept as provenance only).

### 8.5 Autonomy: auto-apply as a user-granted proposal policy (decided)

Some users will want "my agent can do stuff without me confirming each time." That grant is a **policy on the proposal
pipeline's apply step, never raw tool exposure** (§11.1): an auto-apply-enabled agent still emits frozen proposal
batches, and the system applies them — keeping drift detection, per-op statuses, trash-default, batch caps, the activity
log, and (via the operation log) rollback, exactly as if the user had clicked apply. Turning the dial up loses the
review click, not the audit trail or the safety machinery.

- The grant is a Settings toggle (default OFF; see §16), living next to the proactivity dial. v1 can ship it coarse (off
  / auto-apply); scoping it (per-folder, per-op-kind, batch-size ceiling) is a later refinement with the same shape.
- **Agents can't self-enable it.** The setting joins a small `protectedSettings` set in the settings registry that the
  MCP `set_setting` tool refuses regardless of bearer token, and the in-process agent has no `set_setting` at all (§11.1
  consumer view). Changing it is a Settings-UI-only act — the same class of consent as granting Full Disk Access.
- Rejected alternative: a CLI flag. Nobody launches a GUI file manager from a terminal in normal use, and a flag is
  invisible to the person reviewing what the app is allowed to do; Settings is where consent lives and stays auditable.
- Auto-applied batches still notify (a "the agent moved 12 files — review / undo" toast riding the notification
  etiquette of §9.5), and rejections/undo feed back as implicit signals like any other rejection.

## 9. The agent runtime

Rust, in-process, under `src-tauri/src/agent/` (inbox/coalescer, interest, summaries, proposals, memory, tools, llm
loop, notify). The frontend gets display surfaces only.

### 9.1 Job types, not subagents

There are no subagents in v1. There is one agent with four **job types**, each with its own prompt, context recipe, and
(configurable) model:

| Job        | Trigger                                 | Context                                                  | Model setting     |
| ---------- | --------------------------------------- | -------------------------------------------------------- | ----------------- |
| Wake       | Inbox deadline / invalidation           | §9.2, fresh every time                                   | Interactive model |
| Chat       | User message                            | §9.3, thread-scoped                                      | Interactive model |
| Planner    | A wake decides a situation needs a plan | Wake context, focused on one situation, longer tool loop | Interactive model |
| Summarizer | Knowledge-layer walk                    | Tiny: listing in, summary + descend-list out; no profile | Bulk model        |

A "librarian" was considered and rejected: querying summaries is an FTS SELECT; putting an LLM between the agent and the
database is overhead. It's a tool (§11.2). If subagents ever arrive, they'll be called subagents.

### 9.2 Wake context anatomy (fresh every time, no chat history)

1. System prompt: role, hard rules (propose only; etiquette caps; "doing nothing is usually correct").
2. `~/.cmdr/CMDR.md` + rules whose `applies_to` matches the involved paths.
3. Retrieved memory, scoped to involved paths/topics, never all of it.
4. The digest (§6.3).
5. Folder summaries for affected paths.
6. Open proposals touching the same paths, plus an activity-log tail **including recent rejections** (so it never
   re-suggests what the user just declined).

Budget: roughly 5-10k tokens. The stable prefix (system, profile, rules) goes first for provider prompt caching.

### 9.3 Chat context anatomy

Stable prefix + this thread's recent turns verbatim + older turns summarized. Other threads are reachable via a search
tool, never auto-loaded. A notification the user replies to becomes a thread that inherits the originating wake's
context.

### 9.4 Concurrency, budgets, and cancellation

- **Single-flight**: one LLM job at a time per agent. Chat takes priority; wakes queue behind it and their digests merge
  while waiting. A hot bundle whose deadline passes while the slot is busy keeps its full priority and urgency at the
  next wake: late is fine, dropped is not.
- Per-wake budgets: max tool turns, max wall time, max file reads. A runaway loop must be impossible by construction.
- Cancellation follows the house pattern (`AtomicBool`, checked at tool-call boundaries); agent activity is visible and
  killable like any long-running Cmdr task. One nuance: an in-flight provider HTTP call is a network round-trip an
  `AtomicBool` cannot interrupt, the same known gap architecture-patterns.md documents for blocking syscalls. The
  existing `ai/` layer already has a stream-cancel mechanism for exactly this; the agent loop reuses it so an LLM call
  in flight cancels within the design budget, with the `AtomicBool` covering tool-call boundaries.

### 9.5 Notifications and the proactivity dial

- A `notify_user` tool with action buttons (review / apply / dismiss / open chat). Etiquette is policy: max proactive
  notifications per day, confidence floor, no repeats after a rejection. The daily cap counts only notifications
  actually shown; an attempt that never reached the screen didn't spend the user's attention, so it doesn't spend the
  cap.
- The proactivity dial is a setting with ~4 named, hard-coded policy bundles (off / quiet / normal / eager) mapping to
  interest thresholds and caps. **Chosen during the agent-enable onboarding, no silent default** (a too-quiet default
  reads as "the feature does nothing"; too eager is noise); "Normal" is pre-highlighted as the recommendation.
  Per-folder mute and "snooze today" exist at every level.
- Auto-throttling is never silent: after several consecutive dismissals the agent may _ask_ "want me to pipe down?",
  which is on-brand; it never changes settings by itself.

### 9.6 IPC surface (indicative)

The spec body deliberately stays behavior-level, but the project is opinionated about IPC (typed `tauri-specta`
bindings, subscribe-don't-poll), so here is the indicative surface a fresh agent should expect to build. Names are
placeholders; the implementing agent owns the final list.

- Commands: `agent_enable` / `agent_disable`, `agent_get_status`, `agent_preflight_start` / `agent_preflight_cancel`,
  `agent_chat_send`, `agent_get_proposals`, `agent_apply_proposal_batch(batch_id, op_ids)` (the op subset enables
  partial apply), `agent_reject_proposal_batch`, `agent_get_activity_log(page)`, `agent_get_spend`,
  `agent_set_proactivity`, `agent_mute_folder`, `agent_snooze_today`.
- Events (push, never poll): `agent-activity` (new activity-log rows), `agent-proposal-changed`
  (created/updated/invalidated/expired), `agent-notify` (the notification payload with actions),
  `agent-preflight-progress`, `agent-chat-delta` (streamed replies), `agent-status-changed` (degraded modes, §6.5).
- All of it goes through the typed bindings per the AGENTS.md IPC rules. Frontend IPC routes through the
  `src/lib/tauri-commands/` wrapper layer, with a lint (`cmdr/no-raw-bindings-import`) forbidding raw `bindings` imports
  outside it, so the agent's frontend commands get a `tauri-commands/agent.ts` wrapper rather than calling generated
  bindings directly. The review dialog and activity panel are pure consumers of these commands and events.

## 10. The LLM provider layer

### 10.1 Why "hot-swappable providers" is false for agents

Single-shot prompts are interchangeable across providers; agent loops are not. The quirks:

1. **Wire shape**: OpenAI returns `tool_calls` answered by `role:"tool"` messages keyed by id; Anthropic returns
   `tool_use` blocks answered by `tool_result` blocks in the next user message (and errors if any id goes unanswered);
   Gemini returns `functionCall` parts answered by `functionResponse` parts in order, where `response` must be an
   object, never a scalar.
2. **Parallel tool calls**: all providers can emit several calls per turn, each with different batch-answer rules;
   mishandling ranges from API errors to silent degradation.
3. **Opaque reasoning state (the nastiest)**: thinking models attach encrypted state that must be round-tripped exactly.
   Gemini 2.5 puts a `thoughtSignature` on function-call parts that must be re-attached to those exact parts in history,
   or multi-step tool use quietly degrades. Anthropic extended thinking has `thinking` blocks with signatures validated
   server-side. OpenAI reasoning models have the equivalent via Responses-API reasoning items. Any abstraction that
   normalizes messages into a clean common shape and drops these blobs works in demos and breaks on step 3 of a real
   loop.
4. **Schema dialects**: Gemini accepts an OpenAPI-ish JSON Schema subset; OpenAI strict mode wants
   `additionalProperties: false` and all-required; Anthropic is permissive. One tool definition, three lints.
5. The boring rest: different streaming grammars, stop-reason names, error/rate-limit shapes, and three incompatible
   prompt-caching mechanisms.

### 10.2 Architecture

**This section is built.** The tree ships the `genai` crate (pinned `=0.6.5` in `Cargo.toml`, which is authoritative)
wrapped by `src/ai/client.rs`, with `src/ai/CLAUDE.md` and `DETAILS.md` documenting the same per-provider quirk
rationale this spec describes (Responses-API routing, per-provider temperature handling, ~20 providers normalized). Over
it, `src/agent/llm/` ships the `AgentLlm` seam: the trait, its genai-backed impl, a deterministic fake, and a typed
message-part model whose parts carry the opaque per-message provider-state blob. Provider types never leak past it, and
the whole runtime and UI test against the fake. See `agent/llm/CLAUDE.md`.

What that leaves:

- The bulk slot (§10.4) is unbuilt; only the interactive slot resolves a model today.
- The quirk list of §10.1 is verified for the chat loop's shapes, not for every provider under a long multi-turn planner
  loop. Re-verify when the planner job arrives, against the model the bulk and interactive slots actually run.

### 10.3 Support tiers

- **Tier 1, agent-certified**: Anthropic, OpenAI, Gemini, and the local model (§10.4). Pinned known-good default models
  per provider; users may override with an "untested" badge.
- **Tier 2, community-supported**: any OpenAI-compatible endpoint. This single tier covers OpenRouter, Ollama, Groq,
  DeepSeek, xAI, and friends. Note that **OpenRouter is the "gateway service that keeps up with quirks for us"**, is
  already one of Cmdr's integrated providers, normalizes hundreds of models to the OpenAI schema server-side, and
  charges ~5% with no subscription. It remains a user choice, never a default (it is a middleman in the privacy path).
- New-model churn is handled by the eval harness doubling as a **regression suite** (§15): a fixture run costs on the
  order of a dollar, so certifying a model is a button press, not a project.

### 10.4 The local model option

A supported v1 option, not a cut: agent + summaries on the on-device model. The source of truth for what ships is the
model registry in the existing `ai` module (`AVAILABLE_MODELS` / `DEFAULT_MODEL_ID`), not this spec; David's
recollection is "an ~8B tool-calling model chosen ~6 months ago", and swapping in a newer one is an open task (§18.9).
Documented tradeoffs: noticeably weaker judgment and tool use than Tier 1 cloud models. It exists because "nothing ever
leaves your Mac" is a headline capability some users will accept the tradeoffs for. Settings expose **two model slots**:
bulk (summarizer) and interactive (wake/chat/planner), each independently set to any supported provider including local.

Fallback policy (decided): local is allowed in both slots, labeled honestly ("experimental, may underperform on agent
tasks"), and degrades gracefully rather than hard-failing: summaries and simple chat keep working even when multi-turn
tool loops struggle, and the wake/planner jobs do less instead of erroring. After repeated loop failures the agent shows
a polite notice that the local model is struggling and that a cloud model would handle agent tasks better.

## 11. Tools

### 11.1 One registry, designed for the agent

Decided: the tool registry is designed for its **primary consumer, the internal agent**; external AI clients (dev
tooling, Claude Code driving the app, automated tests) are secondary and get the same surface the agent has, including
the proposal-gated write path (§8.1). The interface should feel natural for the agent first; everything else adapts to
that.

For context: the shipped MCP server is built on "security via parity" (external agents act through the same UI actions a
user performs, deliberately without raw fs tools, per `src/mcp/CLAUDE.md`). Its tools are now single-sourced through one
authored registry (`src/mcp/tool_registry.rs`): a `mcp_tools!` table authors each tool once (name, description, JSON
input schema, bearer-token gate, and handler) and expands to every consumer, so name, schema, capability/destructive
gating, and dispatch can't drift, and the gate is a per-entry `TokenGate` rather than a hand-list. That UI-control
surface remains useful for UI-driving use cases (testing, automation), but it is the secondary surface: the agent-first
registry (knowledge, proposals, memory, notify) is the main interface, consumed in-process by the agent and exposed over
MCP to AI clients. The agent-first registry should **extend this consolidated registry rather than stand up a parallel
one**, keeping one authored source for every AI-callable tool.

**Consumer gating is structural, not policy (decided).** D26 ("proposals are the only write path, safety by
construction") must survive the registry reuse: the consolidated registry now carries direct write tools for AI clients
(auto-confirm file ops, `tag`, `favorites`, `indexing`, `set_setting`, rollback-cancel), and an in-process agent that
merely "holds the bearer token but chooses not to use it" would reduce D26 to policy. So the registry grows a consumer
dimension: each authored entry declares its exposure (for example `consumers: [ai_client]` vs `[ai_client, agent]`), and
each adapter — the MCP HTTP server, the in-process agent runtime — is constructed with a consumer identity and can only
list and dispatch its own view. The agent's write path is then physically absent from its dispatch table; proposals are
its only write verb because nothing else exists in its registry view. Enforce it the same way the `TokenGate` is
enforced: a structural set-equality test asserting every tool with a non-`Open` gate is absent from the `agent` view, so
a new destructive tool can't ship agent-visible by accident. This lands inside the registry-refactor milestone (there's
no consumer to gate until the adapters exist). The §8.5 autonomy setting does NOT loosen this view — it acts on the
proposal pipeline's apply step, never on tool exposure.

**This section is built** (D59). `mcp/tool_registry/` carries both dimensions: `consumers` is the exposure axis and
`access` (`Read` / `Propose` / `Write`) is the stronger guarantee the token gate can't give, since `TokenGate::Open`
covers destructive-but-prompting ops. `execute_tool` refuses any name outside the caller's consumer view before
dispatch, and `agent::tools::view` re-checks `tool_access` as a runtime backstop. Structural tests pin the agent view to
exactly its authored `[agent]` entries and require every one to be `Read` or `Propose`.

The transport-agnostic-core refactor this section once called a prerequisite **did not happen and was not needed**:
`execute_tool` is still generic over the Tauri `Runtime` and handlers still take `&AppHandle<R>` plus a
`serde_json::Value`, and the in-process agent consumes that shape directly. Treat the refactor as an optional cleanup,
never a blocker.

In docs, "the agent" means this feature; external MCP consumers are "AI clients" to avoid term collision.

### 11.2 The v1 toolset

Knowledge: `get_folder_summary`, `search_summaries` (FTS), `list_stale_summaries(min_interest)`, drive-index queries
(sizes, counts, recency). Proposals: `create_proposal_batch`, list/withdraw. Memory: scoped write (logged). Interaction:
`notify_user`. Files: `read_file` (below), and an archive-listing tool (zip browse + edit and read-only tar/7z have
shipped, so this reads the existing `ArchiveVolume` listing rather than waiting on a feature).

One-shot AI features (natural-language search, AI rename) are not "the agent" but use the same substrate: e.g. the
search box's NL path calls `search_summaries`. The registry and knowledge DB are shared infrastructure; the agent is
their stateful consumer.

### 11.3 `read_file` guardrails

**Not built, and the shipped privacy line is narrower than this section assumes.** The agent has no content-read tool:
only names, paths, and metadata reach the provider, plus one deliberate exception, the image-derived TEXT of
`search_photos` and `image_facts` (in-image OCR snippets and Vision tags, never image bytes), which the consent copy
names. That is a structural line, enforced by the registry view, not a runtime guard.

So `read_file` is not a designed feature waiting to be implemented: adding it widens provider egress from metadata to
file contents, which means re-deciding the consent story and bumping `CONSENT_COPY_VERSION`, not just writing a handler.
If it is ever built, these are its guardrails: per-call size caps, per-wake read budget, a sensitive-path denylist
(`~/.ssh`, browser profiles, keychains, and similar), content-to-cloud gated separately from content-to-local-model, and
**every read logged to the activity log with a reason**. File content enters context as untrusted data, clearly
delimited, never as instructions; the structural defense remains §8 (content can at worst produce a reviewable
proposal).

A related invariant the spec never anticipated, now live: **a proposal claiming file contents must prove the model
received them.** `propose::evidence::ImageFactsLedger` records per-thread deliveries and `propose_rename_plan` refuses a
plan citing content the ledger has no delivery for. Whatever elides a tool result owes the ledger a revocation, or it
vouches for content the model never read. Any future `Propose` tool inherits this. Depth:
`agent/tools/propose/DETAILS.md`.

## 12. Privacy, consent, and cost

- Opt-in feature with an explicit consent screen recording: which provider, that file/folder **names** are sent during
  summarization, that **contents** are sent only per the content-access policy, and the sensitive-path exclusions. The
  recorded consent matters; the website privacy copy needs an update when this ships (business note).
- The activity log shows decisions, proposals, notifications, memory writes, and file reads, each with a rationale
  (principle 5).
- `cost_meter` powers a visible spend display (per job type) and budget caps (daily/monthly). Initial-index spend is
  shown in the preflight before it happens.

### 12.1 Enable flow and the Full Disk Access gate

- **Everything the agent reads in its home turf is TCC-protected.** Downloads, Documents, and Desktop are exactly the
  paths AGENTS.md's FDA-gate rule covers. The agent's read path (hot-folder summarization, content peeks) MUST respect
  the existing `fda_gate` (`is_fda_pending_runtime()`), and the agent feature effectively requires FDA to be granted;
  enabling it without FDA must not stack TCC popups (the exact failure mode the gate exists to prevent).
- **The enable toggle lives in the existing onboarding wizard's AI step** (the second step): "Enable built-in AI agent
  that helps you organize your files", disabled until a working API key is entered. The FDA step precedes it in the
  wizard, so the gate composes naturally. Enabling starts the rest of the flow: the consent screen (§12), the
  proactivity dial (§9.5), and the preflight (§5.3). The same flow stays reachable from settings for users who skip it
  at onboarding.
- The user-facing copy drafted in this spec (preflight, notifications) is indicative and needs a style-guide pass at
  implementation time.

## 13. Naming and taxonomy

- **"agent"** is the name, user-facing and internal (tables, modules, tool prefixes), per the
  name-internals-after-the-UI rule.
- **"AI"** stays the umbrella for capabilities (settings section, provider config, one-shot features). The agent is the
  persistent, stateful entity.
- External MCP consumers are **"AI clients"** in docs. Future sub-entities, if ever, are **"subagents"**.

## 14. Prompts as repo assets

Markdown files with YAML frontmatter (`name`, `purpose`, intended model class, version note), plain `{{variable}}`
substitution, and `minijinja` only where a prompt genuinely needs conditionals or loops. Dev builds load them from disk
(instant iteration); release builds embed them. A `prompt-lint` check joins the checker: every template compiles, and
the variables each prompt references match what its call site provides (catches the silent `{{folder_sumary}}` class of
bug).

## 15. Testing and evals

- **Deterministic parts get ordinary unit tests** and they are the majority of the system: importance scorer, coalescer,
  digest compactor (budget adherence, compaction order), proposal lifecycle and drift detection, invalidation plumbing,
  retention pruning.
- **LLM behavior gets evals, not string asserts**: a fixture generator for synthetic home directories (build on
  `InMemoryVolume`), and a harness scoring summarizer and planner outputs against expectations (did it propose moving
  the invoices? did it leave the code folder alone?).
- The eval harness doubles as the **provider/model regression suite** for Tier 1 certification.
- **North-star metric: proposal acceptance rate**, tracked locally in `main.db`; opt-in aggregate telemetry can come
  later.

## 16. Settings surface (v1)

Provider/model for the two slots (bulk, interactive); budget caps (the background-refresh budget defaults to ~$10/month,
adjustable); proactivity dial; excluded paths; content-access policy; user-action-log toggle and retention; per-volume
opt-ins (index, summaries); the spend display; the auto-apply grant (§8.5).

Protected settings (decided): the auto-apply grant is settable ONLY through the Settings UI. It sits in a small
`protectedSettings` set in the settings registry that the MCP `set_setting` tool refuses regardless of bearer token (the
in-process agent has no `set_setting` at all, §11.1). The mechanism is generic — any future consent-class setting
(content-access policy is a candidate) can join the set.

Ownership line (decided): the settings store carries user preferences only. Agent operational state (throttle and snooze
state, walk bookkeeping, and similar) lives in `main.db`, written by the backend, never in the settings store.

Exposure principle (decided): every agent tunable this spec names (the proactivity dial and its underlying thresholds,
the daily notification cap, wake deadline tiers, the digest token budget, the refresh budget, proposal batch caps and
expiry, read budgets) is exposed in Settings from v1, even if it reads as too much at first. Consolidating dials into
Settings > Advanced, or dropping some, is a later editing decision, not a v1 gate.

## 17. Build order

**Superseded in part (David's call, 2026-08-18); the shape decisions now live in
`apps/desktop/src-tauri/src/agent/store/proposals/DETAILS.md`, and the proactive half has since shipped end to end
(`agent/wake/`, `agent/memory/`); what it deliberately left is `docs/specs/later/ai/wake-loop-follow-ups.md`.** David's
call: milestones 1, 2, 4, and 5 below ship as ONE release rather than staged, because a proposal store with no window,
or a window with one op kind, is not a shippable half. That plan absorbs them and carries the shape decisions (a group
is one verb and one destination; freeze moves from creation to approval). The ordering rationale below still explains
WHY the original sequence was wrong; it just no longer describes the release plan. Milestones 3, 6, 7, and 8 are
untouched and still queue behind it.

Milestones 1-4 and 9 of the original order are done or mostly done (§0). What follows is the remaining order, and it
deliberately **inverts the original sequence**: the original spent the knowledge layer, event pipeline, and wake loop
before a single proactive proposal reached a user, which delivers the north-star metric (§15, proposal acceptance rate)
last. This order buys that number first and lets it decide how much of the expensive machinery is worth building.

The organizing principle: **each milestone must be shippable to beta users on its own.** Ask Cmdr proved that pattern
works, and the proactive half is exactly the kind of bet that should not be validated by a big-bang release.

1. **The proposal spine, durable.** `proposals` + `proposal_ops` in `main.db`, freeze-at-creation, per-op
   `(inode, size, mtime)` snapshots, drift detection at apply, per-op statuses and partial apply, expiry, trash-default,
   batch caps. Apply through the shipped `OperationManager` (§20.3 fit-check). Migrate `propose_rename_plan` off its
   in-memory `RenameProposalStore` onto this store, so the one shipped proposing feature is the first consumer and the
   store is exercised by real use rather than tests alone. No new LLM behavior; this is the durable, testable spine
   every later milestone rides.
2. **One deterministic detector, end to end.** The narrowest proactive slice that produces a real proposal: a rule over
   the already-shipped `downloads/` watcher and the importance data (for example, installers already opened). No
   summaries, no coalescer, no wake loop, no LLM in the detection path. It needs a review surface (generalize the bulk
   rename review dialog), a notification path, and the proactivity dial with its caps and per-folder mute. Ship it to
   beta and start measuring acceptance rate.

   This is the decision point. If users accept proposals, the rest of the plan is worth its cost. If they don't, the fix
   is upstream of the machinery, and milestones 3-6 would have been built on a wrong premise.

3. **Activity log** (`agent_log`) and its surface. Principle 5 is unpaid so far: the rail's per-tool lines are the only
   transparency, and a proactive agent needs the full decision record before it earns more autonomy. Pull it earlier
   than the original order for that reason.
4. **Event pipeline** (§6): the coalescer, interest scoring, `agent_inbox` with deliver-by deadlines, budgeted digest
   compaction, restart reconciliation. Resolve §18.14 (tap point, and how the `downloads/` watcher relates) first: this
   milestone is a second interest-oriented stage over the indexer's already-corrected stream, never a parallel FSEvents
   subscription. The pure `coalesce` and `compact` seams (§6.3) make this the best parallel-agent target in the whole
   plan.
5. **Wake loop** (§9.1, §9.2): the wake job type, its context recipe, per-wake budgets, degraded modes. This is where
   the LLM finally enters the proactive path, and it enters over a proven proposal store, a proven review surface, a
   real activity log, and a bounded digest.
6. **Knowledge layer** (§5.2, §5.3): summarizer job, `folder_summaries` + FTS, the resumable importance-gated walk, the
   preflight with its cost estimate. Last, not first, because it is the only milestone that spends the user's money
   before delivering anything, and by this point real acceptance data says whether summaries are what the proposals were
   missing. Scope it against that evidence rather than the spec's original whole-drive ambition.
7. **Memory and rules** (§7): `~/.cmdr/rules/*.md` with `applies_to`, `~/.cmdr/memory/`, scoped memory writes. The
   profile half already ships.
8. **The rest**: the bulk model slot (§10.4), `main.db` retention pruning, the index relocation to `~/Library/Caches/`
   (§4.1, independent of everything else and safe to slot in whenever), prompts as repo assets plus `prompt-lint` (§14),
   and the §8.5 auto-apply grant, which should land only after acceptance-rate data justifies it.

**Evals** (§15) run alongside from milestone 1, not after: the fixture generator for synthetic home directories is worth
pulling forward the way §20.4 argued, and the proposal spine is testable without a model.

## 18. Open questions and investigations (honest list)

1. **ANSWERED for the chat loop.** genai (now pinned `=0.6.5`, out of beta) handles multi-call turns, per-provider
   schema strictness, and opaque reasoning-state round-tripping well enough that `agent/llm/` ships over it with no
   gap-filling adapters. The blob rides in the typed message-part model and is persisted in `messages.content_blocks`, a
   backend-only column. Two residuals: the loop is verified for chat's shapes, not for a long planner loop (re-verify at
   §17 milestone 5), and the supply risk stands (a solo-maintainer crate carrying the entire provider layer).
2. SMB volume-identity canonicalization: same share via `nas.local`, IP, and DNS name must converge on one identity; is
   a server GUID available per protocol? (Believed not hard, but undesigned.)
3. Importance-scorer signal weights and the exact scoring formula: needs iteration against real home directories.
4. `kMDItemLastUsedDate` sampling strategy and cost on large folders.
5. Wake deadline tier values (2-5s / 1-5min / 1h) and the digest token budget (2-4k): initial guesses, tune with use.
6. `listing_fingerprint` exact definition (proposed: hash over child names + sizes + mtimes).
7. Conversation/thread data model details, and how a notification reply inherits wake context technically.
8. Memory mining design (v1.5): which implicit signals, what confidence threshold, whether mined memories need their own
   review affordance.
9. Local model refresh: evaluate whether the shipped local model (see `ai` module `AVAILABLE_MODELS` /
   `DEFAULT_MODEL_ID` for the source of truth) should be replaced with a newer small tool-calling model before the agent
   ships.
10. Verify Time Machine and purge semantics for `~/Library/Caches/<bundle id>/` behave as assumed.
11. Tool-schema versioning policy for external MCP consumers as the registry grows.
12. Cost-estimate accuracy in the preflight (tokens-per-folder model needs calibration).
13. Whether `interest_weight` denormalization into `main.db` summaries is worth it vs. always reading from the drive
    index. (Also keeps the "split writers" story honest: the indexer should not write into `main.db`; if denormalized,
    the agent copies the weight at summary time.)
14. Event tap point (§6.1): exactly where the agent's coalescer subscribes on the indexer's corrected event stream, and
    how the standalone `downloads/` watcher and the agent's Downloads-related detectors relate (merge? coexist?).

### From the 2026-07 design review (proposed, not decided)

These came out of a later review pass and are captured as open items, not settled decisions. They may change the shape
of sections above; treat them as inputs to the next planning round.

15. **SHIPPED.** The scorer (§5.1) is its own neutral subsystem serving the agent, the media-ML enrichment scheduler,
    and future consumers. §5.1 stays the requirements source; placement under `src/agent/` and D8's "cached in the drive
    index" are superseded. Plan: `docs/specs/later/importance-subsystem-plan.md`; code:
    `crates/cmdr-index/src/importance/`.
16. **Per-folder "capability enrollment."** A concept for which folders are enrolled in which expensive analyses (e.g.
    deep photo analysis). Suggested vehicle: the agent's settings-suggestions via `notify_user` action buttons, NOT via
    `proposal_ops` (the freeze/drift semantics of §8.2 fit file ops, not settings changes).
17. **FTS5 over `messages` for searchable chat history**, alongside the summaries FTS index (§4.2).
18. **SHIPPED, and it became the product.** Chat plus read-only knowledge tools over the existing drive index shipped as
    Ask Cmdr, ahead of summaries. The §17 rewrite generalizes the lesson: every remaining milestone ships on its own.
19. **Activity log and chat likely share one surface.** The shipped transfer-queue window (the native panel from the
    execution-queue work) is the precedent for a native-panel surface both could reuse. Now a §17 milestone 3 question,
    since the rail exists and the activity log doesn't.
20. **D58 flagged for revisit.** "Every agent tunable in Settings from v1" (§16, D58) may be too much: the main UI keeps
    ~3 dials, with the long tail moved to an advanced section.

## 19. Decision log

Every decision below still stands as intent. Several are now **built** (D1, D3, D4, D22's DB half, D33, D34's chat job,
D36, D37, D38, D41, D42's pinning, D43's interactive slot, D44, D48's posture, D49, D51's wizard step, D55, D59) and two
are **superseded**: D8's "cached in the drive index" (§18.15, now a separate `importance.db`) and D26's `read_file`
assumption (§11.3, no content-read tool exists at all, so the privacy line is narrower than D26 planned for). §0 has the
full map.

- **D1**: Two DB families: per-volume `drive-index-{volume_id}.db` (cache) + `main.db` (durable catch-all). Rationale:
  Regenerable vs. valuable; separate writers; different backup policies; index is per-volume today.
- **D2**: The drive-index DB family lives in `~/Library/Caches/<bundle id>/`. Rationale: Platform-native "purgeable, no
  backup"; Time Machine skips Caches.
- **D3**: `main.db` is a generic catch-all, not agent-specialized. Rationale: Action logs and future durable state land
  there too.
- **D4**: No custom collation in `main.db`. Rationale: Stay `sqlite3`-inspectable; the index DB's collation forced a
  custom query tool.
- **D5**: Everything keys `(volume_id, rel_path)`; volumes table ships in v1. Rationale: Multi-volume (NAS, S3, FTP)
  need arrives within weeks; retrofitting keys is brutal.
- **D6**: Local volume only active in v1; SMB/MTP/S3 summaries deferred. Rationale: Staleness/reconnect semantics differ
  per type; don't block the spine.
- **D7**: Staleness is per-volume, first-class; agent caveats answers. Rationale: Enables answering about unmounted
  volumes (offline NAS index), a headline feature.
- **D8**: Deterministic importance scorer, cached in the drive index. Rationale: Fast, free, testable; gates summaries,
  event interest, and informs the LLM.
- **D9**: Summaries: whole drive at system-decided depth, via prune + threshold + descend-list. Rationale: One pass; LLM
  refines depth only as a byproduct of calls already paid for.
- **D10**: Summaries feed from the drive index, not the filesystem. Rationale: Listing-tier summaries need zero extra
  I/O.
- **D11**: Two summary tiers: listing-only bulk vs. content-aware deep. Rationale: 10-100x cost cliff; content reserved
  for hot folders and on-demand.
- **D12**: Cloud model is the summarization default; local stays an option. Rationale: Opt-in + BYO key + value
  justifies cost; "nothing leaves the Mac" kept for those who want it.
- **D13**: Batch APIs rejected for the initial pass. Rationale: ~24h async window conflicts with "summaries ready right
  after indexing".
- **D14**: Hot folders summarize in parallel with indexing. Rationale: Their paths are known a priori.
- **D15**: Preflight with folder count, cost estimate, privacy disclosure; resumable. Rationale: Transparency; resolves
  the "how many important folders" guess empirically.
- **D16**: FTS5 over summaries first; embeddings deferred. Rationale: Cheap, good enough for "where do invoices live";
  vectors are regenerable later.
- **D17**: Agent receives digests, never raw events; deadline-scheduled inbox; drain-all on wake. Rationale: Bounded
  context; MAX(interest) wake policy falls out implicitly.
- **D18**: No idle/heartbeat LLM calls; noise absorbed deterministically. Rationale: ~Zero cost when nothing happens.
- **D19**: Digest has a hard token budget; aggregator decides granularity. Rationale: The LLM never sees unbounded
  input.
- **D20**: Restart: roll-forward digest; full-rescan recovers via `list_stale_summaries` diff tool. Rationale: Events
  are hints, state is truth; also covers macOS purging the cache DB.
- **D21**: User actions inside Cmdr are first-class agent input. Rationale: Highest-signal events; manual moves and
  rejections carry intent.
- **D22**: Beliefs and rules in markdown (`~/.cmdr/`); operational data in SQLite. Rationale: Human-auditable agent
  "mind"; radical transparency.
- **D23**: `~/.cmdr/CMDR.md` global profile + `rules/*.md` with `applies_to` globs. Rationale: Folder-scoped rules
  without polluting folders.
- **D24**: Folder-level `CMDR.md` cut from v1. Rationale: Injection vector with authority; `applies_to` covers the need.
- **D25**: Agent memory in `~/.cmdr/memory/`, separate from rules, capped, writes logged. Rationale: "Told me" vs.
  "inferred" never blur; user can audit/edit/delete.
- **D26**: No direct write tools; proposals are the only write path, shared by ALL AI consumers. Rationale: Safety by
  construction; structural prompt-injection defense; one consent surface.
- **D27**: Freeze proposals at creation (pattern → concrete list); pattern kept as display text. Rationale: No drift
  between what was shown and what runs.
- **D28**: Per-op child rows with own statuses; partial apply. Rationale: "Apply 11, skip 3 stale" beats all-or-nothing.
- **D29**: Drift detection via per-op `(inode, size, mtime)` snapshot, re-verified at apply. Rationale: Creation→apply
  gap can be days.
- **D30**: Trash over delete; batch op caps; proposals expire. Rationale: Reversibility; reviewable batches; stale
  suggestions are worse than none.
- **D31**: Standing rules (live patterns) deferred to v2 with own consent UX. Rationale: A persistent auto-applying
  pattern is a different risk class.
- **D32**: No `priority` column; `created_by_model` is provenance only. Rationale: YAGNI; no logic on
  "authoritativeness".
- **D33**: Apply rides the upcoming execution-queue feature and the existing op pipeline. Rationale: Zero new write
  paths; preflight/rollback for free.
- **D34**: No subagents in v1; four job types (wake, chat, planner, summarizer) instead. Rationale: One brain, different
  prompts/models per job; hierarchy unearned.
- **D35**: "Librarian" is a tool, not an agent. Rationale: An FTS SELECT needs no LLM intermediary.
- **D36**: Wake context is fresh each time; continuity via DB/memory, not transcript. Rationale: The defining difference
  between an agentic app and a chat app.
- **D37**: Single-flight agent; chat priority; wakes queue and digests merge. Rationale: No self-conflicting concurrent
  writes.
- **D38**: Per-wake budgets (tool turns, wall time, file reads) + house cancellation pattern. Rationale: Runaway loops
  impossible by construction; visible and killable.
- **D39**: Proactivity dial chosen at onboarding; named policy bundles; never silently self-adjusts. Rationale: No
  silent default; "want me to pipe down?" over creepy auto-tuning.
- **D40**: Tier 1 providers: Anthropic, OpenAI, Gemini, local; Tier 2: any OpenAI-compatible endpoint. Rationale:
  Bounded certification surface; OpenRouter (already integrated) carries the long tail.
- **D41**: Own `AgentLlm` trait with opaque per-message provider state, over the already-shipped `genai` integration.
  Rationale: Thinking-state round-trip is the make-or-break; the trait is the asset; never build a parallel provider
  layer.
- **D42**: Pinned default models + "untested" badge + evals as regression suite. Rationale: New-model churn becomes a
  button press, not a project.
- **D43**: Two model slots: bulk vs. interactive. Rationale: Summarization and judgment have different cost/quality
  needs.
- **D44**: Name: "agent" (user-facing and internal); "AI" stays the capability umbrella. Rationale:
  Name-internals-after-UI rule; honest and specific enough.
- **D45**: Prompts as markdown + frontmatter + minijinja-as-needed; dev hot-reload; `prompt-lint` check. Rationale:
  Iterate fast; catch template drift in CI.
- **D46**: Acceptance rate is the north-star metric. Rationale: Directly measures suggestion quality.
- **D47**: Data-dir rename decoupled from this work. Rationale: Aesthetic change with plugin/migration risk; must not
  block the agent.
- **D48**: User action log: local-only, opt-out, ~90-day retention. Rationale: High-signal input with a privacy posture.
- **D49**: The tool registry is agent-first; external AI clients get the same surface as the agent. Rationale: The
  interface stays natural for its primary consumer; one write path for all AI.
- **D50**: Index relocation migrates by moving files when cheap; full rescan is the acceptable fallback. Rationale: Kind
  to existing testers without committing to heavy migration code.
- **D51**: Agent enable is a toggle in the onboarding AI step, gated on a working API key. Rationale: Meets users where
  AI setup already happens; FDA step precedes it naturally.
- **D52**: Background jobs never materialize dataless cloud files; synced-file content is readable; dataless content
  only on explicit ask. Rationale: No unexpected downloads; sync state is already known to the app.
- **D53**: Local model allowed in both slots with honest labeling and graceful degradation. Rationale: Local is a
  headline option; fail soft with a polite notice, never hard-fail.
- **D54**: Background-refresh budget defaults to ~$10/month, transparent and user-adjustable. Rationale: Real utility
  over penny-pinching; the user stays in control.
- **D55**: The execution queue is a separate effort and a prerequisite for the proposals milestone. Rationale: Proposals
  apply through it; its design happens in its own effort.
- **D56**: Agent operational state lives in `main.db`; the settings store is preferences only. Rationale:
  Backend-writable state needs a backend home; respects settings ownership.
- **D57**: Late wakes keep full priority; the notification cap counts only notifications actually shown. Rationale: Late
  is fine, dropped is not; the cap protects attention, which is only spent on screen.
- **D58**: Every agent tunable is exposed in Settings from v1. Rationale: Better too many dials than hidden behavior;
  pruning to Advanced (or dropping) comes later.
- **D59**: Per-consumer registry gating is structural: entries declare consumer exposure; the agent adapter's view
  contains no write-gated tools, pinned by a set-equality test. Rationale: D26 must be by-construction across the
  registry reuse; "holds the token but doesn't use it" is policy, not construction.
- **D60**: User-granted autonomy is an auto-apply policy on the proposal pipeline (Settings toggle, default off), never
  raw tool exposure; the toggle is a protected setting changeable only in the Settings UI (refused by `set_setting` even
  with the token; the agent has no `set_setting`). A CLI flag was rejected. Rationale: keep one write path with its full
  audit/safety machinery while dropping only the review click; self-enabling autonomy must be impossible; consent lives
  where the user can see it, not in an invocation flag.

## 20. How to use this spec (starting sequence)

This document fixes behavior, decisions, and intent. It is deliberately not a build plan. Before writing code, a few
sequencing notes that are easy to lose otherwise:

1. **Read §0 first.** The rest of the spec states the v1.0 target, not the tree. Where they disagree, §0 wins.
2. ~~Run the genai capability check first.~~ Done; see §18.1.
3. **Write a per-milestone plan; do not code straight from this spec.** The spec was written to make planning cheap
   (decisions settled, intent captured), but the planning step still exists: repo context, exact DDL, module layout,
   migration-code shape, and IPC bindings belong in a milestone plan, not here. Coding off the spec forces the
   implementer to make planning calls mid-flight. This held for every Ask Cmdr milestone and it still holds.
4. **Fit-check the shipped `OperationManager` before the proposal spine (§17 milestone 1), not a from-scratch queue
   effort.** The lane-based queue, transfer-queue window, managed instant ops, and the IPC-edge-injected
   `OperationEventSink` (the headless-callable write path) are all in the tree. What is left is confirming batch-of-ops
   apply with per-op statuses and per-op result reporting, and filing any gap as a small extension to the manager.
   Capture the agent's requirements (batch apply, per-op results, cancellation) against its current API.
5. **Pull the synthetic-home fixture generator forward.** Still true, now for the proposal spine and the detector rather
   than the importance scorer (which grew its own evals corpus in the end).
6. **Fold durable intent into colocated `CLAUDE.md` files as milestones land.** `docs/specs/` is wiped periodically by
   design (see the specs README), so the decisions and intent here must migrate into code-adjacent docs as the
   subsystems take shape, or they evaporate with the folder. The §19 decision log is the thing most worth preserving.
   Ask Cmdr did this well: `agent/`, `agent/llm/`, `agent/store/`, `agent/tools/`, and `agent/chat/` each carry a
   `CLAUDE.md` + `DETAILS.md` citing the decision numbers they implement. Keep that up.
