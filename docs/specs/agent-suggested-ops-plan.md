# Suggested ops

The agent proposes file operations, the user approves them in groups from a dedicated window, and the whole thing ships
as one feature. This absorbs milestone 1 of `docs/specs/later/ai/agent-spec.md` §17 (the durable proposal spine) and
extends it to the full shape: many op kinds, an agentic loop wired to filesystem events and user actions, and a store
the agent can query and amend.

Read the spec's §0 status map and §8 first; that document holds the intent and the decision log, this one holds the
build.

## What ships

One feature, released in one go:

- The agent watches filesystem events and user actions, wakes on something worth reacting to, and proposes operations:
  **move, copy, trash, rename, compress, extract** (permanent delete is deliberately excluded, see §"Reversibility").
- Suggestions arrive **grouped**, so "you have 10 new files in Downloads" becomes "move these 5 to X", "move these 4 to
  Y", "trash this 1", each approved or rejected on its own.
- A **"Suggested ops" indicator** shows when suggestions are waiting, and opens a **Suggested ops window** that is also
  reachable from the menu bar.
- Suggestions live in `main.db`, so they survive a restart, and the agent can **query and re-propose its own pending
  suggestions** without dragging every op through its context window.

**Why one release rather than staged.** Half of this is strange to ship: proposals with no window, or a window with one
op kind. The cost is that acceptance rate, the metric that says whether proactive suggestions are wanted at all, arrives
only at the end. Mitigation is dogfooding plus instrumentation: the analytics events land in M1, so a fortnight of local
use produces real numbers before release.

## Decisions already settled

Carried from the spine plan and its three review rounds. **Don't re-litigate these; the reasoning is load-bearing and
some of it was expensive to find.**

- **A creation snapshot is nullable, and non-NULL means server-captured.** There is no third state and no authority
  token. The propose boundary reads cached pane state and must never stat (a dead NAS would hang the tool), so an
  agent-authored op writes NULL; a detector-born op that ran off the indexer or the watcher writes a real fingerprint.
  NULL can only ever say "unverifiable", never "unchanged", and that is a type property rather than a rule.
- **Approval never survives the restart that killed it.** On open, any group past the approval point in a non-terminal
  status becomes `interrupted`, meaning "outcome unknown, a fresh preflight is the source of truth". ❌ Do not reconcile
  per-op status from the operation log; a fresh preflight re-stats reality, which is the only honest answer.
- **`conversation_id` is `ON DELETE SET NULL`, never `CASCADE`.** These tables are the decision record; cascading them
  away with a chat thread would erase the audit trail for operations already applied to real files.
- **Every status and op kind is a `token_enum!`.** The hard rule against string-matching control flow applies at full
  force to a lifecycle machine.
- **Skips are log entries, not rollback units**, and the engine journals every hop as it lands. Shipped; see
  `write_operations/DETAILS.md` § "Bulk rename's hop log".

## Architecture

### Three levels, and the middle one is one executor call

- **Sweep** (`proposal_sets`): one agent wake's output. "10 new files in Downloads." Display and provenance only.
  Nothing is approved at this level.
- **Group** (`proposals`): the reviewable, approvable, executable unit.
- **Op** (`proposal_ops`): one file.

**A group is exactly one call to one executor.** That is the rule; "one verb, one destination" is a lossy summary of it
and was wrong in three ways. What the engine actually constrains, verb by verb:

| Verb     | Executor                                      | Binds                                                                      |
| -------- | --------------------------------------------- | -------------------------------------------------------------------------- |
| move     | `move_files_start`                            | one source volume, one shared `destination` dir                            |
| copy     | `copy_files_start`                            | one source volume, one shared `destination` dir                            |
| trash    | `trash_files_start`                           | the LOCAL (`root`) volume only, **no destination at all**                  |
| rename   | `start_bulk_rename`                           | one volume, **per-op destinations**, all sources sharing one parent folder |
| compress | `compress_start`                              | one source volume, one target archive path, one parent volume              |
| extract  | `copy_between_volumes` with an archive source | a source `ArchiveVolume` + inner paths, and a dest volume + dest path      |

Consequences the group model has to carry:

- **`source_volume_id` is a group field, not a sweep field.** Every executor binds one source volume; the constraint is
  at group level. A sweep may span volumes, a group may not.
