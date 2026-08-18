# Proposal spine (`agent/store/proposals/`)

Sweeps, groups, ops, and the acceptance record, in `main.db` (migration v4, in `../migrations.rs`). Three levels:
a **sweep** (`proposal_sets`) is one agent wake's output, a **group** (`proposals`) is the reviewable unit and exactly
one executor call, an **op** (`proposal_ops`) is one path that may be a whole directory. Depth: `DETAILS.md`.

## Module map

- `write.rs` — `create_sweep` / `create_group` (creation is where a proposal FREEZES) and `repropose_group`.
- `read.rs` — group headers, `COUNT(*)` counts, and the one paged op reader.
- `claim.rs` — preflight, the claim transaction, rejection, and the streaming binding.
- `recovery.rs` — the `interrupted` startup sweep.

## Must-knows

- **The claim is the one place a bug applies ops to real files twice.** One `BEGIN IMMEDIATE`: read the stored
  acceptance, compare the live op set as a hash plus count, conditional `UPDATE ... WHERE status = 'pending'`, refuse on
  a mismatch **or** on `rows_affected == 0` as two distinct typed variants (the recovery differs). ❌ Don't reorder it.
  It also leaves op statuses alone, which is what makes a losing concurrent claim report stale status instead of a
  mismatch the winner caused. That one needs no rule, because a test holds it:
  `tests/claim.rs::two_concurrent_claims_leave_exactly_one_winner_and_a_typed_refusal` goes red if the claim starts
  writing them.
- **The acceptance record is SERVER-OWNED.** The client presents a group id and DESELECTED op ids, never values.
  Comparing `proposal_ops` against itself is a tautology once the agent can amend a pending group, so the record held
  apart from the rows is the whole mechanism.
- **`live_binding` STREAMS.** One row in memory at a time, so a 60 000-op group claims as cheaply as a three-op one.
  `page_ops` is the only op-row-materializing read here, and a test asserts the claim path never calls it — so a
  comparison rewritten to load the ops fails that test rather than quietly costing 60 000 rows twice.
- **`pending` is the only mutable status.** `repropose_group` guards on it and TEARS UP the acceptance record, so a
  preflight against the old op list can't carry an approval onto a new one. `approved`, `interrupted`, `completed`, and
  `rejected` are the user's.
- **Run `recover_interrupted_groups` exactly once per launch, from `agent::start`.** ❌ Not from
  `open_write_connection`, which runs the ladder on every connection open: a sweep there would reclassify a group that
  is genuinely executing.
- **`GroupIntent` makes the wrong shapes unrepresentable.** Each variant carries the target its executor binds AND its
  op shape, so a trash group with a destination, or a move group whose ops carry their own, can't be built. Reversibility
  falls out of the same enum. Add a verb by adding a variant, never by adding a check.
- **A deselected op keeps its ROW** (`OpStatus::Excluded`). The decision record says what was offered, not only what ran.

Service layer above this (selectors, analytics): `../../suggested_ops/CLAUDE.md`.
