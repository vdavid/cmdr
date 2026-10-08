# S3 cost estimates

The line the Copy, Move, and Delete dialogs show before an operation on S3 ("About $0.02 at AWS list prices"). The
estimator and the price table live in `crates/cmdr-s3/src/cost/`; this module wires them to the app.

## Module map

- `mod.rs`: `estimate`, behind the `estimate_operation_cost` command (`commands/s3.rs`), and its IPC types;
  `estimate_rename` + `rounds_to_zero`, F2's "ask first?" verdict (`write_operations/rename/validity.rs`).
- `plan.rs`: the operation plus the scan's files into one `Workload` per S3 provider. Pure.
- `price_source.rs`: the table in use (server copy, disk cache, bundled fallback) and its background refresh.

## Must-knows

- ❗ **No request goes to S3 for an estimate.** The files come from the dialog's settled scan preview
  (`write_operations::cached_cost_facts`), or F2's bounded tally; both keep every object's size and upload date for
  exactly this.
- ❗ **The fetch never blocks an estimate**: a lookup answers from what's in hand, and a stale table refreshes in the
  background for the next one. Cmdr's own dev, test, and capture runs never fetch (`prod_instance`).
- ❌ **No estimate on a rollback or undo**: those just run (David's call). The frontend only asks from a setup dialog.
- A same-account server-side copy bills one account, so it's one workload; across accounts it's two (a download and
  an upload), possibly in two currencies.

Decisions, the planner's request shapes, and the refresh policy: `DETAILS.md`. Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.
