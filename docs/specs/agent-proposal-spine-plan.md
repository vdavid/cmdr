# The durable proposal spine

Milestone 1 of `docs/specs/later/ai/agent-spec.md` §17 (the rewritten build order). Read the spec's §0 status map and §8
before this plan; this document is the implementation plan, the spec is the intent.

## What this delivers, and why it goes first

Today the agent's one proposing feature (`propose_rename_plan`) stages into `RenameProposalStore`, a `HashMap` behind a
`Mutex` with a 15-minute TTL. It works, it is carefully built, and it evaporates on quit. Every later milestone in §17
needs the opposite: a proposal that survives a restart, carries per-op statuses, detects drift between creation and
apply, expires on its own terms, and can be partially applied.

So this milestone builds that store and moves the shipped feature onto it. **No new LLM behavior, no new tool, no new
proposal kind.**

The reason it goes before the deterministic detector (§17 milestone 2) is sequencing, not enthusiasm: the detector's
value is measured by acceptance rate, and acceptance is only meaningful if a proposal can wait for the user. A detector
built on a 15-minute in-memory TTL would measure the TTL, not the suggestion.

**One user-visible change, and it is not the obvious one.** M3a makes the operation log record skipped and failed rename
rows, which it currently drops on the floor. After it, a rename operation that showed 11 items in the operation log
dialog shows 14, three of them labeled "Skipped" (the `operationLog.outcome.skipped` key already exists and is
translated). That is a human-facing surface, so it wants David's eye even though it needs no new copy. The _proposal
durability_ itself ships with no surface at all; see the next section.

## Decide this before starting: the restart question

Everything below assumes an answer, and the answer changes M1's schema and M3b's scope. **David's call, needed before
M1.**

A durable proposal store means a rename batch survives a restart. But nothing would show it: the review dialog opens
only from a live `proposalReady` stream event (`src/lib/ask-cmdr/ask-cmdr-stream.svelte.ts:105`), and proposals are
deliberately not persisted in chat history (`agent/tools/propose/DETAILS.md` § The proposal store). Re-entry needs an
IPC to list pending batches, a bindings change, a UI affordance, and a decision about where it lives.

The options:

- **(a) Durable store, no re-entry surface** (recommended). The batch survives, a fresh preflight is mandatory after a
  restart, and the surface that lists pending proposals arrives with the detector (§17 milestone 2), which needs a
  pending-proposals list anyway. This milestone stays backend-only and small.
- **(b) Durable store plus re-entry now.** Adds the IPC, the bindings, and the UI to M3b, plus the product question of
  whether a rename dialog reappearing after a crash reads as a save or as clutter.
- **(c) Rename keeps a short expiry and durability only ever pays off for the detector.** Cheapest, but then M3b's port
  is pure refactoring with no payoff until milestone 2, which weakens the case for doing it now at all.

The plan below is written for **(a)**.

**Consequence of (a), stated so the next agent doesn't read it as dead code:** under (a) nothing lists pending batches,
so nothing ever reads an `interrupted` or `expired` batch. Those states and their sweeps are still right to build now
(the DDL and the transitions are the expensive things to change later), and they get their first reader one milestone
on. This is the same "built now, wired later" bargain M2's comparators take, and it deserves the same explicit note.

## What the fit-check found (spec §20.4)

The spec asks whether `OperationManager` accepts a batch of ops as one unit with per-op statuses and partial apply, and
says to file any gap as a small extension.

**What already exists.** `file_system/write_operations/rename/bulk.rs` ships
`start_bulk_rename(sink, volume_id, rows, initiator)`, where each `BulkRenameRow` carries
`{ row_id, source, destination, expected_fingerprint }`. It does per-row fingerprint verification against a
server-captured `BulkRenameFingerprint` (`bulk.rs:318`, re-checked per step), per-row skip on mismatch rather than
all-or-nothing, dependency-ordered execution with same-directory temporaries for cycles and case-only renames,
journaling by `op_id`, `Initiator::Agent` / `AgentEdited` provenance, and rollback through the operation log. Batch
entry points also exist for move and trash: `move_files_start(events, sources: Vec<PathBuf>, destination, …)`
(`mod.rs:485`) and `trash_files_start(events, sources: Vec<PathBuf>, …)` (`mod.rs:725`) each take a batch and run as one
managed op.

**What's actually missing, and it isn't batch execution:**

