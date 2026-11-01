# Proposal spine details

Pull-tier docs for `agent/store/proposals/`. Must-knows live in `CLAUDE.md`. The service above:
`../../suggested_ops/DETAILS.md`. The review surface: `apps/desktop/src/lib/suggested-ops/DETAILS.md`.

## Why three levels, and why the middle one is the unit

A **group is exactly one call to one executor**, and that is what decides the schema. The write engine constrains what
one call can span, verb by verb: move and copy take one source volume and one shared destination directory, trash takes
raw local paths and no target at all, delete takes one source volume, `start_bulk_rename` takes per-row destinations
whose sources all share one parent, compress takes one target archive, and an extract is a copy out of an
`ArchiveVolume`.

Two consequences fall out:

- **`source_volume_id` is a GROUP column, not a sweep column.** A sweep may span volumes; a group may not.
- **Rename is the documented exception**: its ops carry their own destinations and the group binds a shared _parent_,
  because `start_bulk_rename` refuses a row whose source and destination parents differ.
- **`proposal_ops.destination` is a per-op NAME, never a path**: always set under rename (the rename review reads it
  unconditionally), set under move or copy only for an op that lands under a new name (`NULL` keeps its own), and
  `NULL` under every other verb. The transfer engine takes the move's or copy's names as one per-source map, so the
  group stays one call. (v4's DDL comment says "rename only"; it describes the shape that shipped then, and a shipped
  migration is never edited.)

A **server group** (`add_server` / `edit_server`) is the one group with no executor call and no ops: its draft is the
whole proposal (§ A server group).

`GroupIntent` encodes exactly that table in the type system. Each variant carries the target its verb binds and the op
shape it takes (`TransferOp`, with its optional name, for move and copy; `NewOp` for the other shared-target verbs;
`NewRename` for rename), so the pairing can't be violated and there is no runtime check to keep in sync with the
executors. Adding a verb means adding a variant, which makes every `match`
here a compile error until it's handled.

## What preflight accepts

**The revision comes first.** `record_acceptance` also takes the `revision` the review showed (`proposals.revision`, v11, bumped by every `repropose_group`) and refuses a different one as `AcceptanceOutcome::RevisionChanged` before touching a row, in the same transaction as the binding. Without it, an agent amending a group while the user reads it gets its NEW verb, target, or list approved by a click on the old one: the acceptance record is torn up on amend, but preflight would simply record a fresh one for whatever the group holds at the click. The dialog maps it to `listChanged`. The rename review sends it too: `RenameProposalSnapshot.revision` comes back with `preflight_bulk_rename` (a different one answers status `listChanged` before any stat) and `apply_bulk_rename` (the typed `BulkRenameError::ListChanged`).

`claim.rs::record_acceptance` takes an `OpSelection`, ids only, in whichever form is smaller: `allExcept { opIds }` (the ops the review turned off) or `only { opIds }` (the ones it kept), so neither "all but three" nor "only three" of 60 000 carries 60 000 ids. Both forms run in one `BEGIN IMMEDIATE`: reset the live set to all-or-nothing, toggle the named ids, then hash the result. They're two spellings of one selection, so they bind the SAME count and digest (`tests/claim.rs::only_these_binds_exactly_what_all_except_the_rest_binds`); the claim never knows which form arrived. Only `pending` and `excluded` rows move, so an op that already ran keeps its outcome. A third form, `fromStored { excludeOpIds, includeOpIds }`, starts from what the last review left instead of all-or-nothing and applies only the flips: an earlier "no" (an approval the engine refused to start leaves its deselected ops `excluded`) survives into the next review without the client naming ops it never paged in, and `SuggestedOpView.status` is what the review seeds its ticks from. A selection that leaves the live set empty (`only` with no ids, ids the group doesn't have, `allExcept` every op) is `AcceptanceOutcome::NothingSelected`: rolled back, no acceptance recorded, never an empty run.

## The claim transaction, in order

`claim.rs::claim_group_for_execution`, one `BEGIN IMMEDIATE`:

1. **Read the stored acceptance record** (`proposal_acceptances`). Server-owned: preflight wrote it, and the client only
   ever presented ids.
2. **Re-read the live op set** as `OpBinding { op_count, digest }` and compare. O(1) in memory at any group size.
3. **`UPDATE proposals SET status = 'approved' WHERE id = ? AND status = 'pending'`**.
4. **Refuse** on a binding mismatch, or on `rows_affected == 0`, as two distinct typed variants.

