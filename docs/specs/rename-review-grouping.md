# One review for one job, not one dialog per batch

**Status**: shipped 2026-09-06, option (a). What follows describes the design as built; option (c) below is the only
part still open.

**Problem**: Ask Cmdr's bulk rename asked the user to approve the same job several times. A 500-file rename opened five
review dialogs at a 60,000-token budget and 22 at the default, because the model can only emit about 101 plan rows per
reply and each reply was staged and reviewed on its own. Worse, each new dialog CANCELLED the plan the previous one was
showing, so a half-answered review could vanish under the cursor mid-turn. The user was asked to make one decision,
repeatedly, on exactly the operation where careful review matters most.

**The fix is presentational**: accumulate a job's proposals into one review, apply them as the operations they already
are, and leave every safety property exactly where it is. ❌ This is not per-rule approval, which David declined on
2026-09-05: approval stays per item, and the volume problem it was meant to solve is answered by the group-level Approve
and Reject buttons the suggested-ops dialog already ships (`SuggestedOpsDialog.svelte`, `approveGroup` / `rejectGroup`),
so 500 files never mean 500 clicks. Every row still gets individually reviewed, preflighted, and fingerprint-rechecked
at write time.

**Read first**: `apps/desktop/src-tauri/src/agent/tools/propose/CLAUDE.md` (the authority boundary, and why proposal
construction never touches a live mount), `.../propose/rename/store.rs`'s module header (what is durable and what is
deliberately not), and `apps/desktop/src-tauri/src/agent/chat/budget.rs` (the batch arithmetic, which owns the numbers
below).

## Why it batches at all, so nobody tries to delete the batching

The cap is the model's REPLY, not the context window. `files_per_batch` takes the smaller of what the prompt holds and
what the completion slot can return:

- `reply_fits = (AGENT_MAX_OUTPUT_TOKENS − REASONING_RESERVE_TOKENS) / PLAN_ROW_TOKENS_PER_FILE = (12,000 − 6,000) / 59 = 101`
- `prompt_fits = (budget × 90% − FIXED_PROMPT_OVERHEAD_TOKENS) / RENAME_TOKENS_PER_FILE`, with the overhead at 6,263 (19
  tool declarations plus a 1,994-token system prompt) and 349 tokens per file.
- `files_per_batch(16,000) = 23` (the PROMPT binds); `files_per_batch(60,000) = 101` (the REPLY binds, against 136 the
  prompt would hold). 16,000 is `DEFAULT_PROMPT_TOKEN_BUDGET`, so 500 files is 22 batches at the default and five at
  60,000.

⚠️ **Every one of these numbers moves when a tool joins the view**, and the batch count with it, so re-derive rather
than quoting this section. `agent/chat/budget.rs` owns them and `agent/chat/context/cost_tests.rs` pins them against the
shipped assets (verified 2026-09-06).

Overshooting does not degrade gracefully: the reply is cut off mid-JSON and the whole plan is lost. So batching stays,
and this spec changes only where the user meets it.

## What changes

1. **A review holds N proposals, not one.** `openRenameReview` (`ask-cmdr-rename-review.svelte.ts`) called
   `discardRenameReview()` and replaced the review with the incoming proposal, so a second batch destroyed the first. It
   became a job-scoped form: `stageRenameProposal` collects, `openStagedRenameReview` shows, rows accumulate, and the
   dialog renders them as one list.
2. **Apply issues one operation per proposal.** `applyRenameReview` calls `applyBulkRename(proposalId, allowedRowIds)`
   once today; it becomes one call per proposal that still has allowed rows, in order. Each returns its own operation
   id, which is what the thread already expects.
3. **The thread line and undo need no new concept.** `noteRenameApplied` already folds a run of operations into
   `jobOperationIds` / `jobFileCount`, and `undoRenameLine` already takes a `'batch' | 'job'` scope. Feeding it N ids
   from one apply is the same shape it already handles from N sequential applies. ⚠️ Its rule that only the NEWEST line
   carries the job-wide undo has to keep holding when the ids arrive together rather than one at a time.
