# Transfer driver refactor

## Why

After the bulk-skip / pre-known-conflicts work (commit `32e6de03`), four functions in `write_operations/` carry the same
skip + conflict-resolve + progress-emit scaffolding around different transfer cores:

- `volume_copy.rs::copy_volumes_with_progress` (cross-volume copy)
- `volume_move.rs::move_between_volumes` (cross-volume move = copy + delete source)
- `volume_move.rs::move_within_same_volume` (same-volume rename)
- `copy.rs::copy_files_with_progress` (local-FS-only copy with `copyfile(3)` / `copy_file_range(2)` / `CopyTransaction`
  rollback)

Two concrete problems flow from this duplication:

1. **Data-safety invariants are repeated, not tested in one place.** The critical guarantee is "the pre-skip `continue`
   is positioned before any destructive call" — `dest_volume.get_metadata`, `copy_single_path`, `source_volume.delete`.
   Today this is enforced by inspection in each of the 4 functions. A regression in any one of them wouldn't be caught
   by the tests of the others. The user explicitly flagged data-loss risk on this.
2. **Only one of the four (`copy_volumes_with_progress`) is directly tested.** The other three take `&tauri::AppHandle`,
   can't be driven from unit tests, and rely on structural-parity-with-copy reasoning. Adding tests requires extracting
   `&dyn OperationEventSink`-taking inner functions — the same shape `copy_volumes_with_progress` already has.

Doing both refactors **together** is more efficient than sequentially: the testable-inner extraction is a prerequisite
for the shared-driver migration, and the migration would otherwise touch the same files twice.

## What we're building

A shared **transfer driver** that owns the scaffolding common to copy and move:

- pre-known-conflicts bulk-skip prelude
- per-iter cancellation check
- conflict detection (`dest.get_metadata`) + conflict-resolution dispatch (`resolve_volume_conflict`)
- per-iter skip accounting (files_done + bytes_done bump + throttled progress event)
- skip-arm `continue` before any destructive call
- post-loop bookkeeping (final progress event, completion event)

Each of the 4 operations becomes a thin wrapper that supplies an operation-specific `transfer_one_source` closure (or
function pointer). The closure does **only the per-source work**:

- Copy: stream bytes via `copy_single_path` (cross-volume) or `copy_single_item` (local-FS).
- Move: `copy_single_path` then `source.delete` (cross-volume), or `volume.rename` (same-volume).

Performance-critical hot paths stay where they are (APFS clonefile, SMB compound writes, MTP USB bulk,
`FuturesUnordered` concurrent window). The driver doesn't touch them.

## What we're NOT building

- **No change to the per-byte transfer code.** All `copyfile(3)`, `copy_file_range(2)`, `chunked_copy`,
  `smb2::FileWriter`, `MtpReadStream` paths remain exactly as they are. The driver hands the per-source closure full
  control of "how to actually move/copy this one item".
- **No generic "transfer engine framework" for hypothetical future operations** (sync, dedup, partial-resume). Those are
  deferred to when there's a concrete user-facing requirement. The driver should be the smallest abstraction that
  eliminates today's duplication, not a forward-looking framework.
- **No frontend changes.** Wire format, event shapes, IPC commands all stay identical.
- **No change to `delete.rs` / `trash.rs`.** They share some shape with copy/move but they don't have conflict
  resolution, don't take a destination volume, and lack the multi-axis bytes+files complexity. Folding them in would
  force concessions in the driver API. Track for a possible second pass.
- **No change to `move_op.rs` (local-FS move path).** This is the local-only move that handles same-fs `fs::rename` and
  cross-fs staging-then-rename, NOT a volume-level move. It also emits `write-source-item-done` at three sites and has
  its own conflict-resolution shape (file-level via `resolve_conflict` in `helpers.rs`, not `resolve_volume_conflict`).
  Migrating it would force trait-bound divergence on the driver. Out of scope; if the duplication still bothers us after
  this refactor, do it as a follow-up.
- **No async-trait migration.** That's tracked in `async-volume-trait-plan.md`. This refactor is orthogonal: the driver
  is async (uses `&dyn OperationEventSink` like `copy_volumes_with_progress`), but doesn't need a full Volume-trait
  async migration to land.

## Design decisions

### Closure-based driver, not trait-based

**Why**: A trait (`TransferStrategy` with `fn transfer_one(...)`) would need an object-safe shape and impose explicit
struct ceremony on each operation. Closures express "do this one thing" with zero ceremony, and the four operations are
sufficiently lifetime-tangled (capturing `&source_volume`, `&dest_volume`, `state`, `&mut transaction` in copy.rs) that
a closure with a precise type signature is more honest. If we later want a trait for plugin-style operations, conversion
is trivial.

