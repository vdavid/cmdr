# Suggested ops

The agent proposes file operations, the user approves them in groups from a review dialog, and approved groups become
ordinary queued operations. This absorbs milestone 1 of `docs/specs/later/ai/agent-spec.md` §17 (the durable proposal
spine) and extends it to the full shape: many op kinds, an agentic loop wired to filesystem events and user actions, and
a store the agent can query and amend.

Read the spec's §0 status map and §8 first; that document holds the intent and the decision log, this one holds the
build.

## The guiding principle (David, 2026-08-18)

**We do not trust the agent.** Its suggestions can be formally valid and factually hallucinated, and we can never know
which. So the job is not to make the agent safe; it is to **lay everything out for the user to decide**.

**Once the user approves, it is exactly as if the user started the action** — because they did. Responsibility transfers
at the approval click. Approved ops are queued ops: same queue, same conflict prompts, same folder creation, same
overwrite behaviour, same everything.

Two rules fall out of this, and they resolve most design questions here without further debate:

- ❌ **Never add an agent-specific safety behaviour to the execution path.** No auto-skip on collision, no refusing to
  create a destination folder, no refusing an overwrite. If a normal user-started op can do it, an approved op can do
  it. A special case here is not extra safety; it is a second execution path that will drift from the real one.
- ✅ **Put the effort into disclosure instead.** Everything the user needs to judge a suggestion goes in the review
  dialog, in terms they can check. That is where this feature earns its trust.

## What ships

One feature, released in one go:

- The agent watches filesystem events and user actions, wakes on something worth reacting to, and proposes operations:
  **move, copy, trash, delete, rename, compress, extract**.
- Suggestions arrive **grouped**, so "you have 10 new files in Downloads" becomes "move these 5 to X", "move these 4 to
  Y", "delete this 1", each approved or rejected on its own.
- A **Suggested ops indicator** in the status corner (between the queue indicator and the hourglass) opens the
  **Suggested ops dialog**, also reachable from the menu bar.
- Approving closes the dialog, queues the ops, and the queue surface takes over with execution running as normal.
- Suggestions live in `main.db` with **no expiry**, so a user can decide two weeks later.

**Why one release rather than staged.** Half of this is strange to ship. The acceptance-rate analytics land in M1 so
David's own QA pass produces real numbers before launch.

## Decisions

### From David

- **Dialog, not a window.** A soft dialog, per `docs/guides/building-ui.md`. (The ops queue is likely to become one too;
  don't depend on that, but don't build anything that would block it.)
- **Delete is a proposable verb**, alongside trash. It is the only op with no undo at all, so the review row says so
  plainly. It also happens to be the only destructive verb with a Volume-trait route, so it is what makes agent cleanup
  reachable on SMB, MTP, and inside archives.
- **No cap on suggestion count.** 60,000 ops in one group is legitimate. The store, the queries, and the dialog must
  stay responsive at that size; see §Scale.
- **No expiry.** A suggestion waits until the user acts on it.
- **Evidence: disclose, and keep the one existing refusal.** A review row shows the agent's reason _labelled as the
  agent's words_, next to the deterministic facts Cmdr itself knows about the file (size, dates, last-opened, index and
  importance data), so the user can check a claim against something the agent could not invent. Rename keeps its
  existing content-claim refusal, because there the claim IS the content.
- **Copy is David's to review at QA time**, not a blocker during the build.

### Mine, called as architect

- **The operation log keeps `Initiator::Agent` / `AgentEdited`.** Behaviour is identical to a user-started op either
  way; this is purely the audit trail, and the values already exist. Recording plain `User` would make "what did the
  agent talk me into?" permanently unanswerable, which is a loss with no upside. The principle above is about
  _behaviour_, not about erasing provenance.
- **An op may be a directory.** "Delete `~/projects-git/vdavid/cmdr/target/`" is ONE op, not 60,000. Every executor
  already takes paths that may be directories.

## Architecture

### Three levels, and the middle one is one executor call

- **Sweep** (`proposal_sets`): one agent wake's output. "10 new files in Downloads." Display and provenance only.
- **Group** (`proposals`): the reviewable, approvable, executable unit.
- **Op** (`proposal_ops`): one path, which may be a file or a directory.