1. **The engine reports no per-op results to its caller.** `start_bulk_rename` returns
   `WriteOperationStartResult { operation_id, operation_type }` immediately; every outcome lives in the spawned task and
   escapes only as an aggregate `WriteCompleteEvent { files_processed, files_skipped }` plus a per-success
   `WriteSourceItemDoneEvent`. Writing `proposal_ops.status` needs a result channel out of a spawned managed op.
2. **Skipped and failed rows are journaled nowhere.** `record_bulk_rename_outcomes` opens with
   `if outcome != BulkRenameOutcome::Done { continue; }` (`bulk.rs:780`). `ItemOutcome::Skipped` exists in
   `operation_log/types.rs` and bulk rename never writes it.
3. **Journaling happens after the whole batch, not per row.** `open_volume_op` writes a header (`bulk.rs:134`), the run
   executes (`:143`), and `record_bulk_rename_outcomes` writes every leaf row in one pass afterwards (`:162`). See the
   pre-existing bug this causes, below.
4. **Per-op status is not one-to-one with rows.** A cycle group aborts as a unit when any member's fingerprint
   mismatches (`bulk.rs:429`, `:524`), and `settle_local_conflicts` deactivates rows in a fixpoint loop.
5. **`journal_op_id` is per-batch, not per-op.** `start_bulk_rename` mints one `operation_id` for the whole batch
   (`bulk.rs:102`) and leaf rows are keyed by source path. There is no per-op journal id and the engine cannot produce
   one, so any per-op join is on `(operation_id, source_path)`, never on a per-op key.
6. **Move and trash have no per-source expected fingerprint and no per-source outcome**, and a move batch shares one
   destination — a proposal batch moving files to N destinations is N ops, not one.

Items 1-4 are M3a's work. Item 5 is not work but a constraint both milestones inherit: M1's schema names the column
per-batch, M3b joins on it. Item 6 belongs to §17 milestone 2, where a detector gives it a real caller; M3b writes down
the contract it will have to meet.

### A pre-existing bug this fit-check turned up, worth fixing regardless

**A crash mid bulk-rename leaves renamed files with no journal rows and no undo.** Because all leaf rows are written
after the run completes (item 3 above), a crash or force-quit mid-batch means the renames happened on disk,
`operation_items` is empty, `finalize_op` never ran, and `restore_move` has nothing to reverse (it requires
`unit.outcome == ItemOutcome::Done`, `operation_log/rollback.rs:643`). The operation log's startup reconciliation covers
`RollingBack` ops only (`operation_log/mod.rs:195`), so a crashed original op stays `Running` with zero item rows
forever.

"Design for the crash" is principle 1, and this is the agent's own write path, the one where a wrong name is most likely
and undo matters most. **M3a fixes it as a side effect** (journaling per row as it lands), which is a good part of why
M3a is worth doing standalone. Flagging it separately so David can decide to pull it forward independently of this plan.

## The central design decision: a creation snapshot exists or it doesn't

Spec §8.2 says each op snapshots `(inode, size, mtime)` at creation, re-verified at apply. The shipped rename flow
instead captures fingerprints at **preflight**, when the user opens the review, binding them to the approved row subset
(`AcceptedPreflight`, carrying `allowed_row_ids` + `allowed_destination_names` + `fingerprints`).

The two answer different questions, and a proposal that can be days old wants both:

- **The creation snapshot answers "has this file changed since the agent looked at it?"** Without it, a batch created
  Monday and applied Friday gets re-fingerprinted at preflight and looks perfectly fresh, though every claim in it
  describes a file that has since changed.
- **The preflight snapshot answers "is this still what I showed the user?"** It closes the window between the dialog
  rendering and the apply click, and binds the approved subset so a stale client can't replay a different one.

**But the creation snapshot cannot be captured at the propose boundary, and this is the plan's sharpest constraint.**
`build_proposal` reads `PaneStateStore`, whose `PaneFileEntry` carries `size: Option<u64>` and
`modified: Option<String>` — a **display string pushed by the frontend** — with no inode and no device. The one live-FS
call in the whole boundary is a single `symlink_metadata` existence check (`rename/plan.rs:275`). That is deliberate:
"Handlers read Rust-side stores, pane caches, and SQLite only — never a live `statfs`/`readdir`, so a dead NAS can't
hang a tool" (`agent/tools/CLAUDE.md`). Statting N sources inside a tool handler would break that guardrail on exactly
the path a hung mount would freeze.

