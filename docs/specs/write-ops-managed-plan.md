# Write operations: managed instant ops, sink lift, and debt sweep

Three improvements to `src-tauri/src/file_system/write_operations/` and its command/frontend edges. Do all three.
Worktree: `.claude/worktrees/write-ops-managed`, branch `write-ops-managed`.

Order: M1 (debt, clears the ground) → M2 (sink lift, no behavior change, sets up M3) → M3 (the behavior change:
rename/mkdir/mkfile become managed ops).

## Why this, and the guiding intent

`rename_file`, `create_directory`, and `create_file` are today standalone sync-ish `#[tauri::command]`s
(`commands/rename.rs`, `commands/file_system/write_ops.rs`) that bypass the whole write-operations subsystem: no
`operation_id`, no manager registry, no busy-volume/eject guard. Meanwhile copy/move/delete/trash all flow through
`manager::spawn_managed`. That split means: renaming or creating a folder on a mounted USB/MTP/SMB volume doesn't mark
the volume busy (you can eject mid-rename), the op is invisible to the queue window, and the command files carry real
business logic (validation helpers, downloads-watcher registration, listing notify) that the "smart backend / thin
frontend" principle says belongs in a `file_system` module.

The tension to respect: rename/mkdir/mkfile are **instant, scan-free, result-returning** ops. The inline-rename editor
and the new-file/new-folder dialogs depend on getting the result back synchronously (the new path for cursor placement
and editor-open; conflict/timeout/success for the rename dialog flow) and on **snappy validation feedback**. So the
design must NOT turn them into fire-and-forget event-driven ops like copy/move. The mutation gets *wrapped* by the
manager (registry + busy set + operation_id) but still **runs inline and returns its result**. Validation IPC
(`check_rename_validity`, `check_rename_permission`) and the FE-side conflict pre-checks (`findFileIndex`) stay exactly
as they are — snappy, unmanaged.

---

## M1 — Small-debt sweep

Low-risk cleanup, first so M2/M3 build on tidy ground.

### M1.1 Remove the deprecated `WriteOperationConfig.overwrite`

`overwrite: bool` (types.rs ~488) is superseded by `conflict_resolution`. The same effect is reachable via
`conflict_resolution: ConflictResolution::Overwrite`. Nothing in the frontend sets it (verified: no `overwrite:` /
`config.overwrite` in `apps/desktop/src` TS/Svelte). Serde ignores unknown incoming fields by default, so dropping it is
wire-compatible.

- Remove the field from the struct and its `Default`.
- Remove the `else if config.overwrite { ConflictResolution::Overwrite }` branch in `conflict.rs:128` (the
  `apply_to_all_effective` → `config.conflict_resolution` fallthrough remains).
- Fix the `tests.rs` references, exactly: `test_config_deserialization` (~54–61) exists *to* deserialize
  `{"overwrite": true}` — delete that test wholesale. The `assert!(!config.overwrite)` at ~50 and ~68 are line removals
  inside otherwise-valid tests. Keep the `{"conflictResolution": "overwrite"}` case (~78) — that's
  `ConflictResolution::Overwrite`, unrelated.
- Regenerate bindings (`pnpm bindings:regen`); `WriteOperationConfig.overwrite` drops from `bindings.ts`.

Tests: existing `conflict.rs` + `tests.rs` suites must stay green. No new behavior; TDD not warranted (this is a
deletion). Verify no caller relied on the shortcut by a full `pnpm check rust`.

### M1.2 Fix the stale "TEMPORARY … milestone 2" comment (conflict.rs ~179)

The comment says `blocking_recv` is "TEMPORARY … Will become rx.await in milestone 2 full async migration." That
migration isn't planned; `blocking_recv` is the correct, long-lived choice here (this branch runs inside
`spawn_blocking`). Rewrite to describe the current state and the WHY (per `describe-current-not-history`): the local-FS
conflict path is synchronous and runs on the blocking pool, so it blocks the thread on the oneshot; the volume path
(async) uses `rx.await`. Drop the milestone framing.

### M1.3 Tidy the compat re-export sprawl (judgment call)

