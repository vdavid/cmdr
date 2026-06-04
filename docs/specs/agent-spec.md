# Agent: v1.0 spec (with v1.5+ outlook)

Status: design complete, not yet implemented. 2026-06-04.

This spec captures a full design session between David and an AI agent. It is written so that a
fresh agent (or human) can pick it up with no other context. Decisions below are settled unless
they appear in §18 (open questions); intentions and principles (§2) govern anything this spec
doesn't explicitly answer. §19 is the decision log with rationale, kept as a second angle on the
same material for future planning and implementing agents.

Related: the data directory rename is deliberately NOT part of this work; see
[data-dir-rename-spec-draft.md](data-dir-rename-spec-draft.md).

## 1. What this is

A persistent agent that lives inside Cmdr. It builds and maintains compressed knowledge of what
lives where in the user's file system, watches file system events and user actions, and proactively
suggests file operations (tidy Downloads, unpack that dataset into the right project folder, name
those screenshots), which the user reviews and applies. It also answers questions ("where do we
usually store invoices?") through a chat surface.

It is opt-in, BYO API key, and it never touches files directly: it can only propose. The user
applies proposals through a review dialog, and execution runs through Cmdr's existing, hardened
file operation pipeline.

UI is intentionally out of scope here beyond naming the surfaces: a review dialog for proposals, a
chat surface, and notifications. This spec is about agent behavior, inputs, outputs, storage,
context, and memory.

## 2. Principles (these govern everything below)

1. **Deterministic bottom, LLM top.** Cheap, fast, testable Rust handles everything with an
   obviously correct answer (event coalescing, importance scoring, staleness, digest compaction,
   proposal validation). The LLM is reserved for judgment and language. Never put a model in a
   per-event hot path.
2. **The agent costs ~zero when nothing interesting happens.** No idle wakes, no heartbeat LLM
   calls. Noise is absorbed deterministically (counters and staleness marks); it reaches the model
   only as one digest line the next time the agent wakes for a real reason.
3. **Propose, never act.** The agent has no write tools. Its only write path is the proposal queue,
   gated by user review, executed by the existing file-op pipeline (preflight, conflicts, progress,
   rollback, trash). This is also the structural prompt-injection defense: file contents are an
   untrusted input, and the worst a malicious file can achieve is a weird suggestion sitting in a
   review queue.
4. **Continuity through state, not transcript.** The agent does not carry its life story in its
   context window. Durable knowledge lives in the database and in markdown memory; each wake gets a
   fresh, budgeted context assembled from state. Only chat threads keep (bounded) transcripts.
5. **Radical transparency, applied to the agent itself.** Every decision, every proposal, and every
   file the agent reads is logged with a rationale and visible to the user. "The agent read these 3
   files, here's why" does more for trust than any copy.
6. **Derived data lives in the database; beliefs and rules live in markdown.** Summaries, proposals,
   and logs are operational state (SQLite). What the user tells the agent and what the agent infers
   about the user are human-auditable markdown files the user can open, edit, and delete.
7. **Events are liveness hints; state is truth.** The event stream will have gaps (app closed,
   volume disconnected, cache purged). Reconciliation against indexed state and stored fingerprints
   is the recovery mechanism, never event replay.
8. **Don't gamble the user's trust.** Anti-noise etiquette is policy, not vibes: hard caps on
   proactive notifications, a user-chosen proactivity level, per-folder mute, and no repeats after a
   rejection.

## 3. Scope

### v1.0

- Storage split: `drive-index.db` (cache) and `main.db` (durable), schemas in §4.
- Multi-volume **keying** everywhere from day one; only the local main volume is active in v1.
- Deterministic importance scorer with weights cached in the drive index.
- Folder summaries: hot folders first, then importance-gated whole-drive pass; FTS search over
  summaries; preflight cost estimate; resumable.
- Event pipeline: coalescer, interest scoring, inbox with deadline scheduling, token-budgeted
  digests, restart reconciliation.
- The four LLM call types (wake, chat, planner, summarizer) with the context anatomy in §9.
- Proposals: batches with per-op rows, freeze at creation, drift detection, review and apply,
  invalidation, expiry.
- `~/.cmdr/` profile, scoped rules, and agent memory files.
- Tool registry serving the agent as first consumer (MCP external clients are the second).
- Provider layer: Tier 1 = Anthropic, OpenAI, Gemini, plus the local model option; Tier 2 = any
  OpenAI-compatible endpoint.
- Activity log, read log, cost meter with per-job attribution, proactivity dial, degraded modes.
- Unit tests for all deterministic parts; a v0 eval harness with synthetic fixture folders.

### v1.5 (named and deferred)

- Multi-volume summaries opt-in (NAS/SMB first), with per-volume staleness and reconciliation.
- Memory mining from implicit signals (rejections, manual moves) into proposed memory entries.
- Natural-language search integration: the search box's AI path uses `search_summaries` as a tool.
- Archive ops in proposals (extract/compress) once the zip-handling feature ships.
- Eval harness v1 doubling as the provider/model regression suite with pinned certified models.

### v2 / later

- Standing rules (live patterns that keep applying), with their own consent UX. Explicitly NOT in
  v1: a pattern that stays live indefinitely is a different, more dangerous feature than a one-shot
  proposal.
- Subagents (if ever). v1 has job types, not agent hierarchies; see §9.1.
- Folder-level `CMDR.md` files, cut from v1. If reintroduced, they need trust tiers: a folder-level
  file is information about the folder, never authority, unless under a user-marked trusted root
  (a cloned repo or downloaded zip can contain one; injection vector).
- Claude-skills-format support in `~/.cmdr/` (revisit when the plugins roadmap item lands).
- S3/FTP/WebDAV summaries (the keying supports them from day one).
- Local-only daemon mode (agent running while the app is closed).

## 4. Storage

### 4.1 Two databases

| DB                          | Location                               | Nature                                                                        | Backup                            |
| --------------------------- | -------------------------------------- | ----------------------------------------------------------------------------- | --------------------------------- |
| `drive-index-{volume_id}.db` (per volume) | `~/Library/Caches/<bundle id>/` | Regenerable cache: the drive index (today's per-volume `index-{volume_id}.db` files, renamed and relocated) | None; Time Machine skips Caches   |
| `main.db`                   | `<app data dir>` (Application Support) | Durable catch-all: summaries, proposals, logs, conversations, action history  | Time Machine picks it up normally |

Correction from review: the existing drive index is already **per-volume**
(`index-{volume_id}.db`, per `indexing/CLAUDE.md`), not a single file. The relocation is therefore
N files (or a `drive-index/` subdirectory), the naming keeps the volume id, and it composes with
the multi-volume keying in §4.3 rather than colliding with it. Migration for existing installs
needs a decision (§18.17): move the files, or accept that relocation orphans the old
Application Support indexes and triggers a one-time rescan on upgrade. Note the Caches path uses
the **current** bundle id; this is independent of the deferred data-dir rename.

Rationale: regenerable vs. valuable data, different lifecycles, different backup policies, and it
splits the writers (the indexer hammering the cache never contends with agent writes). Putting the
cache in `~/Library/Caches/` is the platform-native way to say "purgeable, don't back up": macOS may
purge it under disk pressure, and that is acceptable (it triggers the same reconciliation path as a
full reindex, §6.4).

`main.db` is deliberately a catch-all, not specialized: user action logs and future durable state
land here too.

Constraints:

- **No custom collation in `main.db`.** It must stay inspectable with plain `sqlite3`. (The index
  DB's `platform_case` collation forced the `index-query` tool; don't repeat that.)
- `schema_version` table plus forward migrations from day one. This DB lives for years.
- Retention policy per log-like table (prune by age and row cap on startup). `main.db` must not
  grow unboundedly.
- Note: `main.db` is itself sensitive (it is a map of the user's life). It stays local; nothing in
  it is ever transmitted.

### 4.2 `main.db` schema (v1 shape)

Names are indicative; the implementing agent owns the final DDL.

- `volumes`: `volume_id` PK, `kind` (local | smb | mtp | s3 | ftp | ...), `stable_identity`,
  `display_name`, `index_opt_in`, `summary_opt_in`, `last_reconciled_at`.
- `folder_summaries`: `(volume_id, rel_path)` PK, `summary` TEXT, `generated_at`, `model`,
  `listing_fingerprint` (what the summary was computed from, for staleness), `stale` flag,
  `interest_weight` (denormalized copy; the authoritative cached weights live in the drive index).
  Plus an **FTS5** index over `summary` and `rel_path`. Embeddings are explicitly deferred: FTS
  first, add vectors later only if FTS disappoints (they are regenerable, so this is reversible).
- `proposals` (batch level): `batch_id` PK, `created_at`, `op_display_name` (friendly name, may
  include the originating pattern as display text, e.g. "All installers you've already opened
  (~/Downloads/*.dmg with last-open > creation)"), `rationale`, `status`, `created_by_model`,
  `expires_at`.
- `proposal_ops` (op level): `op_id` PK, `batch_id` FK, `op` (move | rename | trash | mkdir; extract
  later), `source_volume_id`, `source_rel_path`, `dest_volume_id`, `dest_rel_path`, `status`
  (per-op, enabling partial apply), `snapshot_inode`, `snapshot_size`, `snapshot_mtime` (drift
  detection), `executed_at`, `result`.
- `agent_log`: `ts`, `source` (detector | wake | planner | chat | summarizer), `kind` (proposal |
  notify | memory_write | file_read | observation | error), `target`, `rationale`, `model`,
  `tokens_in`, `tokens_out`, `latency_ms`. This feeds the activity UI and is also context input
  (recent rejections, §9.2). Terminology mapping, since the spec uses several names: the
  user-facing **"activity log"** IS this table; the **"read log"** is its `kind = file_read`
  filtered view, not a separate table; `user_action_log` (below) is separate because its writer is
  the user, not the agent.
- `conversations` + `messages`: chat threads. A notification the user replies to becomes a thread
  carrying the originating wake's context.
- `agent_inbox`: pending event bundles (persisted so a crash loses nothing): `bundle_id`,
  `volume_id`, `rel_path`, `counters` (JSON), `interest`, `deliver_by`, `created_at`.
- `cost_meter`: per-day, per-job-type (initial_index | refresh | wake | chat | planner) token and
  cost accounting. Powers the spend display and the budget caps, and makes "why did this cost $4"
  answerable.
- `user_action_log`: user operations and navigation inside Cmdr (a high-signal intent source, §6.1).
  Local only, opt-out setting, default retention ~90 days.
- `walk_state`: resumable summarization-walk bookkeeping.

### 4.3 Multi-volume identity

Everything keys by `(volume_id, rel_path)`, never absolute paths. The `Volume` trait grows a
`stable_identity()` used by both the drive index and the agent: APFS UUID for local volumes,
server+share for SMB, device serial for MTP, endpoint+bucket for S3. The need for non-local volumes
arrives within weeks of v1 (NAS indexing is a personal target), so the keying ships in v1 even
though only the local volume is active.

Per-volume staleness is a first-class property, not an error: summaries carry their `as_of`
fingerprint, volumes carry `last_reconciled_at`, and the agent caveats answers ("as of May 28").

**Headline consequence worth building toward:** the agent can answer questions about volumes that
are not currently mounted ("where's that 2024 photo backup?" answered from NAS summaries while the
NAS is off). Summaries become an offline index of unmounted drives. Nobody else has this.

Volume-type notes for v1.5:

- SMB: events are surprisingly reliable while connected, but a disconnect leaves no backlog, so
  reconnect means "mark volume needs-reconciliation, importance-gated rescan, diff into a digest"
  (the same mechanism as app restart, §6.4).
- MTP: scanning is expensive and devices detach quickly; summaries are on-demand only, never
  background.

## 5. The knowledge layer

### 5.1 Deterministic importance scorer

A fast, pure-Rust algorithm assigning each folder an interest weight. Inputs (hardcoded heuristics,
tunable):

- Known-unimportant names (`node_modules`, caches, build artifacts, `.git` internals); hidden dirs;
  system vs. user ownership.
- File extensions present, and the **diversity** of extensions (monoculture folders like logs score
  low).
- Modification recency; last-opened recency where available (macOS: `kMDItemLastUsedDate` via
  Spotlight metadata; per-item MDItem queries are slow, so sample rather than sweep; atime is
  unreliable).
- Cmdr's own navigation signals: folders the user actually visits (tab history, last-used paths).
- A `.git` root (or similar project marker) raises the subtree: projects are important.
- Path class priors: Downloads, Desktop, Documents, project roots high; `~/Library`, caches low.

Weights are cached in `drive-index.db` (they are regenerable, so cache placement is correct) and
recomputed cheaply when listings change. The weight serves three consumers: summary generation
gating, event-bundle interest (§6.2), and as an input the LLM sees when reasoning about folders.

Expectation check: David expects a typical user to have only dozens to a few hundred genuinely
important folders. The design does not depend on that guess being right: the pre-scan counts before
anything costs money, and budgets cap the tail (§5.3).

### 5.2 Summarization

One agent-generated summary per folder, at a depth the system decides. The naive version of "the
agent decides the depth" is an LLM call per directory node; the actual design keeps it one pass:

- **Deterministic pruning first.** The importance scorer excludes the obvious (a `node_modules`
  gets, at most, one line in its parent's summary), and an importance threshold gates which folders
  get summarize calls at all.
- **The descend decision rides the call you already paid for.** Each summarizer call returns
  `{summary, children_worth_descending}`. The LLM refines the walk only inside the ambiguous band,
  and each refinement is a byproduct of a summary you wanted anyway. Walk top-down in importance
  order.
- **Feed from the drive index, not the filesystem.** Names, sizes, and mtimes for the listing-only
  tier come from `drive-index.db` with zero extra disk I/O.
- **Two tiers with a cost cliff between them.** Listing-only summaries (metadata in, summary out)
  are the cheap bulk tier. Content-aware summaries (file heads/samples included) cost 10-100x and
  are reserved for hot folders and on-demand requests.
- **Pack siblings.** Many small folders go into one call; never call-per-folder.
- **Hot folders run early and in parallel with drive indexing.** Downloads, Desktop, Documents, and
  detected project roots are known a priori and don't need to wait for the full index.

Model choice: **cloud by default** (the feature is opt-in, BYO key, and the value justifies the
upfront cost; deliberate decision over local-first). The **local model remains a supported option**
(see §10.4) for users who accept the quality tradeoff.

Refresh policy needs a budget or drift quietly re-burns the drive: refresh on access, event-driven
for hot folders, and a monthly token cap for everything else. Staleness detection via
`listing_fingerprint` (hash over child names, sizes, and mtimes; exact definition is an
implementation detail to pin down). Summaries do NOT regenerate on model switch; the `model` column
is provenance, and refreshes happen opportunistically.

### 5.3 Preflight (first run)

Enabling the agent runs the free, deterministic pre-scan and shows a preflight before any tokens
are spent:

> I found ~N folders worth reading. Initial read with {model}: ~$X, roughly N minutes. File and
> folder names will be sent to {provider}; file contents only where you allow it.

Cancelable, resumable (`walk_state` in `main.db`), with progress. Batch APIs were considered for
the initial pass (~50% off) and rejected: their async ~24h window doesn't suit "summaries should
exist right after indexing finishes." Rate limits still require a concurrency-limited drip with
retry/backoff.

## 6. Inputs: events and digests

### 6.1 Sources

- **File system events**: consume the indexer's existing event stream, not a parallel raw FSEvents
  subscription. Review flagged that the indexer already coalesces, dedups, and batches FS events
  (its own flush window, replay vs. live loops, verifier corrections, per `indexing/`); the agent's
  coalescer (§6.2) is a second, interest-oriented stage over that already-corrected stream
  (subscribe, don't poll; don't duplicate dedup machinery). Exact tap point, and how this relates
  to the standalone `downloads/` watcher, is an open question (§18.19). Index roll-forward on
  startup feeds the same path (§6.4).
- **User actions inside Cmdr**: operations and navigation, logged to `user_action_log`. These are
  the highest-signal events because they carry intent: a user manually moving three PDFs from
  Downloads to `~/Dropbox/invoices` is a preference worth learning; a rejected proposal is implicit
  feedback. Actions done outside Cmdr (Finder) appear only as their FS-event results; that's
  acceptable, they're just lower-signal.

### 6.2 Pipeline

```
FS events + user actions
   → coalescer (per-folder counters in a window)
   → interest scorer (deterministic, §5.1)
   → inbox bundles with deliver_by deadlines
   → WAKE → drain the whole inbox into one digest
```

The agent never receives raw events. It receives a digest of everything since its last wake; the
pipeline's only real output decision is **when to wake it**:

- Each bundle gets `deliver_by = now + f(interest)`: hot (new file in Downloads, new file in a
  folder with matching rules) ~2-5s; warm ~1-5 min; cold ~1h. Exact tier values need tuning (§18).
- Any wake, for any reason (a hot deadline, a user question, a proposal invalidation), drains the
  entire inbox, so cold bundles ride along for free. A `MAX(interest)` policy falls out implicitly.
- **No minimum wake frequency.** If only uninteresting things happen, the agent simply does not run.
  Noise (the 10,000 log-file changes) is fully absorbed by the deterministic layer: counters
  incremented, summaries marked stale, zero LLM involvement. The noise becomes one line of
  situational awareness in the next real digest.

### 6.3 Digest compaction

The digest has a **hard token budget** (~2-4k; tune later) and the deterministic aggregator fills it
in importance order with hierarchical compaction: "5M changes in /tmp/log" is one line; the
interesting tail gets per-folder granularity until the budget is full. The aggregator decides
granularity, never the LLM.

Testability seams (per the project's design-for-testability rule, name them at write time): the
coalescer is a pure `coalesce(events, window) -> bundles`, and the compactor is a pure
`compact(bundles, token_budget) -> digest`. Both take values in and return values out, no I/O, so
the §15 unit tests construct inputs directly.

### 6.4 Restart, gaps, and reconciliation

On app restart, the indexer either rolls forward the backlog of FS events (up to ~10M) or, beyond
that, performs a full rescan. Correspondingly:

- **Roll-forward path**: the coalescer runs over the rolled-forward changes and produces a normal
  (budgeted, compacted) digest: "5M changes in /tmp/log, 450k in node_modules, and these 200
  interesting changes in detail."
- **Full-rescan path** (also: macOS purged the Caches DB): the diff is lost. The digest then says
  so: "the app was closed between X and Y; a full reindex happened; the diff is unknown." The agent
  recovers via a deterministic tool, `list_stale_summaries(min_interest)`, which diffs current index
  state against the stored summary fingerprints. "We don't know what changed" becomes "ask the DB
  which of your beliefs are stale."

Both paths express principle 7: events for freshness, state for truth, reconciliation over replay.

### 6.5 Degraded modes

No API key, provider down, rate-limited, offline: the agent silently downgrades to
deterministic-only operation (absorb events, mark staleness, queue work) and never affects the file
manager. A subtle status indicator, no error spam. **Pile-up is bounded on both sides**: the digest
token budget bounds the context side, and queued work folds into reconciliation rather than
replaying, bounding the work side.

## 7. Rules, profile, and memory

Layout (user-authored content lives in a friendly dotdir, machine state in the app data dir):

- `~/.cmdr/CMDR.md`: the global profile, always loaded into every agent context. Personal info,
  preferences, standing guidance.
- `~/.cmdr/rules/*.md`: modular rules with optional YAML frontmatter `applies_to: <glob>` patterns.
  This gives folder-scoped rules WITHOUT placing files in folders.
- `~/.cmdr/memory/*.md`: agent-written takeaways. Deliberately a separate directory from `rules/`
  so "what the user told me" and "what I inferred" never blur. User-auditable: open, edit, delete.
  Size-capped, deduplicated, and every write is logged to the activity log.

Folder-level `CMDR.md` files are cut from v1 entirely (see §3 later-scope for the trust-tier design
if they return). The `applies_to` mechanism covers folder-specific rules without the pollution or
the injection surface.

DRY references work because the agent has a read-file tool: a user can write "see
`~/.claude/CLAUDE.md` for my profile" and the agent follows it (within the read-tool guardrails,
§11.3).

The markdown/DB line (principle 6): summaries, proposals, and logs are DB; beliefs and rules are
markdown.

## 8. Proposals

### 8.1 The contract

The agent's only write path. Every AI consumer shares it (§11.1): the internal agent, and any
external MCP client. One review surface gates "an AI wants to touch your files," regardless of
which AI. Capability-gated per consumer.

### 8.2 Freeze at creation

The agent may *think* in patterns ("delete `~/Downloads/*.dmg` that you've already opened"), but
the proposal tool resolves the pattern to a **concrete op list at creation time**. The pattern
survives only as display text in `op_display_name`. The review dialog shows the friendly name and
expands to the exact file list.

Because creation and apply can be days apart, frozen lists carry drift detection: each op snapshots
`(inode, size, mtime)` at creation, and the executor re-verifies at apply. A mismatch flips that op
to `invalidated` rather than operating on a changed file.

### 8.3 Lifecycle

```
proposed → accepted (= user clicked apply) → executing → executed | failed
        → rejected
        → expired      (proposals auto-expire after days; stale suggestions are worse than none)
        → invalidated  (drift detected, or an FS event touched a source)
```

Per-op statuses enable **partial apply**: if 3 of 14 ops went stale, the dialog applies 11 and
reports 3 skipped, user's choice, never all-or-nothing.

Invalidation plumbing: an incoming FS event affecting any op's source marks the op (deterministic),
revalidates cheaply where possible, and queues an inbox bundle so the agent learns its earlier
suggestion was affected.

### 8.4 Execution

"Approve" means "apply". Applied batches are enqueued into the upcoming execution-queue product
feature (not yet built; design the apply call against its API from day one) and run through the
existing `write_operations` pipeline: preflight, conflict handling, progress, cancellation,
rollback. Destructive ops default to **trash**, not delete. Batches are capped (a few hundred ops)
to keep review usable; large cleanups chunk into multiple batches.

Dropped from the earlier sketch, deliberately: a `priority` column (YAGNI) and any logic on model
"authoritativeness" (`created_by_model` is kept as provenance only).

## 9. The agent runtime

Rust, in-process, under `src-tauri/src/agent/` (inbox/coalescer, interest, summaries, proposals,
memory, tools, llm loop, notify). The frontend gets display surfaces only.

### 9.1 Job types, not subagents

There are no subagents in v1. There is one agent with four **job types**, each with its own prompt,
context recipe, and (configurable) model:

| Job        | Trigger                       | Context                  | Model setting        |
| ---------- | ----------------------------- | ------------------------ | -------------------- |
| Wake       | Inbox deadline / invalidation | §9.2, fresh every time   | Interactive model    |
| Chat       | User message                  | §9.3, thread-scoped      | Interactive model    |
| Planner    | A wake decides a situation needs a plan | Wake context, focused on one situation, longer tool loop | Interactive model |
| Summarizer | Knowledge-layer walk          | Tiny: listing in, summary + descend-list out; no profile | Bulk model           |

A "librarian" was considered and rejected: querying summaries is an FTS SELECT; putting an LLM
between the agent and the database is overhead. It's a tool (§11.2). If subagents ever arrive,
they'll be called subagents.

### 9.2 Wake context anatomy (fresh every time, no chat history)

1. System prompt: role, hard rules (propose only; etiquette caps; "doing nothing is usually
   correct").
2. `~/.cmdr/CMDR.md` + rules whose `applies_to` matches the involved paths.
3. Retrieved memory, scoped to involved paths/topics, never all of it.
4. The digest (§6.3).
5. Folder summaries for affected paths.
6. Open proposals touching the same paths, plus an activity-log tail **including recent
   rejections** (so it never re-suggests what the user just declined).

Budget: roughly 5-10k tokens. The stable prefix (system, profile, rules) goes first for provider
prompt caching.

### 9.3 Chat context anatomy

Stable prefix + this thread's recent turns verbatim + older turns summarized. Other threads are
reachable via a search tool, never auto-loaded. A notification the user replies to becomes a thread
that inherits the originating wake's context.

### 9.4 Concurrency, budgets, and cancellation

- **Single-flight**: one LLM job at a time per agent. Chat takes priority; wakes queue behind it
  and their digests merge while waiting.
- Per-wake budgets: max tool turns, max wall time, max file reads. A runaway loop must be
  impossible by construction.
- Cancellation follows the house pattern (`AtomicBool`, checked at tool-call boundaries); agent
  activity is visible and killable like any long-running Cmdr task. One review-flagged nuance: an
  in-flight provider HTTP call is a network round-trip an `AtomicBool` cannot interrupt, the same
  known gap architecture-patterns.md documents for blocking syscalls. The existing `ai/` layer
  already has a stream-cancel mechanism for exactly this; the agent loop reuses it so an LLM call
  in flight cancels within the design budget, with the `AtomicBool` covering tool-call boundaries.

### 9.5 Notifications and the proactivity dial

- A `notify_user` tool with action buttons (review / apply / dismiss / open chat). Etiquette is
  policy: max proactive notifications per day, confidence floor, no repeats after a rejection.
- The proactivity dial is a setting with ~4 named, hard-coded policy bundles (off / quiet / normal /
  eager) mapping to interest thresholds and caps. **Chosen during the agent-enable onboarding, no
  silent default** (a too-quiet default reads as "the feature does nothing"; too eager is noise);
  "Normal" is pre-highlighted as the recommendation. Per-folder mute and "snooze today" exist at
  every level.
- Auto-throttling is never silent: after several consecutive dismissals the agent may *ask* "want
  me to pipe down?", which is on-brand; it never changes settings by itself.

### 9.6 IPC surface (indicative)

The spec body deliberately stays behavior-level, but the project is opinionated about IPC
(typed `tauri-specta` bindings, subscribe-don't-poll), so here is the indicative surface a fresh
agent should expect to build. Names are placeholders; the implementing agent owns the final list.

- Commands: `agent_enable` / `agent_disable`, `agent_get_status`, `agent_preflight_start` /
  `agent_preflight_cancel`, `agent_chat_send`, `agent_get_proposals`,
  `agent_apply_proposal_batch(batch_id, op_ids)` (the op subset enables partial apply),
  `agent_reject_proposal_batch`, `agent_get_activity_log(page)`, `agent_get_spend`,
  `agent_set_proactivity`, `agent_mute_folder`, `agent_snooze_today`.
- Events (push, never poll): `agent-activity` (new activity-log rows), `agent-proposal-changed`
  (created/updated/invalidated/expired), `agent-notify` (the notification payload with actions),
  `agent-preflight-progress`, `agent-chat-delta` (streamed replies), `agent-status-changed`
  (degraded modes, §6.5).
- All of it goes through the typed bindings per the AGENTS.md IPC rules; the review dialog and
  activity panel are pure consumers of these commands and events.

## 10. The LLM provider layer

### 10.1 Why "hot-swappable providers" is false for agents

Single-shot prompts are interchangeable across providers; agent loops are not. The quirks:

1. **Wire shape**: OpenAI returns `tool_calls` answered by `role:"tool"` messages keyed by id;
   Anthropic returns `tool_use` blocks answered by `tool_result` blocks in the next user message
   (and errors if any id goes unanswered); Gemini returns `functionCall` parts answered by
   `functionResponse` parts in order, where `response` must be an object, never a scalar.
2. **Parallel tool calls**: all providers can emit several calls per turn, each with different
   batch-answer rules; mishandling ranges from API errors to silent degradation.
3. **Opaque reasoning state (the nastiest)**: thinking models attach encrypted state that must be
   round-tripped exactly. Gemini 2.5 puts a `thoughtSignature` on function-call parts that must be
   re-attached to those exact parts in history, or multi-step tool use quietly degrades. Anthropic
   extended thinking has `thinking` blocks with signatures validated server-side. OpenAI reasoning
   models have the equivalent via Responses-API reasoning items. Any abstraction that normalizes
   messages into a clean common shape and drops these blobs works in demos and breaks on step 3 of
   a real loop.
4. **Schema dialects**: Gemini accepts an OpenAPI-ish JSON Schema subset; OpenAI strict mode wants
   `additionalProperties: false` and all-required; Anthropic is permissive. One tool definition,
   three lints.
5. The boring rest: different streaming grammars, stop-reason names, error/rate-limit shapes, and
   three incompatible prompt-caching mechanisms.

### 10.2 Architecture

**Correction from review: the provider layer already exists in the codebase.** The tree ships the
`genai` crate (pinned `=0.6.0-beta.19` at review time) wrapped by `src/ai/client.rs`, with
`src/ai/CLAUDE.md` documenting the same per-provider quirk rationale this spec describes
(Responses-API routing, per-provider temperature handling, ~20 providers normalized). Do NOT run an
adoption spike and do NOT hand-roll adapters in parallel to it. The work is:

- **A small owned trait** (e.g. `AgentLlm`) as the agent-facing seam over the existing client:
  messages carrying an opaque per-message provider-state blob, tool declarations, normalized tool
  calls and stop reasons. Provider types never leak past it.
- **Extend the existing genai integration to agent-loop requirements** and verify the quirk list
  against it: multi-call turns, schema strictness per provider, and above all opaque
  thinking-state round-tripping (§10.1 point 3). Whether genai 0.6.x handles these for multi-step
  tool loops is the real open question (§18.1); if it falls short, the options are upstream
  contribution, a local patch, or per-provider adapters behind the trait for the gaps only.

### 10.3 Support tiers

- **Tier 1, agent-certified**: Anthropic, OpenAI, Gemini, and the local model (§10.4). Pinned
  known-good default models per provider; users may override with an "untested" badge.
- **Tier 2, community-supported**: any OpenAI-compatible endpoint. This single tier covers
  OpenRouter, Ollama, Groq, DeepSeek, xAI, and friends. Note that **OpenRouter is the "gateway
  service that keeps up with quirks for us"**, is already one of Cmdr's integrated providers,
  normalizes hundreds of models to the OpenAI schema server-side, and charges ~5% with no
  subscription. It remains a user choice, never a default (it is a middleman in the privacy path).
- New-model churn is handled by the eval harness doubling as a **regression suite** (§15): a
  fixture run costs on the order of a dollar, so certifying a model is a button press, not a
  project.

### 10.4 The local model option

A supported v1 option, not a cut: agent + summaries on the on-device model. The source of truth
for what ships is the model registry in the existing `ai` module (`AVAILABLE_MODELS` /
`DEFAULT_MODEL_ID`), not this spec; David's recollection is "an ~8B tool-calling model chosen ~6
months ago", and swapping in a newer one is an open task (§18.11). Documented
tradeoffs: noticeably weaker judgment and tool use than Tier 1 cloud models. It exists because
"nothing ever leaves your Mac" is a headline capability some users will accept the tradeoffs for.
Settings expose **two model slots**: bulk (summarizer) and interactive (wake/chat/planner), each
independently set to any supported provider including local.

## 11. Tools

### 11.1 One registry, multiple consumers (intent), and the current MCP reality

**Intent (David's stated direction):** the MCP infrastructure was built with this agent as its
**first** intended consumer; external MCP clients (dev tooling, Claude Code driving the app,
automated tests) are the second. One tool registry serves both, capability-gated per consumer: the
internal agent gets memory and notify tools; external clients don't. AI write paths converge on
the proposal queue (§8.1).

**Current reality (per `src/mcp/CLAUDE.md`, flagged in review):** the shipped MCP server is built
on a different write philosophy, "security via parity": external agents act through the same UI
actions a user performs (navigate, operate through the UI), deliberately without raw `fs.read`/
`fs.write` tools. That is not a proposal-gated registry, and the existing server's tools are
UI-control oriented rather than knowledge oriented.

These two must be reconciled during implementation, and the integration path is an open question
(§18.16): likely shape is that the knowledge/proposal/memory/notify tools land in a registry the
internal agent consumes, the external MCP server selectively exposes the knowledge tools and
`create_proposal_batch`, and whether external clients keep UI-parity writes alongside (or migrate
to proposals) is a product/security decision to make then, not silently here.

In docs, "the agent" means this feature; external MCP consumers are "AI clients" to avoid term
collision.

### 11.2 The v1 toolset

Knowledge: `get_folder_summary`, `search_summaries` (FTS), `list_stale_summaries(min_interest)`,
drive-index queries (sizes, counts, recency). Proposals: `create_proposal_batch`, list/withdraw.
Memory: scoped write (logged). Interaction: `notify_user`. Files: `read_file` (below), and an
archive-listing tool when zip support ships.

One-shot AI features (natural-language search, AI rename) are not "the agent" but use the same
substrate: e.g. the search box's NL path calls `search_summaries`. The registry and knowledge DB
are shared infrastructure; the agent is their stateful consumer.

### 11.3 `read_file` guardrails

The privacy and injection surface, so: per-call size caps, per-wake read budget, a sensitive-path
denylist (`~/.ssh`, browser profiles, keychains, and similar), content-to-cloud gated separately
from content-to-local-model, and **every read logged to the activity log with a reason**. File
content enters context as untrusted data, clearly delimited, never as instructions; the structural
defense remains §8 (content can at worst produce a reviewable proposal).

## 12. Privacy, consent, and cost

- Opt-in feature with an explicit consent screen recording: which provider, that file/folder
  **names** are sent during summarization, that **contents** are sent only per the content-access
  policy, and the sensitive-path exclusions. The recorded consent matters; the website privacy copy
  needs an update when this ships (business note).
- The activity log shows decisions, proposals, notifications, memory writes, and file reads, each
  with a rationale (principle 5).
- `cost_meter` powers a visible spend display (per job type) and budget caps (daily/monthly).
  Initial-index spend is shown in the preflight before it happens.

### 12.1 Enable flow and the Full Disk Access gate

Flagged in review as a hard prerequisite the first draft missed entirely:

- **Everything the agent reads in its home turf is TCC-protected.** Downloads, Documents, and
  Desktop are exactly the paths AGENTS.md's FDA-gate rule covers. The agent's read path (hot-folder
  summarization, content peeks) MUST respect the existing `fda_gate` (`is_fda_pending_runtime()`),
  and the agent feature effectively requires FDA to be granted; enabling it without FDA must not
  stack TCC popups (the exact failure mode the gate exists to prevent).
- **The enable flow is its own consent flow, not a new first-launch wizard step**: agent enable +
  provider/model choice + the consent screen (§12) + the proactivity dial (§9.5) + the preflight
  (§5.3), reachable from settings (and optionally offered after onboarding). Exact placement and
  its relationship to the existing onboarding wizard is an open question (§18.18).
- The user-facing copy drafted in this spec (preflight, notifications) is indicative and needs a
  style-guide pass at implementation time.

## 13. Naming and taxonomy

- **"agent"** is the name, user-facing and internal (tables, modules, tool prefixes), per the
  name-internals-after-the-UI rule.
- **"AI"** stays the umbrella for capabilities (settings section, provider config, one-shot
  features). The agent is the persistent, stateful entity.
- External MCP consumers are **"AI clients"** in docs. Future sub-entities, if ever, are
  **"subagents"**.

## 14. Prompts as repo assets

Markdown files with YAML frontmatter (`name`, `purpose`, intended model class, version note), plain
`{{variable}}` substitution, and `minijinja` only where a prompt genuinely needs conditionals or
loops. Dev builds load them from disk (instant iteration); release builds embed them. A
`prompt-lint` check joins the checker: every template compiles, and the variables each prompt
references match what its call site provides (catches the silent `{{folder_sumary}}` class of bug).

## 15. Testing and evals

- **Deterministic parts get ordinary unit tests** and they are the majority of the system:
  importance scorer, coalescer, digest compactor (budget adherence, compaction order), proposal
  lifecycle and drift detection, invalidation plumbing, retention pruning.
- **LLM behavior gets evals, not string asserts**: a fixture generator for synthetic home
  directories (build on `InMemoryVolume`), and a harness scoring summarizer and planner outputs
  against expectations (did it propose moving the invoices? did it leave the code folder alone?).
- The eval harness doubles as the **provider/model regression suite** for Tier 1 certification.
- **North-star metric: proposal acceptance rate**, tracked locally in `main.db`; opt-in aggregate
  telemetry can come later.

## 16. Settings surface (v1)

Provider/model for the two slots (bulk, interactive); budget caps; proactivity dial; excluded
paths; content-access policy; user-action-log toggle and retention; per-volume opt-ins (index,
summaries); the spend display.

## 17. Build order (v1 milestones, roughly)

1. **Storage**: `main.db` with migrations, retention, volumes table; relocate/rename the
   per-volume index DBs (`index-{volume_id}.db`) to `~/Library/Caches/<bundle id>/` as
   `drive-index-{volume_id}.db`, with the existing-install migration decision from §18.17. Uses
   the current bundle id; not blocked on the deferred data-dir rename.
2. **Importance scorer** (+ cache in the drive index) with thorough unit tests.
3. **Provider layer**: the trait, the `genai` spike, adapters for Tier 1, the two model slots.
4. **Knowledge layer**: summarizer pipeline (hot folders first, preflight, resumable walk), FTS,
   knowledge tools.
5. **Event pipeline**: coalescer, inbox, deadlines, digest compaction, restart reconciliation.
6. **Wake loop** + budgets + single-flight + degraded modes + activity log.
7. **Proposals**: schema, freeze-at-creation, drift, invalidation; review dialog + apply via the
   execution queue; notify tool + dial.
8. **Chat** surface wiring; `~/.cmdr` files; memory writes.
9. **Evals v0** alongside 4-8, not after.

## 18. Open questions and investigations (honest list)

1. Does the already-shipped `genai` integration (`src/ai/client.rs`, pinned 0.6.x) handle the
   agent-loop requirements: multi-call turns, per-provider schema strictness, and opaque
   thinking-state round-tripping in multi-step tool loops? If not: upstream contribution, local
   patch, or gap-filling adapters behind the `AgentLlm` trait.
2. SMB volume-identity canonicalization: same share via `nas.local`, IP, and DNS name must converge
   on one identity; is a server GUID available per protocol? (Believed not hard, but undesigned.)
3. Importance-scorer signal weights and the exact scoring formula: needs iteration against real
   home directories.
4. `kMDItemLastUsedDate` sampling strategy and cost on large folders.
5. Wake deadline tier values (2-5s / 1-5min / 1h) and the digest token budget (2-4k): initial
   guesses, tune with use.
6. Refresh budget defaults (monthly cap value).
7. The execution queue's API doesn't exist yet (the product feature is upcoming); proposals target
   it by design, so its shape is a dependency.
8. `listing_fingerprint` exact definition (proposed: hash over child names + sizes + mtimes).
9. Conversation/thread data model details, and how a notification reply inherits wake context
   technically.
10. Memory mining design (v1.5): which implicit signals, what confidence threshold, whether mined
    memories need their own review affordance.
11. Local model refresh: evaluate whether the shipped local model (see `ai` module
    `AVAILABLE_MODELS` / `DEFAULT_MODEL_ID` for the source of truth) should be replaced with a
    newer small tool-calling model before the agent ships.
12. Verify Time Machine and purge semantics for `~/Library/Caches/<bundle id>/` behave as assumed.
13. Tool-schema versioning policy for external MCP consumers as the registry grows.
14. Cost-estimate accuracy in the preflight (tokens-per-folder model needs calibration).
15. Whether `interest_weight` denormalization into `main.db` summaries is worth it vs. always
    reading from the drive index. (Also keeps the "split writers" story honest: the indexer should
    not write into `main.db`; if denormalized, the agent copies the weight at summary time.)
16. The MCP integration path (§11.1): how the agent's tool registry relates to the existing
    UI-parity MCP server, which knowledge/proposal tools get exposed externally, and whether
    external clients keep UI-parity writes alongside the proposal queue. Product and security
    decision, not just plumbing.
17. Index DB migration for existing installs when relocating per-volume `index-{volume_id}.db`
    files to Caches: move the files, or accept a one-time full rescan on upgrade?
18. Enable-flow placement: standalone settings-launched flow vs. an offer after the existing
    onboarding wizard; how it composes with the FDA gate when FDA is not yet granted.
19. Event tap point (§6.1): exactly where the agent's coalescer subscribes on the indexer's
    corrected event stream, and how the standalone `downloads/` watcher and the agent's
    Downloads-related detectors relate (merge? coexist?).
20. iCloud and cloud-stub files: `read_file` or content-tier summarization on dataless files
    triggers `fileproviderd` downloads (a cost/bandwidth landmine if the agent silently
    materializes gigabytes). Likely policy: never materialize dataless files in background jobs,
    treat them as metadata-only, content access on demand with user awareness. Needs a decision.
21. Agent-state persistence vs. the settings registry: settings are frontend-owned writes per the
    existing settings architecture, but the agent (backend) has durable operational state (for
    example throttle/snooze state). Likely resolution: user preferences stay in settings, agent
    operational state lives in `main.db`, never in the settings store. Confirm against the
    settings registry's ownership rules.

## 19. Decision log

| #   | Decision                                                                                    | Rationale                                                                                  |
| --- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| D1  | Two DB families: per-volume `drive-index-{volume_id}.db` (cache) + `main.db` (durable catch-all) | Regenerable vs. valuable; separate writers; different backup policies; index is per-volume today |
| D2  | Index DB lives in `~/Library/Caches/<bundle id>/`                                           | Platform-native "purgeable, no backup"; Time Machine skips Caches                           |
| D3  | `main.db` is a generic catch-all, not agent-specialized                                      | Action logs and future durable state land there too                                        |
| D4  | No custom collation in `main.db`                                                             | Stay `sqlite3`-inspectable; the index DB's collation forced a custom query tool             |
| D5  | Everything keys `(volume_id, rel_path)`; volumes table ships in v1                           | Multi-volume (NAS, S3, FTP) need arrives within weeks; retrofitting keys is brutal          |
| D6  | Local volume only active in v1; SMB/MTP/S3 summaries deferred                                | Staleness/reconnect semantics differ per type; don't block the spine                       |
| D7  | Staleness is per-volume, first-class; agent caveats answers                                  | Enables answering about unmounted volumes (offline NAS index), a headline feature           |
| D8  | Deterministic importance scorer, cached in the drive index                                   | Fast, free, testable; gates summaries, event interest, and informs the LLM                  |
| D9  | Summaries: whole drive at system-decided depth, via prune + threshold + descend-list         | One pass; LLM refines depth only as a byproduct of calls already paid for                   |
| D10 | Summaries feed from the drive index, not the filesystem                                      | Listing-tier summaries need zero extra I/O                                                  |
| D11 | Two summary tiers: listing-only bulk vs. content-aware deep                                  | 10-100x cost cliff; content reserved for hot folders and on-demand                          |
| D12 | Cloud model is the summarization default; local stays an option                              | Opt-in + BYO key + value justifies cost; "nothing leaves the Mac" kept for those who want it |
| D13 | Batch APIs rejected for the initial pass                                                     | ~24h async window conflicts with "summaries ready right after indexing"                     |
| D14 | Hot folders summarize in parallel with indexing                                              | Their paths are known a priori                                                              |
| D15 | Preflight with folder count, cost estimate, privacy disclosure; resumable                    | Transparency; resolves the "how many important folders" guess empirically                   |
| D16 | FTS5 over summaries first; embeddings deferred                                               | Cheap, good enough for "where do invoices live"; vectors are regenerable later              |
| D17 | Agent receives digests, never raw events; deadline-scheduled inbox; drain-all on wake        | Bounded context; MAX(interest) wake policy falls out implicitly                             |
| D18 | No idle/heartbeat LLM calls; noise absorbed deterministically                                | ~Zero cost when nothing happens                                                             |
| D19 | Digest has a hard token budget; aggregator decides granularity                               | The LLM never sees unbounded input                                                          |
| D20 | Restart: roll-forward digest; full-rescan recovers via `list_stale_summaries` diff tool      | Events are hints, state is truth; also covers macOS purging the cache DB                    |
| D21 | User actions inside Cmdr are first-class agent input                                         | Highest-signal events; manual moves and rejections carry intent                             |
| D22 | Beliefs and rules in markdown (`~/.cmdr/`); operational data in SQLite                       | Human-auditable agent "mind"; radical transparency                                          |
| D23 | `~/.cmdr/CMDR.md` global profile + `rules/*.md` with `applies_to` globs                      | Folder-scoped rules without polluting folders                                               |
| D24 | Folder-level `CMDR.md` cut from v1                                                           | Injection vector with authority; `applies_to` covers the need                               |
| D25 | Agent memory in `~/.cmdr/memory/`, separate from rules, capped, writes logged                | "Told me" vs. "inferred" never blur; user can audit/edit/delete                             |
| D26 | No direct write tools; proposals are the only write path, for ALL AI consumers               | Safety by construction; structural prompt-injection defense; one consent surface            |
| D27 | Freeze proposals at creation (pattern → concrete list); pattern kept as display text         | No drift between what was shown and what runs                                               |
| D28 | Per-op child rows with own statuses; partial apply                                           | "Apply 11, skip 3 stale" beats all-or-nothing                                               |
| D29 | Drift detection via per-op `(inode, size, mtime)` snapshot, re-verified at apply             | Creation→apply gap can be days                                                              |
| D30 | Trash over delete; batch op caps; proposals expire                                           | Reversibility; reviewable batches; stale suggestions are worse than none                    |
| D31 | Standing rules (live patterns) deferred to v2 with own consent UX                            | A persistent auto-applying pattern is a different risk class                                |
| D32 | No `priority` column; `created_by_model` is provenance only                                  | YAGNI; no logic on "authoritativeness"                                                      |
| D33 | Apply rides the upcoming execution-queue feature and the existing op pipeline                | Zero new write paths; preflight/rollback for free                                           |
| D34 | No subagents in v1; four job types (wake, chat, planner, summarizer) instead                 | One brain, different prompts/models per job; hierarchy unearned                             |
| D35 | "Librarian" is a tool, not an agent                                                          | An FTS SELECT needs no LLM intermediary                                                     |
| D36 | Wake context is fresh each time; continuity via DB/memory, not transcript                    | The defining difference between an agentic app and a chat app                               |
| D37 | Single-flight agent; chat priority; wakes queue and digests merge                            | No self-conflicting concurrent writes                                                       |
| D38 | Per-wake budgets (tool turns, wall time, file reads) + house cancellation pattern            | Runaway loops impossible by construction; visible and killable                              |
| D39 | Proactivity dial chosen at onboarding; named policy bundles; never silently self-adjusts     | No silent default; "want me to pipe down?" over creepy auto-tuning                          |
| D40 | Tier 1 providers: Anthropic, OpenAI, Gemini, local; Tier 2: any OpenAI-compatible endpoint   | Bounded certification surface; OpenRouter (already integrated) carries the long tail        |
| D41 | Own `AgentLlm` trait with opaque per-message provider state, over the already-shipped `genai` integration | Thinking-state round-trip is the make-or-break; the trait is the asset; never build a parallel provider layer |
| D42 | Pinned default models + "untested" badge + evals as regression suite                         | New-model churn becomes a button press, not a project                                       |
| D43 | Two model slots: bulk vs. interactive                                                        | Summarization and judgment have different cost/quality needs                                |
| D44 | Name: "agent" (user-facing and internal); "AI" stays the capability umbrella                 | Name-internals-after-UI rule; honest and specific enough                                    |
| D45 | Prompts as markdown + frontmatter + minijinja-as-needed; dev hot-reload; `prompt-lint` check | Iterate fast; catch template drift in CI                                                    |
| D46 | Acceptance rate is the north-star metric                                                     | Directly measures suggestion quality                                                        |
| D47 | Data-dir rename decoupled into its own spec                                                  | Aesthetic change with plugin/migration risk; must not block the agent                       |
| D48 | User action log: local-only, opt-out, ~90-day retention                                      | High-signal input with a privacy posture                                                    |
