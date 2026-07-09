# Operation log — implementation plan

Status: PLANNED, 2026-07-09. Owner: David. This plan designs Cmdr's **operation log**: a durable, cross-volume journal of
every file mutation, with rollback support, indexed search, retention, MCP query/rollback tools, and a thin alpha UI.
Codebase claims below were verified against the live tree (`main`, 68c9053d5, verified 2026-07-09) via CodeGraph and the
subsystem C+D.md files — confirm with `codegraph_search` if refs drift.

This is the **first durable database in the app**. Every other SQLite store (drive index, `importance.db`) and every
JSON store is either a disposable per-volume cache (delete-and-recreate on schema change) or quarantine-and-restart. The
operation log must live for years, so it introduces two things nothing in the tree has today: a **forward-migration ladder**
(no delete-and-recreate on version bump) and **retention discipline** (prune by age and size, actually reclaim disk).

## Why this exists

David wants Cmdr to answer "what did I do to my files, and can I undo it?" — durably, across volumes, fast enough to
search, and cheap enough that logging never slows an operation. Concretely:

- **Provenance.** Every logged action distinguishes who initiated it: the user, an external AI client (over MCP), or (in
  the future) the in-app agent. Trust and auditability start here.
- **Rollback.** The journal can reverse operations: undo a copy (delete the copies), a move (move back, only if nothing
  was overwritten), a trash (restore from trash), a rename, a compress (delete the created archive), and later a zip edit
  (result need not be byte-identical). Cross-volume rollback works when both volumes are connected; when not, it's
  unavailable with a clear reason.
- **Search.** "When did I delete `dog.jpg`?" answers in reasonable time, for both the user and an agent — indexed
  retrieval, not a scan.
- **Retention.** Logs collect by default, bounded by an age limit (default forever) and a size limit (default 3 GB).
- **Foundation for undo.** A later Cmd+Z is implementable as "roll back the last rollbackable user-initiated operation."
  This plan builds toward it without building it.

### Product values in play (from `docs/design-principles.md` and `AGENTS.md` § Principles)

- **Protect the user's data.** The rollback engine writes to the filesystem, so it is data-safety-critical: it re-checks
  every per-item precondition against a recorded snapshot before touching anything and skips on drift (partial rollback),
  never operating on a changed file. TDD is mandatory here (`tdd-red-green`).
- **Respect the user's resources.** Logging is off the operation's hot path: the capture layer pushes lightweight events
  to a bounded channel; a single dedicated writer thread does batched inserts. A 1M-item op journals at near-zero
  marginal cost by piggybacking on stats the op already does; reverting a 1M-file copy (deleting the copies) is a plain
  managed delete, no radically slower.
- **Rock solid.** Failed and canceled ops are journaled too, with per-item completion state — because what completed
  before a cancel is exactly what a rollback needs. Everything is cancelable, including a rollback in flight.
- **Elegance above all.** The journal is written at one chokepoint (the managed operation pipeline), never by IPC
  handlers. Rollback is not a bespoke engine — it is inverse operations pushed back through the same hardened pipeline,
  themselves journaled.
- **Delightful UX + radical transparency.** The user can see their own history (thin alpha UI now, richer later), and
  every rollback is itself a visible, cancelable operation.

## Product requirements (from David — fixed unless physically impossible)

Recorded verbatim as the contract this plan satisfies; each is traced to a milestone below.

1. **Provenance**: agent-initiated (future), user-initiated, external-AI-client-initiated (MCP) are distinguished. → M2.
2. **Granularity**: enough to understand what happened, compact enough not to waste disk. A mass rename is one grouped
   operation containing a sequence of per-item renames, displayable as a group. → M1 (schema) + M2 (capture).
3. **Rollback**: copy, move (only if no overwrites), trash, rename, compress (delete the created archive), later zip
   edits. Cross-volume works when connected; else unavailable with a clear reason. → M3.
4. **Statuses**: two-axis model, see D3. → M2.
5. **Storage**: durable, not a cache, not volume-specific — a new database. → M1.
6. **UI**: display is out of scope EXCEPT (a) the Debug window shows some items, and (b) a thin alpha "Operation log" menu
   item → soft dialog, last 50 + load-50-more, ALPHA badge, i18n'd and style-guide compliant. → M6 (debug), M7 (alpha).
7. **MCP**: query + rollback tools REQUIRED (so an agent can test end to end without the FE). → M5.
8. **Performance**: logging must not measurably slow ops; reverting a 1M-file copy ≈ a plain delete of the same files. →
   M2 (perf test) + M3 (streaming rollback).
9. **Search**: indexed retrieval; "when did I delete dog.jpg" in reasonable time, usable by user and agent. → M4.
10. **Retention**: collect by default; age limit default forever (presets 30d, 90d, 1y, custom); size limit default 3 GB
    (presets 100 MB, 250 MB, 1 GB, 2 GB, 3 GB, 5 GB, custom). → M4 (enforcement) + M6 (settings).

## Current state (the map the implementer needs)

Verified against the code on 2026-07-09. Read the colocated `CLAUDE.md` + `DETAILS.md` of each subsystem before building
on it.

### The operation pipeline (the write chokepoint)

- **`src-tauri/src/file_system/write_operations/`** — one managed pipeline every write flows through.
  `manager.rs::OperationManager` is a process-global singleton with two entry methods: `spawn_managed(descriptor, state,
  deferred)` for streaming transfers/deletes (copy, move, delete, trash, compress/`ArchiveEdit`), and
  `run_instant(descriptor, op)` for scan-free metadata ops (rename, mkdir/`CreateFolder`, mkfile/`CreateFile`) that await
  inline and return their result. **The lead's belief is confirmed:** rename/mkdir/mkfile were routed through the manager
  (via `run_instant`), so they carry manager bookkeeping (a `Running` record, busy-volume marks) but reserve no lane.
- **`WriteOperationType`** (`types.rs:41`, verified): `Copy | Move | Delete | Trash | Rename | CreateFolder | CreateFile
  | ArchiveEdit`. This is the taxonomy the journal's `kind` mirrors. Compress and zip-inner edits both cross the wire as
  `ArchiveEdit`; their identity is frontend-only. **Design the journal `kind` extensibly** (see D2) so archive-edit
  variants (compress vs edit vs future extract) and future op kinds slot in without schema churn.
- **`OperationEventSink`** (`event_sinks.rs:101`) — the trait every write op emits through (`emit_progress`,
  `emit_complete`, `emit_cancelled`, `emit_error`, `emit_settled`, `emit_source_item_done`, `note_source_landed_clean`,
  and others). It is built **only at the IPC edge** (`commands/file_system/write_ops.rs`,
  `commands/file_system/volume_copy.rs` each `Arc::new(TauriEventSink::new(app))`) and injected as `Arc<dyn
  OperationEventSink>` all the way down. The whole managed path runs headless under the test `CollectorEventSink` with no
  Tauri runtime. This is the "lifted above the starters" refactor: the journal hook rides this same seam.
  - **Crucial limitation for capture (see D4):** the sink carries only aggregate terminal counts (`WriteCompleteEvent`:
    `files_processed`, `files_skipped`, `bytes_processed`) plus a per-source-done signal (`WriteSourceItemDoneEvent {
    operation_id, source_path }`) and a filename-only `current_file` on progress. It does **not** carry per-item dest
    paths, sizes, mtimes, or per-item outcomes. So journaling per-item rows needs a richer hook than decorating the sink.
- **Per-item stats already happen** (piggyback points, near-zero marginal cost): copy stats each source at
  `transfer/copy/single_item.rs:263` and `:308` (`fs::symlink_metadata`); trash stats each item at `delete/trash.rs:124`
  and receives optional `item_sizes`; the scan phase (`scan.rs`) stats every entry for byte totals and inode dedup. Size
  and mtime are in hand at exactly the points a per-item journal row would be written.
- **Batching model**: one user action = ONE managed operation carrying many source items; per-item work loops inside the
  deferred future. There is no per-item sub-record today — the journal introduces per-item rows.
- **Cancellation & partial completion**: `OperationIntent` (`AtomicU8`, `Running → RollingBack | Stopped`). Cancel keeps
  fully-copied files (deletes the last partial); Rollback deletes all copied files in reverse. `CopyTransaction`
  (`state.rs`) records `created_files` in order. The terminal `WriteCancelledEvent` carries `files_processed` and
  `rolled_back`. `write-settled` fires exactly once per op, panic-safe, after the terminal event — the natural
  "finalize the journal entry" point.
- **Trash mechanics** (`delete/trash.rs`): macOS `NSFileManager.trashItemAtURL_resultingItemURL_error` inside an
  autoreleasepool; Linux `trash` crate. **The in-trash location is NOT recorded today** (passes `None` for
  `resultingItemURL`, trash.rs:41-43) and there is no restore path. Capturing it is a trivial change (pass a
  `Some(&mut url)` out-param at trash.rs:45; the OS already returns it). Trash restore (D3) depends on this.
- **Compress** (`archive_edit/compress.rs:162`): seed a valid empty zip (or remote seed), then copy-into via
  `route_archive_copy_into`. The target `.zip` is committed by the mutator's own temp+rename; whether the target
  pre-existed (overwrite) vs was net-new is known at seed time (compress.rs:176-183) — capture it there for rollback
  eligibility (delete the created archive only if net-new).