**A group is exactly one call to one executor.** What the engine actually constrains, verb by verb:

| Verb     | Executor                                      | Binds                                                                      |
| -------- | --------------------------------------------- | -------------------------------------------------------------------------- |
| move     | `move_files_start`                            | one source volume, one shared `destination` dir                            |
| copy     | `copy_files_start`                            | one source volume, one shared `destination` dir                            |
| trash    | `trash_files_start`                           | the LOCAL (`root`) volume only, no destination                             |
| delete   | `delete_files_start`                          | one source volume (has a Volume route, unlike trash)                       |
| rename   | `start_bulk_rename`                           | one volume, **per-op destinations**, all sources sharing one parent folder |
| compress | `compress_start`                              | one source volume, one target archive path, one parent volume              |
| extract  | `copy_between_volumes` with an archive source | a source `ArchiveVolume` + inner paths, and a dest volume + dest path      |

Consequences:

- **`source_volume_id` is a group field, not a sweep field.** A sweep may span volumes, a group may not.
- **Trash binds no volume at all**: `trash_files_start` takes raw `PathBuf`s and calls `NSFileManager.trashItemAtURL`;
  there is no Volume route and archive-inner sources are rejected at the command boundary.
- **Rename is the documented exception**: its ops carry their own destinations and the group binds a shared _parent_.
  `start_bulk_rename` refuses when `row.source.parent() != row.destination.parent()`. Give it its own group kind.
- **Extract is a copy, not an archive edit.** There is no `Extract` in `WriteOperationType`. Use the headless
  `write_operations/transfer/volume/copy.rs::copy_between_volumes` (it takes an injected `Arc<dyn OperationEventSink>`),
  ❌ never the `#[tauri::command]` wrapper in `commands/file_system/volume_copy.rs`, which builds `TauriEventSink` at
  the edge. The resolution work an extract needs (`resolve_source`, the parent-aware resolve routing an archive-inner
  batch to its `ArchiveVolume`, and `resolve_dest_path`, tilde expansion plus root-anchoring) is private to that command
  module and has to be lifted. That is real M2 work.
- ❌ **A group's destination must never be inside an archive.** Copy or move INTO a zip, and move OUT of one, plan from
  their own `WalkDir` rather than the per-source engine, so there is nowhere to apply a source binding. M2 makes those
  routes REFUSE a binding rather than run unbound, which is the right backstop but the wrong layer on its own: a
  user-started copy-into-zip works while an approved one refuses, and it refuses AFTER the user approved — the exact
  agent-specific execution behaviour the guiding principle forbids. So the constraint belongs at the PROPOSAL boundary:
  `GroupIntent` must not be constructible with an archive-inner destination, the way it already forecloses a trash group
  with a destination. Keep the executor fence as the backstop; make the proposal layer mean it never fires. (Extract is
  unaffected: it has an archive SOURCE and a normal destination, and runs through the bound `copy_between_volumes`.)

### Selectors: how the agent proposes 60,000 ops

