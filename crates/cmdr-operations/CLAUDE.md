# Write operations (`cmdr-operations`)

Copy, move, delete, trash, rename, create, and zip edits as managed ops behind one `Operations` handle. Tauri-free; the
app hosts it in `apps/desktop/src-tauri/src/operations_host/`.

## Module map

`lib.rs` (handle, public list), `handle.rs` (builder), `src/host/` (seams), `manager.rs` (lanes, admission), `state.rs`,
`scope.rs` (`OpScope`), `ledger.rs` + `reversal.rs`, `types.rs`, `testing/`. Rest: DETAILS § "Files (top level)".

## Must-knows

The full text of each, with its why: DETAILS § "Must-knows, in full".

- **The host is a value**: seams and the `VolumeHost` come through `Operations::builder`, an op reaches its engine
  through its `OpScope`, and nothing here names the app. ❌ No `static` engine state; a test owns its engine
  (`testing::TestOperations`). Seams, their call rule, the runtime: `src/host/CLAUDE.md`.
- **Test scaffolding reached from outside the crate is `any(test, feature = "testing")`**, ❌ never `cfg(test)` alone.
- **Only the installed engine's `claim_ledger` touches a ledger file**, once; ❌ never from `build()`.
- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile.** An instant op emits no
  `WriteCompleteEvent` (analytics WRAPS its driver) and refuses with a typed `MutationError`, ❌ never a sentence.
- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ never `Drop`.
- **The intent's writers are `IntentCell`'s.** Cancel keeps copied files; Rollback VERIFIES before each destructive act
  (❌ never a batch); cancel beats Pause. A wind-down reversal sees its op only as a `WindDown`.
- **A loop parks where it checks cancel** (`stop_or_park_*`, a scan's `ScanPause`), ❌ never hand-rolled. Parking on a
  PERSON owes the `human_wait.rs` clock AND `announce_human_wait`.
- **`write-conflict` takes an `ArmedConflict`**; ❌ the dispatch mutex never spans a write; ❌ never fuse
  `AlreadyResolved`, `StaleAnswer`, or `NoPendingConflict`.
- **Crossing types takes a person's consent, per SHAPE** (`conflict::resolution_for_clash`); ❌ never reduce a
  conditional before it.
- **Emit through `OperationEventSink`**; `write-settled` fires once, AFTER the terminal event.
- **EVERY local write lands via `overwrite::stage_and_land_file`**, its destination announced (`host::Announced`). ❌
  Only a `temp` leftover goes on sight.
- **Every mutation journals through its `OpScope`**, a VOLUME op with its REAL volume id. ❌ No rotation temp in
  `in_flight_temps`. A landed row's node (`journal_landed.rs`) is read AFTER the item's last date write, ❌ never by a
  stat that can fail the op.
- **A drive that LEFT is the mount table's answer**, ❌ never an errno.
- **Copy reserves `total_bytes`**, ❌ never `dedup_bytes`; NSURL only on a `statvfs` shortfall.
- **❌ The `types` floor `use`s no sibling.** `LifecycleStatus` is the ONE lifecycle answer.
- **Every preview runs under a `ScanWatchdog`**; whoever settles it CLAIMS the outcome.
- **A FAILED op is retained out-of-band**; `retain_failure` emits only after the record is GONE.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