**Decision: `creation_snapshot` is nullable, and non-NULL means server-captured. There is no third state.**

- NULL ⇒ unverifiable. Non-NULL ⇒ verified, **by construction**, because only a producer that legitimately stats can
  write one.
- **`propose_rename_plan` writes NULL, and never grows a stat loop.**
- **A detector-born batch writes a real fingerprint**, because it runs off the indexer and the watcher rather than a
  tool handler, and those already hold real metadata.
- The comparator is therefore `compare_creation_snapshot(Option<&Fingerprint>, current)`, total over two cases.

**Why not a `verified` / `cached` / `none` authority token** (the shape an earlier draft of this plan had): `cached`
would mean scraping `PaneFileEntry.modified`, which the frontend pushes, into a type the drift path reads. The propose
module's whole thesis is that the client supplies opaque ids and never authority. "A weak snapshot can only say
unverifiable" would then be a rule nothing enforces, and AGENTS.md prices exactly that: "A rule is a cost… Prefer making
it unrepresentable in a type." Nullable-plus-nothing makes it a type property instead, and halves the comparator's
domain.

- If the cached size has display value ("4.2 MB when the agent looked"), it goes in `payload`, the frozen-display
  column, explicitly outside the drift path.
- If provenance matters for the audit trail, add a nullable `snapshot_source` **documented as reporting-only, never read
  by control flow**, following the house precedent: `SkipReason`'s doc comment (`operation_log/types.rs:208`) says "This
  is reporting fidelity only: no variant may change whether an item is skipped, retried, or forced."

**Consequence for evidence, which the spec never anticipated.** Row evidence (`RenameEvidence`) is checked once at the
tool boundary against `ImageFactsLedger`, which is in-memory, per-thread, and expires in 30 minutes
(`propose/evidence/mod.rs`). A durable proposal outlives every record of what backed its rows, so an evidence claim can
never be re-checked after the ledger window. **Decision: evidence is display-frozen and explicitly unverifiable past the
ledger window**, and the docs say so. This is not new risk (evidence is not re-checked at apply today either), but
durability makes it visible, and it is another reason to want a real creation snapshot from producers that can manage
one.

## Milestones

Sequential. M3a is independent of M1 and M2 and could run in parallel by a second agent — it touches only
`write_operations` and `operation_log` — but there is no hurry, and running it first means the journal records something
real by the time the store wants to read it.

### M1: The store

Schema and query layer only. No feature touches it yet, which is what makes it testable in isolation.

**Schema** (migration v4 in `agent/store/migrations.rs`; append a `Migration`, never edit a shipped step):

- `proposals`: `batch_id` PK, `created_at`, `kind`, `op_display_name`, `rationale`, `status`, `created_by_model`,
  `conversation_id` (**nullable** — a detector-born batch has no thread, and `ON DELETE SET NULL` on a `NOT NULL` column
  is a runtime FK failure), `expires_at`.
- `proposal_ops`: `op_id` PK, `batch_id` FK, `seq` (stable display order), `op`, `source_volume_id`, `source_rel_path`,
  `dest_volume_id`, `dest_rel_path`, `status`, `creation_snapshot` (nullable), the preflight snapshot (null until
  preflight runs), `payload` (JSON: the frozen display artifacts, including rename evidence and any cached size),
  `executed_at`, `result`, `journal_operation_id` (nullable; per-BATCH, see fit-check item 5).

**Intent behind the shape:**

- **`conversation_id` is `ON DELETE SET NULL`, not `CASCADE`.** Both existing child tables in `main.db` cascade
  (`migrations.rs:157`, `:195`) and copying the neighbours is the obvious move, but spec §8.4 makes these tables the
  **decision record**. Cascading them away with a chat thread would destroy the audit trail for batches already applied
  to the user's real files. There is no conversation-delete path today (only archive), so this is latent — which is
  exactly why it has to be right in the DDL, before one exists.
- **Evidence lives in `payload`, not its own column.** One home for per-kind frozen display data; the store stays
  kind-agnostic.
- **Keys are `(volume_id, rel_path)`, never absolute paths** (D5), reusing the shipped `Location` vocabulary
  (`src/location.rs`) rather than minting a parallel pair type. Today's rename rows store an absolute `source_path` plus
  a `volume_id`; converting at the store boundary is part of this milestone and this is the last cheap moment.