The agent cannot enumerate 60,000 paths through its context window, so `propose_suggestions` accepts a **selector** as
well as an explicit path list. The backend resolves the selector to a concrete op list **at creation time** and freezes
it (spec §8.2's freeze-at-creation, which exists for exactly this).

- A selector names a root, a glob or extension set, and optional deterministic predicates (age, size).
- ❌ **"Last opened" is NOT expressible and must not be faked.** The drive index carries size, mtime, and inode but no
  access time, and `importance.db`'s visit counts are per-FOLDER, not per-file. So the flagship phrasing "installers
  you've already opened" has no data source today. agent-spec §5.1 names `kMDItemLastUsedDate` via Spotlight as the
  eventual route and §18.4 flags its sampling cost as unresolved. Until that lands, a selector predicate for it would
  silently match nothing, which is worse than not offering it. Copy must not promise it either.
- **Resolution happens server-side against the drive index**, never by walking the filesystem in a tool handler (the
  no-live-FS rule in `agent/tools/CLAUDE.md`).
- The pattern survives as display text on the group ("`~/Downloads/*.dmg` older than 30 days"), and the dialog expands
  to the resolved list. This is spec §8.2's `op_display_name` design.
- ❌ A selector is never re-resolved at approval. Freezing is what makes "what the user saw is what runs" true.
- A whole-folder op needs no selector: one op whose path is a directory.

### Scale

No cap means the design has to hold at 60,000 ops in one group:

- **Store**: `proposal_ops` is queried paged and ordered by `(group_id, seq)`, with an index on it. Counts come from
  `COUNT(*)`, ❌ never from loading rows. The claim transaction compares a **hash plus count** of the live op set
  against the acceptance record rather than materializing 60k rows.
- **Dialog**: the op list is virtualized. Approving a group sends the group id and, for a partial approval, the
  _deselected_ op ids, ❌ never 60,000 selected ids across IPC.
- **Agent tools**: `list_suggestions` returns summaries and counts only; `get_suggestion_group` pages through
  `fit_to_result_budget`.

### Reversibility is a per-verb fact, disclosed rather than blocked

- **Reversible via `RestoreMove`**: move, trash, rename.
- **Reversible by deleting what was written**: copy, and a compress that created a new archive.
- **NOT reversible**: permanent delete (`inverse_action` returns `None` for `OpKind::Delete`), and a compress that
  overwrote an existing archive (the seed is unconditional and prior bytes are not retained).

The group model carries a `reversible` fact and the dialog shows it. ❌ Do not refuse an irreversible group; per the
guiding principle, disclose it and let the user decide.

Likewise a move or copy into a folder that does not exist is allowed (the executors call `ensure_destination_dir` on
purpose), and the review row shows a **"target folder will be created"** marker with a tooltip.

### The agent can re-propose what is pending, and nothing else

Freeze moves from creation to **approval**:

- `pending` groups are mutable by their author through a re-propose against the sweep id.
- Approved groups freeze; no agent path can touch them.
- `interrupted` counts as frozen. The user re-approves (minting a **new** group id with a fresh preflight, the old
  group's op rows staying put and the new group getting copies with fresh ids, so the decision record stays whole and
  analytics count one re-approval rather than two proposals) or discards.
- If the agent re-proposes a group the user has open, the dialog shows a non-destructive "this changed" affordance. ❌
  Never swap rows under the cursor.

### The claim transaction binds the op set, via a server-owned acceptance record

Comparing `proposal_ops` against itself is a tautology once the agent can amend, and letting the client supply values
reverses the propose module's authority boundary (fingerprints never leave the process; the frontend hands back opaque
ids). Rename escapes this because `AcceptedPreflight` is a **separate record** held apart from the rows it describes;
the `Mutex` only made the comparison atomic.

**Preflight writes a server-owned acceptance record** into `main.db` (group id, allowed op ids, and a hash plus count of
the values those ops carried). The client presents a group id and deselected op ids, ❌ never values.

**One `BEGIN IMMEDIATE`:**

1. read the stored acceptance record,
2. re-read the live op set and compare (hash plus count, so this is O(1) in memory at any group size),
3. `UPDATE proposals SET status = 'approved' WHERE group_id = ? AND status = 'pending'`,
4. refuse on `rows_affected == 0` **or** a binding mismatch, as **two distinct typed variants** (stale status versus
   changed contents; the user-facing recovery differs).

`withdraw` takes the same conditional shape. Approving several groups at once is several claims issued together, each
still individually conditional.

### The agent's tool surface, and its access classes

The registry's definitions govern: `Propose` means "stages a proposal and opens a review surface, **mutates nothing**";
`Write` means "mutates the filesystem OR app state" and is never reachable from the agent view.

- `list_suggestions(status)` — **`Read`**. Sweeps and groups as summaries with counts. Never individual ops.
- `get_suggestion_group(group_id)` — **`Read`**. Paged ops with `total` / `returned` / `truncated`.
- `propose_suggestions(sweep)` — **`Propose`**. Creates a sweep from explicit paths or a selector, **and re-proposes an
  existing one when given its id**. Amend folds in here rather than shipping a separate mutating tool, which would be
  `Write` under the enum's own tiebreaker and would fail `test_agent_tool_view_never_writes`.