### `&dyn OperationEventSink`, not `&AppHandle`

**Why**: This is the existing testable-event-emission pattern in `copy_volumes_with_progress`. Tests use
`CollectorEventSink`. Production uses `TauriEventSink::new(app)`. The driver takes the sink; the outer `*_start`
AppHandle wrappers construct the sink and own the spawn-and-cleanup. Matches the current codebase pattern; no new
abstraction.

### Driver lives in a new file: `transfer_driver.rs`

**Why**: It's cross-cutting between `copy.rs`, `volume_copy.rs`, `volume_move.rs`. Putting it in any of those biases
ownership. A new file makes its responsibility clear and matches the codebase convention (`scan.rs`, `helpers.rs`,
`eta.rs` are also single-concern files alongside the operation files).

### Driver supports both serial and concurrent paths

**Why**: `copy_volumes_with_progress` uses `FuturesUnordered` with a sliding window of `concurrency` tasks (Phase 4.2).
MTP→anything is serial (`concurrency=1`); SMB→anything is up to 10 in flight. Moves are serial throughout (cross-volume
move is copy+delete; running deletes concurrently risks confusing failure modes). Local-FS copy is serial (the macOS
`copyfile` API and `CopyTransaction` aren't trivially concurrent-safe). The driver exposes both as configurations:
`transfer_one_serial(closure)` for serial; `transfer_one_concurrent(closure, max_in_flight)` for concurrent. Migrating
volume_copy keeps `FuturesUnordered` inside the driver-concurrent path; the closure produces one Future per source and
the driver drains them with a sliding window.

### Data-safety contract in driver doc comment

**Why**: The whole point is enforcing the "continue is before destructive ops" invariant in one place. The driver's doc
comment states this explicitly as a contract: the `transfer_one_source` closure is only invoked when the source is
**NOT** in the pre-skip set and has **NOT** been resolved-as-skip by per-file conflict resolution. The driver tests
verify this property. A future refactor that violates the contract gets caught by the driver tests, not by structural
inspection of 4 different functions.

### Per-source closure receives a `TransferContext`, not many parameters

**Why**: Each closure today needs `source_path`, `dest_item`, source/dest volumes, the operation's progress callback,
state, possibly source hints, and possibly `CopyTransaction`. Passing them as positional args would be unreadable. A
`TransferContext<'a>` struct holds the references; the closure destructures what it needs. Easier to extend later (add a
field; existing closures ignore it).

### Closure bounds: serial and concurrent need different trait bounds

This deserves explicit attention because it's the single most likely source of late-stage surprise.

- **Serial driver entry point**: `transfer_one: impl AsyncFnMut(TransferContext<'_>) -> Result<u64, VolumeError>`.
  `FnMut` permits the closure to capture `&mut transaction` (`copy.rs`), `&mut tracker`, or other per-iteration mutable
  state. No `Send + Sync` required since calls are sequenced.
- **Concurrent driver entry point**: `transfer_one: impl Fn(TransferContext<'_>) -> Fut + Send + Sync`, where
  `Fut: Future<Output = Result<(PathBuf, u64), (PathBuf, VolumeError)>> + Send`. `Fn + Send + Sync` is required so
  `FuturesUnordered` can poll many in flight; per-task mutable state (e.g., `last_file_bytes: AtomicU64` from the
  current concurrent path) must be **constructed inside** the closure body per call, not captured. Today's
  `copy_volumes_with_progress` already does this — the `AtomicU64` is created inside the spawned `async move` block,
  then captured by the inner `on_file_progress`. The driver migration preserves this pattern; the per-task state stays
  inside the closure.
- **Per-task atomics** like `last_file_bytes` are _closure-local_, not captured from outside. The closure body looks
  like `let last_file_bytes = AtomicU64::new(0); ... move |ctx| async move { ... }` (or constructs the atomic at the top
  of the async block).
- **`&mut transaction` cannot be captured by a concurrent closure**. This is fine: only `copy.rs` uses `CopyTransaction`
  and it only runs serial. `copy.rs`'s closure uses the serial entry point (`AsyncFnMut`); volume copies use the
  concurrent entry point (`Fn`); moves use the serial entry point. The bound divergence aligns with the actual usage.

If `AsyncFnMut` is unstable on the target Rust version, the workaround is
`impl FnMut(TransferContext<'_>) -> impl Future<Output = ...>` with explicit boxing (`Pin<Box<dyn Future<...>>>`) — same
shape used elsewhere in this codebase. Verify on M2 step 0.

### `CopyTransaction` stays inside `copy_files_with_progress`'s closure — but only IF the abstraction fits

**Why**: Rollback is a local-FS-only concern (APFS clonefile, file deletes for partial rollback). The driver shouldn't
know about transactions. The plan is for the closure for `copy_files_with_progress` to capture `&mut CopyTransaction`
and thread it through `copy_single_item` exactly as today. On error / cancel-with-rollback, the closure short-circuits
with a typed result; the driver doesn't need transaction awareness.

**Open feasibility question**: `rollback_with_progress` (free function in `copy.rs:834`, NOT a method on
`CopyTransaction` — earlier version of this plan got that wrong) runs **after** the main copy loop returns with an error
or cancel-with-rollback intent. It uses `transaction.created_files` accumulated during the loop, AND emits its own
progress events. Today, `copy_files_with_progress` calls it from the post-loop match arm with full access to `state`,
`app`, the operation totals, and the transaction. If the driver owns the loop, the closure returns control to the driver
before the rollback runs. The driver then needs either: (a) a `on_error_or_cancel` callback the closure registers so the
closure can run its own post-loop rollback after the driver bails, or (b) accept that `copy_files_with_progress` calls
the rollback OUTSIDE the driver, immediately after `drive_transfer_serial` returns. (b) is much cleaner — the driver
returns the partial-progress state, the caller decides whether to roll back. This is the path to design first.

**Decision gate before M3**: if the closure signature for `copy.rs` requires the driver to expose hooks (`on_error`,
`on_cancel_rollback`) that are effectively "give the closure control back", the abstraction loses value for `copy.rs`
specifically. In that case, leave `copy.rs::copy_files_with_progress` **out of scope** for the driver migration. The
remaining 3 functions (volume copy, both moves) still benefit. Decide this at the end of M2 step 0 (see below), not at
M3 step 4.

### Per-iter conflict resolution stays in the driver

**Why**: This is the second-biggest piece of duplication (after bulk-skip). Stop-mode conflict events fire from inside
the driver via the sink; the driver awaits the user response via the same `oneshot` channel pattern used today.
Skip/Overwrite/Rename are decided synchronously and dispatched to the closure (Skip → driver `continue`s,
Overwrite/Rename → driver calls closure with the resolved dest path). The closure never sees Skip; it only sees
decisions where there's actual transfer work.

### `write-source-item-done` is local-FS-only; the driver doesn't try to unify it

**Why**: `copy.rs` uses `SourceItemTracker` to fire `write-source-item-done` when all files belonging to a top-level
source have been processed. `volume_copy.rs` does **not** emit this event today (confirmed: zero matches for
`write-source-item-done` in `volume_copy.rs`). The frontend's source-deselection logic works without it for volume
copies because the operation completion or per-file diffs trigger refreshes.

Earlier versions of this plan claimed the driver would "fire write-source-item-done for both shapes". That's a behavior
change, not a refactor. Skip it. The driver handles it only for the local-FS case via `SourceItemTracker`, which is a
`&mut` non-`Sync` struct that lives on the **serial** driver path only. The concurrent driver path does NOT use the
tracker (and can't — it's not `Sync`). A future maintainer who wires the tracker through the concurrent path gets a data
race; the driver's serial-path entry point owns it via `&mut self` and there's no equivalent on the concurrent entry
point.

If we ever want `volume_copy` to emit source-done events, that's a separate change with its own behavior consideration.

### Migration order: easiest first, BUT with `copy.rs` feasibility prototyped upfront

**Why**: `copy_volumes_with_progress` is already testable (takes sink) and has the most tests. Migrating it to the
driver is the lowest-risk first step. Move next, then `copy.rs` last because it's the largest function with the most
tangled state (`CopyTransaction`, dry-run handling, scan caching, E2E throttle). Each migration step is a separate,
reviewable commit.

**Reviewer flagged**: if `copy.rs` doesn't fit the driver abstraction cleanly, all earlier migrations would have locked
in an API that has to change again. Mitigation: M2 step 0 prototypes the `copy.rs` closure shape **before** the driver
API is locked. If `copy.rs` doesn't fit, it's removed from M3 scope explicitly, the remaining 3 migrations still go
ahead. This way the "easiest first" order works without late-stage surprise.

## Milestones

### Milestone 1: Extract testable inners for move and local copy

**Intention**: Get all four functions taking `&dyn OperationEventSink` before any cross-cutting work. After this
milestone, all four are individually testable; nothing's shared yet but the gap from question 1 of the conversation is
closed.

Why first: enables the test infrastructure that catches regressions during M2-M3. Doing it last means the driver
migration ships without direct tests for 3 of the 4 paths.

Steps:

1. **`volume_move.rs::move_between_volumes`**:
   - Extract `move_volumes_with_progress(events: &dyn OperationEventSink, ...)` for the cross-volume body.
   - The 6 `app.emit(...)` / `TauriEventSink::new(app.clone())` sites become `events.emit_*(...)` calls.
   - Public `move_between_volumes` keeps its AppHandle signature; the tokio::spawn body constructs `TauriEventSink` and
     calls the new inner.
2. **`volume_move.rs::move_within_same_volume`**:
   - Extract `move_within_same_volume_with_progress(events: &dyn OperationEventSink, ...)`.
   - Same pattern.
3. **`copy.rs::copy_files_with_progress`** (the big one):
   - Extract `copy_files_with_progress_inner(events: &dyn OperationEventSink, ...)`.
   - Each `app.emit(...)` site becomes `events.emit_*(...)`. Estimated 15-20 sink-emit sites; **plus** the paired
     `update_operation_status(...)` calls that interleave with most emits (status-cache updates, not events). Budget for
     ~30 total sites that touch this pairing.
   - `copy_single_item` and other helpers that take `&AppHandle`: pass them the sink instead. Helpers might need to
     accept `&dyn OperationEventSink` themselves. Audit each helper for whether it needs the full sink or just one emit
     method.
   - **`rollback_with_progress`** (free function in `copy.rs:834`, NOT a method on `CopyTransaction` — earlier plan
     revision got that wrong) takes `&tauri::AppHandle` today. Refactor to take `&dyn OperationEventSink`. **This is the
     highest-risk piece in M1** — rollback emits its own progress events through the sink, runs after the main loop, and
     is reentrant via the `CopyTransaction`'s `Drop` impl as a panic safety net. The reentrant path doesn't go through
     this function (it uses `transaction.rollback()`, the synchronous error-path rollback), so the sink refactor is
     bounded to the explicit-progress path.
   - Mirror the existing pattern: `volume_rollback_with_progress` in `volume_copy.rs:1157` already takes the sink, exact
     shape to follow.
   - Public `copy_files_start` (in `mod.rs`) keeps its AppHandle signature; constructs `TauriEventSink` and calls the
     inner.
4. **Add direct tests for each extracted inner**:
   - Move tests in `volume_copy_tests.rs` or a new `volume_move_tests.rs` (mirroring `volume_copy_tests.rs`). Use
     `CollectorEventSink` + `InMemoryVolume`. Cover: happy-path move, conflict + Skip, conflict + Overwrite, conflict +
     Stop + auto-resolve via oneshot, pre-known-conflicts bulk-skip (the test we already wanted), cross-volume
     copy+delete correctness on per-file partial failure.
   - Real-FS move integration test (`LocalPosixVolume` on tmpfile) for the cross-volume path.
   - Local-FS copy tests for `copy_files_with_progress_inner`: existing `copy_integration_test.rs` may already cover
     much of this; gap-fill where bulk-skip / per-iter-skip-accounting / data-safety properties aren't exercised. Add
     the pre-known-conflicts test specifically (mirroring `test_pre_known_conflicts_bulk_skip_on_real_local_volumes`).
5. **Verify**: `./scripts/check.sh` green. `cargo mutants` (per `write_operations/CLAUDE.md` testing bar) on the new
   inner functions; mutation score should match the existing 85-90%.

**Parallelism note**: Steps 1, 2, 3 can NOT safely run in parallel (they all touch `mod.rs::*_files_start` and adjacent
helpers). Step 4 sub-items (the new tests) can be added in parallel commits if convenient, but sequential is fine.

**Estimated scope**: ~400-600 lines changed plus ~300 lines of new tests.

### Milestone 2: Feasibility prototype, driver, AND first migration

**Intention**: Don't lock in the driver API until we've proven it can host the hardest case (`copy.rs` with
`CopyTransaction` rollback). Earlier reviewer pass flagged that shipping `copy_volumes_with_progress` first (cleanest
case) would only surface `copy.rs` problems at M3 step 4, **after** the driver API is locked. The fix: prove `copy.rs`
fits up front. M2 also co-lands the first migration (`copy_volumes_with_progress`) so the driver isn't dead code
awaiting M3.

Steps:

1. **Step 0 (feasibility prototype, throwaway code)**:
   - In a feature branch, prototype `copy_files_with_progress`'s closure signature against the proposed driver API.
     Don't migrate; just write the closure body and verify it type-checks with the closure bounds in the "Closure
     bounds" section above.
   - Specifically prove: (a) the closure can capture `&mut transaction` and `&mut created_dirs` under `AsyncFnMut`; (b)
     the post-loop rollback flow works EITHER by closure-registered hook OR by the outer caller invoking
     `rollback_with_progress` after `drive_transfer_serial` returns the partial-progress state; (c) the closure can
     interact with `SourceItemTracker` correctly.

   **Step 0 exit criteria (concrete, not judgment-call)**:
   - (a) Pass = a `cargo check`-clean prototype where the closure type-checks against the driver entry-point bound AND
     captures `&mut transaction`, `&mut created_dirs` under the chosen `AsyncFnMut` (or fallback `FnMut + Future`)
     bound. Fail = compiler rejects the captures without forcing the closure into a more permissive bound that breaks
     the concurrent path.
   - (b) Pass = a runnable example (in a `tests/` or scratch file) where the outer caller invokes
     `rollback_with_progress` after the driver returns with a non-Ok intent, using `transaction.created_files`
     accumulated during the loop. Fail = the driver would need to expose escape-hatch callbacks (e.g., `on_error`,
     `on_cancel_rollback`) that are basically "give the closure control back".
   - (c) Pass = the closure calls `tracker.record(file_info)` and the resulting `Some(source)` produces a
     `write-source-item-done` emit at the correct timing (when all FileInfos for a given top-level source are done, not
     before). Fail = the timing requires the driver to know about the tracker, which would couple the driver to local-FS
     specifics.
   - **All three (a/b/c) must pass.** Any single fail → `copy.rs` is removed from M3 scope (M3 step 4 dropped), the
     remaining 3 migrations proceed with `copy_files_with_progress_inner` retained as a standalone testable inner from
     M1.

2. **Step 1: Create `transfer_driver.rs`**.
3. **Define `TransferContext<'a>`** carrying `source_volume`, `dest_volume`, `state`, `operation_id`, `operation_type`,
   `source_hints`, plus any other references shared across closures.
4. **Define `TransferOutcome`** (success-with-bytes, skip, error) as the closure's return type. Skip is reported
   explicitly (not via the closure being skipped) so the driver can do its bookkeeping.
5. **Define driver entry points** (note: the concurrent entry point may be cut — see "Concurrent driver scope" below):
   - `drive_transfer_serial(events, ctx, source_paths, config, transfer_one)`: serial for-loop with bulk-skip, per-iter
     conflict resolve, cancellation, progress, completion.
     `transfer_one: impl AsyncFnMut(TransferContext<'_>) -> Result<TransferOutcome, VolumeError>`.
   - `drive_transfer_concurrent(events, ctx, source_paths, config, max_in_flight, transfer_one)`: `FuturesUnordered`
     sliding window. `transfer_one: impl Fn + Send + Sync` with the future-returning shape from the "Closure bounds"
     section. Conflict resolution still happens **synchronously on the driver** (per Phase 4 Fix 14 — the whole batch
     blocks on one Stop prompt instead of racing per-task prompts).
6. **`update_operation_status` interleaving**: every existing emit site calls both `state.emit_progress_via_app(...)`
   (or `_sink`) AND `update_operation_status(...)` for the in-process status cache. The driver does both, side by side,
   at every emit point. The plan's "15-20 emit sites" undercount in M1 step 3 includes this pairing; budget for it. The
   driver hides this pairing inside its own emit helpers so the closure never has to remember.
7. **Write driver-only tests in `transfer_driver_tests.rs`**:
   - **Data-safety properties** (the most critical class): the `transfer_one` closure is NEVER invoked for a pre-skipped
     source; the closure is NEVER invoked when conflict resolution returned Skip; on cancel, no new closure invocations
     after the cancel point.
   - **Progress accounting**: pre-skip + per-iter skip + completed transfers sum correctly; total emitted bytes matches
     sum of all sources; final completion event shape.
   - **Conflict resolution**: Stop emits write-conflict event and awaits oneshot; Skip/Overwrite/Rename don't fire
     write-conflict; apply-to-all latches correctly.
   - **Concurrency**: parallel transfers don't double-count; conflict resolution serializes per Phase 4 Fix 14.
   - **Cancellation**: cancellation between sources is honored; cancellation during a transfer is propagated through the
     closure.
   - **Status cache parity**: every emitted progress event has a matching `update_operation_status` call (test by
     injecting a status-cache spy).
8. **Co-land M2 with M3 step 1** to validate the API has a real production user, rather than landing the driver as 1500+
   lines of dead code awaiting M3. Concretely: M2's PR includes the driver + tests + the `copy_volumes_with_progress`
   migration. This catches API-design issues earlier and keeps the diff reviewable in context.
9. Run `cargo mutants` on `transfer_driver.rs`; aim for 90%+ mutation score (this is data-critical code; the testing bar
   is high).

#### Concurrent driver scope

Reviewer flagged: only `copy_volumes_with_progress` uses `FuturesUnordered`. Moves and local-FS copy are serial. If
`drive_transfer_concurrent` exists for one caller, it's a 1-of-4 abstraction, not a shared pattern.

Decision: design the driver with **a serial entry point as the primary API**. The concurrent path can either be:

- (a) **Kept inline in `copy_volumes_with_progress`** — the function uses `drive_transfer_serial` for the slow /
  fallback path AND retains its current `FuturesUnordered` block for the concurrent path. This makes the driver smaller
  and avoids the trait-bound divergence; the concurrent block already lives in one place and isn't truly duplicated.
- (b) **Added as `drive_transfer_concurrent`** if M2 step 0 proves the bound divergence is clean and the abstraction
  earns its weight.

M2 step 0 picks one. **Pass/fail criterion for the concurrent decision**: option (b) is only picked if the
`Fn + Send + Sync` bound is provably clean for `copy_volumes_with_progress`'s closure shape (verify by writing the
closure body in the same prototype branch). Any `unsafe`-style escape hatches, `RwLock<Closure>` wrapping, or "the
closure can't be `Fn` because it captures X" → fall back to option (a). **If unsure, default to (a)** (less code, less
abstraction surface, the concurrent block is a known shape and an in-place duplication of ~80 lines is not the problem
the user is paying us to solve).