- **Trash groups have no destination, and no volume either.** `trash_files_start` takes raw `PathBuf`s and calls
  `NSFileManager.trashItemAtURL`; there is no Volume-trait route and archive-inner sources are rejected at the command
  boundary. The column is nullable, and "one destination" is not the invariant.
- **Rename is the documented exception**: its ops carry their own destinations and the group binds a shared _parent_.
  `start_bulk_rename` refuses when `row.source.parent() != row.destination.parent()`. Don't try to normalize rename into
  the shared-destination shape; give it its own group kind.
- **Extract is a copy, not an archive edit.** There is no `Extract` in `WriteOperationType`; extract-out runs through
  `copy_between_volumes` with an `ArchiveVolume` source. `ArchiveSubkind::Extract` is an operation-log label only, and
  the archive-edit driver handles compress, copy-into, in-archive delete, and the delete half of move-out, never
  extract. **Two functions share that name**: use the headless `write_operations/transfer/volume/copy.rs`'s (it takes an
  injected `Arc<dyn OperationEventSink>`), ❌ never the `#[tauri::command]` wrapper in
  `commands/file_system/volume_copy.rs`, which builds `TauriEventSink` at the edge and would lose the sink decorator
  M2's per-source outcomes depend on. The resolution work an extract needs is private to that command module today
  (`resolve_source`, the parent-aware resolve that routes an archive-inner batch to its `ArchiveVolume`, and
  `resolve_dest_path`, tilde expansion plus root-anchoring); lifting both is real M2 work, not free.

Getting this wrong means inventing a multi-destination batch executor the write engine neither has nor needs.

### Reversibility is a per-verb fact, and it drives the UI

Fingerprints and the claim transaction are both about applying the _right_ thing. Neither says what happens when
applying the right thing is still unrecoverable. Rename, the only shipped precedent, is cheap to undo, which is exactly
why its design does not cover this.

- **Reversible via `RestoreMove`**: move, trash, rename.
- **Reversible by deleting what was written**: copy, and a compress that created a new archive.
- **NOT reversible**: permanent delete (`inverse_action` returns `None` for `OpKind::Delete`; `check_rollbackable`
  refuses with `NotRollbackableReason::PermanentDelete`), and a compress that **overwrote an existing archive**
  (`compress_start` seeds the target unconditionally and the prior bytes are not retained).

Therefore:

- **Permanent delete is not a proposable verb.** Spec D30 already defaults destructive ops to trash, and an agent
  proposing an irreversible delete is a different trust class from one proposing a reversible move. If it is ever
  wanted, it arrives with its own consent flow. **Scope consequence to state plainly:** delete was the only destructive
  verb with a Volume-trait route, so dropping it leaves agent-proposed cleanup with **no destructive verb on SMB, MTP,
  or inside an archive**. The flagship scenario is local and survives intact; "clean up the NAS", the obvious next
  detector, has nowhere to go until that gap is filled deliberately.
- **A compress group whose target archive already exists must refuse, or be shown as an overwrite in the review row.**
  Refusing is the default.
- **The group model carries a `reversible` fact**, and the window gives an irreversible group a different approval
  affordance from a reversible one.

### Conflict policy: agent-started groups never use Stop mode

`WriteOperationConfig::default()` sets `conflict_resolution: ConflictResolution::Stop`. A Stop-mode collision emits
`WriteConflictEvent`, which only the **main window** hosts (`lib/file-operations/operation-conflict.svelte.ts`, which
also raises that window to front). Approving a group from the Suggested ops window under that default would either
interrupt the user in a different window or wedge with nobody to answer.

**Decision: a group started from the Suggested ops window runs skip-on-collision**, and each collision becomes a
`skipped` op row the window reports afterwards. ❌ Never start an agent-proposed group in Stop mode.