Only `propose_suggestions` joins `EXPECTED_PROPOSE_TOOL_NAMES`.

### Per-source outcomes: extend the sink, don't invent a channel

`OperationEventSink::emit_source_item_done` already ships and is emitted by delete, trash, move, and copy. The gap is
narrow: **no per-source skip or fail**. Add an outcome to that event (or a sibling) and have the agent's own **sink
decorator** write `proposal_ops.status`. ❌ `write_operations` must never reach into `agent/store/`.

## Build status (2026-08-18, end of the first execution day)

**Merged to `main` and verified:** M1 (store), M2 (executors), M3 (tool surface), M4a (approval bridge), M6 (rename
port), plus a full-tree verification pass. M5 is merged up to and including inbox persistence: `coalesce`, `interest`,
`compact`, the `Merger` fold, the inbox with its deadlines, and `agent_inbox` (migration v6, the ladder's current head).

**Outstanding, both on unmerged branches:**

- **M4b** (`worktree-sugops-m4b-dialog`): the read commands, the dialog, and the menu wiring are committed and green.
  Left: the approve button (a `APPROVE_WIRED` constant plus one `onclick`, now that M4a is merged), the status-corner
  indicator subscribing to `suggestions-changed`, and nine locales. `i18n-coverage` is deliberately red — German is a
  quality sample and the rest wait for David's copy pass, since translating copy about to be revised is throwaway work.
- **M5** (`worktree-sugops-m5-core`): the readiness gate (`WakeReadiness`, precedence consent → FDA → key) and the wake
  job. Design approved in prose; see the decisions recorded in § M5 above.

**Product questions waiting on David**, none blocking:

1. The dialog's English copy, drafted as if it ships.
2. The operation log now shows skipped renames as their own rows, and a name swap shows three rather than two — real
   hops that were previously invisible.
3. **Wake-created threads will appear in the Ask Cmdr session list.** Every wake opens a conversation (with
   `ConversationOrigin::Notification`), so ten quiet wakes is ten threads the user never started, interleaved with
   theirs. The `origin` column makes it filterable; the decision is a product one and belongs to whoever owns the rail.
4. A ratchet pass on the `invariant-density` +12 that M1's merge left un-shrink-wrapped. This effort's own additions net
   to zero.
5. `cargo-audit` / `cargo-deny` are red on `h2 0.4.15` (RUSTSEC-2026-0258, low severity, transitive). Pre-existing,
   Renovate territory, untouched here.

**Environment hazards this effort hit**, worth knowing before the next parallel run:

- **Concurrent agents share one worktree pin and cwd.** `EnterWorktree` from any session moves every session, so an
  agent's `git commit` can land in a sibling's tree. It happened once (recovered, nothing lost). The working rule that
  held: agents run NO git, the lead does all of it, and file writes go through `cd /tmp` first so the guard has nothing
  ambiguous to verify.
- **A fresh worktree inherits the check runner's fingerprint cache** through the APFS clone of `target/`, so it can
  report a cached OK for a check that would fail on its contents — and a branch predating a merge runs a stale SCANNER
  against a new layout. Both produce confident wrong answers. `--fresh` is the escape hatch; details in
  `scripts/check/checks/DETAILS.md`.
- **28 worktrees × a full `target/` fills a 926 GB disk.** It happened twice. `cargo clean` on one merged worktree freed
  58 GB; APFS block sharing means clearing one clone frees nothing while its source stands, so stale trees have to go
  together.

## Milestones

M1 and M2 are independent and start together. M3 needs M1. M4a needs M1 + M2. M4b needs M4a. M5 needs M1 + M3. M6 needs
M1.

**Serialization note:** every milestone adding an IPC command must register it in the `ipc.rs` manifest before
`bindings.ts` regenerates. Land binding regeneration one branch at a time.

### M1: The store, its lifecycle, and the metric

`proposal_sets` + `proposals` + `proposal_ops` in `main.db` (migration v4) with `source_volume_id`, nullable
`destination`, per-op destinations for rename, the `reversible` fact, the nullable creation snapshot, selector display
text, and no expiry column. The typed lifecycle machine, the binding claim transaction over a hash-plus-count acceptance
record, the same-shaped withdraw, the `interrupted` recovery sweep, re-propose with its pending-only guard, and selector
resolution against the drive index with freeze-at-creation.

**Analytics land here**: `analytics::posthog` events for group proposed / approved / rejected, carrying verb and a
bucketed op count, ❌ never a path.

**Tests, test-first:** two concurrent claims (one wins, the loser gets a typed refusal, not `SQLITE_BUSY`); a claim
whose op set changed refuses with the _binding-mismatch_ variant, distinctly from stale-status; a re-propose against an
approved or interrupted group is refused; reopening an approved group yields `interrupted`; partial approval by
deselection; a 60,000-op group claims without materializing its rows; a selector resolves and freezes, and is never
re-resolved at approval; deleting a conversation nulls the link and deletes nothing (raw SQL).

### M2: The executors

Per-source expected fingerprints for move, copy, trash, and delete; a per-source outcome on the sink event; compress;
extract routed through the headless `copy_between_volumes` with the two resolution helpers lifted out of the command
module. ❌ No agent-specific conflict, destination, or overwrite behaviour: approved ops run exactly as user-started ops
do.

**Tests:** per-verb fingerprint mismatch skips that source and reports it; the sink reports skips and failures; an
approved group's config is byte-identical to the user-started equivalent; existing transfer, delete, and archive suites
pass unchanged.

### M3: The agent's tool surface

Three tools with the access classes above, schemas, registry entries, `ToolId` variants, rail labels, and the one
`EXPECTED_PROPOSE_TOOL_NAMES` addition. Selector schema included. Depends on M1.

### M4a: The approval bridge

The backend half of approval, split out of M4 so UI work doesn't carry backend plumbing. Depends on M1 + M2.

Claim the group, build the executor call its `GroupIntent` describes, attach an agent-side **sink decorator** that
consumes M2's per-source outcomes and writes `proposal_ops.status`, and **mark the group `completed` when the operation
finishes**.

**That last part is load-bearing and M1 flagged it**: `ProposalStatus::Completed` exists and the recovery sweep respects
it, but nothing writes it yet. Until something does, a group that ran to completion before a quit comes back as
`interrupted` on the next launch and asks the user to re-approve work that already happened. ❌ Do not ship the bridge
without it.

The decorator is agent-side by construction; `write_operations` must never reach into `agent/store/`.

### M4b: The Suggested ops dialog and the indicator

A soft dialog per `docs/guides/building-ui.md` and the house primitives, NOT a window (so no capabilities file, no
`build.rs` playwright capability, no opener). Per-group approve and reject, per-op deselection over a virtualized list,
the agent's reason beside Cmdr's own deterministic facts, the irreversible marker, the "target folder will be created"
marker, and the "this changed" affordance. Approving closes the dialog and hands off to the queue.

The indicator goes in `lib/status-corner/StatusCorner.svelte`, **between the queue indicator and the hourglass**
(`status-corner/CLAUDE.md`: the corner owns placement, the hourglass stays last); `children` renders left of both
neighbours, so the indicator needs a real member slot rather than the default children position.

**Adding the menu command touches SIX places, not the three an earlier draft of this plan claimed** — and
`commands/CLAUDE.md` warns the omission fails silently: `command-ids.ts`, `commands/sources/*.ts`, the handler in
`routes/(main)/command-handlers/`, `messages/en/commands.json` (a label AND a description, then ten locales, which
`command-registry.parity.test.ts` enforces), `menuCommands` in `shortcuts-store.ts`, and the menu bar itself. ❌ The
menu bar is `menu/macos.rs` AND `menu/linux.rs` (`MenuItem::with_id` + `register_item`) plus `command_map.rs`'s two
directions — NOT `menu_structure.rs`, which delegates the bar to the two platform builders and owns only context menus.

Copy is drafted here and reviewed by David at QA.

**Depends on M1 and M4a.**

### M5: The agentic loop

The coalescer over the indexer's corrected event stream, the interest scorer, the inbox with deliver-by deadlines,
budgeted digest compaction, restart reconciliation, and the wake job that turns a digest into a sweep. Resolve the tap
point first (agent-spec §18.14): a second interest-oriented stage over an already-corrected stream, ❌ never a parallel
FSEvents subscription.

**Degraded modes** (spec §6.5): define what the indicator and dialog show when the FDA decision is pending (the flagship
scenario reads `~/Downloads`, which `fda_gate.rs` names as TCC-protected), when consent is absent, and when no API key
is configured.

**Design decisions already settled**, from a first pass at the pure core (partial work sits on
`worktree-sugops-m5-core`, uncommitted: a complete `agent/wake/mod.rs`, a deliberate `coalesce` stub, a test harness).
Inherit these rather than re-deriving them:

- **Three pure stages** (`coalesce` / `interest` / `compact`), each a value in and a value out, no I/O. §6.3 names the
  seams and this keeps them honest.
- **Events are this module's own value type**, ❌ not the indexer's. That is what keeps §18.14's tap point genuinely
  open instead of answering it by accident through a type dependency.
- **Bundles carry counters, never file names.** Names would grow memory with the event count on the five-million-change
  path and spend digest budget on detail the agent can pull with a `list_dir` when it actually needs it. This is the
  difference between a digest that degrades gracefully on the pathological case §6 exists for and one that falls over on
  it.
- **Tumbling, epoch-anchored windows**, so a morning burst and an evening burst can never share one deadline.
- **❌ Unscored must never collapse into zero.** The interest scorer mirrors `WeightLookup`'s three-way answer, so a
  folder importance hasn't reached yet stays distinguishable from one scored as junk. Collapsing them would make a new
  project folder look exactly like `node_modules`, and the agent would quietly ignore it.

Depends on M1 and M3.

**Status: the pipeline and the wake job are built; the tap adapter is not.**

Shipped in `agent/wake/` (54 tests): `coalesce` / `merge_bundles` over one `Merger` fold, `interest` + `wake_delay`,
`compact`, the `Inbox` with deliver-by deadlines and restart reconciliation, `agent_inbox` (migration v6) with its
persistence edge, the `WakeReadiness` gates, and `run_wake`. Reasoning for every decision: `agent/wake/DETAILS.md`.

**§18.14 is resolved** (see that `DETAILS.md`): a second observer inside `process_live_batch`, placed after
`detect_renames_by_inode` and the storm coalescing, handing per-batch per-folder ROLLUPS across the existing
`IndexEvent` seam. The `downloads/` watcher coexists rather than merging, and the agent must never assume it is running.

**What remains:**

- **The tap adapter.** Mapping the crate-side rollup into `EventBundle` and calling `Inbox::admit_if_permitted`. The
  observer shape follows `ChurnObserver`, which is passed `&mut` so a live batch cannot be processed without one.
- **The scheduler that calls `run_wake`.** Something owning a timer that fires at `Inbox::next_deadline`, resolving
  provider, model, and prompt budget the way the command layer does today, and supplying the `ChatEventSink` the
  indicator reads.
- **Indicator wiring (M4b surface).** Each `WakeReadiness` gap is a typed state with an action; none of them is silence.
- **A wake creates a conversation, so wake threads appear in the rail session list.** `origin` is already `notification`
  on every one, so filtering needs no schema work, but whether the default view filters them or they get their own
  affordance is a product call.
- **Two tuning knobs, deliberately guesses** (agent-spec 18.5): the unknown-importance weight (0.35) and the
  hot/warm/cold tiers (5s / 5min / 1h). Only their ORDER is pinned as a contract.

### M6: Port the shipped rename feature onto the spine

`propose_rename_plan` becomes one more producer emitting a single-group sweep, dropping its in-memory
`RenameProposalStore` and its 15-minute TTL. Keep the acceptance binding, the revise path, and the evidence ledger
semantics. Check `BulkRenameReviewDialog`'s short-lived-proposal assumption.

## Deliberately out of scope

Auto-apply (spec §8.5), the activity log (`agent_log`), standing rules (spec D31), `~/.cmdr/rules/` and `memory/`,
retention pruning for chat history, and multi-volume sweeps.

## Open, and fine to defer

**Two live groups naming the same file.** Data safety holds (the loser's fingerprint check fails and it skips), but
nothing invalidates the loser or explains the skip. David: "okay for now."