**Estimated scope** (assuming concurrent stays inline per option (a)): ~400-600 lines of new code + ~600-800 lines of
tests.

### Milestone 3: Migrate the four operations to the driver, one at a time

**Intention**: Convert each operation in a separate commit so the impact is reviewable and `git bisect` is meaningful if
anything regresses. Migration order is easiest-first.

Each step is a separate commit with `./scripts/check.sh` green.

Steps:

1. **`copy_volumes_with_progress` → driver** (lowest risk: already takes sink, has the most tests). **Note**: this
   migration **lands as part of M2's PR** (per M2 step 8), not as a standalone M3 commit. Listed under M3 here for
   completeness of the migration matrix.
   - **If concurrent driver exists (M2 option (b))**: concurrent path → `drive_transfer_concurrent`, closure is the
     existing `copy_single_path` call.
   - **If concurrent stays inline (M2 option (a), the default)**: serial path → `drive_transfer_serial`; concurrent path
     keeps its current `FuturesUnordered` block (the duplication left in place is the per-task progress emit, which is
     already a single inlined block).
   - Bulk-skip prelude / skip-arm / per-iter conflict resolution: deleted (now in driver).
   - All 29+ existing tests must pass. Run `cargo mutants` after to ensure mutation score didn't drop.
2. **`move_between_volumes` → driver** (medium risk: tests added in M1, smaller emit surface than copy.rs).
   - Cross-volume → `drive_transfer_serial`, closure is `copy_single_path` + `source.delete` (with the existing
     `delete_volume_path_recursive` for directories).
   - The per-iter skip accounting + bulk-skip prelude that I added: deleted.
   - The latent-bug fix (per-iter Skip arm bumping `files_done`): becomes intrinsic to the driver, not a per-call thing.