- **Every status is a `token_enum!`**, matching `operation_log::types` and `agent::types`. The hard rule against
  string-matching control flow applies at full force: a lifecycle driven by `status == "accepted"` is precisely the
  silent breakage that rule exists to stop.
- **The status set covers the whole of spec §8.3**: `proposed`, `accepted`, `executing`, `executed`, `failed`,
  `rejected`, `expired`, `invalidated`, plus `interrupted` (below).
- **No custom collation** (D4), so `main.db` stays `sqlite3`-inspectable.

**The claim transaction, specified.** This is the one place where getting it wrong applies a batch to the user's real
files twice, so the plan names the shape rather than the goal. Today atomicity comes free from one `Mutex` spanning
check-and-remove (`store.rs:241`). Under SQLite with WAL and `busy_timeout = 5000` (`store/connection.rs:32`), a
read-then-write in a deferred transaction gives either a lost update or a five-second stall ending in `SQLITE_BUSY` — an
error, not the typed refusal the design wants. So:

- `BEGIN IMMEDIATE`, then a single conditional
  `UPDATE proposals SET status = 'accepted' WHERE batch_id = ? AND status = 'proposed'`,
- the refusal derived from `rows_affected == 0`, returned as a typed variant, never an error,
- rows read inside the same transaction.

Note the shape to preserve from today's code: `accepted_preflight()` drops and re-acquires the lock between `get` and
the read, and `apply_bulk_rename` reads, may re-preflight, then takes — safe today only because the take is the single
consumer. **The port must keep exactly one claim point** and must not split the fresh-preflight path into
read-then-write.

**Crash recovery, which the in-memory store got for free.** Today the take _removes_ the record, so a crash mid-apply
leaves nothing to resurrect. Persisting `accepted` removes that guarantee. So: **on open, any batch past the acceptance
point in a non-terminal status (`accepted` / `executing`) moves to `interrupted`, which means "outcome unknown; a fresh
preflight is the source of truth."**

❌ **Do not reconcile per-op status from the journal.** In exactly the crash `interrupted` exists for, the journal holds
a header and zero item rows (fit-check item 3) while N files on disk are already renamed, so a reconcile would report "0
applied" over a batch that applied seven. The fresh preflight re-stats reality, which is the only honest source. After
M3a lands per-row journaling the journal becomes a real incremental record and a reconcile becomes _possible_ — but it
stays unnecessary, because the preflight already answers the question. Revisit only if a producer appears that can't
preflight.

**The recovery sweep runs exactly once at startup**, in `agent::start`, and must be idempotent. ❌ Not in
`AgentDb::open_write_connection`, which opens a fresh connection and reruns the migration ladder on every call
(`agent/mod.rs:49`) — hooking there turns the sweep into a full-table scan per write.

**Expiry sweep lands here too**, because expiry is lifecycle correctness and the state machine owns it. **Mechanism:
expire on read plus a sweep on open, no timer.** The two precedents disagree — the shipped in-memory store expires on
read plus a sweep on every `stage` (`store.rs:157`, `:173`), while `operation_log::retention::spawn` runs a startup
prune plus a periodic timer (`operation_log/mod.rs:203`) — and expire-on-read preserves today's semantics exactly, adds
no background work for a table with no reader yet, and matches the subscribe-don't-poll principle. Age and row-cap
**pruning** is a different concern with no pressure yet (every proposal today requires a chat turn, and `messages` grows
strictly faster with no retention either), so it defers to §17 milestone 8's general retention work.

**Expiry moves from monotonic to wall clock.** Today's TTL is `Instant`-based (`store.rs:114`), which on macOS does not
advance across system sleep; a persisted `expires_at` is wall-clock like the rest of `main.db`. A laptop asleep for two
days will now expire a proposal that used to survive. Accepted deliberately, recorded so it isn't discovered.

**Who writes.** `agent/mod.rs` states that the chat runtime owns the write-connection lifetime and single-writer
discipline, and `revise_bulk_rename_row` / `cancel_bulk_rename_proposal` are **sync** `#[tauri::command]`s that will now
mutate SQLite. A sync command blocking on WAL contention stalls the whole IPC thread (`commands/CLAUDE.md`). **Decide in
M1**: either those commands become `async` + `blocking_with_timeout`, or proposal writes route through one owned writer.
Either way the "single-writer discipline" sentence in `agent/mod.rs` and `store/DETAILS.md` stops being true and must be
updated.