Two refusals rather than one because the user-facing recovery differs: a stale status means somebody already answered
(nothing to re-review), a binding mismatch means the list changed under the review (review it again). Collapsing them
would make the dialog say the wrong thing in one of the two cases.

**Why the claim leaves op statuses alone.** A claim that also flipped its ops to a `queued` status would break the
concurrency story: the winner's change alters the live op set, so a concurrent loser computes a different binding and
refuses with a MISMATCH instead of the honest stale status. Per-op execution statuses are the executor's to write.

**Why comparing the ops against themselves wouldn't work.** Once the agent can re-propose a pending group, "do the ops
match the ops?" is a tautology. The record held APART from the rows it describes is the whole mechanism; the transaction
is only what makes reading it and comparing the live set atomic.

### The binding digest

SHA-256 over each live op's `(id, source_path, destination)`, each field length-prefixed and presence-tagged so no two
different op sets can concatenate to the same bytes, plus the row count carried alongside. Streamed with
`Statement::query` and folded row by row: one row is in memory at a time.

The count is not redundant with the hash. It is what makes a smuggled-in extra op refuse for a legible reason, and it is
what a log line or a refusal message can carry without leaking a path.

`page_ops` is deliberately the only function that builds `Vec<ProposalOp>`, and it carries a `cfg(test)` thread-local
call counter so `tests/scale.rs` can assert the claim path never calls it. Thread-local, not global: the test harness
runs tests in parallel threads in one process, so a shared counter would read another test's calls.

## Creation is per-group, and that's fine

`create_group` runs its header insert and all of its op inserts in one `BEGIN IMMEDIATE` (immediate rather than
deferred because it reads `MAX(seq)` and then writes; a deferred upgrade after another connection wrote yields
`SQLITE_BUSY_SNAPSHOT`, which the busy timeout does not retry). A sweep's groups each get their own transaction, so a
crash mid-sweep can leave a sweep with some of its groups. That's harmless by design: a group is independently
reviewable, and a half-written GROUP is what would matter — that one is atomic.

## Statuses, and who owns each one

`ProposalStatus` (`agent/types.rs`):

- `pending` — the only mutable state. The agent may re-propose it; the user may approve or reject it. No expiry: a
  suggestion waits as long as it takes.
- `approved` — claimed, ops handed to the queue, execution in flight. The one way back is `claim.rs::release_claim`,
  for a claim the write engine then refused to start: conditional on `approved` AND on no op carrying an outcome, so a
  group anything ran for can never be offered (and run) twice.
- `interrupted` — the app restarted while approved. Frozen: nothing here knows which ops ran, so the user re-approves
  (minting a NEW group with a fresh preflight, the old group's rows staying put so the decision record stays whole and
  analytics count one re-approval rather than two proposals) or discards.
- `completed` — every op reached a terminal outcome. Written by the executor.
- `rejected` — the user said no.

`OpStatus`: `pending` (in the live set) / `excluded` (deselected at review, row kept) / `done` / `skipped` / `failed`.
The last three are the executor's, and they are what make a partial apply reportable.

The **recovery sweep** flips `approved` → `interrupted` and nothing else. It must not touch `completed`: re-approving a
finished group would run its ops a second time. It runs from `agent::start`, once, and is idempotent because the first
run leaves no `approved` rows.

## DDL notes

- **`proposal_sets.conversation_id` is nullable, `ON DELETE SET NULL`**, where the neighbouring conversation-linked
  tables cascade. A sweep is a decision record: what the user was asked and what they answered outlives a tidied-up chat
  thread, and a sweep from a background wake has no thread at all. A group and its ops DO cascade with their sweep — an
  orphan group would be a decision nothing explains.
- **No expiry column and no cap.** Both are product decisions from the plan: a suggestion waits until the user acts, and
  60 000 ops in one group is legitimate.
- **`proposal_ops` carries one unique index, `(group_id, seq)`**, which serves both the paged read (an index range scan,
  no sort) and the binding's ordered stream. A `(group_id, status)` index would force a sort on the ordered read and buy
  little: nearly every op in a live group is `pending`.