- **What bypasses the managed pipeline** (verified — these need explicit handling in M2):
  1. `paste_clipboard.rs::write_payload_to_dir` (paste-clipboard-as-file, issue #35) — writes `pasted.<ext>` directly
     via `Volume::create_file`, no manager, no sink.
  2. `commands/rename.rs`'s direct `move_to_trash_sync` (single trash outside the batch trash path).
  3. Native drag-out fulfillment (`native_drag::fulfillment`) — not a real write op (destination is outside Cmdr).

### The MCP tool registry (for M5)

- **`src-tauri/src/mcp/tool_registry.rs`** — one `mcp_tools!` macro table; every tool authored once (name, description,
  hand-authored `serde_json` JSON schema, `TokenGate`, handler `run:` shape). Handlers live in `mcp/executor/*.rs`.
  Read `mcp/CLAUDE.md` + `mcp/executor/CLAUDE.md` first. Names are snake_case, params camelCase (hard convention).
- **Capability gating** is `TokenGate` (`tool_registry.rs:32`): `Open` (reads/nav/search, and destructive ops that still
  prompt the user), `Always` (config mutation with no confirmation), `IfAutoConfirm` (gated iff `autoConfirm == true`:
  copy/move/delete/compress), `IfConfirmAction`. Enforced in `server.rs` before dispatch via
  `auth.rs::tool_call_requires_token`. Structural tests force conscious gating: `test_autoconfirm_tools_are_gated`,
  `test_gate_table_is_complete_and_correct`, `EXPECTED_TOOL_NAMES`, and the tool-count assertion. New tools must update
  all of them.
- **Provenance today**: there is **none** crossing into the backend. An MCP `copy` handler emits the same `mcp-copy` FE
  event the UI would, and the FE dispatches the identical `fileCopyCommand`. The only provenance concept is FE-side
  `NavigateSource` (`'user' | 'mcp' | ...`, `navigate.ts:136`), used for navigation only, and it never crosses IPC. So
  AI-vs-user attribution does not exist and must be introduced (D5).

### The house DB patterns (for M1, from `importance/` — mirror, but diverge where noted)

- **One writer thread per DB.** `importance/writer.rs` is the template: a cloneable handle wrapping a bounded std
  `mpsc::sync_channel(1024)`, a plain named OS thread running a receive-match-transact loop, message enum with
  request/reply (`NextGeneration(Sender<u64>)`) and barrier (`Flush(Sender<()>)`) variants, errors logged and swallowed.
  Callers hop on via `spawn_blocking`. `rusqlite` 0.39 (bundled), no pool for writes; reads use short-lived read-only
  connections. **Divergence:** the operation log is a single cross-volume DB, so no per-volume `WriterRegistry` map — one
  `OperationLogWriter` in managed state.
- **Connection factory** (`importance/store/connection.rs`): WAL + `auto_vacuum = INCREMENTAL` + `busy_timeout=5000` +
  `synchronous=NORMAL`. **Keep all of this.** **Divergence:** importance registers the `platform_case` collation on
  every connection; the operation log does NOT (see D2 — inspectability), storing a precomputed folded column instead.
- **Schema versioning — the key divergence.** Everything in the tree is delete-and-recreate (index `SCHEMA_VERSION=14`,
  importance `=2`) or JSON quarantine-and-restart. **There is no forward-migration helper anywhere.** The operation log
  builds the first real migration ladder (D2). Delete-and-recreate stays ONLY as the last-resort genuinely-corrupt-file
  path, never the version-bump path.
- **Retention/vacuum**: importance has none (it's disposable). The pattern to borrow is the index writer's
  `indexing/writer/maintenance.rs`: a tiered `pick_vacuum_cap(freelist)` + `PRAGMA incremental_vacuum(cap)` on a timer so
  a big prune doesn't stall the writer. Importance sets `auto_vacuum = INCREMENTAL` but never actually calls
  `incremental_vacuum` — the operation log must call it.
- **Dev bins**: `crates/index-query/` (`publish = false`, links the app as a lib). `importance-snapshot.rs` /
  `importance-measure.rs` / `importance-tune.rs` are the model for an `operation-log-dump` bin: open the DB read-only, print
  or export, call into library functions, never reimplement logic. `cargo run -p index-query --bin operation-log-dump`.
- **Eval-style tests**: `importance/evals/` — pure fitness functions, `Scenario` fixtures, hard constraints as ordinary
  `#[test]`s plus a pinned soft-score floor. The rollback-correctness suite (M3) mirrors the hard-constraint half: a
  scenario is an initial FS state + a mutation sequence; the invariant "apply-then-rollback == original state" is a
  `#[test]`.
- **App data dir**: `crate::config::resolved_app_data_dir(app)` (`config.rs:27`). The operation log lives here as
  `operation-log.db` (durable, Time Machine backs it up — deliberately, see D1).

### The frontend surfaces (for M6, M7)

- **Settings**: registry-based (`src/lib/settings/settings-registry.ts`), one declaration per setting + a typed
  `SettingsValues` key (`types.ts`) + i18n keys + a hand-rendered `SettingRow` in a `sections/*Section.svelte`. The
  preset-select-with-custom pattern already exists: `network.shareCacheDuration` (`type: 'duration'`, `component:
  'select'`, `constraints.options` with `labelKey`s, `allowCustom`, `customMin/Max`) is the age-limit template; a numeric
  preset select (bytes) is the size-limit template. "Forever" is a sentinel value with its own `labelKey`. Guide:
  `docs/guides/adding-a-new-setting.md`.
- **Debug window**: separate Tauri window, `routes/debug/+page.svelte` with a `SECTIONS` sidebar + per-panel
  `Debug*Panel.svelte`. Add a `SectionId`, a `SECTIONS` entry, and `DebugOperationLogPanel.svelte` fetching via a typed
  `commands.*` wrapper.
- **ALPHA/BETA badge exists**: `src/lib/ui/StatusBadge.svelte` (`status: 'alpha' | 'beta'`), data-driven via
  `getBadgeStatus(featureId)` reading repo-root `feature-status.json`. Add an `operation-log` feature with `"status":
  "alpha"` and mount `<StatusBadge>`.
- **Soft dialog**: `src/lib/ui/ModalDialog.svelte` + `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`). The **What's-new
  dialog is the exact analog** (menu-triggered, list-rendering, manually reopenable, bounded slice): `whats-new-trigger.
  svelte.ts` holds `$state({ open, releases, ... })` + `openWhatsNew()`/`closeWhatsNew()`, `WhatsNewDialog.svelte` uses
  `dialogId="whats-new"`, mounted in `routes/(main)/+page.svelte`. Model the Operation-log dialog on it with paging
  state `{ open, entries, offset, hasMore }`.
- **Native menu**: `src-tauri/src/menu/` (`mod.rs` id constants + both mapping fns; `macos.rs`/`linux.rs` build the
  bars; `menu_handlers.rs` emits `execute-command`). The What's-new item (App-scoped, opens a dialog) is the template,
  not Compress (file-scoped). The "four places" rule (`command-registry.ts` entry, handler in
  `routes/(main)/command-handlers/`, `mod.rs` mappings + `MenuItem::with_id` in both `macos.rs` and `linux.rs`,
  `menuCommands` in `shortcuts-store.ts`) plus the `COMMAND_IDS` id — miss one and it half-works.
- **i18n**: 10 locales (`de en es fr hu nl pt sv vi zh`), keys in `src/lib/intl/messages/<locale>/<area>.json` with `@key`
  translator descriptions, typed `MessageKey`. Consume via `tString`/`t`/`<Trans>` from `$lib/intl`. Guide:
  `docs/guides/i18n.md`. New strings land in `en/` and propagate through the parity/translation pipeline to the other 9.

## Naming (decided by David — one term: "operation")

David's decision: **unify on "operation" everywhere.** The rationale, recorded so a future agent can defend it against a
"let's rename this" pass in either direction:

- **The journal row IS a managed operation**, 1:1 with the pipeline's `operation_id`. The codebase already says
  "operation" everywhere — the `operations` / `operation_items` tables survived every earlier naming pass precisely
  because that's the honest name for the thing.
- **"Activity" fails as a countable entity.** "A copy is an activity" is bad English; activity is a mass noun — fine for
  a feed *surface*, wrong for *rows*. The rows are operations.
- **"Action" is reserved.** It's the natural future name for the agent spec's navigation/intent stream
  (`user_action_log`); spending it on mutations would mean two words for one entity (vs. the pipeline's "operation").
- **NOT "write operation" in this domain.** A qualifier earns its place only where the contrast class lives in the same
  namespace. In the journal/UI domain every member is a write (reads are unjournalable — nothing to roll back), so
  "write" carries zero information there → the surface is "Operation log", the module `operation_log`. **But at CODE
  level the contrast class is real** (`search/`, `indexing/` read paths in the same namespace), so the `write_operations`
  module and the `WriteOperationType` type **keep their `write_` prefix — explicitly NOT renamed in this effort.** This
  is the two-namespace rule: state it so nobody "unifies" it later in either direction (don't strip `write_` from the
  code module; don't add `write` to the journal/UI name).

Applied:

- **Internal** subsystem/module: `operation_log` (`src-tauri/src/operation_log/`). DB file: `operation-log.db`. Dev bin:
  `operation-log-dump`. Writer/panel/FE helpers: `OperationLogWriter`, `DebugOperationLogPanel`,
  `getRecentOperationLogEntries`, `src/lib/operation-log/`, `OperationLogDialog.svelte`, `dialogId="operation-log"`,
  command `log.operationLog`, `openOperationLog()`, `feature-status.json` id `operation-log`, i18n `operationLog.json`.
- **MCP tools**: `operations_list` / `operations_get` / `operations_rollback` (noun-first grouping; the implementer
  confirms this matches the registry's existing tool-naming style and adjusts to `operation_log_*` if that reads better
  against the shipped names).
- **User-facing** surface: **"Operation log"** (sentence case per the style guide) — menu item, dialog title, settings
  section, ALPHA badge.
- **Table names stay `operations` / `operation_items`** — they were right all along; no rename.
- **Convergence:** when the agent ships, its decision log (agent-spec `agent_log`) joins the same user-facing timeline.
  Whether that merged surface is later *labelled* "Activity" is a UI-copy decision for then — "activity" stays available
  as a possible future surface name, but never as an entity/row name. This plan builds the mutation half; the surface is
  designed so agent rows join later (D7).
- **Not renamed on purpose:** the git worktree directory and branch (`action-log` / `david/action-log`) keep their names
  — git branches aren't product naming, and the churn isn't worth it. Don't "fix" them.

## Agent-spec reconciliation (edits this plan requires)

The agent spec (`docs/specs/later/agent-spec.md`) describes storage this plan changes. Required edits, made in M1's docs
pass, each recorded with rationale:

1. **§4.1 / D3 (`main.db` as the durable catch-all holding "user action logs")** → the operations journal gets its
   **own** `operation-log.db`, not a table in `main.db`. Rationale (D1): a multi-GB append-heavy journal would bloat the
   Time Machine backups of `main.db` and defeat its "small, inspectable catch-all" nature; the two also have different
   write cadences (per-mutation vs agent-episodic) and different retention. `main.db` stays the agent's durable
   catch-all; the mutation journal is a peer durable DB. Weighed honestly against D3's "generic catch-all" intent — the
   split serves that intent better (keeps `main.db` small and inspectable) than a giant table would.
2. **§4.2 `user_action_log` ("user operations and navigation inside Cmdr")** (the agent spec's own table name, kept
   verbatim here) splits along its two mandates:
   - The **operations** half (mutations) is subsumed by this `operation-log.db` journal — richer than the spec's sketch
     (per-item rows, snapshots, rollback linkage).
   - The **navigation/intent** half stays where the importance plan already put it: `importance.db`'s `record_visit`
     (counts + recency, local-only) today, folding into the agent's future intent stream later. This plan does NOT record
     navigation (D6 — mutations only).
   So the agent spec's `user_action_log` becomes "navigation/intent" only, and the operations it imagined are the operation
   log. Note the three-way boundary explicitly so no future effort builds a second operations recorder.
3. **§8.4 / D33 (proposals apply through the op pipeline)**: the operation log now journals every applied proposal batch
   for free (proposals execute through the managed pipeline, which the journal hooks). A rejected proposal never becomes
   an operation, so it never appears in this journal — the agent-spec "agent-suggested / accepted / rejected" states live
   in the future `proposals` / `proposal_ops` tables (agent-spec §8), which reference operations here. This plan's
   `initiator = agent` value is reserved for when the agent lands; v1 delivers `user` and `ai_client` only (D5).
4. **Naming (§4.2 `agent_log`):** with David's decision to name this journal the **"operation log"** (Naming section),
   there is no term collision with the agent spec's `agent_log`: this journal is operations, the agent's is its decision
   log, and "action" stays reserved for the `user_action_log` navigation/intent stream. Their future *merged UI surface*
   may be labelled "Activity" later (a UI-copy call, not an entity name). Note this in the agent-spec edit so a future
   planner reads the surfaces correctly and doesn't re-collide the terms.

## Key decisions (with intent — adapt if reality differs, but know the why)

**D1 — A separate durable `operation-log.db` in the app data dir, Time Machine-backed.** Weighed: (a) a table in the agent
spec's `main.db` — rejected, `main.db` doesn't exist yet and this plan won't create it, and a multi-GB append-heavy
journal defeats `main.db`'s small-inspectable-catch-all purpose and bloats its backups; (b) the drive index / importance
cache DBs — rejected, those are disposable per-volume caches in `~/Library/Caches/` that Time Machine skips and the OS
may purge, and the journal is durable and cross-volume. **Chosen:** its own `operation-log.db` beside where durable app data
lives (`resolved_app_data_dir`), which Time Machine backs up normally — correct, because a mutation history IS valuable
user data. It is single (not per-volume) because cross-volume operations (copy from disk A to disk B) are one operation
with one identity. Like `main.db` it carries: **no custom collation** (stay `sqlite3`-inspectable — D2), a
`schema_version` + **forward migrations** from day one (this DB lives for years — D2), and **retention** by age and size
(M4). It is itself sensitive (a map of the user's file activity): stays local, never transmitted; note this for
`docs/security.md`.

**Time Machine backup — confirmed, kept (David's call).** `operation-log.db` is backed up like any Application Support
file. Rationale: mutation history is valuable user data, and restoring it brings back undo-ability, so it belongs in
backups. The worst-case cost — a heavy-churn user's backups growing with the journal — is **accepted**; retention (D9,
capped at 3 GB by default) bounds it. If heavy-churn testers ever complain, the named lever is a **future advanced
"exclude operation log from backups" toggle** (setting an exclusion attribute on the file) — not built now, just the
identified escape hatch so the decision is reversible.

**D2 — Schema: interned dirs + grouped operations + per-item rows; app-side case folding, no collation; a migration
ladder.** The shape (indicative DDL; the implementer owns the final columns):

- `meta` — `schema_version` (integer) and bookkeeping. The migration anchor.
- `dirs` — interned directory prefixes: `dir_id` PK, `volume_id`, `parent_dir_id` (nullable for a volume root), `name`,
  `name_folded`, `UNIQUE(volume_id, parent_dir_id, name_folded)`. This is the drive index's `(parent, name)` pattern.
  **Why intern:** a 1M-file operation under one tree shares a handful of directories; storing full paths per item would
  bloat the DB and the backup. Item rows reference `dir_id` + a leaf name. Interning is per-DB (reused across
  operations), so a hot directory is stored once forever.
- `operations` — one row per user-level batch: `op_id` PK (**reuse the pipeline's existing `operation_id` UUID string**
  so the journal row correlates with the live op), `kind` (see taxonomy), `initiator` (`user | ai_client | agent`),
  `execution_status`, `rollback_state`, `not_rollbackable_reason` (nullable enum), `rolls_back_op_id` (nullable FK to
  `operations.op_id`, the rollback linkage), `source_volume_id`, `dest_volume_id` (nullable), `started_at`, `ended_at`,
  `item_count` (the **planned** total from the scan, informational — NOT the completeness yardstick; see D4), `items_done`,
  `bytes_total`, `search_coverage` (`full | top_level_only`; `full` means the index subtree was current AND every
  `search_only` leaf row persisted — D-granularity and D4 below) plus a nullable **`search_coverage_reason`** typed enum
  set when `top_level_only` (`capped | index_absent | index_stale | volume_not_live | search_row_incomplete`) so the
  distinct honest-gap causes stay distinguishable for the future agent.
  **No stored rendered summary** (Finding 3): the UI label ("Move 214 items") is formatted at render time from the typed
  ingredients already in the row (`kind` + `item_count` + volume names) via `$lib/intl`, so it localizes per viewer.
  A stored string frozen in English at capture would break the i18n'd dialog and the style guide. (An optional
  `dev_summary` for the Debug panel / `operation-log-dump` is fine — explicitly dev-only and non-localized, never shown in
  the alpha dialog.)
- `operation_items` — per-item rows: `item_id` PK, `op_id` FK, `seq` (order within the op, for grouped display and for
  reverse-order rollback), `entry_type` (`file | dir`), `row_role` (`rollback_unit | search_only`, D-granularity below),
  `source_dir_id` FK, `source_name`, `source_name_folded`, `dest_dir_id` (nullable) FK, `dest_name` (nullable), `size`
  (nullable), `mtime` (nullable), `outcome` (`done | skipped | failed | rolled_back | ...`), `overwrote` (bool — did this
  item overwrite an existing dest?). Index on `source_name_folded` (and a `dest_name_folded` companion) for search.
  **Directories the op created are first-class rows** (`entry_type = dir`), not just files (Finding 2): a copy that built
  `a/b/c/` records `a`, `a/b`, `a/b/c` as `dir` rows with `seq` after the files beneath them, so a `seq DESC` rollback
  removes files before the dirs that held them and the created tree is fully reversed. The created-dirs list already
  exists in the transient `CopyTransaction`; journaling makes it durable.
- **D-granularity — per-kind row granularity, chosen to give leaf-level search AND a natural rollback unit.** The
  pipeline's per-op granularity varies by kind and strategy (verified): copy iterates per-leaf
  (`transfer/copy/mod.rs:302,352`), delete per-leaf (`delete/walker.rs:124`), cross-FS move stages per-leaf via
  `copy_single_item`, but **same-FS move renames per TOP-LEVEL item** (`transfer/move_op.rs:162`, one rename moves a
  whole subtree) and **trash trashes per TOP-LEVEL item** (`delete/trash.rs:109`, one OS move). A uniform "one row per
  op-step" would then make "when did I trash `dog.jpg`?" match nothing when `dog.jpg` sat inside a trashed folder — only
  the top-level `photos` row exists. That breaks the product's search headline. So granularity is explicit per kind:
  - **copy, delete, cross-FS move**: **leaf `rollback_unit` rows** — the op already walks per-leaf, so `record_item`
    at the existing leaf points is free, and every row is both searchable and the rollback unit.
  - **same-FS move, trash**: the op's natural rollback unit is the **top-level** item (one rename-back / one restore
    from trash reverses the whole subtree), so the top-level entries are `rollback_unit` rows. But leaf search still
    needs the descendants, so the subtree's leaves are additionally recorded as **`search_only` rows** (their dest
    paths derived from the top-level dest + relative path), sourced from the **drive index** where the subtree is
    indexed — zero extra filesystem I/O, since the tree is already in the index (requirement 8 preserved; trash must not
    grow a scan it doesn't do). Rollback streams only `row_role = rollback_unit` rows; search queries all rows.
    - **`search_coverage = full` requires the index to be both PRESENT and CURRENT, not merely present** (Finding 1a).
      The drive index is eventually consistent (network watchers are lossy under load, volumes load `Stale` on launch,
      the verifier debounces ~30 s), so a just-downloaded-then-immediately-trashed file could enumerate a stale leaf set.
      To keep "never a silent gap" an honest promise (the future agent will trust these flags when reasoning about
      history), gate `full` on **the subtree's listing epoch being current AND the volume's index phase being `Live`**
      (`aggregator/readers.rs::get_listed_epochs_for_ids` gives the per-directory `listed_epoch` + rolled-up
      `min_subtree_epoch`); downgrade to `top_level_only` when the subtree is absent, stale, or the volume isn't `Live`.
      Preference order recorded for the implementer: **do the epoch+phase gate**; only if the epoch read proves genuinely
      expensive at implementation time, fall back to the weakened wording ("`full` = complete as of the last index scan")
      with that rationale noted — never leave `full` meaning "present but possibly stale" silently.
    - **Enumerate BEFORE the mutation, PERSIST only AFTER the item succeeds** (Finding 1b + the round-5 partial-failure
      finding). Two distinct steps that must NOT collapse into one:
      1. **Enumerate** the `search_only` leaf set (and its epoch check) into memory *before* the OS mutation fires — the
         index reconciler prunes the subtree the moment it sees the FSEvent from the trash/rename, so a lazy or
         finalize-time read would find the rows already gone and wrongly stamp `full` over a miss.
      2. **Persist** those buffered leaves only *after* the top-level item's mutation actually succeeds — at the same
         `trash.rs:109/124` / `move_op.rs:162` point where that item's `rollback_unit` row is recorded. Trash and same-FS
         move process **per top-level item with partial failure** (`trash.rs:109` continues past a failed item; a
         mid-batch cancel stops the rest), so persisting at enumeration time would leave `search_only` leaves for a
         subtree that was never trashed — and since the D8 search query has no per-item outcome filter, "when did I trash
         `dog.jpg`" would return a trash that never happened (the same silent-lie class the coverage-honesty work closed).
         Gate persistence on the item's success; drive it from the buffered set. (Equivalent alternative if an
         implementer prefers: persist eagerly but tag each `search_only` leaf with its parent item's outcome and have
         search exclude non-`done` rows — pick one; the success-gated persist is the recommended default.)
      This is a hard ordering-and-outcome constraint, restated in D4 and `DETAILS.md`.
    - **Bounded leaf enumeration — a per-operation cap (David's performance concern).** Because the enumeration is now
      synchronous before the mutation, a same-FS move or trash of a 1M-file folder would pay a 1M-row index read before a
      sub-second APFS rename — disproportionate. So cap the `search_only` leaves per operation at a named tunable constant,
      **initial value 50,000** (a guess, to be benchmark-tuned — David's framing: cheap to change later, but benchmark from
      day one). The enumeration query runs `LIMIT cap + 1`, so the synchronous cost is bounded by construction. **Over the
      cap ⇒ record the top-level `rollback_unit` row only and set `search_coverage = top_level_only` with a `capped`
      reason** (a **typed `search_coverage_reason` enum**, D2 — the reasons must be distinguishable: `capped` vs
      `index_absent` / `index_stale` / `volume_not_live` / `search_row_incomplete`, so the future agent can tell "too big
      to index in the journal" from "index wasn't current"). **Rollback is unaffected by the cap** — the top-level
      `rollback_unit` row is the undo unit for these kinds regardless, so a capped op still fully reverses.
- **Kind taxonomy — extensible (requirement 2, archive extensibility).** Store `kind` as a compact enum mirroring
  `WriteOperationType` (`copy | move | delete | trash | rename | create_folder | create_file | archive_edit`), plus an
  `archive_edit` **subkind** column (`compress | edit | extract-later`) so compress, zip-inner edits, and future
  archive-extract slot in without a schema change. A new op kind is an additive enum value, not a migration. Design the
  read/rollback code to `match` exhaustively on the typed kind (`no-string-matching` rule — never branch on a display
  string).
  - **The `archive_edit` subkind is supplied by the CAPTURING DRIVER, not derived from `WriteOperationType` (Finding 3).**
    Compress and zip-inner edits both cross IPC as `ArchiveEdit` — the identity is frontend-only, so at
    `start_write_operation`/`open_operation` the backend can't tell them apart, yet their rollback eligibility differs
    (compress → rollbackable iff net-new and unchanged; zip edit → not_rollbackable in v1). Resolution: the specific
    driver that already knows (the compress path at `compress.rs:176-183`, or the zip-edit driver) passes the subkind —
    and, for compress, the net-new flag — to `finalize_operation`, where eligibility is computed with the subkind in
    hand. `open_operation` at `start_write_operation` stands; the subkind and net-new flag arrive later, at capture/
    finalize. This keeps the generic open point generic and the archive-specific knowledge where it lives.
- **App-side case folding, not a collation.** importance/index use the `platform_case` collation for correct
  case-insensitive UNIQUE, which forced the `index-query` tool because raw `sqlite3` can't read those columns. The action
  log wants `sqlite3`-inspectability (D1), so it stores a precomputed `name_folded` (Unicode-lowercased, NFC) column with
  a plain b-tree index and queries on it. Divergence recorded: folding is done in Rust once at insert, may differ
  slightly from a filesystem's exact case rules, which is acceptable for a *record* (the journal is not a live filesystem
  mirror; it's history), and it keeps the DB openable in any SQLite browser.
- **The migration ladder (the novel part).** `meta.schema_version` drives an ordered list of forward migration steps
  (each an idempotent transaction: `ALTER TABLE`/backfill/index). On open: if the stored version < current, run the
  ladder step by step inside transactions, bumping version stepwise; if > current (a downgrade), refuse and log (don't
  destroy a newer DB). Delete-and-recreate stays only for a genuinely unparseable file. This is the first such ladder in
  the codebase — document the pattern in `DETAILS.md` as the template future durable DBs follow.

**D3 — Two-axis status + provenance, not one enum (requirement 4).** David's suggested list (agent-suggested,
accepted/rejected, queued, in progress, done, canceled, rolled back) conflates three independent things. Split them:

- **`execution_status`**: `queued | running | done | failed | canceled`. The op's lifecycle, mirrored from the manager's
  `LifecycleStatus`.
- **`rollback_state`**: `not_rollbackable(reason) | rollbackable | rolling_back | rolled_back | partially_rolled_back`.
  Whether and how the op can be / has been reversed. `rolling_back` is the **transient in-flight state** (D7, Finding 7):
  set on the original op the moment its inverse operation starts, resolved when the inverse finalizes (→ `rolled_back` /
  `partially_rolled_back`, or back to `rollbackable` if the inverse was fully canceled). It guards against double-rollback
  races (a second rollback request on a `rolling_back` op is refused with a typed reason) and against retention pruning a
  rollback's own source rows mid-stream (D9 skips `rolling_back` ops). All states cross IPC/MCP as typed enums, never
  strings (`no-string-matching`).
- **`initiator`**: `user | ai_client | agent` (provenance, D5).

**agent-suggested / accepted / rejected belong to the FUTURE proposals table, not this journal.** A proposal is a
pre-operation object (agent-spec §8); a *rejected* proposal never becomes an operation, so it never appears here. When
the agent lands, `proposal_ops` references `operations.op_id` for the ops that were accepted and executed. Making this
boundary explicit (here and in the agent-spec reconciliation) prevents a future effort from smearing proposal states
into the execution journal.

**Rollback eligibility is computed at op time AND rechecked at rollback time.** At finalize (M2), the capture layer
computes and stores `rollback_state` + `not_rollbackable_reason` from what actually happened:

- `copy`: `rollbackable` iff no item `overwrote` (deleting the copies is safe); any overwrite ⇒ `not_rollbackable`
  (reason: overwrote existing files — the originals are gone). Matches David's "copy and move only if no overwrites."
- `move`: `rollbackable` iff no overwrites (move back is safe); overwrite ⇒ `not_rollbackable`.
- `trash`: `rollbackable` (restore from trash) iff the in-trash location was recorded and the item is still there
  (rechecked at rollback).
- `rename`: `rollbackable` iff the renamed item is unchanged (rechecked).
- `create_folder`: eligible iff net-new; at rollback, remove the folder **only if it is still empty** (recheck) — a file
  added to it since ⇒ skip, `partially_rolled_back`.
- `create_file`: eligible iff net-new; at rollback, remove the file **only if unchanged since the creation snapshot**
  (size/mtime recheck) ⇒ skip on drift.
- `archive_edit` subkind `compress`: eligible iff the archive was net-new (an overwrite of a prior archive is
  `not_rollbackable` — the prior bytes aren't retained). **At rollback, delete the archive only if it is unchanged since
  the creation snapshot** (size/mtime recheck) — NOT merely "net-new" (Finding 5): the user may have zip-edited the
  archive afterward, and deleting it then would be an undo that destroys their additions. Drift ⇒ skip,
  `partially_rolled_back`. This is the same "recheck the thing you're about to delete" discipline as `create_file` /
  `create_folder`. Zip-inner `edit`: deferred, `not_rollbackable` in v1 with reason "zip editing rollback not yet
  supported" (designed to become rollbackable later, result need not be byte-identical — requirement 3).
- `delete`: **never** `rollbackable` (a permanent delete can't be restored) — reason recorded.

At rollback time (M3), the stored eligibility is a fast gate, then per-item preconditions are rechecked against the
snapshots (drift → skip that item). **Cross-volume eligibility is computed at rollback time from mount state**: if the
op's source/dest volume isn't currently connected, rollback is unavailable with a clear reason ("Volume 'Backup' is not
connected"), not a stored-forever `not_rollbackable`.

**D4 — Capture rides the operation pipeline through a dedicated per-item observer at the sink's altitude, NOT by
decorating `OperationEventSink`.** The sink is UI-event-shaped and lacks per-item dest/size/mtime/outcome (see Current
state). Options weighed: (a) extend `OperationEventSink` with journal methods — rejected, it muddies "sink = UI events"
and every implementor (`TauriEventSink`, `CollectorEventSink`, tests) grows journal noise; (b) a fully separate observer
threaded independently — rejected, duplicates the sink's deep plumbing. **Chosen:** a sibling `OperationJournal` trait
(`record_item(JournalItem { source, dest, size, mtime, outcome, overwrote })`, `open_operation(...)`,
`finalize_operation(...)`) bundled **with** the sink into one `OperationObservers` context that is injected at the same
IPC edge and threaded down the same seam the sink already uses. Rationale: reuses the existing plumbing (one context
object instead of two threaded params), keeps journaling separable and independently testable (a test can install a
capturing `OperationJournal` with no sink), and keeps the sink focused. The per-kind record points, at the exact places
the op already stats each item (D-granularity fixes the unit per kind):
- **copy** — per-leaf at `transfer/copy/mod.rs:302,352` / `single_item.rs:263/308`, **plus the created-directory rows**
  from `CopyTransaction`'s created-dirs list (Finding 2), each with `seq` after its contents.
- **delete** — per-leaf at `delete/walker.rs:124` (Finding 4). A 1M-file delete produces ~1M leaf rows, deliberately:
  "when did I delete `dog.jpg`" is David's literal benchmark, and the retention budget (D9/D10), not a row cap, manages
  the cost — an order-of-tens-to-~150 MB journal for a 1M-file delete, reclaimed on prune. Document that tradeoff in
  `DETAILS.md`; do not cap.
- **trash** — the top-level `rollback_unit` row at `delete/trash.rs:109/124` (capturing `resultingItemURL`), plus the
  subtree's `search_only` leaf rows enumerated from the drive index **before `move_to_trash_sync` fires** but **persisted
  only after that top-level item succeeds** (Finding 1b + round-5: enumerate-before dodges the reconciler prune;
  persist-after-success avoids recording leaves for an item that failed or was canceled), with `search_coverage` gated on
  the subtree's epoch being current AND the volume `Live` (Finding 1a; `top_level_only` otherwise).
- **move** — per-leaf for cross-FS (stages via `copy_single_item`); the top-level `rollback_unit` row for same-FS
  (`move_op.rs:162`) plus drive-index-sourced `search_only` leaves, enumerated **before the rename** and persisted
  **after the item succeeds**, epoch/phase-gated the same way.
All at near-zero marginal cost (no new syscalls beyond the free drive-index reads — requirement 8). The `open`/`finalize`
calls bracket the op at `start_write_operation` and the `write-settled` guard.

- **`run_instant` ops (rename/mkdir/mkfile)** don't flow through the sink and emit no write events — hook them at
  `manager.rs::run_instant` (or the `create.rs`/`rename.rs` call sites), recording a single-item operation. Their result
  is the command return, so capture wraps that.
- **Bypasses** (Current state): `paste_clipboard` creates a file (a mutation) — **route it through the managed create
  path** (the write-ops DETAILS already suggests this) so it journals for free. **David approved this: it is a firm M2
  work item, not optional.** The single `move_to_trash_sync` in `rename.rs` journals as a one-item trash op. Native
  drag-out is explicitly **out of scope** (the destination is another app, outside Cmdr; there's nothing to roll back to)
  — record the boundary.
- **The observer must never fail the operation, AND a lossy journal must never falsely report an op as rollbackable.**
  Two independent guarantees:
  - **Send discipline: lossless with backpressure, matching importance's actual behavior.** Note the precedent
    correctly: `importance/writer.rs` sends on a bounded `mpsc::sync_channel(1024)` via `SyncSender::send`, whose std
    semantics **block when full** and only `Err` on receiver disconnect — importance applies backpressure, it does NOT
    drop on a full channel (an earlier draft of this plan miscited it as drop-on-full). The operation log does the same:
    `record_item` blocks briefly if the writer is behind, so no item silently vanishes. This is safe for requirement 8
    because a batched row insert is far cheaper than the per-item file I/O the op is already doing (copying bytes,
    trashing, renaming), so the writer outpaces every real op and the channel effectively never fills during one — the
    block is a theoretical backstop, not a hot-path cost. **The one way the writer could stall is retention/vacuum on
    the same thread**, so D9's `incremental_vacuum` runs in bounded slices *between* insert batches (never one long
    stop-the-world vacuum), keeping the hot path drained. A DB *error* (not fullness) still logs a warning and drops
    that row — the operation never fails for a journal problem — which is exactly why the completeness guarantee below
    is mandatory.
  - **Completeness accounting at finalize (mandatory, independent of send discipline).** A dropped or errored row is
    invisible to rollback: the drift/partial machinery only sees rows that exist (drift = a row whose file changed); a
    *missing* row is simply never considered, so a move rollback could silently leave the moved file at the destination
    while reporting success — a data-loss-shaped bug. Defense: the completeness check compares **the number of
    `record_item` calls the op ACTUALLY ISSUED (items reached) against the rows durably written** — NOT against the
    planned `item_count` (Finding 1). This decoupling is load-bearing: a canceled or failed op reached fewer items than
    planned, and those reached items are exactly what a rollback needs (the rock-solid goal), so it must stay rollbackable
    for them. A successful op issues `item_count` calls; a canceled op issues however many it reached. Only a genuine
    drop/DB-error *between issued and written* — a real journal hole — triggers `journal_incomplete`. (Do not let the
    column name `item_count`, the planned total, mislead the implementer into comparing against it.)
  - **Completeness is scoped per `row_role` (Finding 2).** The two row populations of a top-level move/trash op
    (`rollback_unit` vs `search_only`) share one op, so `finalize_operation` returns **per-`row_role` issued-vs-written
    counts**, routed to different consequences: a **`rollback_unit` shortfall** ⇒ `not_rollbackable(journal_incomplete)`
    (rollback correctness); a **`search_only` shortfall** ⇒ downgrade that op's `search_coverage` to `top_level_only`
    (reason `search_row_incomplete`, distinct from the `capped` / stale / absent reasons)
    (search honesty). Without this split, a dropped search leaf would either wrongly kill a perfectly-journaled trash op's
    rollback (over-coupling) or leave `search_coverage = full` silently lying (the round-3 honesty promise the future
    agent trusts). With it, `full` means "index current AND every leaf row persisted," and rollbackability depends only
    on the rollback rows. Data safety of the *operation* still outranks completeness of the *journal* — but an incomplete
    journal degrades to "can't safely undo this" / "search is marked partial," never to a silent under-reverse or a false
    coverage claim.

**D5 — Provenance plumbing: introduce an `initiator` field threaded from the initiation site into the write-op IPC.**
Today nothing tells the backend whether a copy came from the user or an MCP client (Current state). Introduce it:

- Add an optional `initiator` param to the write-op start commands (`copy_files_start`, `move_files_start`,
  `delete_files_start`, `trash_files_start`, `compress_files`, rename/mkdir/mkfile), defaulting to `user`.
- The MCP FE listeners (`mcp-listeners.ts`) that dispatch `fileCopyCommand`/`fileDeleteCommand`/etc. in response to
  `mcp-copy`/`mcp-delete`/... set `initiator: 'ai_client'`, mirroring the existing `NavigateSource: 'mcp'` pattern.
- The descriptor carries `initiator` into the journal at capture.

**David confirmed full `initiator` threading through every write-start command in v1** — it's bounded and clean (an
optional param + one dispatch-site tag), so it's the plan of record, not a recommendation to weigh. (If a specific
command's threading turns out surprisingly costly at implementation time, the interim is to tag only ops invoked directly
by an MCP tool handler and record the gap — but the target is full coverage.) `agent` is reserved (no agent yet); the
future agent's ops (proposal applies) get tagged `agent` when that lands.

**D6 — Mutations only; navigation/intent is out of scope.** The journal records file mutations, never navigation or
intent signals. Those stay with importance's `record_visit` (counts + recency) and the agent's future intent stream
(agent-spec §6.1). Stated as a hard boundary (and in the agent-spec reconciliation) so the two never blur into parallel
recorders. Rationale: navigation is high-frequency, low-value-per-event, and already has a home; mixing it in would
bloat the durable journal and muddy "what did I *do* to my files."

**D7 — Rollback = inverse operations through the same managed pipeline, streamed, precondition-rechecked, partial, and
NON-DESTRUCTIVE.** A rollback is not a bespoke engine — it constructs a new operation (itself journaled, `rolls_back_op_id`
set) and pushes it through `OperationManager` so it inherits preflight, progress, cancellation, and the safe-overwrite
temp+rename machinery. Design constraints (data-safety-critical, TDD-mandatory — M3):

- **The inverse op runs with a pinned non-destructive conflict policy: never `Overwrite`.** This is the subtle
  data-loss trap: the snapshot recheck (below) verifies the *item being restored*, but NOT that the *restore target is
  clear*. If the user created a new file at the original source path after a move, a move rollback moving dest→source
  would, under `Overwrite`, destroy that new file — data loss *caused by an undo*, the worst possible outcome; under
  `Stop` it would abort the whole rollback; under a silent `Skip` it would leave the item unmoved with no signal. So pin
  the inverse op's `ConflictResolution` to **`Skip`** (never `Overwrite`, never `Stop`): a restore-path collision
  records that item as **skipped, feeding `partially_rolled_back`**, with a clear reason ("couldn't restore X — the
  original location is occupied"). Same rule for trash restore and rename-back collisions. An undo must be strictly
  additive-or-nothing per item.
- **Stream the item list from the journal**; never materialize 1M paths in memory. A paged cursor over `operation_items`
  filtered to `row_role = rollback_unit` (D-granularity — `search_only` leaves under a top-level move/trash unit are for
  search, not for reversal) ordered by `seq DESC` (reverse, so a rollback undoes in inverse order — and, per Finding 2,
  removes created files before the `entry_type = dir` rows that contained them, matching `CopyTransaction::rollback`).
- **Transactional isolation from retention (Finding 6):** the paged cursor spans successive short-lived read connections,
  NOT one WAL snapshot, so the writer-thread `Prune` could delete rows between pages and silently under-restore. The fix
  is NOT one long read transaction (a 1M-item rollback would block WAL checkpointing for the whole file-I/O duration);
  it's the `rolling_back` state (below) — retention skips any op in `rolling_back`, so the rows a live rollback is
  streaming can't be pruned out from under it.
- **Re-verify each item's precondition against its recorded snapshot** (size/mtime/existence) at execution time. Drift
  (the file changed since the op) ⇒ skip that item, record the skip, continue — **partial rollback with per-item
  outcomes**, mirroring the agent-spec proposal partial-apply philosophy. Never operate on a changed file. The recheck
  covers the *source* item; the pinned `Skip` policy (above) covers the *restore target* — both are needed.
- **Unverifiable precondition ⇒ drift ⇒ skip (Finding 2 — snapshots aren't portable).** The snapshot fields degrade
  across backends: `mtime` is nullable and second-granularity, often absent or coarse on MTP/SMB; inodes don't exist on
  MTP at all and aren't carried by the `Volume` trait's `SourceItemInfo`/`FileEntry`. So the recheck runs on whatever the
  backend can prove, and **any field it cannot verify — stored or live `mtime` is `None`, or an inode is needed but
  unavailable — counts as drift**: skip the item with a typed reason (`unverifiable_precondition`), feeding
  `partially_rolled_back`. Never proceed on an unprovable precondition — this is the data-safety engine, so it fails
  safe, not optimistic. Where a strong identity check IS available (a `LocalPosixVolume` with real inodes), use it as an
  optimization; the trait-level fallback is the path-plus-`mtime` rule with the skip-on-unverifiable default.
- **Per-kind inverse**:
  - copy → managed delete of the copied dest items (only those with `outcome = done`), files first then created dirs
    once empty (the `entry_type = dir` rows, `seq DESC`). Deleting 1M copies is a plain managed delete, ≈ the cost of any
    delete of those files (requirement 8).
  - move → managed move dest → source (only if the op was `rollbackable`, i.e. no overwrites); top-level unit for
    same-FS, per-leaf for cross-FS.
  - trash → managed move from the recorded in-trash location back to the original path. Needs the `resultingItemURL`
    capture (M2). Failure modes to handle: trash emptied (item gone → skip, report), item moved within trash (recorded
    location stale → skip), same-name collision at the restore path (→ skip under the pinned non-destructive policy,
    report; never overwrite).
  - rename → managed rename back, if unchanged. **Case-only self-collision guard (Finding 8):** on a case-insensitive
    volume, restoring `dog.jpg` → `dog.JPG` sees the target path "exists" because it IS the same inode; a naive
    collision check would skip and the rollback would silently no-op. So a collision whose target is the same entry as the
    item being restored is not a real collision (a case-only or identity rename) — proceed. Where real inodes exist
    (`LocalPosixVolume`), compare inodes; on the trait level (MTP has no inode), fall back to comparing the
    case-normalized paths — a target that differs from the source only by case is the self-collision, and a same-name
    sibling on MTP is caught by the same-path check. This is a `LocalPosixVolume`-only inode optimization over a
    trait-safe fallback (Finding 2).
  - create_folder → remove the folder, only if still empty (recheck).
  - create_file → remove the file, only if unchanged since the creation snapshot (recheck).
  - archive_edit/compress → managed delete of the created archive, **only if unchanged since the creation snapshot**
    (size/mtime recheck, Finding 5 — net-new is necessary but not sufficient; a later zip-edit must not be destroyed);
    drift ⇒ skip.
  - delete → refused (not rollbackable).
- **In-flight state + double-rollback guard + crash reconcile (Finding 7):** set the original op to `rolling_back` before
  the inverse operation starts; resolve it at the inverse's finalize (→ `rolled_back` / `partially_rolled_back`, or back
  to `rollbackable` if the inverse was fully canceled with nothing reversed). A rollback request on an op already
  `rolling_back` is **refused with a typed reason** (`AlreadyRollingBack`, an enum across IPC — `no-string-matching`),
  preventing concurrent double-reversal.
  - **Synchronous inverse-spawn failure must not wedge the op (Finding 3).** If the inverse never starts — the volume
    drops between the connectivity gate and `spawn_managed`, or lane admission errors synchronously — the op would be
    stuck `rolling_back` with no inverse row, and the `AlreadyRollingBack` guard would then refuse every retry until
    restart. So on a synchronous spawn failure, **reset `rolling_back → rollbackable` in the same call, before returning
    the typed error**, so the user can immediately retry. Set `rolling_back` as late as possible (right at a successful
    spawn) to shrink this window, but the reset is the guarantee.
  - **Crash mid-rollback, two sub-cases, both explicit.** On restart, an op left `rolling_back` resolves deterministically
    via a **startup reconcile** (beside the migration-ladder open path): (i) if an inverse op row exists but is
    unfinalized (crashed mid-stream — `not_rollbackable` by the completeness rule), reconcile the original from that
    inverse's recorded per-item outcomes (→ `rollbackable` if nothing was durably reversed, else `partially_rolled_back`);
    (ii) if **no inverse op row exists** (crashed after setting `rolling_back` but before/at spawn — the Finding-3 path
    that didn't reset), reconcile straight back to `rollbackable` (nothing was reversed). Either way a re-issued rollback
    safely resumes, because every per-item inverse is an idempotent recheck-then-act. This is a designed mechanism, not
    luck.
- **Cross-volume gate** at execution: if a required volume is disconnected, fail fast with a clear reason before touching
  anything.

**D8 — Search: an indexed folded-name column now, FTS5 deferred.** "When did I delete dog.jpg" = an exact/prefix name
lookup, not full-text. Weighed: FTS5 over item names — deferred; it's heavier for 1M-row ops and exact-name lookup
doesn't need tokenization. **Chosen:** the indexed `source_name_folded` / `dest_name_folded` columns (D2). The benchmark
query is `SELECT ... FROM operation_items JOIN operations USING(op_id) WHERE source_name_folded = 'dog.jpg' AND kind IN
('delete','trash') ORDER BY ended_at DESC` — index-served. **Search spans all rows regardless of `row_role`** (both
`rollback_unit` and the `search_only` leaves that D-granularity records under top-level move/trash units), so "when did I
trash `dog.jpg`" hits even when `dog.jpg` sat inside a trashed folder — the exact asymmetry a uniform-granularity design
would have left as a silent miss. The one uncovered case (a trashed subtree that wasn't indexed) is flagged on the op as
`search_coverage = top_level_only`, so it's a queryable known-gap, not a wrong answer. Item names repeat massively across
a 1M-file op; a b-tree index handles duplicate keys fine, and (unlike directories) names must stay queryable so they are
NOT interned. FTS5 is a clean later add (a virtual table over names) if substring/fuzzy search is ever wanted — note it
as the extension path.

**D9 — Retention: prune whole oldest operations by age and size, GC orphaned dirs, then incremental-vacuum.** Enforce on
startup + on a periodic timer. Prune **whole operations** (never orphan items from a kept operation, never leave a
dangling `rolls_back_op_id`): a rolled-back pair (original + its inverse) prunes together, or the surviving side's
dangling link is nulled. **Never prune an op in `rolling_back`, nor the op it is reversing** (Finding 6/7): a live
rollback streams its source op's rows across successive read connections, so pruning them mid-stream would silently
under-restore; skipping `rolling_back` ops (and their `rolls_back_op_id` target) closes that race without a long read
transaction. Age limit and size limit are settings (M6); the size limit prunes oldest-first until under budget. **Then GC interned `dirs`**: because item rows are the only referrers and interning keeps a dir row forever (D2),
pruning operations alone leaves a monotonically-growing `dirs` floor the size limit can never reclaim. So after pruning
items, delete `dirs` rows that are no longer live. **A dir is live iff an item references it (via `source_dir_id` or
`dest_dir_id`) OR it is an ancestor of a live dir** — a referenced dir's whole parent chain must survive, since path
reconstruction walks `parent_dir_id` to the root; deleting a bare "no item references it" set would orphan those chains.
So GC deletes the complement of the referenced-dirs-plus-their-ancestors closure (equivalently, iterate leaf-up: delete
dirs with no item ref AND no child-dir ref, repeat until stable), on the writer thread alongside the vacuum. After the
prune + GC, run the tiered `incremental_vacuum`
(borrow `indexing/writer/maintenance.rs`, in bounded slices per D4 so it never starves capture) so freed pages actually
return to the OS — importance never calls it, and this DB must not grow unboundedly. All pruning is a writer-thread
message (one writer).

## Architecture

```
write_operations/ (existing pipeline)              operation_log/ (new durable subsystem)
  manager.rs (spawn_managed / run_instant)           store/    operation-log.db (durable, TM-backed, migration ladder)
  start_write_operation ── injects ──►                 dirs (interned) + operations + operation_items
    OperationObservers { sink, journal }               app-side name_folded (no collation → sqlite3-inspectable)
      OperationEventSink (UI events, existing)        writer.rs  one dedicated thread, bounded mpsc, batched inserts,
      OperationJournal  (NEW per-item observer) ──►      lossless+backpressure, request/reply + barrier + retention
        record_item at existing stat points          capture: OperationJournal impl → open/record_item/finalize
        open/finalize at lifecycle bounds              computes + stores two-axis status + rollback eligibility
  run_instant (rename/mkdir/mkfile) ── hook ──►      rollback engine: builds inverse op, streams items seq DESC,
  compress: capture net-new-vs-overwrite               rechecks snapshots (drift→skip=partial), pushes back
  trash: capture resultingItemURL (NEW)                through OperationManager (journaled, rolls_back_op_id)
                                                     query API: name-indexed search + paged operation/detail reads
  MCP registry (mcp_tools!) ── new tools ──►         retention: prune whole ops by age+size, GC dirs, incremental_vacuum
    operations_list / operations_get        dev bin: operation-log-dump (crates/index-query)
    operations_rollback (IfAutoConfirm)
                                                     FE: Debug panel (dev) + "Operation log" menu → soft dialog (ALPHA),
  settings registry ── retention settings ──►          last 50 + load-50-more, i18n × 10 locales, initiator provenance
```

## Milestones

Each milestone is independently shippable and leaves the tree green. Sequential is the default. Run `pnpm check --fast`
while iterating, full `pnpm check` per milestone, `--include-slow` before wrapping a milestone that touches the DB or
rollback. Smoke-test 1–2 tests before a full run after touching test infra (`test-infra-smoke-first`). A formatter hook
reflows `docs/specs/*.md` on open — keep unrelated reflow churn out of commits.

### M1 — Durable DB foundation: schema, migration ladder, writer, connection factory, dev bin

The durable store with nothing hooked into it yet — so the novel parts (migration ladder, interning) are proven in
isolation before capture or rollback depend on them.

- New `src-tauri/src/operation_log/store/`: the connection factory (WAL + incremental auto-vacuum + busy_timeout + NORMAL
  sync, **no `platform_case` collation**), the DDL for `meta`/`dirs`/`operations`/`operation_items` (D2), the dir
  interning helper (`intern_dir(volume_id, path) -> dir_id`), and the app-side `fold_name` function.
- New `src-tauri/src/operation_log/writer.rs`: one dedicated OS thread, bounded `mpsc::sync_channel` (lossless with
  backpressure — `SyncSender::send` blocks when full, like importance; see D4), message enum (`OpenOperation`,
  `RecordItems(batch)`, `FinalizeOperation` — which returns **per-`row_role` durable-row counts** for the completeness
  check (D4), and carries the archive subkind + net-new flag from the capturing driver (Finding 3) — `Prune{...}`,
  `Flush(reply)`, `Shutdown`), per-message transaction; a DB error logs and drops that row (never fails
  the op), but fullness blocks rather than drops. Batched inserts (coalesce many `record_item`s into one transaction).
  Retention `Prune`/vacuum runs in bounded slices between insert batches so it never starves the hot path (D9). A single
  `OperationLogWriter` handle in managed state (no per-volume registry — divergence from importance).
- **The migration ladder** (D2): an ordered `MIGRATIONS` list, `open()` runs steps forward inside transactions, refuses
  downgrades, delete-and-recreate only on an unparseable file. This is the reusable pattern.
- Dev bin `crates/index-query/src/bin/operation-log-dump.rs`: open `operation-log.db` read-only, print recent operations +
  items (calls library read functions, never reimplements).
- **Docs:** new `operation_log/CLAUDE.md` + `DETAILS.md` (sibling, enforced), an `operation_log/` row in
  `docs/architecture.md`, the migration-ladder pattern documented in `DETAILS.md` as the template for future durable DBs,
  the D1/D2 decisions with rationale, and the **agent-spec reconciliation edits** (§4.1/D3, §4.2 `user_action_log`
  split) made in `agent-spec.md` with a pointer back here. Note the DB's sensitivity for `docs/security.md`. Link this
  plan from `docs/specs/index.md` under "In progress".
- **Tests (TDD red→green — the migration ladder is the risky, first-of-its-kind logic):** fail-first then implement — a
  v1→v2 forward migration preserves rows and bumps version; a downgrade is refused; an unparseable file recreates; dir
  interning dedups (`intern_dir` twice on one path returns the same `dir_id`; siblings differ); `fold_name` folds case
  and normalizes; a full open→write→read round-trip of one operation + items. All over a temp-dir DB, no FFI.
- **Checks:** `pnpm check --fast` iterating; full `pnpm check` at end (clippy, rust tests, `claude-md-details-sibling`,
  `docs-reachable`, `docs-dead-links`, file-length). `--include-slow` before wrapping.

### M2 — Capture at the chokepoint: per-item observer, all op kinds, provenance, eligibility

Make every managed mutation journal itself, with per-item rows, two-axis status, and provenance — without measurably
slowing operations.

- New `src-tauri/src/operation_log/capture.rs`: the `OperationJournal` trait + its production impl (feeds the writer) and a
  test capturing impl. Bundle it with the sink into an `OperationObservers` context injected at the IPC edge and threaded
  down (D4). Add `record_item` calls at the existing per-item stat points (`single_item.rs:263/308`, `trash.rs:124`, the
  move loops); `open`/`finalize` at `start_write_operation` and the `write-settled` guard.
- **`run_instant` capture** (rename/mkdir/mkfile) as single-item operations; **compress** captures the `archive_edit`
  subkind + net-new-vs-overwrite flag at seed time (compress.rs:176-183) and passes them to `finalize_operation` (Finding
  3 — the subkind isn't derivable from `WriteOperationType`); **trash** captures `resultingItemURL` (new `Some(&mut url)`
  out-param at
  trash.rs:45). **Bypasses**: route `paste_clipboard` through the managed create path (firm work item — David approved)
  so it journals; journal the `rename.rs` single-trash; native drag-out out of scope (recorded).
- **Provenance** (D5): the `initiator` param threads through **every** write-op start command (David confirmed full
  threading — default `user`); `mcp-listeners.ts` sets `ai_client`; carried into the journal.
- **Two-axis status + rollback eligibility** (D3) computed and stored at finalize from what actually happened (overwrite
  flags, per-item outcomes, `kind`, and — for `archive_edit` — the **subkind + net-new flag the driver supplies**, Finding
  3), including the **per-`row_role` journal-completeness check** (D4): compare the **`record_item` calls the op actually
  issued** (items reached, NOT the planned `item_count`) against the durably-written rows, split by `row_role` — a
  `rollback_unit` shortfall ⇒ `not_rollbackable(journal_incomplete)`, a `search_only` shortfall ⇒ downgrade
  `search_coverage` to `top_level_only`. Failed and canceled ops journal too and **stay rollbackable for the items they
  reached** (issued == written for those) — what completed before a cancel is exactly what a rollback needs (lead
  direction 11; the rock-solid principle).
- **Docs:** `operation_log/DETAILS.md` capture section (the observer seam, the per-kind record points and granularity, the
  bypass boundary, and the **row-volume tradeoff** — a 1M-file delete journals ~1M leaf rows on the order of
  tens-to-~150 MB, deliberately, because leaf search is the requirement and retention (D9/D10) manages the cost, not a
  row cap); a `write_operations/CLAUDE.md` guardrail line ("mutations journal via `OperationJournal`; new managed ops get
  per-item `record_item` at their stat point") only if omitting it could silently drop a mutation from the journal; the
  provenance mechanism documented once (single-source) and pointed to.
- **Tests:** _smoke first_ (one captured copy round-trips to the DB). _TDD red→green for the data-shape correctness:_ a
  grouped multi-item copy produces one `operations` row + N leaf `operation_items` in `seq` order **plus `entry_type =
  dir` rows for every directory the copy created, sequenced after their contents** (Finding 2); an overwrite sets
  `overwrote` and flips `rollback_state` to `not_rollbackable`; a canceled op journals `execution_status = canceled` with
  the completed items marked `done` and the rest absent/`skipped`; a delete stores `not_rollbackable`; **a trashed folder
  records the top-level `rollback_unit` row AND `search_only` leaf rows enumerated from the drive index before the OS
  move, and falls back to `search_coverage = top_level_only` when the subtree is absent, STALE, or the volume isn't
  `Live`** (Finding 1a/1b — assert the stale-subtree case downgrades rather than stamping `full` over a stale leaf set);
  **a trash op whose one top-level item FAILS records no `search_only` rows for that item's subtree** (round-5:
  persist-after-success, so search can't return a trash that never happened), while a sibling item that succeeded keeps
  its leaves; `initiator` threads through (user vs ai_client); **the completeness check** — an op whose journal dropped a
  `rollback_unit` row (inject a DB error on one `record_item`) finalizes `not_rollbackable(journal_incomplete)`, never
  `rollbackable` (data-safety invariant, red→green); **a canceled op stays rollbackable for what it reached** — 300
  items issued, 300 written, 1,000 planned ⇒ `rollbackable`, NOT `journal_incomplete` (Finding 1, guards the
  issued-vs-planned confusion, red→green); **a dropped `search_only` leaf downgrades coverage, not rollback** — inject a
  DB error on one search leaf of a trash op and assert `search_coverage` becomes `top_level_only` while `rollback_state`
  stays `rollbackable` (Finding 2); **the archive subkind reaches finalize from the driver** — a compress op finalizes
  with `subkind = compress` + net-new flag and its computed eligibility, proving the subkind came from the driver not
  from `WriteOperationType` (Finding 3); **over-cap enumeration downgrades coverage with the `capped` reason** — a
  same-FS move/trash whose subtree exceeds the leaf cap records the top-level `rollback_unit` row only, sets
  `search_coverage = top_level_only` with reason `capped` (distinct from stale/absent), and **still rolls back fully**
  (the cap doesn't touch the undo unit).
  _After:_ a **performance test** asserting capture adds no measurable time — and specifically that requirement 8 holds
  **under backpressure**: run a large synthetic op against (a) no journal, (b) a keeping-up journal, and (c) a
  deliberately-stalled/slow writer with a concurrent retention vacuum on the writer thread; assert op throughput in all
  three stays within budget (the stalled case is what a naive keeping-up-writer test would miss). An integration test
  over **each op kind — copy, delete (per-leaf, at `delete/walker.rs:124`), move (same-FS top-level + cross-FS
  per-leaf), trash, rename, mkdir, mkfile, compress** — asserting the journaled shape and granularity per D-granularity.
- **Benchmark (REQUIRED — David explicitly asked, not optional):** because the leaf enumeration is synchronous before a
  sub-second move/trash, measure it from day one and tune the cap from real numbers: (a) enumeration latency vs subtree
  size at 1k / 10k / 100k / 1M entries; (b) op-latency delta for a same-FS move with journaling on vs off (target ~zero);
  (c) background persist throughput. Record results in `docs/notes/` (per the evidence-anchor rule) and set the leaf-cap
  constant from them. Note David's framing in the plan/notes: the cap is cheap to change later, but benchmark it now.
- **Checks:** full `pnpm check --include-slow` (capture touches the live pipeline).

### M3 — Rollback engine (TDD-mandatory, data-safety-critical)

Reverse operations through the managed pipeline, streamed, precondition-rechecked, partial, cross-volume-gated. This is
the milestone where TDD is non-negotiable.

- New `src-tauri/src/operation_log/rollback.rs`: `rollback_operation(op_id)` — gate on stored `rollback_state` (refuse if
  already `rolling_back`, with the typed `AlreadyRollingBack` reason), gate on volume connectivity, **set the op to
  `rolling_back` as late as possible (right at a successful spawn)**, then build the inverse operation per kind (D7),
  stream `operation_items` filtered to `rollback_unit` `seq DESC` via a paged cursor (never materialize the list),
  re-verify each snapshot (drift or **unverifiable field** ⇒ skip + record, D7), and push the inverse through
  `OperationManager` with the pinned non-destructive `Skip` policy. The inverse op is itself journaled with
  `rolls_back_op_id` set; the original resolves out of `rolling_back` at the inverse's finalize. **On a synchronous
  spawn failure, reset `rolling_back → rollbackable` before returning the typed error** (Finding 3), so a retry isn't
  wedged. Trash restore handles the emptied / moved-within-trash / same-name-collision failure modes; rename handles the
  case-only self-collision via the local-inode-else-path rule (D7).
- **Startup reconcile** (Finding 7 + 3): on open, any op left `rolling_back` resolves deterministically — from its
  unfinalized inverse op's recorded per-item outcomes if one exists (→ `rollbackable` if nothing durably reversed, else
  `partially_rolled_back`), or **straight back to `rollbackable` when no inverse op row exists** (crashed after setting
  `rolling_back` but before spawn) — so a re-issued rollback safely resumes via the idempotent recheck. Lives beside the
  migration ladder's open path (M1) but is a rollback concern, specced here.
- Per-item outcomes flow back into the journal: the original op's items get `outcome = rolled_back` where the inverse
  succeeded; skipped items are reported. The result is `rolled_back` or `partially_rolled_back` on the original.
- **Docs:** `operation_log/DETAILS.md` rollback section (the per-kind inverse table, the streaming + recheck contract, the
  `rolling_back` state machine + startup reconcile, the retention race it closes, the partial-rollback semantics, the
  trash + case-only-rename failure modes); the future-Cmd+Z note (D-undo below) as a designed-for extension.
- **Tests (TDD red→green, MANDATORY — mirror the importance eval-style hard-constraint pattern):** an
  `operation_log/rollback_tests` (or `evals/`) suite where a scenario is an initial `InMemoryVolume` state + a mutation
  sequence, and the **hard invariant `apply-then-rollback == original state`** is a `#[test]` for each rollbackable kind
  (copy, move, trash, rename, create_folder, create_file, compress). Plus the specific traps this review surfaced:
  - **copy with created directories rolls back to bit-identical original** — a copy that built `a/b/c/` leaves NO empty
    dirs behind after rollback (the `entry_type = dir` rows are removed after their contents; round-2 Finding 2).
  - **a new file at the restore path is NEVER overwritten by the undo** — for move, trash, and rename rollbacks, occupy
    the original path with a new file and assert the rollback skips that item (`partially_rolled_back`) and leaves the new
    file byte-for-byte intact (the pinned-`Skip` data-loss guard).
  - **compress rollback never deletes a modified archive** — compress, then zip-edit the archive, then roll back; assert
    the archive is untouched and the item is skipped (`partially_rolled_back`), because the recheck sees drift (Finding 5).
  - **case-only rename rolls back on a case-insensitive volume** — on a case-insensitive fixture, `dog.JPG` → `dog.jpg`
    then rollback succeeds (the same-inode target is not treated as a blocking collision; Finding 8).
  - **double-rollback is refused** — a second `rollback_operation` on an op already `rolling_back` returns the typed
    `AlreadyRollingBack` reason (Finding 7).
  - **retention can't prune a rollback's source mid-stream** — with an op in `rolling_back`, a concurrent retention pass
    leaves it (and its `rolls_back_op_id` target) intact; the streamed rollback restores every item (Finding 6).
  - **crash-mid-rollback reconcile, both sub-cases** — (i) simulate an unfinalized inverse op; on reopen the original
    resolves from its recorded outcomes; (ii) simulate `rolling_back` with **no inverse op row** (Finding 3); on reopen
    the original resolves straight to `rollbackable`. Both then let a re-issued rollback finish idempotently (Finding 7).
  - **synchronous inverse-spawn failure doesn't wedge the op** — force `spawn_managed` to fail synchronously (volume
    dropped after the connectivity gate); assert the op is reset to `rollbackable` (not stuck `rolling_back`) and an
    immediate retry is accepted (Finding 3).
  - **unverifiable precondition ⇒ skip, never proceed** — on an `InMemoryVolume` configured to return `modified: None`
    (standing in for MTP/SMB), a rollback item whose mtime can't be verified is skipped with the typed
    `unverifiable_precondition` reason and the op lands `partially_rolled_back` — the null-snapshot data-safety rule
    (Finding 2), red→green without hardware. Include an MTP-shaped case (no inode) exercising the trait-level path
    fallback for the rename self-collision guard.
  Plus: a mid-op cancel then rollback restores exactly the completed items; **drift on one item skips only that item**;
  a `delete` refuses rollback; a cross-volume rollback with the dest volume "disconnected" fails fast with the typed
  reason; a move that overwrote refuses; streaming a large synthetic op doesn't materialize the full list (assert bounded
  memory / paged-cursor use). Real red→green: see each test fail for the right reason before implementing. Re-run these
  yourself, don't integrate on trust (`verify-delegated-work`).
- **Checks:** full `pnpm check --include-slow`.

### M4 — Search + query read API + retention + incremental vacuum

The consumable read side and the disk discipline.

- New `src-tauri/src/operation_log/query.rs`: a read API over short-lived read-only connections — `search_operations`
  (filters: time range, name substring/exact on the folded columns, kind, initiator, status; paged), `get_operation`
  (header + paged items), `recent_operations(limit, offset)` (the alpha UI's "last 50 + load 50 more"). The benchmark
  query (D8) is index-served.
- **Retention** (D9): a writer `Prune` message enforcing the age limit and size limit (prune whole oldest operations,
  **skip any op in `rolling_back` and its `rolls_back_op_id` target** so a live rollback's source can't be pruned
  mid-stream — Finding 6, keep rollback pairs consistent, null dangling `rolls_back_op_id`), then **GC orphaned `dirs`**
  (the referenced-plus-ancestors closure, D9), then the tiered `incremental_vacuum` in bounded slices. Run on startup and
  on a periodic timer. Read the limits from settings (M6 wires the UI; M4 reads the values with the D10 defaults so
  retention works before the UI lands).
- **Docs:** `operation_log/DETAILS.md` query + retention sections; the search index-design rationale (D8) and the
  repeated-names note; the retention pruning-whole-operations invariant.
- **Tests:** _TDD red→green:_ the benchmark "when did I delete dog.jpg" query returns the right op fast; **"when did I
  trash dog.jpg" finds it even when `dog.jpg` sat inside a trashed folder** (the `search_only` leaf, Finding 1), and an
  op flagged `search_coverage = top_level_only` surfaces as a known gap rather than a false negative; filters compose
  (kind + initiator + time range); paging is stable; retention prunes oldest whole operations and never orphans an item
  or leaves a dangling `rolls_back_op_id`; a rolled-back pair prunes together; **the `dirs` GC reclaims a directory once
  its last item is pruned but keeps a dir whose ancestor chain still has live descendants** (the closure correctness
  case); the size-limit prune brings the DB under budget and `incremental_vacuum` actually shrinks the file. _After:_ a
  retention integration test over a populated DB with mixed ages/sizes.
- **Checks:** full `pnpm check --include-slow`.

### M5 — MCP tools

Let an agent test the whole feature end to end without the FE (requirement 7).

- Add to `mcp/tool_registry.rs` (`mcp_tools!`) + `mcp/executor/`: `operations_list` (filters: time range, name
  substring/FTS, kind, initiator, status; paged — gate `Open`), `operations_get` (paged items — gate `Open`),
  `operations_rollback` (destructive — gate `IfAutoConfirm`, opening a confirmation dialog by default for parity,
  like copy/move/delete; `autoConfirm: true` bypasses with the bearer token). All three tools share the `operations_`
  grouping prefix (David's terminology decision — one term, "operation"; confirm it reads well against the shipped
  registry names and switch to `operation_log_*` if that fits better). Names snake_case, params camelCase (`operationId`, `limit`,
  `offset`, `nameContains`, `kind`, `initiator`, `since`, `until`). Update `EXPECTED_TOOL_NAMES`, the count assertion, and
  the gate-table tests (they fail otherwise — by design).
- The handlers read through the M4 query API and call the M3 rollback engine (via the FE round-trip / ack pattern for the
  destructive one, matching how `copy`/`delete` dispatch). "Get journal entries for the last operation" =
  `operations_list(limit=1)` + `operations_get`.
- **Terminal-result observation (Finding 4):** `operations_rollback` returns after dispatch, NOT after the
  reversal finishes (the inverse is an async managed op). So a testing agent closes the loop by **polling
  `operations_get` until the original op's `rollback_state` leaves `rolling_back`** (settling to `rolled_back` /
  `partially_rolled_back`), optionally gated on the existing MCP `await` tool for the ack. Document this "dispatch then
  poll to terminal" contract in `mcp/DETAILS.md` so "an agent can test end to end" is actually closed, not just started.
- **Docs:** `mcp/DETAILS.md` + `mcp/executor/CLAUDE.md` note the new tools, the `operations_rollback` gate
  rationale, and the dispatch-then-poll observation contract; update the tool-count prose; `docs/guides/mcp-development.md`
  if the pattern needs a pointer.
- **Tests:** _TDD red→green:_ the gate-table + count + snapshot tests updated (schema wire bytes pinned); handler unit
  tests over pure decision cores (filter parsing, the empty-result and not-rollbackable messages). _After:_ if feasible,
  an E2E MCP drive (`scripts/mcp-call.sh` / Playwright) doing copy → `operations_list` → `operations_rollback`
  → **poll `operations_get` until `rollback_state` settles out of `rolling_back`** → verify the files are reversed; name that poll
  loop in the test so the end-to-end observation path is exercised, not assumed.
- **Checks:** full `pnpm check` (the MCP snapshot + gate tests); `--include-slow` if the E2E runs here.

### M6 — Retention settings + Debug window panel

- **Settings**: an age-limit setting (`type: 'duration'`, `component: 'select'`, presets 30d/90d/1y + a "Forever"
  sentinel + custom, default forever) and a size-limit setting (numeric bytes, `component: 'select'`, presets
  100MB/250MB/1GB/2GB/3GB/5GB + custom, default 3 GB), modeled on `network.shareCacheDuration`. A new "Operation log"
  settings section (or under an existing group), registered in `SettingsContent.svelte` + `SettingsSidebar.svelte` and
  mirrored in `test/e2e-playwright/settings.spec.ts`. Backend reads them for retention (M4) via a command + applier case.
- **Debug panel** (requirement 6a): `DebugOperationLogPanel.svelte` + a `SECTIONS` entry, fetching recent operations via a
  typed `commands.*` wrapper (a thin `getRecentOperationLogEntries` IPC over the M4 query API).
- **Docs:** `docs/guides/adding-a-new-setting.md` is the reference (no new doc needed); note the retention settings in
  `operation_log/DETAILS.md` as the source of the limits M4 reads.
- **Tests:** settings-parity + i18n-parity tests (the new keys); a Vitest test for the preset/custom control; the E2E
  settings spec updated. Debug panel is dev-only (light testing).
- **Checks:** full `pnpm check`; `i18n:check-parity` / `settings-i18n-parity`.

### M7 — "Operation log" alpha UI (menu + soft dialog + ALPHA badge + i18n)

The thin alpha surface (requirement 6b) — debugging/demo quality, but i18n'd and style-guide compliant.

- **Menu + command** (the What's-new template, App-scoped): a `log.operationLog` command — `COMMAND_IDS` id,
  `command-registry.ts` entry, handler in `routes/(main)/command-handlers/app-dialog-handlers.ts` (`openOperationLog()`),
  `mod.rs` id constant + both mappings + `MenuItem::with_id` in `macos.rs` AND `linux.rs`, `menuCommands` in
  `shortcuts-store.ts`. **Placement: the View menu** (David's call).
- **Keyboard shortcut** (David's call): a **configurable** shortcut for opening the dialog, **default ⌥⌘O**
  (Option+Command+O), registered through the app's existing shortcut system (the `command-registry.ts` `shortcuts` field
  + `shortcuts-store.ts`, so it shows up wherever shortcuts are configured today and can be rebound). **The implementer
  must first check the shortcut registry for a ⌥⌘O collision and flag it** if taken (pick an alternative and note it)
  rather than silently double-binding. The shortcut is an M7 work item and gets its own E2E (open the dialog via the
  shortcut, not only the menu).
- **Soft dialog** (the What's-new analog): `src/lib/operation-log/operation-log-trigger.svelte.ts` holding `$state({ open,
  entries, offset, hasMore })` + `openOperationLog()` (fetches page 1 via the M4 query IPC) + `loadMore()` (appends 50);
  `OperationLogDialog.svelte` with `dialogId="operation-log"` (registered in `dialog-registry.ts`), rendering the grouped
  operations (a mass rename shown as one collapsible group — requirement 2), each with initiator, kind, timestamp, and
  status; mounted in `routes/(main)/+page.svelte`. **The per-op summary label is formatted client-side from the typed row
  fields** (`kind` + `item_count` + volume names) via `$lib/intl` with an ICU plural key, so it localizes per viewer
  (Finding 3) — the backend never ships a rendered English string for the dialog. An **ALPHA badge** via `StatusBadge` +
  an `operation-log` entry in `feature-status.json` (`"status": "alpha"`). Copy is style-guide compliant (active voice,
  sentence case, no "error"/"failed", friendly) and fully i18n'd.
- **i18n**: keys in `messages/en/` (a new `operationLog.json` area for the dialog + `commands.json` for the menu/palette
  entry + `settings.json` already done in M6), each with an `@key` description, then propagated to all 10 locales through
  the translation pipeline (`docs/guides/i18n-translation.md`).
- **Docs:** `operation_log/DETAILS.md` UI section (the alpha surface scope, the convergence-with-agent-log note per
  D7/naming); update `docs/architecture.md` if a FE module row is warranted.
- **Tests:** a Vitest/component test that the dialog renders grouped entries and paging appends; an E2E (Playwright)
  opening the dialog **both via the View menu item and via the ⌥⌘O shortcut** and asserting the dialog + ALPHA badge; a
  test/flag for the shortcut-collision check (⌥⌘O not already bound); i18n parity for the new keys.
- **Checks:** full `pnpm check --include-slow`; `i18n:check-parity`, `i18n:check-stale`, `i18n:check-icu`.

## Cross-cutting

- **Design for future undo (D-undo).** A later Cmd+Z is "roll back the last rollbackable user-initiated operation" =
  `SELECT op_id FROM operations WHERE initiator='user' AND rollback_state='rollbackable' ORDER BY ended_at DESC LIMIT 1`,
  then `rollback_operation`. The two-axis status + `rolls_back_op_id` linkage make this a query, not a new engine. Don't
  build it; don't preclude it. (A rollback is itself a user operation, so "undo the undo" — redo — also falls out, though
  it's not in scope.)
- **The journal never compromises the operation, and never lies about rollbackability or search coverage.** Capture
  sends onto a bounded channel that blocks briefly if the writer is behind (lossless — never silently drops on fullness)
  and a DB error logs + drops that one row without failing the file op; the finalize-time completeness check (comparing
  items *issued* vs *written*, per `row_role`) then downgrades a missing `rollback_unit` row to
  `not_rollbackable(journal_incomplete)` and a missing `search_only` row to `search_coverage = top_level_only`. So the
  *operation's* data safety outranks the *journal's* completeness, but an incomplete journal degrades to "can't undo" /
  "search marked partial," never to a silent under-reverse or a false coverage claim (D4, `AGENTS.md` principle 4).
- **No string-matching for classification** (`no-string-matching`): `kind`, `initiator`, `execution_status`,
  `rollback_state`, `not_rollbackable_reason`, and per-item `outcome` cross every boundary (IPC, MCP, DB) as typed enums,
  never message substrings. The name search is an equality/prefix match on a folded column, not a substring of a
  user-facing string.
- **Resources + the memory watchdog.** Rollback of a huge op streams the item list (paged cursor, D7) so it never holds
  1M rows; capture batches inserts. If a recompute/prune ever holds meaningful memory, hook cancellation into the
  existing indexing watchdog rather than a second ceiling (as the importance plan did).
- **Cancellation + crash-safety.** A rollback is a managed op, so it's cancelable like any transfer (cancel keeps what
  was restored, records the rest as skipped). A crash mid-capture loses at most the un-flushed tail; the op it belonged
  to never reaches finalize, so it lands `execution_status = running` with no completeness stamp and is treated as
  not-rollbackable on the next open (a record, not a ledger the op depends on). A crash mid-rollback is handled by the
  `rolling_back` startup reconcile (D7, Finding 7): the original op resolves out of `rolling_back` from its inverse's
  recorded per-item outcomes, and a re-issued rollback finishes idempotently (each per-item inverse rechecks its snapshot
  and skips what's already reversed).
- **Single-source docs.** The migration-ladder pattern, the capture seam, the rollback contract, and the search
  index-design each get ONE canonical home in `operation_log/DETAILS.md`; the agent spec and any future consumer point here
  rather than restating (`docs.md` single-source rule). `docs/architecture.md` gets a map row (what + where + pointer),
  never the mechanism.
- **Dependencies.** No new crate anticipated (`rusqlite` vendored, `mpsc` std, folding via `unicode-normalization` if not
  already present — check `cargo tree` first; if a new crate is needed, `cargo deny check` + a verified ≥3-day-old version
  per the project `dependencies` rule).
- **Evidence-anchor volatile claims.** The trash `resultingItemURL` behavior, `NSFileManager` restore semantics, and the
  `incremental_vacuum` reclaim behavior each carry a `(verified on <version>, <method>, <date>)` note in `DETAILS.md`
  when measured (`docs.md` evidence rule).

## Parallelization notes (only the extremely safe ones)

Default is sequential. Two safe parallel splits, if speed is ever wanted:

- **M6 and M7 can overlap** once M4's query API exists: settings/Debug panel (M6) and the alpha dialog (M7) touch
  disjoint FE files, and both only read the query API. The i18n keys are additive per area.
- **The M1 dev bin** and the M1 tests are independent of each other.

Everything else (M1→M2→M3→M4, and M5 depending on M3+M4) is a real dependency chain; run it in order.

## Product calls — all resolved by David

Every product-level call this plan raised is now decided; recorded here so the implementer treats them as fixed:

- **Naming:** unify on **"operation"** everywhere (Naming section) — module `operation_log`, DB `operation-log.db`, UI
  "Operation log". "Action" stays reserved for the agent's `user_action_log`; the code-level `write_operations` /
  `WriteOperationType` keep their `write_` prefix (the two-namespace rule).
- **Retention defaults:** forever / 3 GB (requirement 10), confirmed even with Time Machine backup on (D1 — the churn
  cost is accepted; a future "exclude from backups" toggle is the named lever if it ever bites).
- **`paste_clipboard` routing:** approved — a firm M2 work item (route through the managed create path so it journals).
- **Provenance completeness:** full `initiator` threading through every write-start command, confirmed (D5).
- **Menu placement + shortcut:** the Operation log item lives in the **View menu**, with a configurable **⌥⌘O** default
  shortcut (M7; implementer checks for a ⌥⌘O collision first).
- **Leaf-enumeration performance:** capped (initial 50,000, benchmark-tuned) with a required M2 benchmark (D-granularity,
  M2). Cheap to change later; measured from day one.