`state.rs` re-exports `operation_intent` + `scan_cache` types; `types.rs` re-exports `event_sinks` + `error_classification`
types, so `state::…` / `types::…` paths keep resolving. This was churn-avoidance during earlier splits. Decide per the
`ideal-over-cheap` rule, but bounded: only collapse a re-export if the fix is a clean import-path update at the callers,
not a sprawling touch-every-file churn. Document what was changed and what was deliberately left (with the reason) in
the `write_operations/DETAILS.md` module inventory. If the churn/benefit ratio is bad, leave it and record the decision.
Do NOT bump any file-length allowlist to accommodate this — flag instead.

**Commit** M1 as its own increment ("Write ops: drop deprecated `overwrite` config flag and tidy stale comments").

---

## M2 — Lift the event sink above the starters

**Goal:** the managed path (`start_write_operation`, the starters, the volume entry points, `WriteSettledGuard`) becomes
sink-injectable; `TauriEventSink` is constructed ONLY at the IPC edge (`commands/file_system/write_ops.rs`). No behavior
change — pure dependency inversion. This is the precondition that lets M3's new instant-op path be sink-injectable and
testable from birth, and it removes the last Tauri coupling from the orchestration layer.

### Current coupling (what to change)

`TauriEventSink::new(...)` is constructed *inside* the orchestration layer at these sites:
- `mod.rs:314` (copy handler), `mod.rs:526` (trash handler — already wraps in `Arc<dyn OperationEventSink>`).
- `transfer/move_op.rs:134` (`move_files_with_progress` builds it from `&app`).
- `transfer/volume_copy.rs:245`, `transfer/volume_move.rs:177`, `:693` (volume copy/move build it from `app`).
- `delete/walker.rs:30`, `:579` (delete builds it from `app`).
- `WriteSettledGuard::new` (state.rs:169) takes `app` and emits `write-settled` via `app.emit`; there's already a
  `#[cfg(test)] new_with_sink` + an `EmitSink::App | Sink` enum.

### The change

1. **`WriteSettledGuard`**: collapse `EmitSink` to a single `Arc<dyn OperationEventSink>` field (drop the `App` variant
   and the `#[cfg(test)]` gate). Production constructs it with `Arc::new(TauriEventSink::new(app))` at the edge; the guard
   calls `sink.emit_settled(event)`. Make `emit_settled` a **required** trait method (drop the `#[allow(dead_code)]`
   no-op default at `event_sinks.rs:123-127`) so a future sink can't silently swallow settle — `TauriEventSink` and
   `CollectorEventSink` already implement it.

2. **`start_write_operation`** (mod.rs): take `events: Arc<dyn OperationEventSink>` instead of `app: tauri::AppHandle`.
   The deferred future uses the sink for the settle guard and the write-error safety-net emits (`sink.emit_error(...)`
   instead of `app.emit("write-error", ...)`). The handler closure receives/captures the sink instead of `app`.

3. **The starters** (`copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`): take
   `events: Arc<dyn OperationEventSink>` instead of `app`. Their handler closures pass the sink straight into the
   `*_with_progress` functions (no internal `TauriEventSink::new`).