- **The creation snapshot (`snapshot_size` / `snapshot_mtime` / `snapshot_inode`) is nullable.** A selector fills it
  from the index row; an explicit path list (and a rename plan) gets one bounded stat per path through the source volume before the write
  (`../../../tools/suggestions/snapshot.rs`), and a path that was missing, refused, or too slow stays `NULL`. Display
  only: execution binds a LIVE stat at preflight (`../../suggested_ops/CLAUDE.md`).
- **`selector` holds the selector's JSON**, for display and provenance. It is never re-run, and that half needs no
  rule: `../../suggested_ops/tests.rs::a_selector_freezes_at_creation_and_is_never_resolved_again` counts the
  resolver's calls across propose → preflight → approve and fails at two. Why freezing is what makes "what the user
  saw is what runs" true: `../../suggested_ops/DETAILS.md`.
- Every classification column is a TEXT token, so `main.db` stays `sqlite3`-inspectable and nothing branches on a
  message string.
- **A server group's `source_volume_id`** is the edited place's id, or `""` for an add, and nothing reads it (§ A server
  group). `proposal_server_drafts` CHECKs that `editing_id` and `previous` are set together.

## What a producer may add beside the ops

A verb whose review needs more than a path and a destination keeps it in its OWN table, keyed by op id and cascading with
it, rather than widening `proposal_ops`. Rename does exactly that: `proposal_rename_evidence` (migration v5) holds why
each proposed name is believable, and only `tools/propose/rename/` writes or reads it. The spine stays
verb-agnostic, so adding a verb never means adding a column every other verb leaves NULL.

The catch a producer owns: `create_group` is its own transaction, so a sidecar row can only be written afterwards. State
what a group missing its sidecar means, and fail closed — rename refuses to load such a group at all.

**The one exception is a server group's draft, which the spine writes itself** (§ A server group). Rename's evidence is
display-only, so a group missing it can fail closed later. A server group's draft IS the operation: a pending card with
no draft is one nothing can show or save. So `create_group` and `repropose_group` write it from the intent inside the
group's own transaction, and it can't be missing.

## A server group

**Decision: a server proposal is a ZERO-OP `GroupIntent::SaveServer` variant, its draft in a sidecar the spine writes in
the group's own transaction.** Verbs `add_server` / `edit_server` (picked by whether the intent carries a `ServerEdit`),
reversibility `server_settings`, and `proposal_server_drafts` (migration v13: `draft`, `editing_id`, `previous`,
`completion_note`), where `draft` and `previous` are the spine's own `ServerDraft` as JSON (`draft.rs`; the writes and
the claim are `server.rs`, the fail-closed read `read.rs`).

**Why a variant, not a sibling `server_proposals` table**: one lifecycle. Sweeps, the `pending`-only amend with its
`revision`, the claim's `pending → approved`, rejection, the recovery sweep, the thread line, the agent hearing every
answer (`../../outcomes.rs`), the badge (`count_pending` counts groups), `suggestions-changed`, analytics, and
`list_suggestions` all apply unchanged. A sibling table would duplicate each, or leave a server card outside one: a
badge that doesn't count it, an answer the agent never hears.

**Against it, and the answer**: file-shaped code assumes ops. The acceptance record and the op binding mean nothing for
zero ops, so a server group takes its OWN claim, and neither claim takes the other's kind:

- `record_acceptance` refuses a server group as `AcceptanceOutcome::NotAFileGroup`. With no acceptance on record,
  `claim_group_for_execution` then refuses it as a binding mismatch, so the file path can't approve one.
- `claim_server_group` refuses a file group as `NotAServerGroup`. Otherwise it is one `BEGIN IMMEDIATE`: check the
  status and the `revision` the card showed, read the draft, then `UPDATE ... WHERE status = 'pending' AND revision = ?`.
  **The revision IS the binding**: a draft changes only through `repropose_group`, which bumps it in the same
  transaction it replaces the draft (`tests/server.rs` holds that a failed draft write leaves both old), and nothing
  changes it after the claim.

**The draft fails closed.** A server group with no draft row, or one that won't decode, is a typed
`AgentStoreError::ServerDraft` on every read (`read_server_proposal`, `list_groups`, the claim), ❌ never a default
draft: a card with invented fields is one the user could approve. `ServerDraft` is `deny_unknown_fields` and has no
field that could hold a secret (no password, key, key file, agent, or auto-reconnect), so a stored secret is
unrepresentable both ways. The error names the group, never the JSON (it holds a host and an account).