**Tests** (unit, over a temp DB; match the neighbourhood, which uses `tempfile::tempdir()` in `store/tests.rs`, rather
than the `TestDir` helper used elsewhere; ❌ no hand-rolled poll loops or fixed sleeps, per `docs/testing.md`):

- **Written test-first, real red→green:**
  - Two concurrent claims of the same batch: exactly one gets the rows, the other a typed refusal (not a `SQLITE_BUSY`
    error). This is the replay guard and the reason the milestone exists in this order.
  - Every illegal lifecycle transition is refused by type or typed error (`executed` → `accepted`, `rejected` →
    `accepted`, `invalidated` → `accepted`).
  - Reopening with an `accepted` batch yields `interrupted`, never a live acceptance.
  - A mixed-status batch reports partial apply correctly: 11 applied, 3 skipped, batch status derived from its ops
    rather than set independently.
- Written after: DDL round-trips; `fresh_open_builds_current_schema` extended; migration from a v3 DB preserves
  conversations; the expiry sweep against an injected clock (❌ no `sleep`-based test); **the cascade test issues raw
  SQL** — there is no conversation-delete API (`store/query` exports `archive_conversation` and no delete), and pinning
  the DDL before a delete path exists is the point.

**Docs:** `agent/store/DETAILS.md` takes the DDL rationale, the claim-transaction shape, the crash-recovery rule, and
the nullable-snapshot decision. `agent/store/CLAUDE.md` is **568 words against a 600-word warn threshold**, so budget
for it: at most one tight new must-know line, and trim something existing to pay for it. ❌ Do not add a
`claude-md-length` allowlist entry (that needs David's consent, per `.claude/rules/file-length-allowlist.md`).

**Checks:** `pnpm check rust`, plus `pnpm check oxfmt` for the doc edits.

### M2: Drift comparators

Pure functions, no I/O, so this is the milestone that is almost entirely TDD. **Deliberately not wired into rename.**

- `compare_creation_snapshot(Option<&Fingerprint>, current) -> DriftVerdict` and the preflight equivalent, both total
  over the local and remote fingerprint variants.
- **A missing or weak field reads as "can't tell", never as "unchanged".** A NULL creation snapshot is unverifiable by
  type; remote fingerprints additionally have no inode and carry `Option` size and mtime, so they need the same care at
  the field level.
- Verdicts, and what each would mean once wired: creation mismatch ⇒ `invalidated` (a stale belief the user should
  re-judge); preflight mismatch ⇒ the engine's existing per-row skip (a race, not a stale belief).

**Why the comparators ship unwired.** Rename creates and reviews in one modal flow, seconds apart, and writes a NULL
creation snapshot, so there is nothing to compare. The Monday-to-Friday failure mode needs a producer whose batches
nobody reviews immediately, which is §17 milestone 2. Building the pure comparators now is cheap and testable; wiring an
invalidation path and its review-surface copy for a producer that doesn't exist yet would be the same speculative
generality this plan refuses elsewhere.

**Tests, test-first:** the drift matrix (inode changed, size changed, mtime changed, all unchanged, file gone, NULL
snapshot, remote with missing fields, remote with changed fields), plus one mutation-pinned test: make the comparison
treat `None` as equal and watch "unverifiable ≠ unchanged" fail.

**Docs:** drift semantics go in `agent/store/DETAILS.md` as the canonical home.

### M3a: The engine reports and journals every row's outcome

Standalone value, disjoint from the store, and it fixes the pre-existing crash bug above. Split out from the port
because `write_operations` has the densest rulebook in the repo and is not where a second concern should ride along.

- **Journal each row as it lands**, moving `record_volume_leaf` into the execution loop rather than a single pass after
  the run. This makes the journal a real incremental record, so a crash mid-batch leaves rollback-able rows for the
  renames that happened.
- **Journal the temp hop of a rotation as its own leaf.** "Per row as it lands" is not a complete rule: a cycle rotates
  through a same-directory temporary (`bulk.rs:428`) and a case-only rename does the same two-hop (`bulk.rs:397`), so
  mid-rotation one file's real name exists only at a temp path. The in-process failure path handles that today
  (`restore_local_cycle`, `bulk.rs:467`), but a crash bypasses it. Recording `source → temp` as its own leaf lets
  recovery find the orphan by name, and the rotation's completion updates it. The alternative — journal a rotation
  atomically at its end, accepting that a cycle is all-or-nothing across a crash — is defensible but leaves the orphan
  unfindable, so prefer the first. **Settle this before starting M3a**; it's the one open shape in the milestone.
- **Bring the rotation temp under the existing recovery machinery.** `unique_temporary_path` mints
  `.cmdr-bulk-rename-{row_id}-{uuid}` (`bulk.rs:645`, remote at `:656`) and registers it with nothing but the
  downloads-watcher ignore set (`note_rename_write`). Every recovery and visibility mechanism keys on `.cmdr-tmp-*`: the
  persisted `in_flight_temps` ledger sweeps only what it recorded, `reap_stale_transfer_temps` matches that prefix, and
  `staging.rs` hides Cmdr's own temps by that prefix. So a crash mid-rotation today strands a file whose real name is in
  no ledger, no journal, and no sweep, and which nothing ever cleans up. Fix while already in here: either mint the temp
  with the `.cmdr-tmp-` prefix, or register it with `in_flight_temps`.
- **Write skipped and failed rows too**, with a `SkipReason`, so the journal stops being silent about them.
- **Report every row's outcome to the caller**: a result channel out of a spawned managed op, whether that's a
  completion callback or a sink method. This is new engine code with its own tests.
- **The channel hands outcomes to whoever started the batch; the engine never learns what a proposal is.** The seam
  already exists and spec §8.4 names it: the write pipeline is headless-callable because writes emit through
  `OperationEventSink`, built at the IPC edge and injected in. So the agent-side caller injects the receiver at
  `start_bulk_rename`. ❌ `write_operations` must not reach into `agent/store/` — that's a dependency inversion in the
  module already under `docs/specs/module-cycle-untangling.md`.
- **The outcome-to-status mapping accounts for cycle-group aborts**, where one row's mismatch settles a whole group.

Like M2's comparators, **the result channel ships with no consumer**: M3b is what wires it. The journaling and
temp-prefix fixes stand on their own regardless.

**User-visible consequence, deliberate:** the operation log dialog renders `itemOutcomeLabel(item.outcome)` per item row
and the `operationLog.outcome.skipped` key already exists in all ten locales, so skipped renames start appearing in a
shipped surface (and in the `operations_get` / `operations_list` MCP tools). No new copy needed, but it wants David's
eye per principle 4.

**Also check:** `operation-log.spec.ts` exists as an E2E spec and new item rows may move its assertions. And
`restore_move` returns `Skipped(SkipReason::Failed)` for any item whose outcome isn't `Done`
(`operation_log/rollback.rs:643`), so once `Skipped` rows exist, undo over such a batch attributes "failed" to rows that
were cleanly skipped. Safe, but the reporting is wrong and this milestone is what makes it reachable — fix it here.

**Oracles:** `bulk.rs`'s own tests plus the `operation_log` suites. **Checks:** `pnpm check rust`, plus
`pnpm check desktop-e2e-playwright` for the operation-log spec.

### M3b: Move the rename feature onto the spine

The behavior-preserving milestone. The shipped flow has a 12-item invariants register
(`docs/specs/agent-context-harness-plan.md`) and a careful acceptance-binding design; the job is to keep every one while
changing where the state lives.

- Replace `RenameProposalStore` with the DB-backed store. Keep the authority boundary exactly: the frontend sends opaque
  ids, never paths or names; apply resolves everything server-side from stored rows.
- Keep `AcceptedPreflight`'s two-part binding (`allowed_row_ids` + `allowed_destination_names`). It is not redundant
  with fingerprints, and invariant 10 depends on both halves: a revise clears the acceptance AND the recorded names
  catch a future path that forgets to clear it.
- Keep `revise_row` as the one user-owned mutation, still invalidating the acceptance.
- Persist row evidence into `payload`, with the ledger-window note from above.
- Consume M3a's per-row outcomes to write `proposal_ops.status`, and write `proposals.journal_operation_id` at claim
  time from the `WriteOperationStartResult` that `start_bulk_rename` already returns. M3b owns both writes; the engine
  owns neither.
- **Expiry becomes per-batch, set by the producer at creation**, replacing the global 15-minute constant. That constant
  was never a property of proposals in general, only of this modal flow, and generalizing it in the store would carry
  the wrong assumption into the detector's multi-day batches.

**Tests.** The oracles are the shipped suites, and they must pass unchanged wherever they test behavior rather than
storage internals: `agent/tools/propose/rename/tests.rs` (24 tests, several constructing `RenameProposalStore` directly,
so they need the port), `commands/agent/bulk_rename.rs` tests, and the Vitest trio (`ask-cmdr-rename-review.test.ts`,
`ask-cmdr-rename-undo.test.ts`, `BulkRenameReviewDialog.a11y.test.ts`). **There is no bulk-rename E2E spec** —
`ask-cmdr.spec.ts` mentions neither rename nor proposal — so don't budget a slow Playwright lane for an oracle that
doesn't exist. New tests, written after (this is a port, so coverage earns more than red→green here): a staged proposal
survives a store reopen; an accepted preflight does not; an expired batch is indistinguishable from a missing one to
callers, matching today's `get` semantics.

**Note the expiry-visibility seam:** callers see an expired batch as missing (today's semantics), while `expired` is a
real recorded status for the audit trail and for the detector's future review surface. Two different readers, one state;
write it down so the next agent doesn't "fix" one of them.

**The IPC expectation is that nothing moves.** Under option (a) the surface is unchanged (same
`RenameProposalRowSnapshot`, same `BulkRenamePreflight`), so `bindings-fresh` passing is a real assertion, not a
formality. If M3a's result channel adds an event type that crosses IPC, name it before running the check rather than
discovering it there.

**Docs:** `agent/tools/propose/CLAUDE.md` and `DETAILS.md` stop describing an in-memory store; the mechanism's canonical
home becomes `agent/store/DETAILS.md` and the spec's §8 points at it (single-source rule: spec keeps intent, colocated
doc keeps mechanism). `file_system/write_operations/DETAILS.md` gains a short "how a proposal batch applies" section
plus **what a second op kind owes**: per-source expected fingerprints, per-source outcome reporting, trash-default for
destructive ops (D30), lane reservation, journaling with the real volume id, and the N-destinations caveat for move.

**Checks:** `pnpm check rust bindings-fresh ipc-enum-camelcase svelte-tests`, then plain `pnpm check` at the end of the
effort and `--include-slow` once.

## What this milestone deliberately does not do

Recorded so a future agent doesn't read the omissions as oversights:

- **No `volumes` table**, though D5 says it ships in v1. Nothing in this milestone reads it, and the keying it exists to
  support is delivered by using `Location` in the op rows. It arrives with the first multi-volume producer.
- No `agent_log` table. Proposal transparency rides the existing review surface; the activity log is §17 milestone 3.
- No `notify_user`, no proactivity dial, no auto-apply (§8.5) — all presume a proposal the user did not ask for.
- No second op kind and no move/trash executor: contract written, implementation deferred to a real caller.
- No retention for `conversations` / `messages`: that is a product call about the user's chat history, and folding it
  into an infrastructure change would smuggle it past review.
- No change to what reaches the provider, so `CONSENT_COPY_VERSION` does not move.

## Open questions

1. **The restart question** (above) — blocking, needed before M1.
2. **Should the pre-existing crash damage be pulled forward** ahead of this whole plan? M3a fixes both halves (no
   journal rows for a crashed batch, and a stranded rotation temp no sweep can see), but they are data-safety holes in
   the shipped agent write path today and don't depend on any of this.
3. **How long should a detector-born batch live?** The spec says "days" without a number. It interacts with the
   creation-snapshot check: a longer life means more invalidated ops, which is honest but could read as flakiness.
   Suggest three days, moved by acceptance data.
4. **Where is the batch cap authoritative?** `MAX_RENAMES = 200` is enforced at the tool boundary (`rename/plan.rs:26`).
   Adding a store-level cap creates two enforcement points for one invariant. Suggest the store owns the hard ceiling
   (it protects every future producer) and the tool boundary keeps its own tighter, kind-specific limit, with the
   relationship written down.
5. **Overlapping live batches over the same path.** With multi-day batches, two proposals naming one file becomes
   normal. Data safety holds (the loser's fingerprint check fails and it skips), but nothing invalidates the loser or
   explains the skip. Deferred decision, named here so it isn't a surprise.

## Notes on the checker

`invariant-density` currently warns on `main` from unrelated drift (six subsystems over their allowlist;
`apps/desktop/src-tauri` at 339 against 334). Don't silence it and don't let this effort add to it: prefer a type over a
new ❌ line, per `.claude/rules/file-length-allowlist.md`.