That is one decision across **three different config shapes**, so M2's "never Stop mode" test needs three assertions and
two documented non-applicable verbs: move and copy carry `WriteOperationConfig.conflict_resolution`, extract carries
`VolumeCopyConfig.conflict_resolution`, compress takes a bare positional `conflict: ConflictResolution`, rename has no
conflict parameter at all (collisions are preflight's job), and trash has nothing to collide with.
`ConflictResolution::Skip` exists on all three paths, so the decision is implementable everywhere it applies.

### The destination must already exist

`copy_files_start` and `move_files_start` both call `ensure_destination_dir(&destination)` before validating, on
purpose: "a move into a brand-new folder just works" for a path the user typed. For a path a **model** authored and the
user approved by reading a label, a typo silently `mkdir -p`s a new tree.

**Decision: an agent-proposed move or copy group binds an existing destination and refuses otherwise.** If proposing
into a new folder is ever wanted, the review row has to say "creates a new folder" in those words.

### The agent can re-propose what is pending, and nothing else

The agent needs to add ops, drop ops, and regroup as it learns more, which collides with spec D27's freeze-at-creation.
The resolution: **freeze moves from creation to approval.**

- `pending` groups are mutable by their author, through a re-propose against the sweep id (see the tool surface below).
- The moment a group is approved it freezes and no agent path can touch it.
- `interrupted` counts as frozen: it is past the approval point, so the agent may not amend it. The user's choices are
  to re-approve (which mints a **new** group id and runs a fresh preflight, so the claim is never replayed against the
  old one) or to discard. **The old group's op rows stay put and the new group gets copies with fresh ids**, so the
  decision record stays whole and the analytics count one re-approval rather than two proposals.
- The safety property the original freeze protected is "what the user saw is what runs", and approval-time freezing
  preserves it, because the claim binds the op set (below).

**The review race.** The user may have a group open while the agent re-proposes it. The store applies it and the window
shows a non-destructive "this changed" affordance rather than swapping rows under the cursor. ❌ Never mutate the rows
someone is reading without saying so. **This is a display fix layered on the correctness fix, not a substitute for it.**

### The claim transaction binds the op set, not just the status

The spine plan's SQL guarded `proposals.status` and said nothing about `proposal_ops`. With an agent that can amend
pending rows, a re-propose can land between the preflight that stat'd the ops and the claim, and a status-only guard
still succeeds. The rename precedent does not carry over: `accepted_matches` compares the acceptance against the live
proposal **inside the same `Mutex` guard** as the removal, so its serialization comes from the lock, not the design.

**The acceptance must be a separate server-owned record, or the comparison is a tautology.** If the claim re-reads
`proposal_ops` and compares it against `proposal_ops`, a re-propose moves both sides at once and the check means
nothing. If instead the client supplies the values, that reverses the authority boundary the whole propose module rests
on: fingerprints never leave the process, the frontend hands back opaque ids, and every later step resolves from stored
rows so a client-supplied value is never trusted. Rename escapes this because `AcceptedPreflight` is a **separate
record** (`allowed_row_ids` + `allowed_destination_names` + `fingerprints`) held apart from the rows it describes; the
`Mutex` only made the comparison atomic, the separation is what made it mean something.

So: **preflight writes a server-owned acceptance record into `main.db`** (group id, allowed op ids, and the values those
ops carried when it cleared them). The client presents a group id and op ids, ❌ never values.

**One `BEGIN IMMEDIATE` that does all of it:**

1. read the stored acceptance record for this group,
2. re-read the live `proposal_ops` rows and compare them against that record,
3. flip status conditionally: `UPDATE proposals SET status = 'approved' WHERE group_id = ? AND status = 'pending'`,
4. derive the refusal from `rows_affected == 0` **or** a binding mismatch, as **two distinct typed variants** (stale
   status versus changed contents; the user-facing recovery differs).

`withdraw` takes the same conditional shape. Without it, an agent withdrawing a group the user just approved is
undefined.

### The agent's tool surface, and its access classes

The registry's own definitions govern: `Propose` means "stages a proposal and opens a review surface, **mutates
nothing**", `Write` means "mutates the filesystem OR app state, and when in doubt a tool is `Write`", and `Write` is
never reachable from the agent view (`test_agent_tool_view_never_writes`).

- `list_suggestions(status)` — **`Read`**. Sweeps and groups as summaries: id, title, verb, destination, op count,
  status. Never individual ops.
- `get_suggestion_group(group_id)` — **`Read`**. The ops in one group, paged through `fit_to_result_budget` with `total`
  / `returned` / `truncated`.
- `propose_suggestions(sweep)` — **`Propose`**. Creates a sweep, **and re-proposes an existing one when given its id**:
  amend is folded in here rather than shipping a separate mutating tool. A re-propose is literally staging a proposal
  and opening a review surface, so it fits the `Propose` definition; a standalone `amend_suggestions` that mutates
  durable state would be `Write` under the enum's own tiebreaker and would fail the structural test.

Only `propose_suggestions` joins `EXPECTED_PROPOSE_TOOL_NAMES`, so the hand-authored allowlist keeps meaning one careful
read rather than three ceremonial ones.

**Context sizing, scoped honestly:** a re-propose that adds one op addresses ops by opaque id and costs one id, not 200
rows. A _first_ proposal still makes the model emit every path it wants to act on; only a detector-born sweep avoids
that. The budget claim covers amend, not creation.

**Evidence.** Any `Propose` tool inherits the `ImageFactsLedger` / `EvidenceScope` contract, which exists because 12
real files got fabricated names. A destructive group's rationale is a strictly harder claim than a rename's. Decide
before M3 what evidence a trash group must carry and what refuses a sweep; ❌ do not ship a destructive verb with a
weaker evidence bar than rename has today.

### Per-source outcomes: extend the sink, don't invent a channel

Per-source reporting already half-exists. `OperationEventSink::emit_source_item_done(WriteSourceItemDoneEvent)` ships
and is emitted by delete (`delete/walker.rs`), trash (`delete/trash.rs`), move (`transfer/move_op.rs`), and copy
(`transfer/copy/mod.rs`). The real gap is narrow: **the sink reports "done" only, with no per-source skip or fail.**

So: add an outcome to that event (or a sibling), and have the agent's own **sink decorator** write
`proposal_ops.status`. The injected-sink seam is the one the spec already names, it keeps `write_operations` from ever
reaching into `agent/store/`, and it makes M2 much smaller than a bespoke result channel would.

## Milestones

M1 is the hub. M2 and M3 are parallel once it lands; M4 needs both M1 and M2.

**One serialization note:** M2, M3, and M4 each add IPC commands, and every one must be registered in **both** `ipc.rs`
and `ipc_collectors.rs` before `bindings.ts` regenerates. Three branches regenerating one generated file will conflict,
so land the binding regeneration one branch at a time.

### M1: The store, its lifecycle, and the metric

`proposal_sets` + `proposals` + `proposal_ops` in `main.db` (migration v4) with `source_volume_id`, a nullable
`destination`, per-op destinations for rename, the `reversible` fact, the nullable creation snapshot, and per-group
expiry. The typed lifecycle machine, the binding claim transaction, the same-shaped withdraw, the `interrupted` recovery
sweep, and re-propose with its pending-only guard.

**TTL policy, stated once:** the shipped rename store expires proposals after 15 minutes, and every accessor makes an
expired record indistinguishable from a missing one. Suggested ops need days, so expiry becomes **per-group, set by the
producer**, and the rename port raises its own TTL by roughly two and a half orders of magnitude.
`BulkRenameReviewDialog` assumes a short-lived proposal; check that assumption when M6 lands.

**Analytics land here, not later.** The whole one-release argument rests on acceptance rate, so `analytics::posthog`
gains events for group proposed / approved / rejected, carrying verb and a bucketed op count and ❌ never a path. It
already rides the consent gate and dev suppression.

**Tests, test-first:** two concurrent claims of one group (exactly one wins; the loser gets a typed refusal, not
`SQLITE_BUSY`); a claim whose op set changed since preflight refuses with the _binding-mismatch_ variant, distinctly
from the stale-status variant; a re-propose against an approved or interrupted group is refused; reopening an approved
group yields `interrupted`; a mixed-status group reports partial approval; deleting a conversation nulls the link and
deletes nothing (raw SQL, since no conversation-delete API exists yet).

**Checks:** `pnpm check rust`.

### M2: The executors

Per-source expected fingerprints for move, copy, and trash; a per-source outcome on the sink event; compress wired with
its overwrite refusal; **extract routed through `copy_between_volumes` with an `ArchiveVolume` source**, not the
archive-edit driver; skip-on-collision config for agent-started groups.

**Tests:** per-verb fingerprint mismatch skips that source and reports it; the sink reports skips and failures, not only
successes; an agent-started group never enters Stop mode; a compress onto an existing archive refuses; existing
transfer, delete, and archive suites pass unchanged.

**Checks:** `pnpm check rust`, plus `pnpm check desktop-e2e-playwright` for the operation-log spec.

### M3: The agent's tool surface

Three tools with the access classes above, their schemas, registry entries, `ToolId` variants, rail labels, and the one
`EXPECTED_PROPOSE_TOOL_NAMES` addition. Settle the evidence bar for destructive groups first. Depends on M1.

**Tests:** the structural registry tests; an oversized result pages rather than blowing the turn; a re-propose against a
frozen group is refused at the tool boundary as well as in the store.

### M4: The Suggested ops window and the indicator

Scoped against `docs/guides/adding-a-window.md`, whose checklist this milestone must follow rather than approximate:

- The route (`routes/suggested-ops/+page.svelte`) and an opener modeled on `lib/file-operations/queue/queue-window.ts`
  (vibrancy, reduce-transparency fallback, position resolution, E2E ordering).
- `capabilities/suggested-ops.json`, hand-trimmed the way the queue window's is.
- **The `build.rs` playwright capability line.** A new window label missing from the generated capability under
  `#[cfg(feature = "playwright-e2e")]` means the plugin cannot inject and every `evaluate` / `waitForSelector` **hangs
  until timeout** rather than failing. The guide records that this silently bit the queue window, the very window this
  one copies.
- The menu item, which is bidirectional in `menu/command_map.rs` (a `*_ID` const, the id→command arm, the command→id
  arm, and the list), plus `menu_structure.rs`, plus a label AND a description in the command registry, which
  `command-registry.parity.test.ts` enforces.
- **A ten-locale i18n pass** (`de en es fr hu nl pt sv vi zh`), with `en-us-parity.test.ts` and
  `miscui-i18n-parity.test.ts` enforcing parity.
- Per-group approve and reject, per-op deselection, the "this changed" affordance, and a distinct affordance for an
  irreversible group.

**The indicator's precedent is the status corner, not the title bar**: `lib/indexing/IndexingStatusIndicator.svelte`
mounted inside `lib/status-corner/StatusCorner.svelte` from `routes/(main)/+page.svelte`, at the main content's
top-right. Adding a member means updating `status-corner/CLAUDE.md`, whose rule is that the corner owns placement and
the hourglass stays last. Whether that satisfies "top-right of the title bar" is David's call.

**Human-facing surface**, so layout and copy are David's per principle 4.

**Depends on M1 and M2** (its stated oracle, open → approve → the operation runs, needs the executors).

**Tests:** Vitest for the view logic, a11y tests per the house pattern, and a Playwright spec for open → approve → the
operation runs.

### M5: The agentic loop

The coalescer over the indexer's corrected event stream, the interest scorer, the inbox with deliver-by deadlines,
budgeted digest compaction, restart reconciliation, and the wake job that turns a digest into a sweep. Resolve the tap
point first (agent-spec §18.14): a second interest-oriented stage over an already-corrected stream, ❌ never a parallel
FSEvents subscription. The pure `coalesce` and `compact` seams make this the most testable part of the program.

**Degraded modes, inherited from spec §6.5 and not optional here:** the flagship scenario reads `~/Downloads`, which
`fda_gate.rs` names as TCC-protected. Define what the indicator and window show when the FDA decision is pending, when
consent is absent, and when no API key is configured. Silence is not an answer for a feature with a persistent
indicator.

Depends on M1 and M3.

### M6: Port the shipped rename feature onto the spine

`propose_rename_plan` becomes one more producer emitting a single-group sweep, dropping its in-memory
`RenameProposalStore`. Keep the acceptance binding, the revise path, and the evidence ledger semantics. Last, because it
is the riskiest change with the least new value, and it lands on a store that real use has already exercised.

**Oracles:** `agent/tools/propose/rename/tests.rs`, the `commands/agent/bulk_rename.rs` tests, and the Vitest trio.
There is no bulk-rename E2E spec, so don't budget a slow lane for one.

## Deliberately out of scope

- **Permanent delete as a proposable verb** (see Reversibility).
- **Auto-apply** (spec §8.5). Everything here goes through the user; autonomy should follow acceptance data.
- **The activity log** (`agent_log`). The window plus the operation log covers this feature's transparency.
- **Standing rules** (spec D31), `~/.cmdr/rules/` and `memory/`, retention pruning for chat history, and multi-volume
  sweeps.

## Open questions

Product questions go to David directly. The technical leftovers:

1. **The group size cap is per-verb, and only the number is open.** `MAX_RENAMES = 200` is a review burden calibrated
   for renames; 200 trashes is a categorically different review. The store owns a hard ceiling; each verb sets its own
   tighter one.
2. **Overlapping live groups over the same path.** With multi-day suggestions, two groups naming one file becomes
   normal. Data safety holds (the loser's fingerprint check fails and it skips), but nothing invalidates the loser or
   explains the skip yet.