**An amend may change a group's kind.** The draft row follows the intent: `repropose_group` writes it for a server
intent and deletes it for a file one, in the same transaction.

**Completion and give-back.** `mark_server_group_completed` is the file group's conditional `approved → completed` plus
the `ServerSaveNote` (a save that wrote the host but not its share) in the same transaction. `release_claim` gives a
server claim back unchanged (zero ops, so its no-outcome guard holds trivially); WHEN it may is the approval service's
call: only a save that wrote nothing (`../../suggested_ops/DETAILS.md` § Approving a server, which also says why no save
writes a note today). A completed group never goes back. `recover_interrupted_groups` freezes an
`approved` server group as `interrupted`: the app died mid-save, and nothing knows whether it landed.

**`source_volume_id` (NOT NULL) of a server group** holds the edited place's id for an edit and `""` for an add
(`GroupIntent::stored_source_volume`); `NewGroup::source_volume_id` is ignored for one, and nothing reads the column.

## What lives a layer up

Selector resolution, the analytics events, and the propose/approve/reject service are `agent/suggested_ops/`. This
module keeps `rusqlite` as its only real dependency, so nothing about the drive index or analytics can reach into the
persistence layer.

## What execution writes back

`complete.rs`. Two writes, both conditional, both shaped so a late or duplicate call cannot undo a decision.

**`record_op_outcome` overwrites, deliberately.** A cross-filesystem move speaks twice for one source: `Done` when staging finishes, then `Done` again once the source-delete phase removes it, or `Skipped` when the rename phase left it standing. Staging succeeding says nothing about where the item ended up, so the LAST word is the verdict. It refuses only one row: an `excluded` op was never in the accepted set, so nothing that ran can be about it, and a stray path match must not overwrite the record of what was offered.

**`mark_group_completed` is conditional on `approved`**, the same shape as the claim and the rejection. A group the recovery sweep already froze must not be resurrected by a late settle from the operation that died with the last launch.

**Decision: `Completed` means execution FINISHED, not that every op succeeded.**
**Why**: it is written when the operation settles, whatever the outcome, because the distinction the status carries is "no longer in flight" versus "we lost track of it". A cancelled group keeps `pending` rows for the ops nothing ever reached, and those rows are the honest record. The alternative — marking only on success — leaves a cancelled group `approved`, so the next launch calls it `interrupted` and asks the user to re-approve an operation they deliberately stopped. Pinned by `tests/completion.rs`.

## A decided group's thread line

`thread_line.rs`, over `proposals.thread_line_id` (migration v10, `ON DELETE SET NULL` so a deleted thread just drops
the pointer). The row is an ordinary `role = 'event'` `ProposalDecided` row; the column is how a settle finds the
"running" line its start wrote, so the thread keeps one line per decision. `open_running_line` writes only for a group
still `approved` with no line yet, and `settle_line` rewrites the line it finds (keeping its id and `seq`) or appends
one. Each checks the pointer and writes in one `BEGIN IMMEDIATE`, because start and settle run on different connections
and a quick operation can settle first. `running_lines` (a `json_extract` over the joined rows) is the launch pass's read.
Why the line exists at all: `../../suggested_ops/DETAILS.md` § What the user's answer teaches the agent.

## A destination a route serves is unrepresentable

`WritableDestination` wraps the `Location` that `Move`, `Copy`, `Extract`, and `Compress` bind, and refuses one a ROUTE serves: a path continuing inside an archive, or one inside a repo's virtual `.git` trees. It asks `volume::manager::path_routes_over_its_parent`, so the check and the routing split at exactly the same place — they must agree, or the executor fence fires on a group this layer allowed.

**Decision: the constraint is here, not only in the executor.**
**Why**: copy or move INTO a zip, and move OUT of one, plan from their own `WalkDir` rather than the per-source engine, so a source binding has nowhere to be applied; a repo's snapshot has no file to write to at all, and the transfer routing refuses it outright. The write engine refuses either after the fact, which is the right backstop and the wrong layer on its own: it fires AFTER the user approved, so a user-started copy-into-zip would work while an approved one refused — the agent-specific execution behaviour the guiding principle forbids. At the proposal boundary a refusal costs the agent a retry and the user nothing. Keep the executor fence; this is what makes it never fire.

The compress TARGET is an archive and stays legal: only a non-empty inner remainder is refused. Extract is unaffected, its sources being the archive-inner half.