4. **`rememberDeniedNames` loses its cross-batch job.** It existed because batches were sequential, so a later batch
   could learn from names the user rejected in an earlier one. Once the user decides once, at the end, there is no later
   batch to teach. It stays for the user's NEXT message ("not like that, try dates instead"), and its doc comment says
   so, or the next reader will think it broke.

## What does not change, and why that is the point

Every guardrail lives in the reviewed proposal, and all of them still apply per row: the evidence check, pane-scoped
source validation, the fingerprinted preflight, and the per-row fingerprint recheck at write time. Also unchanged:

- **`MAX_RENAMES = 200`** per proposal (`propose/rename/plan.rs`), and the reply cap keeps a real one near 101.
- **A rename group binds a shared parent**, because `start_bulk_rename` refuses a row whose source and destination
  parents differ (`GroupIntent::Rename`, the proposal spine's documented exception). A job spanning several folders is
  therefore several groups and several operations; it is still one review, and the job-wide undo already spans them.
- **The accepted preflight stays process-local** (`AcceptedRenamePreflights`). A restart must force a fresh preflight
  rather than resurrect an approval given before the app died. Accumulating proposals must not make an approval outlive
  the process.

Two things the July plan listed as blockers are already resolved in shipped code, and this spec depends on both:

- **The proposal has no expiry.** `store.rs`: "The proposal is durable. It has no expiry; a suggestion waits until the
  user acts on it, and it survives a restart because the spine holds it." A paced review cannot lose batch one.
- **The dialog already shows every row at once.** `store.rs`: "Paging a rename group would buy nothing: the review
  dialog shows every row at once." 500 to 1,000 rows render without paging or virtualization (David, 2026-08-28).

## The one design choice

**When is a job complete, so the dialog opens once?** Options:

- **(a) On turn end.** The model stops emitting plan calls, the review opens with everything staged. Simplest, and the
  turn boundary already exists. Cost: nothing is visible while the model works through five batches.
- **(b) Count-derived.** The app knows the file total and `files_per_batch`, so it can predict the batch count. Brittle:
  the model may propose fewer rows than hinted, and then the review never opens.
- **(c) Open immediately and grow.** Rows appear as batches land. Best feedback, and it makes preflight a moving target
  while the user is already reading.

**David chose (a)**, 2026-09-05, and that is what shipped: `proposalReady` stages, and `done` / `failed` / the user's
own Stop each open one review over everything staged. A `discarded` thread hands its plans back instead — there is no
conversation left to review against. (c) remains the follow-up. ❌ Not (b): a prediction that can silently fail to
resolve is worse than a wait.

## Size

Small, and deliberately so: this is frontend and store-shape work, ❌ not a write-engine change, which is what made the
per-rule alternative expensive.

- `ask-cmdr-rename-review.svelte.ts`: the review holds a list of proposals; open accumulates instead of replacing; apply
  loops. The row model, revise, and preflight paths are per-proposal already.
- `BulkRenameReviewDialog.svelte`: render the accumulated list, and show which folder a row belongs to once a job can
  span parents.
- `ask-cmdr-stream.svelte.ts`: hold `proposalReady` events until the turn ends, per (a).
- No backend change was needed, as expected: `preflight`, `AcceptedRenamePreflights`, `apply_bulk_rename`, and
  `cancel_bulk_rename_proposal` are all keyed by proposal id and hold nothing review-wide. `store.rs`'s module header
  now says so, so nobody widens one of them to "the review".

## Tests

All six landed; the first was seen red before the fix. TS titles read as prose, so the names below map to
`ask-cmdr-rename-review.test.ts`, `ask-cmdr-rename-undo.test.ts`, `BulkRenameReviewDialog.a11y.test.ts`, and
`propose/rename/tests/store.rs`.

- **`a_second_batch_does_not_destroy_the_first_review`** — the regression that motivates the whole spec. **Test-first.**
- **`applying_a_multi_batch_review_starts_one_operation_per_proposal`**, and the thread line carries every id.
- **`the_job_wide_undo_still_appears_only_on_the_newest_line`** when ids arrive together.
- **`a_row_the_user_turned_down_is_never_sent`**, unchanged per proposal but now asserted across proposals.
- **`an_approval_does_not_survive_a_restart`** — the process-local preflight, pinned because accumulation makes a review
  live longer.
- A component test that a review spanning two parents shows both, and applies two operations.