4. **The `*_with_progress` functions** that still build a sink internally (`move_files_with_progress`,
   `delete_files_with_progress`, `delete_volume_files_with_progress`, `copy_volumes_with_progress`, the volume-move
   equivalents): change to take `&dyn OperationEventSink` / `Arc<dyn OperationEventSink>` and drop the internal
   construction. (Copy's inner + trash already take a sink — match that shape everywhere.)

5. **Volume entry points** (`copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`): take the sink
   instead of `app`. Their both-local branch that calls `copy_files_start`/`move_files_start` passes the sink through.

6. **The IPC edge** (`commands/file_system/write_ops.rs`: `copy_files`, `move_files`, `delete_files`, `trash_files`):
   build `let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));` and pass it to the starters.
   This is the ONLY production `TauriEventSink::new` site after the lift (plus the settle guard's, also at the edge).

7. `emit_completion_analytics` stays inside `TauriEventSink::emit_complete` (it's the production sink's job); untouched.

### Watch-outs

- The write-error safety net in the deferred (`mod.rs` `Ok(Err(e))` / `Err(join_error)` arms) currently uses
  `app.emit("write-error", ...)`. Route through `sink.emit_error(WriteErrorEvent::new(...))`. Double-emit stays harmless
  (FE removes listeners on first receipt).
- `AppHandle` is still needed by a few non-event things? Verify: the starters use `app` ONLY for the sink. If any path
  needs `app` for something else (e.g. `run_on_main_thread`), keep that thread but pass `app` explicitly there. Confirm
  by compiling.
- Keep `init_operation_event_emitter` / `init_busy_volume_emitter` as-is (those emit manager/busy events via a stored
  app handle, a separate concern from the per-op sink).

### Tests

No behavior change, so existing suites are the proof. The payoff: the managed spawn path can now be driven end-to-end
with `CollectorEventSink`. Add ONE test that drives `start_write_operation` (or a starter) with a `CollectorEventSink`
through a trivial local copy and asserts the settle event arrives via the sink (previously only reachable via the
`#[cfg(test)] new_with_sink`). Run `pnpm check rust` + the write_operations unit tests; `cargo mutants` not required for
a pure inversion, but run the existing `settle_event_tests` + `tests` modules.

**Commit** M2 ("Write ops: inject the event sink at the IPC edge, decoupling the managed pipeline from Tauri").

---

## M3 — Route rename / mkdir / mkfile through the manager

The behavior change. Split into M3a (move logic into modules), M3b (the managed-instant path), M3c (wire + FE + docs).

### The frontend contract (the crux — decide once, here)

**The mutation commands stay request/response and keep returning their result.** `rename_file` still returns
`Result<(), IpcError>` (the FE maps timeout/conflict/success as today); `create_directory`/`create_file` still return the
new path `String`. The FE rename/mkdir/mkfile flows are **unchanged** — they call the same wrappers and consume the same
returns. What changes is purely backend: the mutation is now *wrapped* by a manager registration so it (a) gets an
`operation_id`, (b) marks its volume busy for its (sub-second) duration so eject can't fire mid-mutation, and (c) appears
briefly in the queue window's `operations-changed` snapshot.

**Validation stays unmanaged and snappy.** `check_rename_validity`, `check_rename_permission`, and the dialogs'
FE-side `findFileIndex`/`getFileAt` conflict pre-checks do NOT go through the manager. They're read-only, must feel
instant, and run per-keystroke / on-commit. Only the actual mutating syscall is wrapped.

**Instant ops don't reserve a lane or queue behind transfers.** The lane system exists to stop two big *transfers*
thrashing one device. A metadata syscall (rename/mkdir/mkfile) must not queue behind a multi-minute copy — an inline
rename that hangs until its 5 s IPC timeout is worse than useless, and the MTP/SMB connection layer already serializes
physical access. So instant ops register (status `Running`), mark their volume busy, run inline, and clean up — WITHOUT
lane reservation or an admission gate. This is the key semantic decision; document it in `write_operations/DETAILS.md`.

**How they surface in the queue window:** as a `Running` row that goes terminal and is pruned almost immediately (the
store already prunes terminal rows and the backend removes the record on completion). For a ~50 ms local rename the row
may never render before it's pruned; for a slow MTP rename it shows "Renaming… / running" with no progress bar (the row's
`fraction` is null → just label + spinner, which the existing `QueueRow` already handles for the warm-up window). This is
coherent with existing near-instant-op behavior. Local `root` ops cause NO busy-set churn (`root` is excluded from the
busy set), so inline-renaming 50 local files won't flicker the eject menu; only volume ops mark busy.

### M3a — Move rename + create logic into `file_system` modules (commands become thin)

Create the module homes so the command files end up as thin pass-throughs (per `commands/CLAUDE.md`: "No business logic
here").

- **New `write_operations/rename.rs`** (module, not the `delete::trash` re-export — pick a non-colliding name if needed;
  `rename` is free at this level). Move from `commands/rename.rs`: `check_rename_permission_sync`, `check_dir_writable`,
  `check_macos_flags`, `check_sibling_conflict` (both cfg arms), `check_sibling_conflict_via_volume`,
  `check_rename_validity_impl`, `notify_rename_in_listing`, and the DTOs `RenameValidityResult` / `ConflictFileInfo`.
  Expose: the validation entry points (permission check, validity check) and a **managed rename mutation** entry point
  (M3b). Keep the DTOs `#[derive(specta::Type)]` so bindings still generate; the command re-uses them.
- **New `write_operations/create.rs`** (or fold into an existing suitable module). Move `create_directory_core` /
  `create_file_core` from `commands/file_system/write_ops.rs`. Expose a **managed create-dir / create-file** entry point
  (M3b) that returns `(PathBuf, expanded_path)` like today. Decide where `emit_synthetic_entry_diff` /
  `should_emit_synthetic_diff` live: they're a listing-cache update concern. Move them out of the raw command into the
  module (co-located with the create op) OR into `file_system/listing`; either is fine as long as the command file
  becomes a thin wrapper. Document the choice.
- **The command files** (`commands/rename.rs`, `commands/file_system/write_ops.rs`): shrink to thin `#[tauri::command]`
  wrappers that expand tilde, resolve `volume_id`, call the module's managed entry point (wrapped in the existing
  timeout tiers — 2 s for validity/permission, 5 s for rename/create), and map errors to `IpcError`. No validation
  helpers, no `note_pending_write_for_cmdr`, no `notify_mutation` at this layer anymore — those move into the module with
  the logic they guard.
- `move_to_trash` (commands/rename.rs) is a separate concern (already delegates to `trash::move_to_trash_sync`); leave
  it, or move it alongside if trivial. Not in scope to change its behavior.

Keep every existing unit test passing. The `commands/rename.rs` `#[cfg(test)] mod tests` that exercises
`check_rename_validity` / `rename_file` / `move_to_trash` end to end can stay at the command layer (they drive the
public command surface) OR move with the logic; prefer keeping command-surface tests at the command and adding
module-level tests for the moved helpers. TDD: for the moved pure helpers, the existing tests are the safety net (they
must stay green through the move — a real regression check).

### M3b — The managed-instant execution path

Add a manager entry point for scan-free, near-instant, result-returning ops:

```rust
// manager.rs (sketch — refine during execution)
impl OperationManager {
    /// Runs a scan-free, near-instant op inline under manager bookkeeping:
    /// registers a Running record (so it shows in the queue + gets an id),
    /// marks its volumes busy (eject guard), awaits `op`, then frees. Does NOT
    /// reserve a lane or gate on admission — a metadata syscall must not queue
    /// behind a transfer. Returns the op's own result to the caller.
    pub(crate) async fn run_instant<T>(
        &'static self,
        descriptor: OperationDescriptor,
        op: impl Future<Output = T>,
    ) -> T { /* register(Running) + register_operation_status + emit_changed;
               let _guard = ManagedTaskGuard::new(id);   // RAII net (see below)
               let r = op.await;
               free_and_remove(id) + emit_changed; _guard.disarm(); r */ }
}
```

Notes:
- **RAII cleanup is mandatory, not happy-path only.** The command still wraps this in a 5 s `tokio::time::timeout`
  (M3a keeps the tier), so a slow MTP/SMB rename that exceeds 5 s makes the timeout **drop the `run_instant` future
  mid-`op.await`** — and the async volume path can also panic. Either exit must still free the record + unregister the
  busy status, or the eject guard sticks ON forever (the volume can never be ejected again) and a phantom `Running` row
  lingers. Mirror the spawn path: hold a `ManagedTaskGuard` (its `Drop` already does
  `free_and_remove` → `unregister_operation_status` → `recompute_and_emit_busy_volumes`) across the `op.await`, and
  `disarm()` it on the happy path right after the explicit `free_and_remove`. Consider an instant-specific guard that
  ALSO calls `emit_changed` on drop so the queue snapshot re-emits on the drop path too (cosmetic; the busy release is
  the load-bearing part). This is the single most important correctness point in M3 — pin it with a test (see M3 tests).
- Reuse `free_and_remove` for the happy-path cleanup (it releases the empty `reserved_lanes`, removes the record,
  unregisters busy, and drops any `WRITE_OPERATION_STATE` entry — harmless if absent). Do NOT run an admission pass on
  completion (instant ops reserve no lanes, so nothing waits on them).
- Instant ops need NO `WriteOperationState` (no intent/pause/conflict oneshot), so `run_instant` doesn't insert one.
  Consequence: `cancel_operation` on an instant op → `cancel_if_queued` false (it's Running) → falls through to
  `cancel_write_operation(id, false)` which finds no state → safe no-op. Acceptable (instant ops finish before a human
  can cancel). Optionally hide the cancel affordance for instant op types in `QueueRow` (polish; note but don't gate on).
- **New `WriteOperationType` variants**: `Rename`, `CreateFolder`, `CreateFile` (names TBD; keep them queue-label
  friendly). They must be added to the Rust enum → they flow to `bindings.ts` via specta. Handle the new arms wherever
  `WriteOperationType` is exhaustively matched: `analytics::emit_completion_analytics` (instant ops emit no completion
  analytics → add explicit no-op arms, not a catch-all, so a future op type can't silently skip analytics), and any
  other exhaustive match (grep `match .*operation_type` / `WriteOperationType::`). Instant ops do NOT emit
  `write-progress` / `write-complete` / `write-error` events (the command return is the result channel), so the event
  builders don't need instant-specific handling beyond compiling.

### M3c — Wire the mutations, frontend queue surfacing, docs

- **rename**: the managed rename entry point in `write_operations/rename.rs` builds an `OperationDescriptor`
  (`operation_type: Rename`, `volume_ids: [volume_id]` for non-root / `[]` for root, `lanes: []`, a `path_summary`-style
  from→to summary), and runs the actual rename (both branches: `std::fs::rename` for root, `volume.rename` for non-root)
  + the `note_pending_write_for_cmdr` dual-registration + `notify_rename_in_listing` INSIDE `manager().run_instant(...)`.
  Returns `Result<(), _>` to the command.
- **mkdir/mkfile**: same shape; `operation_type: CreateFolder` / `CreateFile`; the closure does the
  `note_pending_write_for_cmdr` + `volume.create_directory` / `create_file` and returns `(PathBuf, expanded)`; the
  command emits the synthetic diff after (or the module does). Returns the path `String` to the command.
- **Frontend queue** (`operations-store` / `QueueRow` / `queue.json`):
  - Regenerate bindings so `WriteOperationType` includes the new variants.
  - `QueueRow.svelte:35` icon ternary: add explicit arms for the new types (register any new glyph in
    `lib/ui/icons/icon-map.ts`) — else they fall through to `trash-2` (wrong).
  - `queue.json` `queue.row.label`: add `rename {Renaming} create_folder {Creating folder} create_file {Creating file}`
    select arms — **snake_case arm names**, because `WriteOperationType` is `#[serde(rename_all = "snake_case")]`, so the
    variants cross the wire as `rename` / `create_folder` / `create_file`. camelCase arms would never match and fall to
    `other → "Working"`. (copy/move/delete/trash hid this — they're single words.) Sentence case, present-continuous per
    the style guide. Same snake_case values in the `QueueRow.svelte:35` icon arms.
  - Confirm the store's terminal-prune + progress-merge handles a row that appears then vanishes with no progress event
    (the explore confirms it does; add/extend an `operations-store` test that an instant op — Running snapshot then
    removed — never assumes a progress bar).
  - Empty-state / window-description copy (`queue.empty.body`) enumerates "copies, moves, and deletes" — leave as-is
    (instant ops are transient and not the queue's purpose) or broaden minimally; David reviews copy. Flag, don't invent.
- **Docs**: update `write_operations/CLAUDE.md` must-knows — the "**Every write op spawns through
  `manager::spawn_managed`** (all five paths)" invariant is now FALSE for instant ops; reword it to carve out the
  instant path explicitly (else a future agent "cleans up" `run_instant` into `spawn_managed` and silently reintroduces
  lane-queuing for metadata syscalls — the regression the design forbids). Same for the `manager.rs:1-53` module-doc
  "five independent spawn paths" framing. Add the busy-set + queue behavior for instant ops.
  `write_operations/DETAILS.md` (a "Managed instant ops" section: the no-lane/no-queue decision + why, the RAII cleanup
  contract, the result-returning contract, queue surfacing), `commands/CLAUDE.md` (rename/create
  are now managed; the validity/permission checks remain the unmanaged snappy path), and the frontend `queue/CLAUDE.md`
  + `file-operations/CLAUDE.md` where operation types are enumerated. Keep each within its must-know bar; put depth in
  DETAILS. Do NOT bump `claude-md-length` allowlists — if a C.md would grow past budget, move depth to its D.md.

### Tests (M3, real red→green for the behavior)

Per the module's ~85–90% mutation bar and `tdd-red-green`:

- **Backend, TDD:** write failing tests FIRST for the managed-instant path:
  1. `manager::run_instant` marks the volume busy for the op's duration and unmarks after (drive with a paused future /
     a `Notify` to observe the mid-flight busy set), and registers→removes the record (observe via `list()`).
  2. A managed rename/mkdir on a non-root (`InMemoryVolume` with a non-root `lane_key`) marks that volume busy while
     running; a root rename marks nothing busy (root excluded). Assert via `busy_volume_ids()`.
  3. `run_instant` does NOT reserve a lane: a transfer on the same lane is admitted concurrently (assert the transfer
     isn't queued behind the instant op).
  4. **The B1 net (most important):** when the `op` future is dropped mid-flight (simulate the timeout: `select!` /
     drop the `run_instant` future while `op` is parked on a `Notify`) OR panics, the record is removed AND the busy
     registration is released (`busy_volume_ids()` no longer lists the volume; `list()` no longer has the record). This
     fails without the RAII guard.
  5. The rename mutation still returns its result (success / conflict-without-force / timeout-shape) unchanged — port
     the existing `commands/rename.rs` tests to prove the managed wrapper is transparent to the caller.
  These are genuine red→green: the `run_instant` method and the new op types don't exist yet, so the tests fail to
  compile / fail first, then pass.
- **Backend, existing:** all `write_operations` + `commands/rename.rs` + `commands/file_system/write_ops.rs` tests stay
  green through the move (M3a) — a real regression net for the logic relocation.
- **Frontend:** extend `operations-store.svelte.test.ts` for an instant op (Running → removed, no progress); a
  `QueueRow` test that the new op types render the right icon + label (not the `trash-2` / "Working" fallbacks).
- **E2E (focused):** one Playwright spec proving an inline rename and a new-folder still work end to end and feel
  instant (the managed wrapper didn't regress the snappy path). Use the file-op E2E patterns; run ONLY the rename/mkdir
  specs (`pnpm check desktop-e2e-playwright` scoped). Read `test/e2e-playwright/CLAUDE.md` first.
- `cargo mutants --file` on `manager.rs` + the new `rename.rs` / `create.rs` after M3 lands, to hold the bar.

**Commit** M3 in clean increments: M3a (relocation, tests green), M3b (`run_instant` + op types, TDD), M3c (wiring +
FE + docs).

---

## Checks + wrap

- `pnpm check --fast` while iterating; full `pnpm check` per milestone; `pnpm check --include-slow` before wrap
  (never tail/truncate the output — `no-tail-checker`).
- Strip milestone tags (`M1`/`M2`/`M3`/"milestone") from touched code + docs before wrap (they live only in this plan).
- Do NOT bump any `file-length` / `claude-md-length` allowlist. If something would grow past budget, split/move depth to
  DETAILS and, if still over, flag it in the final report.
- Do NOT merge to main, do NOT push. Leave the worktree + branch in place for review.

## Parallelization

Sequential is fine and safer. Note M3 does NOT strictly depend on M2: `run_instant` emits no per-op events (only
`emit_changed` via `OPERATIONS_APP` and `register_operation_status` via the busy emitter — neither is the injectable
sink), so the instant path needs no sink at all. Recommend M1 → M2 → M3 anyway for a clean tidy-then-invert-then-build
progression, but they could be reordered if needed.
