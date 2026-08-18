# Proposal spine details

Pull-tier docs for `agent/store/proposals/`. Must-knows live in `CLAUDE.md`. The feature this serves, end to end:
`docs/specs/agent-suggested-ops-plan.md`.

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

`GroupIntent` encodes exactly that table in the type system. Each variant carries the target its verb binds and the op
shape it takes (`NewOp` for the shared-target verbs, `NewRename` for rename), so the pairing can't be violated and there
is no runtime check to keep in sync with the executors. Adding a verb means adding a variant, which makes every `match`
here a compile error until it's handled.

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
refuses with a MISMATCH instead of the honest stale status. Per-op execution statuses are the executor's to write (M2).

**Why comparing the ops against themselves wouldn't work.** Once the agent can re-propose a pending group, "do the ops
match the ops?" is a tautology. The shipped rename proposal store solved the same problem the same way: `AcceptedPreflight`
is a separate record held apart from the rows it describes (`agent/tools/propose/rename/store.rs`), and its `Mutex` only
made the comparison atomic. Here the separate record is a table and the atomicity comes from the transaction.

### The binding digest

SHA-256 over each live op's `(id, source_path, destination)`, each field length-prefixed and presence-tagged so no two
different op sets can concatenate to the same bytes, plus the row count carried alongside. Streamed with
`Statement::query` and folded row by row: one row is in memory at a time.

The count is not redundant with the hash. It is what makes a smuggled-in extra op refuse for a legible reason, and it is
what a log line or a refusal message can carry without leaking a path.

`page_ops` is deliberately the only function that builds `Vec<ProposalOp>`, and it carries a `cfg(test)` thread-local
call counter so `tests/scale.rs` can assert the claim path never calls it. Thread-local, not global: the test harness
runs tests in parallel threads in one process, so a shared counter would read another test's calls.

## Statuses, and who owns each one

`ProposalStatus` (`agent/types.rs`):

- `pending` — the only mutable state. The agent may re-propose it; the user may approve or reject it. No expiry: a
  suggestion waits as long as it takes.
- `approved` — claimed, ops handed to the queue, execution in flight.
- `interrupted` — the app restarted while approved. Frozen: nothing here knows which ops ran, so the user re-approves
  (minting a NEW group with a fresh preflight, the old group's rows staying put so the decision record stays whole and
  analytics count one re-approval rather than two proposals) or discards.
- `completed` — every op reached a terminal outcome. Written by the executor (M2).
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
  from the index row; an explicit path list from the agent has nothing behind it to fill it with. M2's per-source
  fingerprint check reads it where it exists.
- **`selector` holds the selector's JSON**, for display and provenance. ❌ It is never re-run; see
  `../../suggested_ops/DETAILS.md`.
- Every classification column is a TEXT token, so `main.db` stays `sqlite3`-inspectable and nothing branches on a
  message string.

## What lives a layer up

Selector resolution, the analytics events, and the propose/approve/reject service are `agent/suggested_ops/`. This
module keeps `rusqlite` as its only real dependency, so nothing about the drive index or PostHog can reach into the
persistence layer.