3. **`move_within_same_volume` → driver** (medium risk, simpler shape).
   - Closure is `volume.rename(...)`.
4. **`copy_files_with_progress_inner` → driver** (highest risk: largest function, tangled with `CopyTransaction`,
   dry-run, scan caching, E2E throttle). **CONDITIONAL on M2 step 0 outcome** — if `copy.rs` doesn't fit the driver,
   skip this step and leave `copy_files_with_progress_inner` as a standalone testable inner from M1.
   - The closure receives the `TransferContext` plus per-call mutable refs to `transaction` and `created_dirs` (these
     are local-FS-only concerns that don't belong in `TransferContext`).
   - Pre-flight scan and dry-run handling stay OUTSIDE the driver (they're pre-loop concerns, not per-iter).
   - `SourceItemTracker` lives on the driver's **serial** entry point only (tracker is `!Sync`). The driver's serial
     entry exposes a `mark_source_done(path)` callback; for local-FS, the closure threads through the tracker and calls
     back when `tracker.record(file_info)` returns `Some(source)`. Volume copies don't use this (they don't emit
     `write-source-item-done` today).
   - **Highest-risk substep**: the rollback path. `rollback_with_progress` (free fn in `copy.rs:834`) emits progress
     events through the sink AND `CopyTransaction` is reentrant via `Drop` as a panic safety net (the Drop path uses the
     sync `transaction.rollback()`, not the progress variant). After this migration, the post-loop rollback must run
     somewhere the closure or its caller controls — likely the cleanest shape is: driver returns
     `TransferLoopOutcome { files_done, bytes_done, intent }`, and `copy_files_with_progress_inner` (the outer caller)
     decides whether to invoke `rollback_with_progress` based on `intent`. The driver doesn't need to know about
     transactions at all.
   - **Transaction ownership clarification**: `CopyTransaction` is owned by `copy_files_with_progress_inner` across both
     the driver call and the post-loop rollback. The closure captures it through `&mut` (via the `AsyncFnMut` bound).
     The driver never sees the transaction. Today's structure (post-loop match on `intent`, calling
     `rollback_with_progress` with the transaction's accumulated state) is preserved exactly. The migration is
     materially the same shape as today; the only change is that the driver owns the loop instead of inline code.
   - Real-FS integration tests (`copy_integration_test.rs`) must all pass.
5. **Sanity sweep**:
   - `./scripts/check.sh --include-slow` (covers E2E, including the cancellation-during-copy and rollback E2E specs).
   - `cargo mutants` on all four migrated functions plus the driver. Mutation score should match or exceed pre-refactor
     levels.
   - Manual QA matrix: MTP→SMB copy with conflicts (the original repro), local→local copy with conflicts, MTP→SMB move
     with conflicts, same-MTP rename, cancel-mid-copy, rollback-mid-copy.

**Parallelism note**: NOT parallel. Each step depends on the driver being stable from M2 and on each previous migration
being merged.

**Estimated scope**: ~500-800 lines deleted (the duplicated scaffolding), ~200 lines added (per-operation closures and
wrappers).

### Milestone 4: Cleanup + docs

**Intention**: Wrap up. Doc updates, dead-code removal, mutation-test gap-fills.

Steps:

1. **Update `write_operations/CLAUDE.md`**:
   - Add a "Transfer driver" section explaining the shared scaffolding and the closure-based per-operation override.
   - Update the "Files" table with `transfer_driver.rs`.
   - Document the data-safety contract (the "closure is never invoked for skipped sources" invariant).
   - Update the "Key decisions" with the driver rationale.
2. **Remove dead code**: any per-function helpers (`account_skipped_file` in volume_copy.rs, etc.) that the driver now
   subsumes.
3. **Update `volume/CLAUDE.md`** if any Volume trait usage changed during migration (likely minor).
4. **File-length check**: After deletion, files that shrank below their allowlist entries get their entries LOWERED in
   `file-length-allowlist.json` (per the file-length-allowlist rule, lowering is the only allowlist change that doesn't
   need explicit consent). Files affected:
   - `volume_copy.rs` and `volume_copy_tests.rs`: definitely shrink after the volume-copy migration (M2).
   - `volume_move.rs`: definitely shrinks after the move migration (M3 steps 2-3).
   - `copy.rs`: only shrinks if M3 step 4 happened. Skip the allowlist update for `copy.rs` if step 4 was dropped.
5. **CHANGELOG / commit notes** if applicable.

**Parallelism note**: Doc updates are independent and can be parallel commits.

**Estimated scope**: ~100-200 lines of docs + small code cleanups.

## Testing strategy

Per layer:

- **Unit**: `transfer_driver_tests.rs` (data-safety, progress accounting, conflict resolution, concurrency,
  cancellation). `InMemoryVolume`-based. ~20-30 tests.
- **Integration (in-memory)**: `volume_copy_tests.rs` updates verify all 4 operations work end-to-end via the driver
  with `CollectorEventSink`. Existing tests should require no rewrites if the driver preserves event shapes.
- **Integration (real-FS)**: One test per operation against `LocalPosixVolume` on tmpfile. Cross-volume cases use
  `LocalPosixVolume` + `InMemoryVolume` to simulate the Volume trait dispatch.
- **Cross-volume integration (Docker SMB)**: `volume/smb.rs` has `#[ignore]` tests requiring Docker SMB containers. The
  migration should not regress these; run with
  `apps/desktop/test/smb-servers/start.sh && cargo nextest run --run-ignored ignored-only`.
- **E2E**: Playwright/WebDriver tests cover cancel-mid-copy, rollback-mid-copy, "Skip all" UX. Run with
  `./scripts/check.sh --include-slow`.
- **Mutation**: `cargo mutants --file <each migrated file>` after each M3 step. Aim for 85-90% mutation score on
  `transfer_driver.rs` and the migrated functions (matches the existing bar for `write_operations/`).

## Risks and mitigations

1. **Closure type signatures are gnarly.**
   - Risk: Captured `&mut state` or `Arc<Mutex<_>>` with conflicting lifetimes; Send + Sync bounds on the closure for
     concurrent path.
   - Mitigation: Prototype the closure signature for `copy_volumes_with_progress` first (it's the cleanest case). Use
     `Arc` for shared state. Pin down `Send + Sync + 'a` bounds explicitly in the driver's generic constraints. Document
     the bounds in the driver's doc comment so future operations know what their closure must satisfy.

2. **`CopyTransaction` rollback during cancel.**
   - Risk: Rollback emits its own progress events. If the driver controls the loop and the closure controls the
     transaction, rollback may need to call back into the driver to emit "rollback progress" events.
   - Mitigation: Design the rollback flow in M2 before any migration. Likely shape: driver exposes a
     `RollbackProgressEmitter` to the closure (a thin wrapper over the sink), and on cancel-with-rollback the closure
     drives its own rollback emit loop. Document the contract.

3. **Concurrent path semantics regression.**
   - Risk: `FuturesUnordered` sliding window + per-task progress emission is delicate (Phase 4 Fix 14). Moving it into
     the driver might subtly change ordering or throttling behavior.
   - Mitigation: Keep the concurrent driver's logic line-for-line equivalent to the current `copy_volumes_with_progress`
     concurrent path. Verify via the existing `test_concurrent_copy_*` tests (5+ tests). Add a "concurrent driver
     doesn't reorder events differently" test.

4. **`SourceItemTracker` semantics differ between volume and local-FS.**
   - Risk: Volume ops iterate top-level paths; local-FS iterates per-file `FileInfo`. Folding both into the driver risks
     getting the `write-source-item-done` timing wrong for one shape.
   - Mitigation: The driver exposes a `mark_source_done(path)` callback that the closure invokes when a top-level source
     is genuinely complete. For volume ops, the closure calls it once per source. For local-FS, the closure threads
     through `SourceItemTracker::record` and calls `mark_source_done` only when the tracker says all files for a source
     have landed. Test both shapes explicitly.

5. **E2E regression on cancel + rollback specs.**
   - Risk: Playwright tests for cancel-during-copy depend on event ordering and timing. Driver changes could flake them.
   - Mitigation: Run `./scripts/check.sh --include-slow` after each M3 step, not just at the end. If a flake appears,
     isolate to that migration step before proceeding.

6. **Mutation score drop after migration.**
   - Risk: New mutations slip through if the consolidated code has weaker test coverage than the per-function originals.
   - Mitigation: Run `cargo mutants` after each M3 step. Fill gaps immediately, not at the end. The existing
     per-function mutation tests in `volume_copy::tests::tests` plus the new driver tests should compose to a higher
     overall score, but verify.

7. **Refactor scope creep.**
   - Risk: While in the area, it's tempting to clean up adjacent concerns (the `_with_progress` method-variant noise on
     the Volume trait, the `Condvar` → channel migration tracked in `async-volume-trait-plan.md`, etc.).
   - Mitigation: Strict scope. This refactor changes ONLY the loop scaffolding, not the Volume trait, not the
     conflict-resolution channel, not the per-byte copy strategies. Each adjacent concern stays its own plan.

## Estimated total scope

- M1: ~400-600 lines changed + ~300 lines of tests
- M2: ~400-600 lines new (driver, assuming concurrent stays inline per option (a)) + ~600-800 lines of driver tests;
  co-lands the `copy_volumes_with_progress` migration so the API has an immediate user
- M3 (assuming `copy.rs` fits the abstraction): ~500-800 lines deleted (scaffolding duplication) + ~200 lines added
  (closures + wrappers). If `copy.rs` is dropped from M3 scope after M2 step 0, subtract ~300 lines of deletion and skip
  step 4.
- M4: ~100-200 lines of docs + cleanup

**Total**: 12-19 hours of focused work for an experienced contributor. Plan for ~2 working days, with the assumption
that M2 step 0 (`copy.rs` feasibility prototype) may surface enough complexity that `copy.rs` gets removed from M3
scope. That outcome is fine; the 3 remaining migrations still eliminate most of the duplication and provide direct test
coverage where there was none.

## Open questions for the implementer

- Should `delete.rs::delete_volume_files_with_progress` also adopt the driver in a follow-up? It has no
  conflict-resolution branch but DOES have skip semantics (skip-on-error). Not in scope here; flag if the refactor
  surfaces an obvious win.
- Should the bulk-skip set construction (`build_pre_skip_set`) be exposed as a public test-only helper so consumers of
  the driver can audit it independently? Probably yes, behind `#[cfg(test)]`.
- Concurrent driver's `max_in_flight: usize` parameter: should it come from `Volume::max_concurrent_ops()` (current
  behavior) or be a driver-level config knob? Current behavior is correct; the driver should accept the caller's chosen
  value and not duplicate the `min(src, dst, 32)` logic.
