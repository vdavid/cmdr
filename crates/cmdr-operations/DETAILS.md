# Write operations details

Pull-tier docs for the `cmdr-operations` crate: architecture, flows, and decision rationale. Read this before any
non-trivial work here: editing, planning, reorganizing, or advising. Must-knows live in `CLAUDE.md`; the seams and the
runtime in `src/host/DETAILS.md`; the app's answers to them in `apps/desktop/src-tauri/src/operations_host/DETAILS.md`.

Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md` (umbrella) plus colocated child docs for
`apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`, `apps/desktop/src/lib/file-operations/delete/CLAUDE.md`,
`apps/desktop/src/lib/file-operations/mkdir/CLAUDE.md`, and `apps/desktop/src/lib/file-operations/mkfile/CLAUDE.md`.

Subdirs:

- `src/host/CLAUDE.md` — the seams the host answers, the runtime the engine spawns on, and the cycle rules for both.
- `src/transfer/CLAUDE.md` — copy + move (local FS, cross-volume, MTP, SMB), conflict resolution, the shared transfer
  driver, platform-specific copy backends.
- `src/delete/CLAUDE.md` — delete walker (local + volume-aware), trash, the oracle-aware delete fast path.
- `src/archive_edit/CLAUDE.md` — zip edits and fresh compression; `src/transfer_tree/CLAUDE.md` — the live tree a
  transfer shows.

## Purpose

Implements the destructive file operations as background tasks that report to the host through event sinks. Every
operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites,
network mounts, cross-filesystem moves, and name/path length limits. The crate names no `tauri` (`index-crate-isolation`
enforces it): the engine reports facts, and the host decides what a person sees.

**Decision: the crate and its handle are named for what the UI says.** "Operations" is the app's word already (the
operation queue, `operations-changed`, `src/lib/file-operations/`), it covers everything the crate holds (copy, move,
delete, trash, rename, create, zip edits), and it avoids a `cmdr_transfer::transfer::` stutter. The engine-wide seam is
`EngineEvents`, so it never reads as the per-op `OperationEventSink`.

Ask Cmdr bulk renames use the same managed-operation lifecycle. Every non-forced local rename is atomic-no-overwrite
through `cmdr_local::rename_local_exclusive` (`crates/cmdr-local/DETAILS.md` § "Local renames are atomic-exclusive"),
and root bulk-renames share that primitive directly. The dialog's target-exists preflight improves review UX, but only
the kernel operation closes the check-to-rename race.

Pre-flight scans reuse cached listings when the source volume reports `WatchCoverage::EveryWriter`, avoiding redundant
`list_directory` calls. The freshness contract and per-backend debounce windows are documented in
`apps/desktop/src-tauri/src/file_system/volume/CLAUDE.md` and
`crates/cmdr-listing/src/authoritative.rs::try_get_authoritative_listing`.

## Must-knows, in full

`CLAUDE.md` keeps each of these as one line; this is the full text, with the why each line compresses.

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**; blocking work runs in
  `host.runtime().spawn_blocking`, so a starter returns an id before I/O.
- **An instant op emits no `WriteCompleteEvent`** (analytics WRAPS its driver): ❌ never emit inside one, or the
  in-archive early return drops in-zip ops. It refuses with a typed `MutationError`, ❌ never a sentence.
- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ never `Drop`.
- **The intent's writers are `IntentCell`'s** (`operation_intent.rs`). Cancel keeps copied files; Rollback removes what
  it still recognizes, VERIFYING before each destructive act (❌ never a batch or a fork; only the `Drop` net is
  unconditional). Pause is orthogonal, cancel wins. A wind-down reversal sees its op only as a `WindDown`.
- **A loop parks where it checks cancel**, via `state.stop_or_park_sync()` / `_async()`, ❌ never hand-rolled; a SCAN's
  own is `ScanPause`, owing `note_parked`. **Parking on a PERSON owes both edges**, the `human_wait.rs` clock AND
  `announce_human_wait(sink)`, or the ETA lies. Clicks stop via `backend_cancel`; `backend_abort` /
  `cancel_all_write_operations` are the quit deadline's. The boundary reads `stop_work`, `backend_cancel`'s child, which
  a concurrent copy's first failure also fires (alone, so the intent stays `Running`): `src/transfer/DETAILS.md` §
  "Cancel and rollback reach a parked driver".
- **`write-conflict` takes an `ArmedConflict`** (`conflict_slot.rs`), so the slot is armed first; ❌ the dispatch mutex
  never spans a write. An answer NAMES its clash (`ConflictId`): ❌ never fuse `AlreadyResolved`, `StaleAnswer`, or
  `NoPendingConflict`, nor leave a settled prompt up.
- **Crossing types takes a person's consent, per SHAPE** (`conflict::resolution_for_clash`, ALL THREE engines): only a
  plain Overwrite answered for that `ClashKind` replaces, ITS "* all" carry included; the config policy, same-kind
  carries, and `OverwriteSmaller`/`Older` `Skip`. ❌ Never reduce a conditional before it. What ARRIVES is the caller's
  word (`IncomingItem`).
- **Emit through `OperationEventSink`** (spawns: `src/host/`); `write-settled` fires once, AFTER the terminal event. A
  cross-FS move speaks TWICE per source on `write-source-item-done`, so the LAST wins.
- **EVERY local write lands via `overwrite::stage_and_land_file`** (temp+rename, rename-aside when replacing); announce
  each destination BEFORE the syscall: the writers take it as `host::Announced` (a rename's SOURCE too, by hand).
  Leftovers carry a KIND (❌ only a `temp` goes on sight); ❌ only the installed engine's `claim_ledger` opens the
  ledger. § "What the sweep does with each kind".
- **Every mutation journals through its `OpScope`**; a VOLUME op passes its REAL volume id. Bulk rename journals each
  hop as it lands: ❌ never batch to the end, nor put a rotation temp in `in_flight_temps` (its sweep DELETES it).
- **A transfer carries both volumes as typed sides** (`transfer_sides.rs`), captured at start: a vanished drive can't be
  named later. A drive that LEFT is the mount table's answer, ❌ never an errno.
- **Free space: `free_space.rs`**, `statvfs` first, slow NSURL only on a shortfall; a shortfall is the person's call
  (`SpaceShortfall`). Copy reserves `total_bytes`, ❌ never the dedup'd `dedup_bytes` (delete's).
- **❌ The `types` vocabulary floor `use`s no sibling**, `types/events.rs` included. It holds `LifecycleStatus`, the ONE
  lifecycle answer: ❌ never re-derive it from a presence test, no new variant.
- **Every preview runs under a `ScanWatchdog`**, bounded by INACTIVITY: feed the progress callback, and whoever settles
  it CLAIMS the outcome.
- **A FAILED op is retained out-of-band**, the one exception to removal; `retain_failure` emits only after the record is
  GONE. **A test owns its engine** (`testing::TestOperations`), ❌ never a `static`.

## Driving the engine without Cmdr

A host hands over the two seams nothing works without (an engine with no volumes writes nothing; one with no events
leaves every dialog on "Copying…") and takes the do-nothing answer for the rest:

```rust
let engine = Operations::builder(Arc::new(MyVolumes), Arc::new(MyEvents))
    .volume_host(my_volume_host)        // optional: its runtime runs every op, else cmdr-fs's fallback runtime
    .operation_log(operation_log)       // optional: else a detached log, which journals nothing
    .own_writes(Arc::new(MyOwnWrites))  // optional, like index / pacing / name_order / operation_sinks
    .build();
// Once, on the engine that is the host's only one, and only then: replays and sweeps
// the last run's leftovers. `None` runs unpersisted and raises `LedgerUnpersisted`.
let _sweep = engine.claim_ledger(Some(data_dir))?;
let started = engine
    .start_volume_copy(
        Arc::new(MySink),               // this operation's `OperationEventSink`
        "root".into(),
        vec!["/tmp/a.txt".into()],
        "root".into(),
        "/tmp/out".into(),
        VolumeCopyConfig::default(),
        Initiator::User,                // `cmdr_operation_log::types::Initiator`
        None,                           // no source binding
    )
    .await?;
// `MySink` hears progress, then exactly one terminal event, then `emit_settled`.
engine.cancel_write_operation(&started.operation_id, false);
```

`MyVolumes` answers `OperationVolumes` (and its supertrait, the rollback planner's `VolumeLookup`): the volume behind an
id, path routing, and arrivals. `MyEvents` answers `EngineEvents`: the queue snapshot, the busy-volume set, the transfer
pair, error reports. Everything the builder doesn't name defaults to a no-op answer (`src/host/DETAILS.md` § "Optional
seams default to doing nothing"). The crate's own tests build a `testing::TestOperations` instead (§ "Test isolation: an
engine of the test's own"), and the Criterion bench (`benches/transfer_benchmarks.rs`) a detached engine through
`testing::bench`.

## Files (top level)

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The spine: `CLAUDE.md` § Module map.
The full top-level inventory is here:

- `lib.rs` (the `Operations` handle and its `Inner`, and the public list: the `pub use` lines, everything outside the
  crate may name; every submodule but `host` is private), `testing/` (`any(test, feature = "testing")`:
  `TestOperations`, an engine a test owns, in `engine.rs`; the Criterion bench's door in `bench.rs`; and what test cells
  outside the crate reach beyond the public list), `local_start.rs` (the local copy, move, delete, and trash starters
  and the `start_write_operation` lifecycle they share), `handle.rs` (building an `Operations` and reaching its parts),
  `scope.rs` (`OpScope`, what one operation reaches its engine through), `host/` (what the engine needs from the app:
  the runtime, the volumes, the engine-wide events, and the optional seams, § "The engine's host"), `manager.rs`
  (registry + lane admission; what the queue window asks in `manager/queue_api.rs`, its request answers in
  `manager/wire.rs`, reordering the line in `manager/reorder.rs`, and a retained failure's way to start again in
  `manager/retry.rs`, § "Retained failures"), `state.rs` (`WriteOperationState`, plus `state/settled_guard.rs`: the
  settle guard, `state/registry.rs`: `WriteOperationRegistry` and its walks, and `state/controls.rs`: the by-id cancel /
  abort / pause / resume / conflict-answer entry points, re-exported through `state`, and `state/activity.rs`: the one
  wait classifier, § "Naming a wait"), `outage_slot.rs` (which volume the operation waits on to come back, and how
  patiently: `src/transfer/DETAILS.md` § "Waiting out a volume that went away"), `status_cache.rs` (the status cache,
  the busy-volumes set it derives, the external drag-out seam, and `get_operation_status`), `summary.rs` (what the queue
  row says an operation acts on, § "What a row says it acts on"), `operation_intent.rs` (`OperationIntent`,
  `PauseGate`), `human_wait.rs` (how long a person, or a volume that left, has kept the operation waiting),
  `metadata_losses.rs` (per-class counts of the metadata a destination couldn't keep, for the completion's note:
  `src/transfer/volume/DETAILS.md` § "Copies keep the source's metadata"), `mount_boundary.rs` (where a delete's walk
  stops: `src/delete/DETAILS.md` § "A mount is a wall"), `archive_edit/` (the zip-edit driver).
- The in-flight ledgers and what reverses them: `ledger.rs` (`CopyTransaction` and `WrittenFile`, the vocabulary of what
  an operation currently has at the destination, plus the `Drop` panic net) and `reversal.rs` (the policy over it: the
  recheck before each destructive act, and `ReversalTally`), plus `unblocked_removal.rs` (the one removal every reversal
  of a copy goes through: a Locked, `deny delete`, or Read-only copy of the operation's own is cleared through
  `Volume::clear_blockers` after its plain removal is refused, never by a volume's own `delete`, which is also F8's).
  That recheck shares `verify_snapshot` + `SkipReason` with the operation-log history engine — ❌ never a batch or a
  fork of either, so one reversal can't quietly grow a laxer rule than the other; only the `Drop` net is unconditional.
  The persisted leftover ledger is seven files: `in_flight_records.rs` (what a record IS: its kind, its path space, and
  what that kind needs), `in_flight_temps.rs` (the log and the store, and what an operation records),
  `in_flight_landing.rs` (a staged temp turning into a landing before its way is cleared, and back),
  `in_flight_claim.rs` (the engine's one claim: the only code that opens, replays, and truncates a ledger file),
  `in_flight_log_writer.rs` (the thread that writes the log, so no record is file I/O on an async worker),
  `in_flight_sweep.rs` (what a launch and a returning drive DO with one), and `in_flight_sweep_surface.rs` (the four
  filesystem primitives those rules act through). § "What the sweep does with each kind" and § "Who claims the ledger"
  below hold the rules; `src/transfer/DETAILS.md` § "Which path space a recorded partial lives in" holds the path-space
  decisions, and § "Testing the in-flight temp ledger" below the rules it imposes on tests.
- **`tests.rs` is the SHARED-vocabulary suite only**: the intent machine, the config the frontend sends, the `io::Error`
  conversion, and `no_spawn_async_sync_in_the_engine` — a fail-closed whole-tree scanner (the root must exist and at
  least 300 files must be scanned) that excludes itself by the filename `tests.rs`, so ❗ moving it to another file
  silently makes it fail on its own prose. The `sync(2)` half is clippy's (`clippy.toml`). Anything with a subject of
  its own lives next to that subject (`overwrite_tests.rs`, `ledger_tests.rs`, `delete/delete_cancel_tests.rs`).
- **The scan is four files, and only one of them knows what a write operation is.** `scan_walker.rs` holds the shared
  local-FS recursion (`WalkContext`, `walk_dir_recursive`, `walk_sources_with_per_path`, and the hardlink dedup): one
  descent, parameterized by callbacks, so the copy scan, the delete scan, and the dry run share a symlink stance, cancel
  checkpoints, and the authoritative-listing fast path. It names no operation type. `scan.rs` turns a walk into totals
  (`scan_sources`, `scan_subtree_with_oracle`). `scan_source_tracker.rs` is the per-TOP-LEVEL-source bookkeeping over a
  flat result (`SourceItemTracker`, `FileVerdict`, `top_level_source_path`), which is what makes
  `write-source-item-done` fire once per source rather than once per file. `scan_dry_run.rs` is the preview that writes
  nothing. They're flat siblings rather than a `scan/` directory, matching `scan_preview.rs` / `scan_cache.rs` /
  `scan_bridge.rs` / `scan_watchdog.rs` — and because `pub(super)` here means "visible to `cmdr-operations`", which a
  directory would silently narrow.

  The one thing the walk carries for a caller that isn't a write operation: `WalkContext::online_only`, an
  `OnlineOnlyWatch` the delete confirmation's preview arms when its sources reach into a cloud drive. It answers whether
  trashing a selected folder would pull evicted files back down from the provider, and publishes `online_only_found` on
  the scan events. Armed nowhere else, so every ordinary walk is unchanged; the rule and its rationale live in
  `src/delete/DETAILS.md` § "A trash of online-only cloud content becomes a delete".

  Every walk also leaves out AppleDouble sidecars (`WalkContext::sidecars`, a `cmdr_fs::apple_double::SidecarRule`): on
  a volume with no native xattrs (FAT, exFAT, an SMB share without streams) the kernel keeps `name`'s xattrs in `._name`
  and serves them through `getxattr`, so the metadata copy already carries them, and walking `._name` too landed it on
  APFS as a stray 4,096-byte hidden file. Only a LISTED entry is filtered, never a selected source, and only when its
  sibling exists (an orphan is nobody's xattrs, so it copies). Because the filter sits in the walk, counts, bytes, the
  preview, and the copy agree. `scan_dry_run.rs` applies the same rule, and the cross-FS move's sweep doesn't count a
  kept item's sidecar as an arrival. Deleting the owner removes its sidecar in the kernel, so the sweep never touches
  one. The volume engine does the same for a local source through `Volume::list_directory_as_source`
  (`src/transfer/volume/DETAILS.md` § "A source is listed as a source", which also notes the same-FS merge-move edge).
  The rule, the kernel sources it mirrors, and why APFS and Linux never skip: `crates/cmdr-fs/src/apple_double.rs`.

- Scan and preview: `scan.rs`, `scan_preview.rs` (the lifecycle API, with its two workers in `scan_preview/local.rs` and
  `scan_preview/volume.rs`), `scan_cache.rs` (the carriers a walk produces; the engine's preview store is its child
  `scan_cache_previews.rs`), `scan_bridge.rs` (the scan-progress seam the drivers feed, and `ScanPause`, the park that
  lets a walk honor its owner's Pause), `scan_watchdog.rs` (the inactivity bound on a preview), `compress_estimate.rs`.
  A preview walks for a `ScanPreviewPurpose`: a `Transfer` walk also builds the operation's live tree and
  `CachedScanResult` carries it to the operation the way it carries `per_path`, a `Compress` walk samples the size
  estimate, and a `Delete` walk keeps totals only. `transfer_tree/` is that tree (its own `CLAUDE.md`), installed on
  `WriteOperationState::transfer_tree` once a copy's, move's, or compress's scan is done; `tree_view.rs` answers
  `get_transfer_tree_view` from it. Conflicts and overwrite: `conflict.rs` (policy), `conflict_preflight.rs` (the
  transfer dialog's BEFORE-the-write check, § "The pre-flight conflict check"), `unique_name.rs` (the ` (N)` namer),
  `conflict_slot.rs` (the one-answer-wins slot behind `resolve_write_conflict`), `overwrite.rs` (tests in
  `overwrite_tests.rs`), `look_alike.rs` (a name a byte-exact volume holds in another Unicode spelling, and where a new
  folder, file, or rename target goes; § "Look-alike names"). Cancellation and durability: `cancellable.rs`,
  `rollback.rs`, `durability.rs`. `rollback.rs` wears two hats: the history dialog's reversal, and the executor the
  operation-log engine injects.
- Vocabulary and edges: `types.rs` (+ `types/events.rs`, every event payload the app broadcasts, and `types/errors.rs`,
  `WriteOperationError` with the typed payloads its variants carry — both re-exported through `types`), `event_sinks.rs`
  (the collector sinks and the event builders), `host/` (the seams the app answers, § "The engine's host"),
  `error_classification.rs`, `transfer_sides.rs` (the two volumes a transfer runs between, the mount-table question, and
  the one boundary that words a stop; tests in `transfer_sides_tests.rs`), `target_names.rs` (the new NAME a renamed
  source takes in a move, § "Renames that run as moves"), `mutation_error.rs` (the typed refusal an instant mutation
  returns), `mutation_reply.rs` (its reply within the deadline, and the late settle, § "A slow instant mutation says it
  is still running"), `validation.rs`, `free_space.rs` (the copy's free-space pre-flight, § "The free-space
  pre-flight"), `eta.rs`. Journaling: `journal.rs`, `journal_search.rs`. The scratch dir archive edits stage local bytes
  in: `scratch_dir.rs` (the remote edit itself is `archive_edit/remote.rs`). Entry points: `create/` + `create.rs`,
  `rename/` + `rename.rs`, `create_numbered.rs` (paste-as-file's numbered create), `routing.rs` (the one routing every
  cross-volume transfer takes: `start_volume_{copy,move,compress}`, plus `start_rename_by_move`; `routing/retry.rs` is
  the Retry recipe a copy or move leaves with the manager), `routed_writes.rs` (the refusal every drive-only entry point
  asks first for an in-zip or `.git/<category>/` path, `host/DETAILS.md`). `source_binding.rs` is the optional set of
  sources an op may touch. `queued_sources.rs` is the check an op that waited in the queue runs on its sources when
  admitted, parking on `sources_gone_slot.rs` (a leaf the state owns, like `outage_slot.rs`). Layering, so the three
  stay out of a cycle: `source_binding` → `queued_sources` → `state` → `sources_gone_slot`, and the empty batch's
  terminal shape is `WriteCompleteEvent::all_skipped` in `types`. `locked_sources.rs` is a move's question about Locked
  items and the plan its answer makes, parking on `locked_sources_slot.rs` the same way (§ "Moving Locked items").
  Fixtures: `testing/` (an engine a test owns, and what test cells outside the engine reach beyond its public list) and
  `test_support.rs` (the placeholder conflict, the park-window wait, and the queued-operation fixture). Every backend's
  cells through this pipeline live app-side, in `operations_host/backend_suites/`, because they need the app around the
  engine (`apps/desktop/src-tauri/src/operations_host/DETAILS.md` § "Backend suites"). `testing/bench.rs` is the
  Criterion bench's door in: thin wrappers over five entry points, each with a no-op sink and an unregistered state on
  an engine the bench owns (`crates/cmdr-operations/benches/transfer_benchmarks.rs`; `docs/tooling/testing.md` §
  `criterion`).

What the mechanisms DO is in the sections below: the registry, lanes, and `run_instant` in § "Operation manager"; the
zip-edit driver in § "Archive edits"; cancellation, pause, Stop-mode conflicts, safe overwrite, scan-preview caching,
and the compressed-size estimate in § "Key patterns and gotchas"; durability and the two byte totals in § "Key
decisions"; the estimator in § "ETA + throughput"; `WriteSettledGuard` in § "Settle contract"; journaling in
`crates/cmdr-operation-log/DETAILS.md` § Capture. Only the layout facts that none of those carry live here:

- **Two re-export facades are deliberate, not collapsible** (§ "Key decisions" has the why): `lib.rs` names the transfer
  and delete entry points its siblings reach at the crate root (private `use` lines, so nothing outside the crate can),
  and `state.rs` re-exports the `operation_intent` + `scan_cache` + `status_cache` names. `types.rs` has none: it is the
  vocabulary floor (§ "Why `types` imports nothing"), so a sink, a classifier, or a lifecycle name is imported from the
  module that DEFINES it. `OperationEventSink` and `ScanPreviewEventSink` are re-exported at the `cmdr-operations`
  module root for the IPC edge, which builds the app's sinks from `operations_host`.
- **Event structs and their builders live apart on purpose**: the struct definitions in `types/events.rs`, the
  `WriteProgressEvent` (`new` / `with_scan_meta`) and `WriteErrorEvent` (`new`) impls in `event_sinks.rs` beside the
  collector sinks.
- **What splits `types.rs` from `types/events.rs` is one rule**: a struct the app broadcasts as an event payload goes in
  `events.rs`, and so does an enum whose only carrier is one of them (`SourceItemOutcome`, `CancelRollbackOutcome`). A
  name two homes speak stays in `types.rs`, which is why `TransferActivity` sits there: `WriteProgressEvent` carries it
  AND so does `OperationStatus`, a snapshot nobody emits.
- **`types/errors.rs` splits the same way**: `WriteOperationError` and the types that exist only as one of its variants'
  payloads (`ReadOnlySide`, `TransferRole`, `DisconnectedSide`, `TrashRefusalKind`, `RecoveredOriginal`,
  `OversizedFile`). `RecoveredOriginal` itself sits in `types.rs`, and its `new` is `pub(crate)`: its callers are the
  transfer engines. How an error reads in a log line (`log_detail`, its per-key marking table) is the
  `src/types/errors/log_detail.rs` child's.
- **`error_classification.rs` classifies from `errno` / `ErrorKind` only, never the message**. The native copy calls on
  both platforms (`macos_copy`, `linux_copy`) share `classify_copy_io_error`, which also picks the path: a refused write
  names the destination, anything else the source. One table, so the platforms can't drift apart again. A dropped link's
  errno (`cmdr_fs::volume::is_transport_errno`, the set the transfer engine's retry and outage wait also ask) maps to
  `ConnectionInterrupted`.
- **A vanished DRIVE is decided by the mount table, not by the errno** (`transfer_sides.rs`). A drive pulled mid-write
  answers `ENOENT`, `EIO`, `EBADF`, or `ENXIO` depending on which call was in flight, and every one of those would read
  to the user as a missing file or a broken disk. So each transfer carries `TransferSides` (both volumes' id, name, and
  root, captured when it STARTS — a volume that vanishes is out of the volume list and the mount table before an error
  is worded), and one boundary function, `transfer_stop_event`, rewrites what an engine answered into
  `DeviceDisconnected { path, side }` when that side's root has left, and attaches `progress_at_stop` from the status
  cache while the operation is still registered. `ENXIO` joins `ENODEV` as typed device evidence. A `Cancelled` is never
  rewritten (the user stopped it), and an unreadable mount table names nobody. Every engine's terminal emit and the
  manager's safety net go through it, so the two can't disagree. The test seam answers the mount table per thread
  (`transfer_sides::test_hook`), which is how a test pulls a drive without one.
- **`validation.rs`'s `ensure_destination_dir` runs AFTER `validate_destination_not_inside_source`**, so creating a
  missing destination (and its ancestors) can never materialize a folder inside a source. The volume-aware pipelines
  mirror both the behavior and the order with `Volume::create_directory_all(dest)`; see
  `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Recursive destination create".
- **A file in the way of the destination folder is `WriteOperationError::DestinationNotAFolder { path }`**, from both
  engines, refused before anything is written. `path` is the FILE, which is often a level or more above the folder the
  user typed, and naming it is the variant's whole job: as `DestinationNotFound` the dialog blamed a folder Cmdr was
  asked to create, and as an `IoError` it offered a Retry that could only meet the same file again. The volume engines
  get it from `VolumeError::NotADirectory` (`create_directory_all`, every backend); the local one looks upward for the
  file only after the OS refused (`validation.rs::file_in_the_way`), since `ENOTDIR` and `EEXIST` name only the path
  that was asked about. A destination reached through a link to a folder is a folder to both.
- **`validation.rs::validate_source_names_are_distinct` refuses a copy or move whose top-level items share a name**,
  with the typed `WriteOperationError::DuplicateSourceNames` (carrying the name plus both paths, so the dialog can show
  which two clashed). Two same-named sources both want `<destination>/<name>` and neither engine has an answer: the
  cross-FS move stages both under that one name inside `.cmdr-staging-<op>/`, so the second one's children meet the
  first one's staged files instead of an empty slot and resolve as conflicts against a copy the user never put there,
  and the rename phase then hunts for a staged tree the first source already carried away. Byte-exact names only: a
  case- or normalization-only difference is the destination filesystem's call, and refusing it here would block a
  legitimate transfer onto a case-sensitive volume. (Two sources differing only in Unicode form onto a share that
  composes new names both want the one composed name; the second one's landing refuses the occupied name with
  `DestinationExists`, never overwrites it.) Runs right after `validate_sources` in both `copy_files_start` and
  `move_files_start`. Pinned by
  `validation_integration_test.rs::{two_sources_sharing_a_name_are_refused, sources_differing_only_in_case_are_allowed, distinct_source_names_are_allowed}`.
- **`unique_name.rs::numbered_name(stem, ext, counter)` is the ONE ` (N)` formatter** (`counter 0` = bare, `1..` =
  ` (N)`). `find_unique_name`, `next_available_name`, `create_numbered.rs`, and the volume namer
  (`transfer/volume/naming.rs::find_unique_volume_name`) all go through it, so the numbering paths can't drift.
  `archive_edit/conflicts.rs::find_unique_inner` is deliberately outside this: it numbers slash-joined inner-path
  strings against an `ArchiveIndex`, and its doc comment says so.
- **Decision: the suffix is ` (N)`, everywhere, including a duplicate.** macOS Finder would name a duplicate
  `photo copy.jpg` with a per-language word for "copy", and `docs/design-principles.md` prefers platform-native over
  generic, so the pull toward it is real. It loses on two counts. One scheme for what is structurally one operation:
  numbering the duplicate path differently from the conflict path would have Cmdr generating `photo copy.jpg` on a
  duplicate and `photo (1).jpg` on a clash, which is worse than either scheme alone, and switching BOTH means
  re-reviewing the suffix in every shipped locale. And ` (N)` is language-neutral, where a translated word would owe
  `split_sequence` every one of those words before it could continue a series. Changing the scheme later is a one-place
  edit here plus its tests, which is the hedge that makes the decision cheap to revisit rather than a reason to revisit
  it.
- **`unique_name.rs::split_sequence(stem) -> (base, next_counter)` is the ONE sequence rule.** It reads a trailing
  ` (N)` off a stem so a search continues the series instead of nesting: duplicating `photo (1).jpg` gives
  `photo (2).jpg`, never `photo (1) (1).jpg`. What counts as a sequence is narrow on purpose, because everything else is
  somebody's filename: the separating space is required and the digits must be ASCII (`Report (final).pdf`,
  `photo(1).jpg`, and `photo (+1).jpg` are plain text); zero padding isn't preserved (`photo (007)` continues at `(8)`);
  and a number with no `u32` successor is plain text too, which keeps the returned counter always advanceable.
- **`unique_name.rs::NameCandidates` is the whole of what the ` (N)` searches share**: the parent, the base to number
  from, and the counter to try next, walked with `current()` / `advance()`. Built per item KIND, and the kind is not
  cosmetic: `for_file` keeps the extension at the end (`photo.jpg` → `photo (1).jpg`), while `for_directory` numbers the
  whole name, because a directory has none (`my.dir` → `my.dir (1)` rather than `my (1).dir`; likewise `backup.2024` and
  `v1.2.3`). `create_unique_dir` and the volume namer's `is_directory` branch pick the second, everything else the
  first. Every search walks the same candidates and differs only in how it TESTS one. `find_unique_name` RESERVES its
  pick with an `O_CREAT|O_EXCL` placeholder and must keep advancing when it loses that race; `next_available_name` only
  probes (`path_exists_or_is_symlink`, so a dangling symlink counts as taken) and creates nothing;
  `transfer/volume/naming.rs::find_unique_volume_name` reserves only when the destination volume is local-FS-backed AND
  the item is a FILE (`local_path().filter(|_| !is_directory)`), and probes otherwise — a directory takes the probe on
  every backend, for a reason `src/transfer/volume/DETAILS.md` states as load-bearing (a placeholder FILE where the copy
  is about to create a directory makes the merge walker merge into it, and leaves rollback nothing to remove). That
  difference is exactly why there's no shared search loop to layer them on as "search, then reserve". `attempts()` (not
  the counter's absolute value) is what a search bounds its own effort by, since a name ending in a high ` (N)` starts
  the sequence there.
- **Reach for the non-reserving `next_available_name` when a file placeholder would be in the way**: a directory claims
  its name with `create_dir` instead, and an ordinary non-overwrite write would otherwise find `find_unique_name`'s own
  placeholder sitting at the destination and raise a conflict against it.
- **`unique_name.rs::ClaimedNames` is what a probe can't have: a record of the names this operation already handed
  out**, before their bytes land. Every non-reserving picker (`next_available_name`, `create_unique_dir`, and both
  branches of the volume namer) consults it and records its own pick in one step, so two sources of one ` (N)` family
  duplicated together (`photo.jpg` and `photo (1).jpg`, which continues its series at exactly the name the first one
  took) can't both arrive at `photo (2).jpg` and turn two requested copies into one. It lives on
  `state::WriteOperationState`, so the ledger's lifetime is the operation's and both engines read the same one;
  interior-mutable because the volume engine's concurrent driver resolves several top-level sources at once. It also
  holds which of those picks left a zero-byte `O_EXCL` placeholder on disk and no write has filled yet, for the volume
  engine's post-loop to take back (`src/transfer/DETAILS.md`, the volume-side Rename reservation).
- **`create.rs` co-locates the instant listing update** (`should_emit_synthetic_diff` / `patch_listings_after_create`,
  both `pub(super)`) that lands a brand-new entry in the pane on local-FS-backed volumes, through the local backend's
  mutation patch (`MutationEvent::Created`, so the composer's index apply and badge invalidation run too).
  `create_numbered.rs` reuses both so a pasted file cursor-lands exactly like mkfile.
- **Scan-phase `expected_files_total` / `expected_bytes_total` come from the host's `OperationIndex::expected_totals`**
  (the app answers with the drive index's count) and are `None` when the index doesn't cover every source; the FE then
  falls back to a tally-only display instead of a progress bar. `scan.rs`'s walker derives `current_dir` from
  `path.parent()`, which is what lets the UI show "in directory: …" beside the filename.
- **`rename/bulk.rs` is Ask Cmdr's reviewed batch-rename driver**, and it does NOT use `run_instant`:
  `start_bulk_rename` takes only backend-owned rows accepted by preflight and runs through `spawn_managed` as one
  lane-queued operation. Its dependency planner renames independent rows directly, peels acyclic chains from their free
  destination, uses one same-directory temporary per cycle, and retains one temporary for a case-only rename on a
  case-insensitive filesystem. Local and remote drivers share the plan, so remote rename-as-copy backends don't
  duplicate every transfer. A batch that won't start answers a typed `RenameStartError` (an unregistered volume is the
  `WriteOperationError` a clicked copy would get, via `unregistered_source_error`), ❌ never a sentence. Cancellation
  happens between components: a started cycle finishes or reverses before the driver observes cancellation again. It
  journals one header and one final outcome per row. The Ask Cmdr command is the only caller, and it never receives
  paths or names from the frontend. On a non-root volume its destinations are new names: § "Look-alike names".
- **Paste-as-file (`clipboard/paste_file.rs::write_payload_to_dir` over `create_numbered.rs`) runs detached under the
  instant-mutation reply deadline** (`write_payload_replying`, § "A slow instant mutation says it is still running"),
  since the payload can be a large image landing on a slow network volume. ❌ Never a bare timeout around it: that DROPS
  the write mid-flight. Its reply and settle carry the created file (`PasteClipboardReply`, `clipboard-paste-settled`),
  which is why it uses `reply_or_hand_off`, the value-carrying core of `reply_within`. The app half takes an
  already-read `ClipboardPayload` + a `&Path`, decoupled from NSPasteboard and the IPC edge, so it's `TempDir`-testable;
  the engine's retry loop writes via `Volume::create_file` (O_EXCL) and bumps the counter on the TYPED
  `VolumeError::AlreadyExists`, so there's no pre-scan-then-write TOCTOU and it works on any writable volume.
  **Partial-file-on-timeout edge (accepted):** past 30 s the write future is dropped and a partial `pasted.<ext>` may
  remain (the user sees a timeout and can retry or delete). It's bounded and rare (local writes never approach 30 s, and
  on a local FS `create_file`'s `spawn_blocking` isn't even cancellable, so the file actually completes), and only
  affects slow network volumes. If it ever matters, route paste-as-file through the managed transfer engine for
  cancellation + no-partial guarantees. Pasteboard read + flavor precedence:
  `apps/desktop/src-tauri/src/clipboard/DETAILS.md` § Paste clipboard content as a file.

## Why `types` imports nothing

`types.rs` is the vocabulary floor: every other module here speaks its enums, event structs, error types, and
configuration, and it speaks nobody else's. Nothing in it may `use` a sibling (the `CLAUDE.md` rule), and that covers
`types/events.rs` and `types/errors.rs` too: a child of the floor is still the floor, so it imports from `super` and
from outside `cmdr-operations`, never sideways.

**Why the rule is absolute.** The dependency graph under `cmdr-operations` is a fan-in: 30-odd modules point at `types`,
and `types` is the sink. A single upward import from the sink closes a circle through everything that fans in behind it
— not a local mistake, a subsystem-wide one. Three such lines (`event_sinks::OperationEventSink`,
`error_classification::IoResultExt`, and `manager::LifecycleStatus`) once welded eleven modules — `analytics`,
`conflict_slot`, `error_classification`, `eta`, `event_sinks`, `manager`, `state`, `status_cache`, `types`,
`unique_name`, `validation` — into one strongly-connected component where nothing could be read, tested, or moved alone,
and it was the app crate's largest tangle. Three lines bought all of that, which is why the rule allows no exception.
Cutting them took the app crate from 121 modules in some cycle to 110 and its largest raw component from 11 to 10,
changing nothing else in the graph (cargo-modules 0.27.0, `pnpm check module-cycles`, 2026-08-23).

**Where the three names live instead.** `OperationEventSink` comes from `event_sinks` and `IoResultExt` from
`error_classification` — the modules that define them, which is where a reader looks anyway (and a `types::` alias for
either is a `use` of a sibling, so the floor rule already forbids it). `LifecycleStatus` sits in `types.rs` beside
`WriteOperationType` and `WriteOperationPhase`, because it IS vocabulary: `types::OperationStatus` carries it and it
crosses the wire to the frontend as a serde/specta enum. `manager` imports it like everyone else.

**How it stays cut.** `module-cycles` re-measures this home on every slow run and in CI, and its allowlist for
`cmdr-operations` holds no tangle at all, so any new circle here fails it. A new upward import from `types` brings the
whole eleven back. `scripts/check/checks/DETAILS.md` § "Rust module cycles" documents how to read its output, including
the ways `cargo-modules` misattributes an edge.

**Two smaller cuts keep it at zero.** Both point a lower module at what it really needs, never at a module above it. The
in-flight sweep calls `cmdr_local::rename_local_exclusive`, the primitive behind `overwrite::rename_no_replace` (§ "What
the sweep does with each kind"). Archive edits share one `EditError` leaf, where twin enums bridged by `From` impls once
sat (`src/archive_edit/DETAILS.md` § "Local vs remote: one closure, one dispatcher").

## Architecture / data flow

```
Frontend
  → WriteOperationState created (`IntentCell` intent, oneshot channel for Stop conflicts)
  → stored in the engine's registry + status cache (the state carries the op's `OpScope`)
  → operationId returned to frontend immediately (dialog opens, cancel is possible)
  → host.runtime().spawn (async wrapper, from the admission pass)
      → host.runtime().spawn_blocking (local I/O) or direct async (volume ops)
          → validate (sources exist, dest writable, dest not inside source)
          → scan phase: walk_dir_recursive, emit scan-progress events
              (delete on a volume also: `take_cached_scan_result(preview_id)` first;
               on hit, build the entry list from `per_path` — top-level files come
               straight from the cache, top-level dirs recurse via the oracle-aware
               walker; on miss, fall through to `scan_volume_recursive`)
          → free-space check (copy: `free_space.rs`, § "The free-space pre-flight")
          → execute phase: per-file copy/delete
              → throttled write-progress events (200ms default)
          → success (copy/move): flush_created_destinations() → emit write-progress (phase: flushing) → fdatasync dests → CopyTransaction::commit(), emit write-complete
          → success (delete/trash): emit write-complete (no sync)
          → cancel (Stopped): CopyTransaction::commit(), emit write-cancelled (rollback: notRolledBack)
          → cancel (RollingBack): rollback_with_progress() → emit write-progress (phase: rolling_back) → emit write-cancelled (rollback: what the reversal managed)
          → error: reverse_copy_transaction() (rechecks each entry, leaves a drifted one standing), emit write-error
      → safety net: start_write_operation emits write-error for unhandled handler errors
  → state removed from both caches
```

## ETA + throughput

Rates and ETA are computed in the backend (`eta.rs`) and shipped on every `WriteProgressEvent` as `bytes_per_second`,
`files_per_second`, and `eta_seconds`. The frontend renders these directly, with no client-side math or sample buffer.

**Why backend, not frontend:** one place to test, one set of fields exposed on the wire, identical behavior across
copy/move/delete/MTP/SMB/local. Putting the math in Svelte couples the estimator to dialog lifecycle and makes any
future client (CLI, menu bar app) reinvent it.

**Why two axes, not one:** the bug we hit in May 2026 was a delete of 5.4 GB / 174k files where the size bar saturated
in the first second (a few large files) and the byte-based ETA collapsed to ~0 s while 165k small files were still
streaming through. The estimator now tracks bytes/sec and files/sec independently and reports
`eta = max(ETA_bytes, ETA_files)`. The operation can't finish before either axis is done, so the larger one is reality.
When one axis has zero remaining work, its ETA is `0` and the other axis dominates naturally, with no branching needed.

**EWMA, not blended overall:** `α = 1 - exp(-Δt / τ)` with `τ = 3 s` (see `EWMA_TAU_SECS`). Pure exponential decay, no
"overall average" anchor. If the network drops mid-operation, the EWMA converges to the new rate within a few τ instead
of being pulled back toward historical numbers. Time-weighted means the response is the same whether progress events
arrive every 50 ms or every 500 ms.

**Files are counted between completions, not per emit.** Bytes flow continuously, so bytes over one emit interval is a
fair sample. Files land whole, so the files rate is the completions since the last measurement over the WORKING time
since it, measured only on a sample that carries a completion, with α weighted by that same span. Per emit, every
completion read as `1 / 0.2 s = 5 files/s` however long the file took, and the first post-seed sample took `1 / Δt`
straight: a thumbnail landing 57 µs after the seed put "17 629 files/s" on a 15-file video move, and with completions 45
s apart nothing pulled it back. A window shorter than `MIN_FILES_WINDOW` (150 ms, just under one emit) stays open for
the next completion. Between completions the rate holds (EWMA-ing the empty samples once decayed it to ~0 and made
`eta_files` explode to a 393 h ETA on a 22 min MTP copy). Pinned in `eta_tests.rs`
(`a_completion_right_after_the_seed_*`, `the_files_rate_follows_the_time_between_completions_*`,
`long_single_file_stream_*`).

**Warm-up:** the estimator returns `None` for ETA until it has ≥ 2 samples in the current phase AND ≥ 800 ms of WORKING
time (`MIN_SAMPLES_FOR_ETA`, `MIN_ELAPSED_FOR_ETA`). This kills the early "200 ms in, rate = 50 MB/s → ETA = 0 s"
footgun. The bytes rate populates on the first delta and the files rate on the first completion window; only the ETA is
gated.

**The rate window measures WORKING time, never wall time.** Each `EtaSample` carries the operation's cumulative
human-wait reading (`human_wait.rs`), and `update()` subtracts whatever accrued since the last sample from the elapsed
wall time. An interval that was entirely somebody's — a five-minute conflict prompt, a pause — re-anchors the counters
and leaves the rates exactly where they were, so the ETA on screen survives the wait instead of jumping. Without it the
first sample after the answer divides one file's bytes by five minutes: a QA pass watched a healthy copy report
`0.4 files/s · 409h 39m left` that way, with the queue row for the same operation saying `55m 33s left`.

- **Only a wait that isn't the transfer's speed counts**, and there are exactly three: the user's pause
  (`PauseGate::pause` / `resume`), an unanswered clash (`ConflictSlot`, which mirrors "a prompt is on screen" onto the
  clock from inside its own transitions, so no path can arm one and forget), and a volume the transfer waits on to come
  back (`OutageSlot`, from the first waiter in to the last one out: nothing moves and nothing CAN, and the UI names the
  wait). A slow SMB read, a busy device, a retry backoff are all the TRANSFER being slow, and the ETA has to say so —
  `eta_tests.rs`'s `a_device_wait_still_moves_the_eta` is the guard.
- **The sources are a UNION, not a sum.** The main window pauses an operation before prompting about its clash, so the
  intervals overlap; adding them would subtract the same seconds twice and leave real working time excluded afterwards.
- **The frontend hides the SPEED during those waits and keeps the ETA** (`operation-session/DETAILS.md` § "Read
  surface"). The two are one design: the speed is a claim about right now, which is nothing; the ETA is a claim about
  the work left, which this exclusion keeps true.

**Skipped work is subtracted before the sample.** `enrich_progress` feeds the estimator `bytes_done` / `bytes_total` /
`files_done` / `files_total` NET of `state.skipped_totals()`, while the event it enriches keeps the gross numbers the
bars render. A skipped file is done and both bars have to reach their totals, but nothing moved for it, so charging it
to the EWMA reads as an instantaneous burst — and a merge declining thousands of children back to back would make that
burst the whole reported speed. Remaining work is identical either way (`done` and `total` lose the same term), so this
changes the rate and never the ETA's numerator. Every skip site calls `state.note_skipped(files, bytes)`; the list and
the throttling that goes with it: `src/transfer/DETAILS.md` § "Skipped work moves the bars, and stays out of the rate".

**The bottleneck axis:** `EtaStats::bottleneck` (and `WriteProgressEvent::bottleneck`) names the axis whose ETA decides,
with hysteresis: the other axis takes over only after its ETA has led by more than 10% (`AXIS_LEAD`) for 3 s of working
time (`AXIS_HOLD`). `Bytes` while warming up, `Files` from the start for an operation that moves no bytes (a rename
move). Every folder bar in the transfer tree fills along it, so one answer serves every surface; a lead that comes and
goes inside the hold would redraw them all. Pinned in `eta_tests.rs` (`the_file_count_takes_over_only_after_*`,
`a_lead_that_ends_before_*`).

**Phase transitions reset:** `update()` reseeds on every `phase` change. Without this, the counters' reset (scanning →
copying both restart from 0) would feed a negative delta into the EWMA. Rollback is treated as a forward phase toward
target `(0, 0)`: the estimator subtracts the new counters from the previous ones and ETA = current value / decay rate.

**Wiring:** every `write-progress` emit site calls `state.emit_progress_via_sink(events, event)`. Production's sink is
the app's `TauriEventSink` (`apps/desktop/src-tauri/src/operations_host/sinks.rs`); tests use `CollectorEventSink`.
`emit_progress_via_sink` calls `enrich_progress` internally, so no caller has to remember. The
`bytes_per_second: None, files_per_second: None, eta_seconds: None` placeholders in the struct literals get overwritten
before the event reaches the FE.

**Frontend display:** every surface reads the three numbers off the operation's SESSION, which owns the one ETA smoother
that operation has in that window (a 25% gap-closure per tick, re-warmed on a phase change) and decides when there is an
honest number to print. Contract and rationale: `apps/desktop/src/lib/file-operations/operation-session/DETAILS.md` §
"Read surface".

## Parking on a person

An operation waiting on a person emits nothing: it is holding still on purpose, between files, with no chunk callbacks
to drive a progress event. So the newest tick every window holds was measured while it was moving, and it keeps a speed
on screen over a copy that has stopped. Two pieces close that, and both live on `WriteOperationState`:

- **The wait is classified for every operation, not just the ones with an in-flight table.**
  `WriteOperationState::activity` asks the transfer probe first (`activity_for`); an operation with no probe — a local
  copy, a delete, a trash — answers from its own pause gate and conflict slot (`decision_wait`), in the same order
  `transfer_probe::wait_reason` uses, and reports no in-flight count and no stillness that isn't the answerer's. A local
  copy therefore says `waiting_on: Conflict` exactly as a volume copy does, which is what lets the frontend decide once,
  in one place, that a parked operation has no honest speed
  (`apps/desktop/src/lib/file-operations/operation-session/DETAILS.md` § "Read surface"). § "Naming a wait" for who
  reads it.
- **Both edges of the wait re-send the last tick.** `announce_human_wait(sink)` re-emits `last_progress` (§ below),
  counters untouched because nothing moved, and `enrich_progress` re-classifies it on the way out. Called from both
  Stop-mode dispatchers (`conflict.rs`, `transfer/volume/conflict.rs`) right after `arm` and right after the answer
  lands. ❌ Arm first, then announce, then prompt: a surface can answer synchronously from inside `emit_conflict`, and
  an announcement after the emit would describe a wait that is already over.
- **A pause needs no announcement**, because a pause changes the registry snapshot's `LifecycleStatus` and
  `operations-changed` carries that to every window on its own. A clash leaves the operation `Running`, which is exactly
  why it needs a voice.
- **`last_progress` is the one copy of "the newest tick", and it is stored WITHOUT its activity.** The transfer probe's
  stall heartbeat reads the same field (filtering to the phases where a stall means anything) rather than keeping a
  second one. So does `latest_operation_progress`, which classifies the wait afresh on the way out. Activity is stripped
  on the way in because a re-send exists precisely because what the operation is doing has changed: a stored
  `waiting_on: Conflict` replayed after the answer would hide the speed for the rest of the transfer.

❌ A new way to park an operation on a person must open the human-wait clock (§ "ETA + throughput") AND announce itself.
Miss the first and the estimate collapses on resume; miss the second and every surface lies about the speed until the
person answers. Pinned by `conflict_stop_tests.rs`.

## Naming a wait

`TransferWaitReason` is the answer to "this operation says `running` and nothing is moving — why?". Four shapes read
identically without it: a slow copy, a wedged mount, a transfer parked on a conflict prompt, and an operation queued
behind a busy lane.

**Decision: each variant names the QUESTION, never who may answer it.** `Conflict`, not `You`. Who can answer is a
property of which surfaces exist, and it moves underneath the enum every time one ships: `resolve_conflict` made the
same park agent-answerable without the operation changing at all, and a name written from the dialog's point of view was
wrong the day that landed. The rule lives on the enum itself (`types.rs`), where somebody adding a variant will read it.

**`Reconnecting` names the one device wait that has a reason of its own**: a volume the transfer runs on went away, and
the operation is waiting for it to come back (`src/transfer/DETAILS.md` § "Waiting out a volume that went away"). It's
the only variant that carries a name, `TransferActivity::reconnecting_volume`, set exactly when the reason is
`Reconnecting` (`state/activity.rs::reconnecting_volume`), because the queue window knows an operation's paths, not its
volumes. It ranks under a pause and a prompt (both deliberate, both named elsewhere) and above every device wait.

**One classifier, two readers.** `WriteOperationState::activity(operation_id)` is the only place a wait is decided.
`enrich_progress` puts it on every `write-progress` event; `status_cache::get_operation_status` puts it on every
`OperationStatus`, which is what `cmdr://state`'s `operations:` rows render as `waitingOn` / `stillForSeconds` /
`inFlight` (`mcp/DETAILS.md` § Resources). A poller and a subscriber therefore can't disagree: a snapshot saying
`moving` while the event stream said `destination` sends an agent down the wrong branch.

- **Classified at READ time, never cached.** The status cache echoes the last progress tick's counters; the wait is
  recomputed on every read, because a wait that is over is worth nothing and a stale one is worse than none. This is
  also why `get_operation_status` reads it OUTSIDE the cache guard, next to `lifecycle`: the probe registry and the
  state map are other locks, and that function keeps them all un-nested.
- **`None` means "can't tell", never "it's moving".** An operation that settled (the cache row outlives the state entry)
  and a backend that keeps no in-flight table with nobody parked on a decision both answer `None`, and every surface
  renders that as silence rather than a stand-in a poller would act on. Pinned by
  `status_cache_tests.rs::a_settled_operation_reports_no_activity`,
  `state_tests.rs::an_operation_with_nothing_to_say_reports_no_activity`, and
  `mcp/tests/resource_operations_tests.rs::a_backend_that_cannot_classify_its_wait_says_nothing_rather_than_moving`.
- **`enrich_progress` leaves an activity a caller already set alone.** The stall watchdog emits from the probe it is
  stepping (`transfer_probe::emit_heartbeat`), and its copy is the one that just decided the transfer is wedged; a
  second lookup there would cost the re-emitted event the very activity it exists to carry. Pinned by
  `state_tests.rs::enrich_progress_keeps_an_activity_the_caller_already_decided`.

## Key patterns and gotchas (shared)

**All blocking work in `spawn_blocking`.** Never call blocking I/O on the async executor.

**`OperationIntent` state machine.** Replaces the old `cancelled: AtomicBool` + `skip_rollback: AtomicBool` pair with a
single atomic-backed enum, held in an `IntentCell`: `Running → RollingBack` (user clicks Rollback), `Running → Stopped`
(user clicks Cancel or teardown), `RollingBack → Stopped` (user cancels the rollback). `Stopped` is terminal.
`IntentCell::is_cancelled` returns true for both `RollingBack` and `Stopped`, so the 40+ cancellation check sites call
`state.intent().is_cancelled()`. The cell has no raw `store`: its only writers are `request_stop` (a click) and
`stop_for_teardown` (which can only produce `Stopped`).

**Cancel vs Rollback: distinct behaviors:**

- **Cancel (`Stopped`)**: Stop immediately. Keep all fully-copied files. Delete only the last _partial_ file (a
  half-written file is corrupted data, not useful to keep). Reports `rollback.outcome: notRolledBack`.
- **Rollback (`RollingBack`)**: Stop copying, then reverse what this operation wrote, newest first, with progress events
  (`phase: RollingBack`). Each entry is rechecked before it goes, and one something else changed since is left alone and
  reported (`src/transfer/DETAILS.md` § "What a reversal does with that identity"). The progress bars drain, and reach
  zero whatever the reversal managed. The user can cancel the rollback (→ `Stopped`), which keeps whatever hasn't been
  reversed yet. Reports `rolledBack` or `partiallyRolledBack`.
- Both are triggered from the same `cancel_write_operation` IPC call, distinguished by the `rollback` parameter.

**Two-layer cancellation.** `IntentCell` (`OperationIntent`) for fast in-loop checks in local file operations. Volume
operations (MTP, SMB) use the same intent checks but run on the async executor (no `spawn_blocking`). `run_cancellable`
wraps blocking local operations (for example, network-mount copies that may block indefinitely) in a separate thread,
polling the flag every 100 ms via `mpsc::channel`.

**Stop-mode conflict resolution.** Creates a per-conflict `tokio::sync::oneshot` channel, **arms `state.conflict_slot`
with the sender BEFORE emitting the `write-conflict` event**, then blocks on the receiver (`blocking_recv()` inside
`spawn_blocking`; the volume path `await`s instead). Arm-before-emit is load-bearing: a responder can only answer a
conflict it has observed, so if the event reached `resolve_write_conflict` (or a test responder sink) before the slot
was armed, the answer would land on nothing and the recv would hang. The type holds it: `ConflictSlot::arm` returns the
only `ArmedConflict`, the one thing `OperationEventSink::emit_conflict` accepts. Arming also MINTS the clash's
`ConflictId`, which is why every emit site arms before it builds the event: the id rides out on it. Frontend calls
`resolve_write_conflict(operation_id, conflict_id, resolution, apply_to_all)`, which answers through the slot.
`cancel_write_operation` calls `conflict_slot.abandon()`, dropping the sender so the receiver returns `Err` (interpreted
as cancellation, and carried to the loop's CANCELLED ending, never a failure: `src/transfer/transfer_driver/DETAILS.md`
§ "A `Cancelled` from any closure is a cancel, ❌ never a failure"). No polling, no safety timeout, immediate unblock on
cancel. Pinned by `conflict_stop_tests.rs` (local) and the `ConflictResponderSink` suites (volume).

**Answering a conflict is arbitrated, the answer NAMES the conflict it is for, and the arbitration is REPORTED.**
`write-conflict` broadcasts to every webview, so several surfaces can render one prompt and each of them can be answered
(the progress dialog and the main window's `operation-conflict.svelte.ts` host today). `conflict_slot.rs` is a
three-state machine — `Idle` / `Awaiting { id, sender, prompt }` / `Answered { id }` — under one `std::sync::Mutex`, and
`answer(conflict_id, response)` performs the whole transition inside it, so exactly one answer reaches the parked
operation.

**The armed state holds the QUESTION, and `arm` builds it.** `arm(tx, |id| event)` mints the id, calls the builder with
it, stores the event, and hands it back for the caller to emit, so the id on the wire, the id the slot is armed with,
and the id `answer` requires back are one value by construction. That question is then readable through `pending()` /
`Operations::pending_write_conflict(op_id)` for anyone who arrives after the broadcast: `write-conflict` only ever
reaches whoever was listening when it went out, so `cmdr://state`'s `pendingConflict:` block (the surface an AGENT
answers from) has no other way to know what is being asked. ❌ Don't add a second way to arm that skips the question: an
operation parked on something nobody can read back is answerable only by a surface that happened to be listening.

**The id is what makes an answer belong to a question.** An operation raises its clashes one at a time, but an answer's
round trip (broadcast out, a person, IPC back) can outlast the clash it belongs to: the operation takes an answer,
carries on, hits the next clash, and parks on it, all before the answering surface's IPC call has returned. Without an
identity that late answer is indistinguishable from an answer to the clash now on screen, and it silently decides a
question nobody was shown. `ConflictSlot::arm` mints a `ConflictId` (a per-operation counter, minted under the same lock
as the state transition so an id can never belong to two questions), the `write-conflict` event carries it, and
`resolve_write_conflict` requires it back.

What each caller gets back is the `ConflictResolutionOutcome` that state produced: `Resolved` (this answer is the one
the operation carried on with), `AlreadyResolved` (someone answered THIS SAME clash first; nothing changed),
`StaleAnswer` (this answer names a clash the operation has left behind — it reached nothing, and whatever is parked now
is untouched), `NoPendingConflict` (the operation is live but isn't asking — never raised a conflict, a cancel abandoned
the pending one, or the waiting task stopped listening), or `UnknownOperation` (nothing is registered under that id). It
crosses IPC (`bindings.ts`) because a surface that can't tell it lost leaves its prompt up over an answer that did
nothing, which is the state the user reads as "I clicked and nothing happened". ❌ Don't collapse `AlreadyResolved`,
`StaleAnswer`, and `NoPendingConflict` into each other: "someone beat you to it", "your question is over and there's a
new one", and "this operation isn't asking anything" mean different things to a caller, and the FE logs them apart. ❌
Never let a stale answer report `Resolved`: that is the confidently-wrong shape the id exists to remove. A mismatched
answer leaves the parked sender in place, untouched — the person the live clash is on screen for hasn't clicked yet.

Every transition also mirrors "a prompt is on screen" onto the operation's human-wait clock (§ "ETA + throughput"),
derived from the state it just wrote rather than remembered alongside it.

**And the operation announces the clash as OVER, naming it** (`write-conflict-resolved`, emitted through the sink from
the same three Stop-mode dispatchers, right where the answer lands and only on `Ok`). The verdict above reaches exactly
one surface: the one whose own IPC call returned. Every other surface showing the same broadcast prompt — the queue
window's copy, the main window's host, anything at all when an AGENT answered over MCP (`resolve_conflict`) — would keep
asking a question with no answer left to give, and being a modal, block every new operation behind it. The FE drops only
the clash the event NAMES (`operation-conflict.svelte.ts`, `operation-session.svelte.ts`), because the operation raises
its next clash the moment it takes an answer, so the retraction for the old one routinely lands with the new one already
on screen. A cancel emits nothing: it leaves no answer, and the prompts go because the operation itself does.

The state machine exists so the take, the id, and the "was it answered?" bookkeeping are ONE transition; a `bool` (or a
counter) beside an `Option<Sender>` can desync from it. Pinned by `conflict_slot.rs::tests` (per-transition, including
the retired-clash refusal) and the `resolve_write_conflict` block in `state_tests.rs` (through the public path). The
frontend half of the same contract — a session clears only the clash it answered, so one that arrived mid-answer
survives — is `apps/desktop/src/lib/file-operations/operation-session/DETAILS.md` § "Answering a clash is a delegation".

**Conflict-dispatch mutex (folder merges).** `WriteOperationState::conflict_dispatch_lock` (a `tokio::sync::Mutex`, next
to `conflict_slot`) serializes the whole Stop-mode dispatch for an operation: there is exactly one human and one oneshot
slot, so two tasks both hitting a Stop-mode clash at once — the concurrent volume-copy spawn loop, or two parallel deep
directory merges — must queue rather than race to emit a `write-conflict` and clobber each other's sender. The dispatch
sequence under the lock: check `is_cancelled` (bail with `Cancelled` so a queued task can't emit a prompt no one will
answer after the dialog tears down — a hang), re-check the apply-to-all latch (a prior "…all" answer collapses the
queued prompt), emit + await, store the latch, release. Released on every exit, NEVER held across the subsequent file
write. Volume-side only today (the local-FS engine's per-file conflicts surface serially inside one `spawn_blocking`).

**`cancel_write_operation` does state transitions.** `rollback=true` → `Running → RollingBack`, `rollback=false` →
`Running → Stopped` or `RollingBack → Stopped`. First caller's decision wins; subsequent calls with different intent are
no-ops (unless transitioning from `RollingBack → Stopped`). `cancel_all_write_operations` always transitions to
`Stopped` (teardown should never silently roll back without visual feedback). Both go through `IntentCell`
(`request_stop` and `stop_for_teardown`), one `try_update` that checks `OperationIntent::may_become` (the whole table)
against the value AT THE WRITE. A load-validate-`store` let a Rollback click that read `Running` overwrite quit's
`Stopped` landing in its gap, deleting the partials quit meant to keep; the `intent_write_gap` test seam parks a writer
there (`state_tests.rs` § "two stops racing on one intent").

**Scan preview state.** `Operations::start_scan_preview` registers one entry in its engine's `ScanPreviews`
(`scan_cache_previews.rs`, a child of `scan_cache.rs`) and spawns a walk that holds the engine until it settles. The
entry is either in flight or settled, and a settled one carries WHY: complete (with its `CachedScanResult`), errored
(with its message), or cancelled. `copy_files_start` / `delete_files_start` consume a completed result via `preview_id`
in `WriteOperationConfig`, skipping a redundant scan. An entry is freed by three paths: (1)
`take_cached_scan_result(preview_id, sources)` at op start (the consume path), (2) `cancel_scan_preview(preview_id)` on
dialog teardown — it sets the in-flight cancel flag AND drops the entry, so a dialog dismissed after the scan completed
doesn't leak the result — and (3) a TTL safety net: `settle_preview` first evicts settled, UNCLAIMED entries older than
`SCAN_RESULT_TTL` (5 min). The TTL is a backstop for a caller that forgets both (1) and (2); the pure
`expired_scan_result_ids` helper is unit-tested. A `CachedScanResult` can hold tens of thousands of `FileInfo`, so none
of these paths is optional.

**The cache is bound to its request, and says so when it's incoherent.** A `preview_id` proves the frontend once asked
for a scan. It proves nothing about WHICH scan, and three of the six consumers act on the cached file list without ever
re-reading their own `sources` again, so an id pointing at a preview of a different selection makes each of them fail
differently: the LOCAL delete walker (`delete/walker.rs`) deletes the previewed tree instead of the requested one, with
no rollback and no progress line naming it; the LOCAL copy (`transfer/copy/mod.rs`) writes the previewed tree to the
destination while its bulk-skip set still reads the requested one; the LOCAL move (`transfer/move_op/cross_fs.rs`)
stages the previewed tree and then fails in Phase 3 looking for the requested name in staging, a half-staged move. The
VOLUME delete and both `transfer/volume/preflight.rs` sites were already source-bound (they iterate `sources` and fall
through per-source on a miss), so they degrade to a rescan rather than acting wrong.

Two mechanisms close it at the choke point, and both live in the preview store (`scan_cache_previews.rs`):

- `CachedScanResult::sources` records what the preview was asked to walk, and
  `take_cached_scan_result(preview_id, requested_sources)` compares it SET-wise against the operation's own list. A
  mismatch is a cache miss: the entry is dropped, a warn names both lists, and the caller takes the fresh-scan fallback
  it already had. The comparison normalizes nothing on purpose. A path that differs only by a trailing separator is an
  IPC-edge bug; a lenient comparison here would just be another belief.
- `settle_preview` carries a coherence canary: a completed walk with `file_count > 0` and an empty `per_path` warns and
  trips a `debug_assert!`. That's the shape that let a LOCAL preview hand the copy drivers an empty `source_hints` map,
  which they read as a confident `is_directory: false`. It's one-directional (a volume batch legitimately caches empty
  `files` with a populated `per_path`), and it's a `debug_assert!`, so a release build still admits the entry and the
  drivers still have to survive it (`transfer/volume/copy_source_hint_tests.rs` is that defense's proof, seeding past
  the canary via `seed_incoherent_scan_result_for_test`).

The map is a private field of `ScanPreviews` so neither can be walked around: its methods (`settle_preview`,
`take_cached_scan_result`, the claims, the read-only totals and cost facts, `release_preview`) are the whole surface,
and the store reads `CachedScanResult`'s private fields only because it's `scan_cache`'s child. A reachable map is a
choke point in name only. A panic that poisons the map makes the store forget every preview and carry on
(`ScanPreviews::write`): an entry a panic interrupted can't be trusted to bind a delete, and a forgotten one is a miss,
which rescans. Pinned by `scan_cache_tests.rs` (both mechanisms, and the poison answer),
`delete/preview_binding_tests.rs` (the destructive one, plus a volume-side regression fence), and one binding test each
in `transfer/copy/copy_tests.rs` and `transfer/move_op/move_op_tests.rs`.

**A completed preview always carries `per_path`, whichever walk produced it.** Both `run_scan_preview` (the LOCAL
`std::fs` walk, via `scan.rs::walk_sources_with_per_path`) and `run_volume_scan_preview` (SMB / MTP) cache one
`CopyScanResult` per top-level source: its type, file/dir counts, and byte totals. That map is the ONLY thing that tells
a cross-volume copy whether each selected item is a file or a folder without paying a stat probe per source, and it's
what a volume delete's fast path reads for top-level file sizes. Downstream code treats a source that isn't in the map
as **unknown** and probes (`src/transfer/volume/DETAILS.md` § "A missing source hint means unknown"); ❌ don't add a
preview path that completes without filling it — an empty map costs one round trip per source on SMB/MTP, and any
consumer that guesses instead of probing is a data-safety bug waiting to happen. Per-source accounting is a before/after
snapshot of the shared walk counters, so hardlink dedup still spans all sources (a later source's `dedup_bytes` share
can read low; informational only). Pinned by `scan.rs::tests::walk_reports_type_and_totals_per_top_level_source`.

**Decision**: `scan_sources_internal` does NOT collect `per_path`, and `ScanResult` / `CachedScanResult` stay two
structs. **Why**: `scan_sources_internal`'s result goes straight back to its caller and never enters the preview cache,
so its empty `per_path` can't be read by a consumer that would mistake it for "these sources are files". Filling it
would buy a symmetry that only exists on paper, at the cost of a per-source counter bracket on the local
copy/move/delete hot path. The uniformity worry underneath is real, though: an empty `Vec` is doing the job of "not
collected", which is the same anti-pattern as `SourceHint::default()` one level up. **The elegant fix, when someone
wants it, is a named enum** (`PerSource::NotCollected` / `PerSource::Collected(...)`), so "empty because there were no
sources" and "empty because this walk doesn't collect them" stop being the same value. That has real reach (six
consumers plus two crates' worth of `BatchScanResult` plumbing) and deserves its own decision rather than riding along
on a safety pass. Until then the contract lives on `scan_sources_internal`'s doc comment: if that result ever starts
crossing the cache, it has to collect `per_path` first.

**Compressed-size estimate (Compress dialog).** When `start_scan_preview` runs with `sample_for_estimate` set (Compress
mode only), the LOCAL walk feeds a cheap deflate-sampling estimator (`compress_estimate::CompressEstimator`) that
predicts the zip's output size, shown live-ish in the dialog beside the scanned byte total. Mechanics and invariants:

- **Local-FS walk only; remote is suppressed.** The per-file `WalkContext::on_file` hook fires only from
  `walk_dir_recursive` / `walk_cached_entries` (the `run_scan_preview` path). `run_volume_scan_preview` (SMB/MTP) never
  samples and never guesses — the estimate is simply `None`. Sampling a remote source would do real network reads and
  defeat the oracle's zero-I/O short-circuit, and an extension-only guess is unbounded (a single mistyped file →
  8×-wrong), so an absent estimate is the honest choice. **Don't add sampling to the volume/oracle path.**
- **Off the walk thread.** The hook is a push on a deliberately UNBOUNDED channel (a `sync_channel` would block the walk
  when the sampler falls behind, e.g. an oracle-cached fast walk of a huge tree — the transient queue of
  `(PathBuf, u64)` is the accepted cost, and post-budget the worker drains via cheap lookups; don't "fix" this into a
  bounded channel). The worker thread deflates a 32 KiB head window per file at reference level 6 under an 8 MiB total
  byte budget, so the sampling CPU never lands on the walk's critical path and worst-case added time is ~105 ms
  regardless of tree size. Media-heavy trees cost near zero (an incompressible-extension table shortcuts the read).
  Files under 4 KiB, budget-exhausted files, and unreadable files take a running-average ratio. The worker cancels with
  the scan (shared `cancelled` flag) and is joined before the complete event; a sampling panic degrades to `None` and
  never fails the scan.
- **Per-class subtotals, scaled on the FE.** The estimate ships as three `CompressedSizeEstimate` subtotals of estimated
  **level-6** bytes, bucketed by each file's sampled compressibility class. The frontend re-scales to the user's
  selected deflate level via a baked per-class curve (`compress-estimate-scaling.ts`) with no re-scan, so moving the
  level slider updates the shown number arithmetically. Level 6 is the reference (shown value = sum of subtotals). It
  rides `scan-preview-complete` (and the `get_scan_preview_totals` recovery path) only; while scanning the dialog shows
  a loading affordance.
- Parameters (window, budget, tiny threshold, extension table, level curve) and their measured accuracy/cost:
  `docs/notes/compress-size-estimate-spike.md`.

**Progress throttled to 200 ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>`
of canonicalized paths.

**Every local write lands via temp + rename** (`overwrite::stage_and_land_file`). Steps: write the bytes to
`dest.cmdr-tmp-<uuid>` (a sibling, so the landing rename is same-directory and therefore atomic); if an existing entry
is being replaced, rename dest → `dest.cmdr-temp-<uuid>` (the aside); rename temp → dest; delete the aside. Both scratch
names are shortened to fit a name near the filesystem's limit (`crates/cmdr-fs/DETAILS.md` § "Scratch names fit in 143
bytes"), so a final name the destination can't hold is refused by the landing rename, which keeps it typed as
`NameTooLong`. The original is intact until the landing rename completes, and the destination name never holds a partial
at any observable moment. The same pattern covers file→folder overwrites (existing dest folder renamed aside, then the
source file lands at the original path). A folder→file overwrite has two shapes, because the aside can only go once the
new content is complete: `safe_overwrite_dir` where the closure finishes the job (the symlink branch, the move engines),
and `displace_with_directory` where it doesn't — the copy's parent-creation site, whose subtree lands leaf by leaf,
hands the aside to the `CopyTransaction` and lets commit, a reversal, or a failure answer for it (a failure keeps it as
a ` (recovered)` sibling, since the half-filled folder keeps the name). `src/transfer/DETAILS.md` § "Cross-type
overwrites" holds the why.

The four copy mechanisms (`copy_strategy::LocalCopyStrategy`) each hand `stage_and_land_file` a closure that puts bytes
at a path, so the landing is written once rather than once per branch. ❌ Don't add a mechanism that writes straight to
`dest`. Details and the no-clobber rule: `src/transfer/DETAILS.md` § "Local copies stage".

**Conditional conflict policies (`OverwriteSmaller` / `OverwriteOlder`)** reduce per-file. The user picks "Overwrite all
smaller" / "Overwrite all older" either upfront (TransferDialog radios) or via the per-file conflict dialog's
apply-to-all buttons. Each conflict re-evaluates against its own source/dest metadata: `OverwriteSmaller` overwrites
only when `dst.len() < src.len()`, `OverwriteOlder` overwrites only when `dst.modified() < src.modified()`. Equal sizes
/ equal mtimes / unknown metadata all reduce to `Skip` — strict comparison so a borderline file is never silently
overwritten. Implemented by `conflict::reduce_conditional_resolution` (sync, local FS) and
`transfer/volume/conflict.rs::reduce_volume_conditional_resolution` (async, volume backends). Both log a
`target: "conflict_resolution"` info line on every Skip with the reason (not-strictly-smaller, not-strictly-older,
missing metadata), so users running an MTP/SMB copy who picked one of these can see in the operation log why their
conflicts got skipped instead of being puzzled by silence. **The apply-to-all storage saves the _original_ conditional
variant**, not the reduced one — subsequent conflicts re-run the comparison against their own files.

**Replacing one kind of entry with another needs a person's consent, per SHAPE.** `Overwrite`, `OverwriteSmaller`, and
`OverwriteOlder` all say "the entry at the destination is stale, put the source there", which is an answer about two
FILES. Across types the act is a different one — replacing a folder with a file discards the folder, replacing a file
with a folder discards the file — so `conflict::resolution_for_clash` reduces any of the three to `Skip` unless somebody
was actually asked, and the item reports as an ordinary skip. All three engines call it, so the rule is one rule: local
(`conflict.rs`), cross-volume (`transfer/volume/conflict.rs`, both the up-front lookup and the Stop arm's under-the-lock
re-check), and in-archive (`archive_edit/conflicts.rs`, both `ConflictMode` arms).

What counts as consent, and what doesn't:

- **Refused**: the configured `conflict_resolution` (nobody picking a policy in the transfer dialog was shown either
  kind), and a same-kind apply-to-all carry reaching across types. The latch alone can't defend this — a config policy
  that IS `Overwrite` walks straight past it.
- **Honoured**: a plain `Overwrite` answered on a Stop prompt for THAT shape, _*and the "* all" carry it latched_*. The
  dialog names both types (`build_conflict_event`'s `source_is_directory` / `destination_is_directory`) and relabels its
  apply-to-all button for the act ("Overwrite folders with files"). Refusing the carry makes that button a trap: the
  first item replaces, every one after it is skipped, and the local engine's per-file skip isn't even visible in the
  toast.
- **Refused whoever asked**: `OverwriteSmaller` / `OverwriteOlder`, including straight off the prompt. The dialog offers
  both on a cross-type prompt, but neither label names the act and neither can ask its own question here — a directory's
  `len()` is its own inode's, so every ordinary file looks "bigger" and "Overwrite all smaller" deleted destination
  folders wholesale. The Stop arm has to run its answer through `answered_resolution_for_clash` BEFORE
  `reduce_conditional_resolution`, or the reduction does that comparison against a directory.

`ClashKind` (same-kind / file-over-folder / folder-over-file) is what makes "per shape" real: `ApplyToAll` holds one
bucket each, a cross-type answer only ever fills its own, and a first-clash cross-type "* all" spreads to same-kind but
never to the other cross-type bucket. A destination that won't stat classifies as same-kind: an unanswerable type is
never grounds for a destructive act.

**Who says what's arriving.** `resolve_conflict` takes an `IncomingItem` rather than stat'ing the source path, because a
stat follows links: the engines move and copy a link as a leaf whatever it points at (`validation::is_real_directory`
holds the why), so a link facing a real directory is a cross-type clash. The move engines pass
`IncomingItem::of_local_source`; the copy engine knows from its own branch. A destination that won't stat is treated as
neither type: an unanswerable question is never grounds for a destructive act.

**The prompt names the kinds the clash was classified as, and the real incoming item.** Consent is only consent if the
prompt tells the truth, so `build_conflict_event` takes its `source_is_directory` / `destination_is_directory` from the
same `IncomingItem` and `symlink_metadata` answer `ClashKind` reads, ❌ never from the link-following stat its sizes and
mtimes come from. And every caller hands over the pair that's really meeting: the copy engine's folder→file branch finds
the blocking file while creating a CHILD's parent, so it maps the file back to the source folder that lands there
(`incoming_folder_for`) and the prompt asks "replace this file with this whole folder?", with the folder's size from the
drive index like any folder side. Pinned end to end per engine and shape in `transfer/conflict_prompt_sides_tests.rs`.

**Validation runs inside `spawn_blocking`.** The `*_files_start` functions return an `operationId` immediately, before
any filesystem I/O. Validation (`validate_sources`, `validate_destination_writable`, etc.) runs inside the handler
closure on the blocking thread pool. This keeps the Tauri IPC handler non-blocking, so the frontend can always open the
progress dialog and offer cancel, even if a network mount is stalled.

**`start_write_operation` emits `write-error` for handler errors.** The spawn wrapper matches on the handler's `Result`:
`Ok(Ok(()))` and `Ok(Err(Cancelled))` are no-ops (handlers already emitted the right events), `Ok(Err(e))` emits
`write-error` as a safety net, and `Err(join_error)` handles panics. Double-emit is harmless because the frontend's
`handleError` removes all listeners on first receipt.

**A volume-aware op never turns its own `Cancelled` into a `write-error`.** The inner handler already emitted
`write-cancelled` on that path, so the outer arm passes `WriteOperationError::Cancelled` through silently
(`transfer/volume/copy.rs`, `local_start.rs`'s safety net, and the archive-edit driver all order it this way).
Re-emitting would show the user a failure for something they asked for, and the retained-failure list would have to
filter it back out (§ "Retained failures" excludes `Cancelled` by typed variant for exactly this reason).

**`cancel_all_write_operations` is the quit teardown's first move, and its ONLY caller is the quit gate.** ❌ A window
going away is not a reason to stop work: an operation outlives the view watching it, so nothing in the frontend may
reach for this. `crate::quit::tear_down_and_exit` calls it with keep-partials semantics, waits up to 1.5 s for the
operations to answer, and only then escalates. `crate::quit::DETAILS.md` § "The teardown's order".

**`abort_all_write_operations` is the second tier, and is ❌ NOT this.** It fires `WriteOperationState::backend_abort`
on top of a cooperative cancel, which makes the cross-volume streaming path stop WAITING for a backend rather than
asking it to stop — buying a bounded wind-down at the cost of the backend's own partial cleanup. That is a trade only a
deadline holder may make, so the caller is the quit gate and nothing else; a teardown that can still afford to wait
stays on `cancel_all_write_operations`. Mechanism, cost, and the invariant it must not break: `src/transfer/DETAILS.md`
§ "Two tiers of cancel". The per-operation `abort_write_operation` is `#[cfg(test)]`: a deadline always aborts
everything, so production has no use for the narrower form.

**A local blocking read or write cannot be given a timeout, which is why there is no tier 2 for the local path.** The
chunked engine checks the cancel before each 1 MiB read (`transfer/chunked_copy.rs`), which is near-instant on a
responsive filesystem and unbounded on a hung one: the `read` and `write_all` themselves are plain blocking calls, and
nothing can put a clock on them. `O_NONBLOCK` is specified not to apply to regular files, a hung SMB or NFS client
blocks down in the VFS whatever the descriptor says, `pthread_kill`-to-`EINTR` is defeated by macOS restarting most
filesystem syscalls, and unwinding a thread mid-`write_all` is not something we can make safe. (Reasoned from POSIX and
documented macOS syscall-restart behavior, not measured, 2026-08-20.) ❌ So don't add a "timeout" here. The only
available move is to stop WAITING for the thread, which is what the quit deadline does, and what
`deadline::blocking_with_timeout` already does elsewhere: the blocking thread runs on, the caller gives up. Abandoning
it costs nothing because every local write stages, so a wedged worker is only ever filling a `.cmdr-tmp-*` nobody will
rename.

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

## Who speaks `MutationError`

Four command families answer with it, so every instant-mutation surface matches ONE exhaustive union: `rename_file`,
`check_rename_permission`, and `move_to_trash` (`commands/rename.rs`); `create_directory` / `create_file`
(`commands/file_system/write_ops.rs`); `check_rename_validity`, whose own answer is a typed `RenameValidityResult` so
the only `Err` it can produce is the deadline or a panicked task; and `paste_clipboard_as_file`, which is a managed
`CreateFile` op under the hood and refuses the way one does (`create_numbered.rs`). A volume's own refusal rides through
`MutationError::Volume` carrying the whole `VolumeError`. Full rules: `docs/guides/error-handling.md`.

## A slow instant mutation says it is still running (`mutation_reply.rs`)

`create_directory`, `create_file`, and `rename_file` answer within `MUTATION_REPLY_DEADLINE` (2 s) with a
`MutationReply`: `Done`, or `StillRunning { pendingId }`. A refusal inside the deadline stays the command's
`Err(MutationError)`. Past it, `reply_within` keeps the work's join handle (`deadline::race_detached`) and a follow-up
task emits `mutation-settled { pendingId, outcome }` when the work ends: `Landed`, or `Refused { error }` with the same
typed refusal an in-time reply carries (a panicked task is `Unexpected`). The frontend's `awaitMutation`
(`apps/desktop/src/lib/tauri-commands/mutation-reply.ts`) listens BEFORE invoking, since the event can overtake the
reply.

`paste_clipboard_as_file` follows the same contract with the created file riding along: `PasteClipboardReply` /
`clipboard-paste-settled`, built on `reply_or_hand_off` (the value-carrying core `reply_within` wraps), awaited by
`awaitClipboardPaste`. The frontend shows a "still pasting" toast past the deadline, and a late landing only toasts the
file name: no cursor land, no auto-rename, since the person may have moved on.

**Why not `TimedOut`.** The deadline never bounded the work, only the reply: on a busy NAS a new folder took 7–12 s, the
dialog said it timed out, and the folder appeared anyway, sometimes (ERR-AREUV, 2026-10). An answer the user sees
disproven is worse than "still working".

**Why 2 s.** The deadline gives up on nothing; it only decides when the person hears the volume is slow, so it sits just
above a healthy share's create or rename (well under a second). ❌ Don't stretch it with `io_budget`: a longer wait only
keeps the dialog silent longer.

**Why no hard limit behind it.** A session's transport already ends a dead server's work (smb2 declares it dead and the
request errors), and a kernel mount that blocks for minutes really is still working for those minutes. The frontend
keeps saying so, and the person can close the dialog; the work was never cancelable mid-syscall anyway.
`MutationError::TimedOut` stays for the reads and the trash path that still use it.

## The rename pre-flight, and who it applies to

`check_rename_permission_for_volume` (`rename.rs`) is what the inline editor asks before it opens: parent writable
(`access(W_OK)`), not user-immutable, not SIP-protected. All three are local syscalls, so **whether the check applies at
all is the volume's answer, ❌ never the shape of its id**: it runs when `volume_id` is `"root"` or a registered volume
reports `supports_local_fs_access()`, and returns `Ok` untouched otherwise, an id that resolves to no volume included
(an unmount race is the rename's to word, and it does).

A volume serving its own I/O spells its paths with a scheme (`sftp://root@host:22/srv/data/x.xml`), which
`symlink_metadata` can't open, so running the check there doesn't answer the question — it manufactures a `NotFound`
carrying the URL. ERR-KVERS was exactly that: the frontend gate skipped only `mtp-`-prefixed ids, so SFTP, WebDAV, and
ADB each inherited a rename that refused before anything was attempted, and the user saw "There's nothing at "sftp://…"
any more" four times in 45 seconds. Skipping costs nothing: `rename_managed` is the authority and refuses with the
backend's own typed answer.

**Both refusal points log, and say which one they are.** `log_rename_refusal` takes a `RenameStage` (`rename pre-flight`
/ `rename`) and writes one line at `target: "volume"` — `debug` for an ordinary typo (`AlreadyExists`, `NameEmpty`, …),
`warn` for anything technical, where the `Display` carries the errno. Without the stage a reader can't tell which half
answered, and without the line at all a refusal reaches the error-report bundle over an empty log: that was ERR-8RFN4,
and the pre-flight kept that blind spot for one release after the rename closed it. Only the file log chain is `Debug`
unconditionally, so both levels land in the bundle.

## Renames that run as moves

**Decision**: every caller of `Volume::rename` asks `Volume::rename_work` first, and an entry that renames by copy (an
S3 folder, or an object past the 64 MiB part floor) is never handed to `rename`: it runs as a same-volume move with a
new NAME (`target_names.rs`) through `routing::start_rename_by_move`. **Why**: each caller assumed one cheap call (F2
waits 5 s on an instant op; a bulk rename hops name by name), while on an object store a folder rename is one copy and
one delete per object, which needs a scan, byte progress, pause, cancel, conflicts, and journaling. The engine side:
`src/transfer/volume/DETAILS.md` § "A same-volume move whose renames copy".

- **F2** (`rename_managed`): an entry that copies starts the background move and answers `Ok` once it has started, the
  way an in-zip rename answers once its edit driver is running; the sink is the global one the archive route uses. The
  progress chip shows it. Before that, `check_rename_validity_impl` reports `by_move` (`RenameByMove`: files and bytes
  from a bounded count, at most `SMALL_RENAME_FILES` + 1, and `confirm_first`), and the frontend opens the Move dialog
  prefilled with the new name instead of renaming when `confirm_first` is set (past 100 files, a count that couldn't
  finish, or an estimate that doesn't round to zero); it confirms through the `rename_by_move` command. The estimate
  prices the count's own files (`s3_costs::estimate_rename`, no extra request), which is what catches Wasabi billing a
  young object's remaining days on even a one-file rename. The engine doesn't price: the check returns a
  `RenameValidityCheck` carrying the complete count as `ScanCostFacts`, and only `priced(costs_something)` turns it into
  the editor's `RenameValidityResult`, so the IPC layer (`commands/rename.rs`) can't forget the cost and the engine
  needs no data dir. The dialog's scan preview then feeds the cost line it shows
  (`apps/desktop/src-tauri/src/s3_costs/DETAILS.md`); a rename that starts without the dialog costs nothing worth a
  line.
- **The MCP rename tool** (autoConfirm) calls `rename_file_replying`, so it takes F2's route; a `StillRunning` reply
  becomes an `OK` saying the rename is still running.
- **Bulk rename and Ask Cmdr's proposals** go through `start_renames`: every row's `rename_work` is asked (eight at a
  time; free on a volume that renames in one call), and a batch with any copying row runs as ONE move with the new names
  (`rename/bulk/by_move.rs`): the executor's dependency order (a chain moves its last link first), conflicts skipped (a
  name something outside the batch holds keeps its owner, the executor's answer too), the sources bound to their
  preflight fingerprints. ❗ A swap (`a ↔ b`) is left out: it would need a temporary name, and a move onto a FOLDER
  that's still there merges into it. `RenamesStarted::swaps_left_out` counts its rows; the review's apply
  (`apply_bulk_rename`) hands the count to the thread's result line ("Skipped 2 renames that swap names…"), while an
  approval through the suggested-ops bridge only logs it. An all-one-call batch keeps the executor.
- **A same-volume move** routes itself (`move_same.rs`), and so does the **operation log's undo**: a same-volume restore
  whose rename copies goes back through the staged per-file move (`rollback.rs`).

## Look-alike names

`look_alike.rs` answers one question for every write that makes a name: does the folder already hold it under another
Unicode spelling? A byte-exact volume (an SMB share, SFTP, a phone) keeps `café` composed and decomposed as two entries
and finds each only by its own bytes, so without this a write asking in the other spelling hears "free" and stands an
identical-looking twin beside the user's entry.

- **Only a difference in FORM counts** (`cmdr_fs::name_fold::differ_only_in_form`): a person can't tell the two apart.
  Case differences read as two names, and a case-sensitive volume keeps both on purpose.
- **It costs a listing only when it can matter**: never for an ASCII name, never on a volume whose lookups match any
  form (`Volume::matches_names_in_any_unicode_form`, APFS), otherwise one listing of the folder after the exact lookup
  missed.
- **A pane's listing stands in for that read where a person is waiting on a name**: new folder, new file, rename (and
  its live validity check), and bulk rename build `ListedFolders::with_pane_listings(volume, volume_id)`, which asks the
  fresh-listing oracle (`Listings::authoritative_listing`, `EveryWriter` coverage only) before listing. **Why:** the
  folder being named into is almost always the one the pane shows, and on a busy NAS re-reading a 3,340-entry share
  folder per new name cost a second or more (ERR-AREUV, v0.50.0). **Why it's safe:** the oracle's debounce can lag
  another client by ~200 ms, which risks only a missed twin: a second, look-alike entry, never an overwrite, since the
  volume holds the two spellings as two names, and an exact clash stays the backend's to refuse as before. A
  `ThisMachineOnly` watch (an OS-mounted share) doesn't qualify: the twin this check exists for is usually another
  client's spelling. The transfers, compress, and the dialogs' existence probe still list (`ListedFolders::new`,
  `look_alike_in`): a transfer asks mid-write, while its own landings may not have reached the cache yet. Both paths log
  at DEBUG under target `look_alike`, the read with its duration. Cells: `look_alike_tests.rs`.
- **New names take the volume's spelling** (`Volume::spell_new_name`; SMB, SFTP, and WebDAV compose). `place_new_entry`
  is the instant ops' door: a new folder or file is refused as `AlreadyExists` beside a look-alike; a rename's target is
  refused the same way unless the user confirmed replacing it, which then replaces the look-alike under ITS spelling
  (one entry). A rename whose look-alike is the entry being renamed is a respell, which is free. The rename editor's
  live check (`check_rename_validity_impl`) reports the look-alike as the conflict, and so does a typed name whose
  composed spelling the folder holds exactly (the rename itself refuses that one with the backend's `AlreadyExists`).
  Local root renames skip all of it. A target that matched the SOURCE in another case (Samba and a QNAP fold case) is
  settled from the same listing (`ListedFolders::entries_in`): source's name listed, target's not, so it's a case-only
  rename of one file and F2 asks nothing.
- **Respelling can land on an exact name.** `place_new_entry` answers `Free(spelled)` when the folder holds the SPELLED
  name byte for byte (`among` treats an exact entry as "no look-alike"); a create or rename then hears the backend's
  `AlreadyExists`. A write that would REPLACE silently instead must ask `look_alike_in` with the ASKED name, which finds
  that entry (compress does).
- **Bulk rename (`rename/bulk/plan.rs`)** respells every destination once, in `start_bulk_rename`, before the plan, the
  run, and the journal see it (non-root volumes only). `settle_remote_conflicts` then treats a destination held under
  another spelling like an exact clash: the row is `Skipped` (`Ambiguous` too), a listing failure fails it, and a
  look-alike that is the row's own source is a respell. It uses `ListedFolders`, so a batch lists each folder once.
  Rotation safety: the check only runs for destinations no active row vacates (`rows_with_unclaimed_destination`, keyed
  on `normalize_for_comparison`, which folds form on macOS), so a look-alike another row moves away is never a clash and
  the planner orders the two rows. Off macOS that key is exact, the vacating row goes unseen, and the row is refused
  rather than landing beside the entry it waits on. Rotation temporaries are `.cmdr-bulk-rename-<numeric id>-<uuid>`,
  ASCII, so they have one spelling. Cells: `rename/bulk/look_alike_tests.rs`.
- **Compress (`archive_edit/compress.rs::archive_landing`)**: on a remote parent, the exact stored spelling is
  overwritten; else ONE look-alike (either direction) is replaced in place, under its stored spelling, and counts as
  "existed" (not rollbackable, like any overwrite); else the target is a new name, spelled the parent's way. Two or more
  look-alikes are refused as `DestinationExists` before anything is seeded. Replacing is safe because the dialog warned:
  its overwrite warning asks `destination_exists` (`transfer-dest-exists.svelte.ts`), which counts a look-alike via
  `held_in_another_spelling`, and MCP auto-confirm refuses to proceed on the same answer. Cells:
  `archive_edit/compress_remote_tests.rs`, `apps/desktop/src-tauri/src/commands/file_system/destination_exists_test.rs`.
- **The dialogs' existence probe**
  (`apps/desktop/src-tauri/src/commands/file_system/path_checks.rs::destination_exists`) is the look-alike-aware twin of
  `path_exists`, which stays byte-exact for the panes (a remembered path and the eviction poll then LIST the path, and a
  "yes" for a look-alike would send them to a miss).
- The transfers apply the same rule through `transfer/volume/landing.rs`: `src/transfer/volume/DETAILS.md` § "Look-alike
  names and new-name spelling". Extract lands through the same merge levels. Instant-op cells:
  `look_alike_instant_tests.rs`; Docker: `smb_look_alike_test.rs`, `sftp_look_alike_test.rs`, and
  `webdav_look_alike_test.rs`.
- **Not covered, on purpose**: paste-as-file (`pasted (N).<ext>`, ASCII), rescue names (` (recovered)` off a name the
  landing already spelled; an error path that never overwrites), remote archive-edit temps (`.cmdr-tmp-<uuid>`), and
  undo (it restores an entry's own stored bytes).

**Decision**: new names Cmdr creates on SMB, SFTP, and WebDAV go out composed (NFC); MTP, ADB, and local volumes keep
the name as given. Names that already exist on the server are never rewritten, an operation on an existing entry (read,
delete, overwrite, merge, same-server move, navigation) uses the exact bytes the server listed, and a download keeps the
server's bytes. **Why**:

- **SMB**: a decomposed name on a share is one that Finder over the kernel mount (which composes on lookup), Windows,
  and Linux clients list and then can't open (`ERR-VETBX`, `crates/cmdr-smb/DETAILS.md` § "SMB names are opaque bytes").
  Apple's own SMB client composes too.
- **SFTP and WebDAV**: the server is usually Linux, which stores a name's bytes as sent, and web servers, PHP, and
  scripts there match bytes. macOS hands out many local names decomposed, so an NFD `café.jpg` uploaded as-is looks
  right in every listing and breaks every link and script that spells it the usual (composed) way.
- **What other tools do** (as surveyed 2026-09-23): Cyberduck NFC-normalizes every local path it sends
  (iterate-ch/cyberduck#5162), FileZilla sends NFC, and Git on macOS precomposes (`core.precomposeUnicode`). OpenSSH
  `sftp`/`scp` and `rsync` send bytes as they are, which is how decomposed names end up on servers.
- **MTP and ADB** never composed, and nothing measured says their clients expect it.

Look-alike detection is what makes composing safe: an incoming composed name finds a decomposed entry the server already
holds as taken, so it's a conflict (or an in-place overwrite), never a second identical-looking entry. Changing a
backend is one `composes_new_names` override.

## Naming the folder that refused a write

`PermissionDenied` carries three things beyond its path and the OS's sentence: the raw `errno`, a `refusal` derived from
it, and `refused_folder`. `WriteOperationError::permission_denied` is the only thing that builds the variant, so
`refusal` can't contradict the errno it came from.

**Why the errno has to travel.** Rust folds `EACCES` and `EPERM` into one `ErrorKind::PermissionDenied`, and they are
opposite advice: `EACCES` is a folder's own permissions, which an administrator could change, and `EPERM` is macOS
itself (SIP, an immutable flag, a privacy protection), where administrator rights change nothing. Sending someone to
chase permissions for the second is a wrong answer they'd act on. `PermissionRefusal` is that distinction as a value,
and it's what `docs/specs/elevated-file-operations.md` decision 12 branches on when the root helper lands.

The errno survives the volume layer too: `VolumeError::PermissionDenied { path, raw_os_error }` carries it from
`VolumeError::from_io_at`, and `map_volume_error` hands it to the constructor, so a volume-routed refusal (a
cross-volume move's source delete hitting a Finder-locked file, cmdr-reports#17) gets the same advice as a local one and
the details block shows the OS's sentence rather than the path twice. A backend that words its own refusals (SMB, MTP,
and ADB, whose errno is the DEVICE's and would earn macOS advice for an Android file) carries `None` and stays
`Unclassified`. An errno-less refusal on an S3 app path (`server_of_path`) is `ObjectStoreAccount` instead
(`WriteOperationError::object_store_refused`): the key's permissions or a provider that paused the account (a usage cap,
a billing hold) answer alike, so its advice names both.

**Why the folder is PROVED, never inferred.** A `rename(2)` needs write access to both parent folders, a `unlink(2)` to
the one it removes from, and a `create_dir_all` to the deepest ancestor that exists; no errno says which refused. So
`validation::refusing_folder` asks the OS on the refusal path: the entry's parent first (its write bit is what governs
creating, removing, and renaming an entry), walking up past ancestors that don't exist yet, then the entry itself when
it's a folder. `access(W_OK)` evaluates ACLs as well as the mode bits on macOS, so it's the honest question.

❗ `None` when every candidate answers "writable" (an ACL the probe and the write read differently, a race, a read
refusal rather than a write one), and the copy then falls back to its per-operation sentence. ❌ Never fill it in from
the operation's shape: ERR-4TEMD was a same-volume move out of a folder the user's macOS account can't change, and the
copy told them to check the destination, which was fine. They finished the job with `sudo` in Terminal.

The probe runs on the REFUSAL path only, so the happy path pays nothing: at most two `access(2)` calls plus the walk's
stats, on a path the failing syscall just answered for. Backends that word their own refusals (MTP, SMB) carry no errno
and no folder, since nothing there proved one.

## Cmdr-own-write hook (downloads watcher)

Every write-op driver MUST announce its destination BEFORE issuing the syscall. The host's answer registers it with the
downloads watcher's ignore set (`operations_host/own_writes.rs` → `downloads::note_pending_write_for_cmdr`), which is
what makes the watcher silently suppress events Cmdr itself caused, so the user doesn't see a "Downloaded foo.bin" toast
when they just used Cmdr to copy 100 files into `~/Downloads`.

**Contract:** call `state.scope().host().announce(&dest_path)` (`EngineHost::announce`, the one way engine code reaches
the `OwnWrites` seam, `host/own_writes.rs`) immediately before the write syscall (or the volume-trait equivalent:
`Volume::write_from_stream`, `Volume::create_file`, `Volume::create_directory`, `Volume::rename`, `Volume::delete`).
Once per path per file landing, never per chunk: the seam's `dyn` call sits beside a syscall, which the dispatch rule
allows. The writers that create or rename onto a path take that destination as the returned `Announced`
(`overwrite::stage_and_land_file` and the copy strategies above it, the local move's `rename_onto_free_name` and
`move_resolved_into_place`, and the volume engine's `StagedWrite::begin`, which announces its own temp), so calling one
unannounced doesn't compile. Removals, creates through `Volume::create_*`, the archive mutator's `note_pending` hook,
and rename SOURCES still announce by hand.

Three writes that are easy to miss announce too, each pinned in `own_write_announce_tests.rs`: an overwrite announces
the aside (`<dest>.cmdr-temp-<uuid>`) before renaming the person's original onto it, a Keep-both copy announces each
numbered placeholder before its `O_CREAT|O_EXCL` create (`unique_name::find_unique_name` takes the host for it), and a
fresh compress announces its zip and temp (`StagedWrite::begin_generated` takes the zip as an `Announced`).

**Locked-in scoping:** the prefix check lives INSIDE the downloads helper (and the underlying
`IgnoreSet::note_pending`). Call sites invoke unconditionally; paths outside the resolved Downloads root silently no-op.
**Don't add `if path.starts_with(downloads_dir)` guards at call sites**: centralizing the scope in the helper keeps it
from drifting across call sites (the downloads watcher's ignore-set design lives in the `downloads` module docs).

**No-op when the watcher is dormant.** If the FDA gate is closed (or `refresh_runtime` hasn't been called yet), the
watcher isn't installed and the helper is a cheap no-op (single mutex `lock + is_none`). Production write ops fire
freely; the cost is one atomic-bool read per write.

**Renames register both halves.** A rename moves a file out of one location into another. The Cmdr-own-write contract
requires registering both the source path (so a rename-OUT-of-Downloads is also suppressed via the watcher's
rename-from-ignored-source branch) and the destination path (so the rename-arrival event is suppressed). See
`commands/rename.rs::rename_file` and `transfer/move_op/` for the pattern.

**Cross-volume writes that land on a local FS** (MTP→Local, SMB→Local) announce via
`EngineHost::announce_on(volume, path)`, which anchors the path on `volume.local_path()` and skips when the destination
isn't a local-FS-backed volume (MTP/SMB/InMemory) — those paths can't trigger the watcher anyway.

Example placement:

```rust
// In `copy_single_item` (transfer/copy/single_item.rs), just before `copy_file_with_strategy`:
let announced_dest = state.scope().host().announce(&actual_dest);
let outcome = copy_file_with_strategy(state, source, announced_dest, ..)?;
```

See also: `apps/desktop/src-tauri/src/downloads/CLAUDE.md` for the watcher architecture, ignore-set internals, and the
FDA-gated lifecycle. End-to-end safety net for the contract lives in
`downloads::runtime::tests::note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`. Each write path's exact
announce sequence (which paths, in what order, and whether each existed yet) is pinned in `own_write_announce_tests.rs`,
each cell on an engine of its own whose `OwnWrites` is `host::testing::RecordingOwnWrites`; a new write path gets a cell
there. That the app's answer reaches the downloads hook is pinned beside it, in `operations_host/own_writes.rs`.

## The engine's host

What the engine needs from the app around it (the runtime, the volumes, the engine-wide events, the per-op sinks, and
the optional seams), what each seam's contract is, and where Cmdr answers it: `src/host/DETAILS.md`.

## The public surface is capped

`index-crate-isolation` holds this crate to 86 root promises, 58 `Operations` methods, one public module (`host`), and
25 public items inside it (`scripts/check/checks/index-crate-isolation-ceilings.go`). The numbers are exactly what the
crate exposes, measured with no headroom, so an addition has to be argued for. Everything counted exists for a named
caller.

Root promises (86), by who needs them:

- **The handle** (3): `Operations`, `OperationsBuilder` (from `Operations::builder`), and `AlreadyClaimed` (a second
  `claim_ledger`'s refusal).
- **The event sinks** (3): `OperationEventSink` and `ScanPreviewEventSink`, which the app's Tauri sinks implement
  (`operations_host/sinks.rs`), and `ArmedConflict`, the only thing `emit_conflict` takes.
- **The queue** (6): `OperationSnapshot` and `OperationsChanged` (`operations-changed`'s payload, which the app wraps
  for the wire and the MCP state resource reads), and `PauseOutcome`, `PauseAllOutcome`, `QueueMoveDirection`, and
  `QueueMoveOutcome`, the queue controls' typed answers the IPC commands and the MCP `queue` tool return.
- **The transfer tree's window** (6): `TransferTreeView`, `TransferTreeViewRequest`, `TransferTreeRow`, `TreeRowKind`,
  `TreeRowState`, and `TransferTreeViewError`, for `get_transfer_tree_view` and `cmdr://state`.
- **Instant mutations** (11): `MutationError` (the typed refusal every instant command returns), the
  reply-within-a-deadline contract the IPC commands answer through (`reply_within`, `reply_or_hand_off`,
  `MUTATION_REPLY_DEADLINE`, `Replied`, `MutationReply`, `MutationSettled`), and rename's batch and validity vocabulary
  (`BulkRenameRow`, `RenameStartError`, `RenameValidityResult`, `RenamesStarted`), which `commands/rename.rs`,
  `multi_rename`, and the agent's suggested renames speak.
- **Pre-flight and undo** (9): `SourceItemInput`, `VolumeScanError`, and `CONFLICT_CHECK_BUDGET` (the transfer dialog's
  conflict check), `ScanCostFacts` (the S3 estimate prices a preview's counts), the source binding a reviewed batch
  supplies (`ExpectedSources`, `LocalContent`, `RemoteContent`, `SourceFingerprint`, for the agent's approved ops), and
  `UndoReport` (`undo_operations`' answer).
- **Engine-free helpers the IPC edge asks** (7): `trash_routing_for_selection` and its `TrashRoutingAnswer` (F8's
  trash-or-delete question), `trash_dir_for_path`, `move_to_trash_sync` (the bare, unjournaled trash move, for the
  open-with reaper's changed copies, which no undo should list; a second objc trash call app-side would fork the refusal
  classification), `held_in_another_spelling` (`destination_exists`'s look-alike half), `same_local_file` (rename's
  case-only check), and `resolve_dest_path` (anchoring a volume-relative destination).
- **The wire vocabulary** (40): the payloads, configs, and typed outcomes from `types` that cross IPC unchanged
  (`WriteOperationConfig`, `VolumeCopyConfig`, `WriteOperationError`, the `Write*Event` and `ScanPreview*Event`
  payloads, `ConflictResolution`, `LifecycleStatus`, `OperationStatus`, …). The Locked-items question added three (its
  event, `LockedSourcesAnswer`, and `LockedSourcesAnswerOutcome`), the same shape as the sources-gone question's.
  `ipc.rs` registers them; `bindings.ts` is generated from them, so none can go private without the frontend losing a
  type.
- **`host`** (1): the public module below.

`Operations` methods (58), grouped by caller below. The two `testing`-gated ones (`render_live_transfer_dump` and
`route_archive_copy_into`) count apart. Each IPC command, MCP tool, and quit step is a one-line pass-through, so the
verbs read one-to-one:

- **Building and lifecycle**: `builder`, `host` (the MCP rollback takes its sink factory), `operation_log` (the startup
  pin proves the engine journals into the app's log), `claim_ledger`, and the quit gate's `cancel_all_write_operations`,
  `abort_all_write_operations`, and `flush_in_flight_temps`.
- **Starting an operation**: `move_files_start`, `delete_files_start`, `trash_files_start`, `start_volume_copy`,
  `start_volume_move`, `start_volume_compress`, `start_rename_by_move`, `start_renames`, `dispatch_rollback`,
  `undo_operations`, and `trash_single_journaled` (the single-item trash command in `commands/rename.rs`).
- **Instant mutations**: `create_directory_managed`, `create_file_managed`, `create_numbered_file` (macOS paste as
  file), `rename_managed`, `check_rename_validity_impl` (priced app-side into the wire answer), and
  `check_rename_permission_for_volume`.
- **Previews and pre-flight**: `start_scan_preview`, `cancel_scan_preview`, `get_scan_preview_totals`,
  `cached_cost_facts`, `scan_volume_for_conflicts_within`, and the agent bridge's `resolve_source_volume` and
  `unregistered_source_error`.
- **The queue and one operation's controls**: `list_operations`, `latest_operation_progress`, `get_operation_status`,
  `operations_need_volume`, `cancel_operation`, `cancel_operations`, `cancel_write_operation`, `pause_operation`,
  `resume_operation`, `pause_all`, `resume_all`, `move_queued_operation`, `place_queued_operation`,
  `retry_failed_operation`, `dismiss_failed_operation`, `dismiss_all_failed_operations`, `resolve_write_conflict`,
  `pending_write_conflict`, `continue_without_gone_sources`, `pending_sources_gone`, `answer_locked_sources`,
  `pending_locked_sources` (the two parked questions, each an answer plus a read for a surface that arrives late), and
  `transfer_tree_view`.
- **Busy volumes and the sink adapter**: `busy_volume_ids` (Eject's guard), `register_external_volume_op` /
  `release_external_volume_op` (drag-out's busy volume, macOS), and `retain_failure` (the app's sink adapter keeps a
  failed op on the queue).

An entry point production reaches only from inside the engine (a routed start like `archive_edit_start` or
`copy_between_volumes`, the raw pause gate) stays `pub(crate)`; an app-side cell drives it through a `testing`-gated
door that forwards to it (`testing::copy_between_volumes`, or the gated `Operations::route_archive_copy_into`).

Every public verb that names an operation takes `&OperationId` (or `&[OperationId]`), so a volume id, a preview id, or a
path can't pass for one. The IPC commands and MCP tools wrap the incoming `String` once, so the wire (`bindings.ts`)
still carries plain text. Inside, the control plane holds the type too: the manager's records, line, and guards, the
state registry, a preview's claim, and the scan bridge. The progress path below it (the drivers, the status cache's
`update`, the probe registry) speaks the same `&str` its events carry, and a start site mints the id typed and lowers it
there.

`host` (25 items): the seam traits the app answers (`OperationVolumes`, `EngineEvents`, `OwnWrites`, `OperationIndex`,
`Pacing`, `NameOrder`, and the two event sinks, re-exported there too), their value types (`OperationErrorReport`,
`AsideAttempt`, `TreeFailure`, `InstantMutationFact` and its `InstantOp` / `InstantOutcome` / `InstantTarget` tokens,
`ExpectedTotals`, `BufferedLeaves`, `Leaf`, `Announced`, `UnregisteredVolume`, `VolumeAnswer`), `OperationSinkFactory`,
and `EngineHost` with the methods a caller reads it through. The seam files are private; a host names a seam only as
`cmdr_operations::host::X`.

`testing` is gated behind the `testing` feature and counts apart: an engine of a cell's own, the fakes it reads back,
the internals the app's backend suites and routing cells drive, and the Criterion bench's door (`testing::bench`). It's
the host's test surface, never a promise to a shipped build.

## Events emitted

- **`write-progress`**: Every ~200 ms during copy/move/delete/trash
- **`write-conflict`**: Stop mode hit a conflicting destination file
- **`write-complete`**: Operation finished successfully. A cross-volume copy or move also carries `metadata_left_behind`
  when the destination couldn't keep something a source item had (`metadata_losses.rs`), and a local copy carries
  `durability_unconfirmed` when its closing flush couldn't confirm the files are on the disk (`src/transfer/DETAILS.md`
  § "Durability"). A move that left Locked items in place carries `locked_kept` (§ "Moving Locked items").
- **`write-cancelled`**: Operation cancelled. Carries `rollback: CancelRollback` — the three-state outcome
  (`notRolledBack` / `rolledBack` / `partiallyRolledBack`), how many items the reversal undid, and what it left behind
  grouped by `SkipReason`.
- **`write-error`**: Operation could not complete. Carries only `error: WriteOperationError` (typed, word-free); no
  rendered prose crosses IPC. The FE renders the title/explanation/suggestion + category from this typed error via
  `transfer-error-messages.ts` in `TransferErrorDialog` and applies category-based colors.
- **`write-settled`**: Emitted once per op after the spawned background task fully returns. See
  [Settle contract](#settle-contract).
- **`volumes-busy-changed`**: The set of volume IDs with an in-flight op changed (an op started or finished). Payload is
  `string[]`. See [Busy-volumes set](#busy-volumes-set).
- **`operations-changed`**: The operation registry's membership or lifecycle status changed. Thin snapshot
  (`{ operations: OperationSnapshot[] }`), NOT 200 ms progress. See [Operation manager](#operation-manager).
- **`write-locked-sources`**: A move found Locked items in what it moves and waits for an answer before any byte moves.
  See [Moving Locked items](#moving-locked-items).
- **`write-source-item-done`**: A top-level source item is finished with, and how (for gradual deselection). Every
  transfer engine reports it, the volume ones included. See [Per-source outcomes](#per-source-outcomes).
- **`dry-run-complete`**: `config.dry_run == true` (returns `DryRunResult`)
- **`scan-preview-progress`**: During `start_scan_preview`
- **`scan-preview-complete`**: Preview scan finished
- **`scan-preview-error`**: Preview scan failed
- **`scan-preview-cancelled`**: Preview scan cancelled

**MCP consumer note**: the MCP server records each op's terminal outcome into a bounded ring (`mcp::terminal_ops`) at
the `TauriEventSink` emit sites for `write-complete` / `write-cancelled` / `write-error`, so its
`await operation_complete` can report a settled op's status. It has to tap these terminal events specifically:
`operations-changed` fires AFTER removal-on-terminal, so a completed or cancelled op leaves the snapshot entirely. A
FAILED op is the one exception — it stays on the snapshot as a retained row (see
[Retained failures](#retained-failures)) — but the ring still owns `Completed` / `Cancelled`. See `mcp/DETAILS.md` §
State stores.

## Operation manager

The full model and the why behind each decision are captured in this section. Design history is in git (former
`docs/specs/2026-06-21-transfer-queue-pause-plan.md`).

`manager.rs` is the single coordinator every write op flows through. It exists because there were FIVE independent spawn
paths (`start_write_operation` for local copy/move/trash + local delete; the volume-delete branch in
`delete_files_start`; `copy_between_volumes`; `move_between_volumes`; `move_within_same_volume`), each hand-rolling its
own `tokio::spawn` + state-insert + status-register + `WriteSettledGuard`, and an op always spawned immediately. The
manager unifies them behind `spawn_managed(descriptor, state, deferred)` and adds a registry with real lifecycle states
plus **lane-based admission** that can serialize ops which would thrash a shared device.

### Lanes and `Volume::lane_key()`

Each op touches the [`LaneKey`](../../apps/desktop/src-tauri/src/file_system/volume/CLAUDE.md)s of its source and
destination volumes (same-volume ops touch one). Lane keys come from `Volume::lane_key()` (in `volume/mod.rs`), NEVER
from parsing a `volume_id` string. Per backend: `LocalPosixVolume` → the volume root (the trait default; each local
mount is its own `LocalPosixVolume`, so the root IS the mount root); `MtpVolume` → `device_id` (one USB pipe per device,
so every storage on a device shares its lane); `SmbVolume` → its `volume_id` (already
`smb_volume_id(server, port, share)` — server+share granularity); `InMemoryVolume` → a `with_lane_key(key)` builder,
defaulting to root so the ~169 existing `new(...)` sites are untouched and tests opt into same-lane vs different-lane.

The both-local branches of `copy_between_volumes` / `move_between_volumes` compute the two lane keys from the live
volume handles and pass them into `copy_files_start` / `move_files_start` as `Some(lanes)`. The plain local commands
pass `None`, so the entry point derives a lane from `volume_ids` (`local_lanes`): empty → the `root` lane, else one lane
per id. This is a faithful proxy for `lane_key()` on the path where no `Volume` handle is threaded through — it uses
each id as an opaque whole, with no substring parsing.

### Admission — global FIFO, atomic multi-lane reservation

The manager keeps one ordered queue (`order`) plus a `lane_use` table (lane → in-use count; budget 1 per lane in v1, a
lane is free iff its count is 0; a `HashMap` not a set so v2 budgets > 1 reshape nothing). An admission pass walks
pending ops front to back (registration order unless the user reordered the line, § "Reordering the line") and admits
the first whose EVERY lane is free, reserving all its slots atomically, flipping it to Running, registering its volumes
busy, and spawning its deferred start. It loops so one pass can admit several disjoint-lane ops. A two-lane op can't
starve behind churn on a single lane — there are no per-lane queues, so the multi-lane op is always considered at its
FIFO position against the whole lane table.

### Reordering the line (`move_queued` / `place_queued`)

The user can move a WAITING op one place earlier or later (the queue window's ⌥↑ / ⌥↓,
`move_queued_operation(id, direction)`), or drag it straight to a place (`place_queued_operation(id, position)`, 1-based
among waiting ops, clamped to the line). Both go through one `reorder` in `manager/reorder.rs`. `order` is then
admission order rather than strictly registration order.

- **Only waiting ops move, and only among each other.** The waiting ops are rewritten into the `order` slots waiting ops
  already hold, so a running or paused op between them keeps its slot. Its slot decides nothing anyway: it already holds
  its lanes. A step trades places with the nearest waiting op on that side; a placement shifts the waiting ops in
  between by one. A running, paused, settled, or unknown id answers `NotQueued` and changes nothing.
- **It answers with the new place** (`QueueMoveOutcome::Moved { position, waiting }`; `AtEdge` for a step off the front
  or back; `Unchanged` for a placement where it already is), counted among waiting ops, 1-based. The window announces
  that answer, so a screen reader hears where the op really is. Only `Moved` broadcasts.
- **No admission pass.** Every pass leaves no admittable waiting op behind, and a reorder frees no lane, so the pass
  would admit nobody. What a reorder changes is who goes FIRST when a lane frees, which is the whole feature
  (`manager::tests::reorder::the_op_moved_to_the_front_is_the_one_admitted_next`).
- **A new way to reorder is a new `Target` in `reorder.rs`**, ❌ not a second ordering mechanism.
- Tests run on an engine they own (`TestOperations`): "the nearest waiting op" is a question about the WHOLE line, which
  only a line no other test queues on answers.

### Deferred start, not "spawn then block on a semaphore"

A queued op holds only DATA describing how to begin: a boxed `FnOnce(Operations) -> Pin<Box<dyn Future + Send>>`
(`DeferredStart`). The manager spawns it only on admission, handing it the engine (the manager keeps a `Weak` to it and
upgrades there), so a queued start owns nothing that keeps a dropped engine alive. Blocking a spawned op on a lane
semaphore would pin a `spawn_blocking` pool thread idle per queued op — a leak that can deadlock the finite pool under
many queued ops. Each deferred future owns the op end-to-end (the `WriteSettledGuard`, the actual transfer/delete, the
terminal-event emit) and ends by calling `ops.manager().on_settled(id)` on the engine it was handed.

### Dequeue on settle — explicit, NOT in `Drop`

`on_settled(id)` (the happy path) frees the op's lane slots, removes it from the registry, cleans the state registry +
the status cache, and runs an admission pass (which may spawn the next op). It's sequenced after the terminal event,
exactly where the old per-site cache cleanup ran. The `ManagedTaskGuard` is the panic safety net: held by each spawned
task, its `Drop` frees lanes + cleans caches but NEVER spawns (no admission pass). Spawning during the previous op's
unwind would re-enter the manager mid-panic (abort) or deadlock on a lock held up-stack. So a panicking op still
releases its lanes, but the next op is admitted only on a healthy settle (the next registration's admission pass, or
another op's `on_settled`, picks it up). The happy path calls `task_guard.disarm()` right before `on_settled` so its
now-redundant Drop is a no-op. Pinned by `manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

### Observing an admission pass (`admission_passes`)

Admission is otherwise invisible from outside: a pass that walks the queue and admits NOBODY changes no status, emits no
event, and touches no lane. The negative assertions ("B must still be Queued after A settles") need exactly that moment,
and without a signal for it a test can only guess at a wall-clock span.

`OperationManager::admission_passes` is an `AtomicU64` bumped ONCE at the end of `run_admission_pass`, read by
`admission_pass_count()` (test-only). A test reads the count, triggers the settle, and waits for it to grow: an advance
means a pass walked the whole FIFO queue and either admitted the waiting op or declined to. Three properties make it
work:

- **Bumped LAST, and on every exit.** The signal means "the pass finished", so a new early `return` inside
  `run_admission_pass` that skips the bump would hang every waiting test. Nothing in production reads the count, so the
  bump is pure signal, never a decision input.
- **`SeqCst` on the bump and the load,** not `Relaxed`. The waiter reads the count, acts, and waits for it to advance,
  so the bump has to be ordered against the admission work it claims finished; a `Relaxed` counter buys a rarer, harder
  flake than the sleep it replaces.
- **Global count, global queue.** A pass triggered by an unrelated concurrent test is equally good evidence, because a
  pass considers every registered op, not just its trigger's.

`Notify` would be wrong here and `watch` is second-best: the settle path runs the pass BEFORE a test could await, so
`notified()` created afterwards is a guaranteed lost wakeup. Prefer a monotonic counter for any future "background work
reached a point" signal.

A test in `manager::tests` calls the private `run_admission_pass()` inline (it returns when the pass is done) for the
paths production never runs one on: `set_paused` admits nobody, so
`paused_running_op_does_not_admit_a_queued_same_lane_op` runs its own pass and asserts B is STILL Queued. Admission
flips a record to Running before it spawns, so a Queued status after a completed pass also proves the deferred start
never ran. The waits themselves use `cmdr_fs::testing::wait_until_async` (`docs/testing.md`).

### The admission pass spawns admitted ops on the APP runtime, not the caller's

The admission pass (from `spawn_managed` or `on_settled`) spawns each admitted op's deferred start on **the engine
host's runtime (its `VolumeHost`'s), deliberately NOT `tokio::spawn`**. `tokio::spawn` binds to whatever runtime is
current when the pass runs — and the pass can run on a runtime that has nothing to do with the op it's admitting.
Admission is global and there is a lock-free window between an op's registration (`spawn_managed` inserts it Queued,
drops the lock) and its own admission pass: a CONCURRENT op's `on_settled`, running on a different runtime, can reach
the pass first and admit the freshly-registered op. So the runtime that spawns an op is racy, not "the op's own caller".
The host's runtime pins every op to the one process-global runtime that outlives them all: the app's own
(`operations_host` passes `volume_host::host()`, whose runtime is Tauri's), or `cmdr-fs`'s shared fallback in a test
binary. ❌ Never a `Handle::current()` captured at setup: that's a caller's runtime again, one level up.

In production this is a no-op (there is exactly one long-lived app runtime, so ambient and app runtime are identical).
**The guard exists for the process-global-manager + per-runtime-caller topology**, which is the test harness: every
`#[tokio::test]` runs on its own runtime, and with a bare `tokio::spawn` an op admitted by a runtime that is then torn
down is orphaned — it never runs, never settles, and leaks its lane forever, wedging later ops (the observed
nondeterministic `wait_until` timeouts). Pinned by
`manager::tests::admitted_op_runs_even_if_the_admitting_runtime_is_dropped` (a throwaway runtime admits an op and is
dropped without driving it; the op must still complete). This race is lane-INDEPENDENT — it hit even unique-lane ops —
so a hermetic-lanes-only test fix could not close it; the guard is the actual fix.

Independently, the archive-edit tests still give each op its OWN lane (`archive_edit::test_support::unique_lane_id`, and
`InMemoryVolume::with_lane_key`), matching the `manager::tests` discipline ("unique operation ids + lane keys"). That's
for test ISOLATION and parallel speed — a shared global lane serializes unrelated tests and couples their timing — not
for orphan-safety, which the guard now owns. The one behavior that needs a `"root"` id (a root parent settling with
`None`) is pinned by `move_out_tests`, whose lanes come from the volume objects, so it passes a `"root"` settle id
WITHOUT reserving the `"root"` lane.

### Lifecycle status and `operations-changed`

`LifecycleStatus` (Queued/Running/Paused/Done/Cancelled/Failed) lives on the manager record. The snapshot also carries
`rollback_support` (see below), `error` (see [Retained failures](#retained-failures)), and `items` (see
[What a row says it acts on](#what-a-row-says-it-acts-on-items)). Admission and settle set Queued/Running and
removal-on-terminal; the pause/resume path sets the `Paused`↔`Running` flip (see [Pause / resume](#pause--resume)). It
is distinct from `WriteOperationPhase` (the progress phase) and `OperationIntent` (the cancel/rollback machine). The
`operations-changed` typed event carries a THIN snapshot (`Vec<OperationSnapshot>`: id, type, status, source/dest
summary), emitted from `spawn_managed` / `on_settled` / `cancel_if_queued` / `set_paused` / the two dismiss commands. It
deliberately excludes 200 ms progress — the queue window reads the per-file `write-progress` stream for live bars. The
manager reports it as an engine-wide fact (`EngineEvents::operations_changed`, § "The engine's host"); the app emits it.

A live RECORD only ever holds `Queued` / `Running` / `Paused`: `Done` and `Cancelled` are declared but never assigned,
because `on_settled` deletes the record before anything could set them. `Failed` is the one terminal status that reaches
a snapshot, and only through the retained-failure list below, never on a record.

**One lifecycle answer, and the query API carries it too.** `OperationStatus` (`get_operation_status`, the
progress/detail query) reports the manager's `LifecycleStatus` in its `lifecycle` field, read through
`manager().lifecycle_status(id)`. ❌ Never re-derive a lifecycle from the state registry's `contains` or any other
presence test: `spawn_managed` inserts the state entry BEFORE admission and a paused op keeps it, so presence means
"exists and hasn't been torn down" and is true for queued, running, and parked alike. A boolean in that spot reported a
parked copy as running, which is what the pause/resume toggle steers by. `lifecycle` is `None` only in the window
between an op settling and its status-cache row being unregistered. Pinned by
`status_cache_tests::a_paused_operation_reports_paused_not_running` and `a_queued_operation_reports_queued`.

`lifecycle_status(id)` reads live RECORDS only, so `None` means one thing: the manager no longer tracks this id. It
deliberately does not consult retained failures, which it cannot reach anyway — a failure is retained from the same
cleanup that unregisters the status-cache row, so `get_operation_status` has already returned `None` by then.
`snapshot()` is the one place the two sources are joined.

⚠️ **Lock order**: `get_operation_status` asks the manager BEFORE taking the status-cache read lock. That is the only
place the two locks meet, and the busy-volume recompute already runs cache-lock-then-out, so nesting the manager lock
inside the cache lock would close a cycle through data-safety code.

### What a row says it acts on (`items`)

`OperationSummaryText::items` (an `OperationItems`, riding to `OperationSnapshot::items`) is what lets the queue row say
"Copying 2 folders and 1 file Dropbox → cmdr-s3-test/target/folder": the top-level `count`, its split into files and
folders (`kinds`), the first 20 names for the row's tooltip, and the `from` / `to` folders in display form. Built once
at registration by `summary.rs` (`path_summary` for local ops, `volume_items` for volume ones), with no I/O.

- **Display form**: a path on the Mac's own disk with the home folder as `~`; a path on any other volume as
  `<volume name>/<path inside it>`, stripped of the volume's `root()` by whole components (`location_on`).
- **Decision: the file/folder split rides the start config (`ItemKinds`, `source_kinds` on both configs), it isn't
  stat'ed here.** **Why**: only the starter knows it for free (the pane's listing), and a `stat` per source is a network
  round trip each on a share. A caller that doesn't say (MCP, compress) gets `None`, and the row says "3 items". A split
  that doesn't add up to `count` is dropped rather than shown.
- **Why it's allowed on the thin snapshot**: it's static for the operation's life and bounded (20 names), so it changes
  only when membership does, like `reverses`. `None` on a rename, a create, a reversal, and an edit inside a zip, where
  the row keeps the two short names.

### Retained failures

**The exception to removal-on-terminal.** Nothing else can hold the failure of an operation that was backgrounded: the
record is deleted on settle, and `write-error` only reaches a window that is listening at that moment — and the queue
window being closed is the exact scenario this is for. So the manager keeps a bounded list of failures OUT OF BAND —
`ManagerInner::failures`, a `VecDeque<RetainedFailure>` (the row plus its way to start again, `manager/retry.rs`) capped
at `FAILURE_CAPACITY = 20`, oldest evicted first — and `snapshot()` appends them after the live rows.

- **`free_and_remove` is untouched.** Admission, laning, the busy-volumes set, and settling behave exactly as before: a
  failed op frees its lane slots, cleans its caches, deletes its record, and admits the next op on the same `on_settled`
  it always did. Retention is a separate structure the settle path never consults. Pinned by
  `manager::tests::a_failed_op_frees_its_lane_and_admits_the_next_exactly_as_before`.
- **Recorded at the emit site.** The app's `TauriEventSink::emit_error` calls `OperationManager::retain_failure` through
  `terminal_effects::error` (`apps/desktop/src-tauri/src/operations_host/terminal_effects.rs`), after the
  `mcp::terminal_ops::record` line and before the emit (order pinned by `terminal_effects_tests.rs`). ❌ Not in the
  `OperationEventSink` trait and not in `CollectorEventSink`: test sinks stay side-effect-free.
- **⚠️ A failure whose record is still LIVE is filtered out of the snapshot.** `emit_error` runs inside the op's own
  task, before `on_settled` removes the record, so for a moment the op is both running and failed. Emitting both rows
  would put one `operationId` in the list twice, and the queue window's keyed `{#each}` throws on a duplicate key.
  `retain_failure` therefore stays silent whenever the record is live; the row surfaces on `on_settled`'s existing
  `emit_changed`, which is the honest moment anyway. It DOES emit on the other branch, where the record is already gone:
  no duplicate is possible without a record, and no `on_settled` is coming, so the row would otherwise sit unbroadcast
  until some unrelated emit — no toast, no chip, nothing until the queue window next opens. Pinned by
  `a_failure_with_no_live_record_broadcasts_itself`.
- **Not every `write-error` is a failure.** `WriteOperationError::Cancelled` (some volume paths emit it) and
  `ArchiveNeedsPassword` (a recoverable prompt the frontend answers and retries) are excluded by TYPED variant match,
  never by message text.
- **First error per id wins.** `write-error` can fire twice for one op — an inner handler emits and returns `Err`, then
  `local_start.rs`'s safety net emits again — and the first one is what actually stopped it.
- **A retained row reports `rollback_support: Settled`** and reuses the live record's source/destination summary, so it
  reads like the running row it replaces. If the record is already gone, it falls back to the event's own
  `operation_type` and no summary.
- **Only an explicit action clears one**: `dismiss_failed_operation(id)`, `dismiss_all_failed_operations()`, or a Retry
  that started. ❌ Never a timer, ❌ never a window close, ❌ never "the next operation starting". A 40-minute copy that
  failed while the user was away has to still be there.
- **A retained failure can keep a way to start again** (`RetainedFailure::retry`, a `manager::StartAgain`), which the
  queue row's Retry runs through `retry_failed_operation(id)`. The routing entry every cross-volume copy and move takes
  attaches it right after the start (`routing/retry.rs::RetryRecipe`, captured from the start's own arguments), to the
  live record or, for an op that already failed in between, straight to its failure; `retain_failure` carries it from
  the record onto the failure. `OperationSnapshot::can_retry` says a row has one (whether the ERROR is worth retrying is
  the frontend's `offersRetry`, the error dialog's own rule). A started retry dismisses the row; a refused one leaves
  it, Retry and all, and returns the typed error. A second click while one is starting finds `retrying` set and gets
  `NothingToRetry`, so a double click can't start the transfer twice.

**Decision: retention lives in the manager, not the frontend. Why.** The failure has to reach two separate webviews, and
it can land while either is closed. A store in the queue window misses it outright (that window is closed in the exact
scenario this exists for); a store in the main window can't be read by the queue window and would need a hand-rolled
state server over `emitTo`, inverting the dependency. The operation log (`journal.rs`) is the durable history and stays
that, but it records an `ExecutionStatus`, not the typed `WriteOperationError` the frontend's message pipeline needs,
and browsing the past is a different job from a live notice. The manager already owns membership and already broadcasts
it to both windows, so retention there needs no new event, no new listener, and seeds correctly through the existing
`list_operations` on window open.

**Decision: the Retry recipe lives in the backend, not the frontend. Why.** The foreground error dialog retries from its
birth context (`dialog-state.svelte.ts::retryPropsFrom`), but a background job's row sits in a separate webview that
never saw how the job began, and the window that started it may be gone, or the start may have come from an agent. The
manager already outlives both and broadcasts to both. The recipe resets the same four things `retryPropsFrom` does (no
preview, no pre-known conflicts, the user as initiator, no source binding), and `routing/retry.rs` says so where both
are visible; ❌ keep the two in step. It's held as a `StartAgain` trait object, never the routing type, so the manager
never names the routing it would re-enter (one module cycle fewer). Only copy and move carry one: delete, trash,
compress, and a local same-volume move show Dismiss alone.

**Decision: runtime-only, capped at 20. Why.** The cap and its reasoning mirror `mcp::terminal_ops::CAPACITY`: enough
that a user returning from lunch still finds what went wrong, bounded so a long batch session can't grow memory without
limit. A restart clears the list, which is consistent with the rest of the manager's state and correct in kind — the
operation log is where a failure lives permanently; this list only exists to make one visible right now.

### Naming a reversal on screen (`reverses`)

`OperationDescriptor::reverses` is `Some(original.kind)` on exactly one construction site, `rollback.rs`'s
`spawn_managed_inverse`, and `None` everywhere else. It rides to `OperationSnapshot` (a static per-operation fact, so it
belongs on the thin registry snapshot rather than the 200 ms tick) and is the only thing that tells a window "this
running operation is an UNDO of a finished one, of THIS kind".

**Why the ORIGINAL kind and not the inverse.** `write_op_type(inverse_kind(kind))` is what the op RUNS as, and it can't
say what the user gets: `Move` covers both undoing a move and undoing a trash, and `Delete` covers undoing a copy, a new
file or folder, and a compress. The frontend feeds `reverses` to the same `OpKind` → variant map that worded the
confirmation the user just answered, so the running bar cannot promise something different from the question
(`apps/desktop/src/lib/file-operations/DETAILS.md` § "The running reversal is named from the SAME variant").

**Why not a phase.** `WriteOperationPhase::RollingBack` is NOT the signal: a cancelled copy cleaning up its own partials
wears that phase too, and there the existing "Rolling back..." wording is honest, because it really is deleting. Reading
the phase instead of this field is how a history reversal of a MOVE ends up telling someone their files are being
deleted.

A retained failure keeps the field (`retain_failure` copies it off the live record), so a reversal that stopped early
still reads as a reversal on the row that explains why.

### Rollback availability (`rollback_support`)

`OperationDescriptor::rollback_support` (a `types::RollbackSupport`) says whether cancelling this op can also UNDO what
it has written (`cancel_write_operation(id, rollback = true)`), and when it can't, WHY. It rides through to
`OperationSnapshot` so the queue window can offer Rollback on exactly the rows the progress dialog would. It's on the
snapshot because the operation queue window is its own webview: it never sees the source/destination volume ids the
dialog decides from, and two surfaces disagreeing about whether an operation is reversible is the kind of drift that
ends with a button that lies.

Every construction site states its own verdict (a struct literal, so a new spawn path can't forget to decide):

- **`Supported`**: local copy/move via `start_write_operation` (`CopyTransaction` deletes the copies it made;
  `MoveTransaction` renames them back), volume copy (`volume/cleanup.rs` deletes the destination copies), and a compress
  into a NEW archive.
- **`CrossVolumeMove`**: copies and deletes the source per file, and its driver treats `RollingBack` exactly like
  `Stopped`, reporting `notRolledBack`.
- **`SameVolumeMove`**: a server-side rename-merge that stops without reversing (also where the volume's renames copy,
  `moves_by_copy`, decided only once the task runs).
- **`ArchiveRewrite`**: a zip edit, a copy or move into one, or a compress over an existing one (all-or-nothing
  temp+rename rewrites). **`ArchiveMoveOut`**: the archive drops entries only after every one extracted.
- **`NothingToPutBack`**: delete and trash. **`InstantChange`**: rename, bulk rename, new file/folder. **`IsReversal`**:
  the operation-log rollback's own inverse op (rolling back a rollback would re-apply what the person just undid).
  **`Settled`**: a retained failure row.

**Decision**: a reason, not a bool. **Why**: the disabled Rollback button explains itself, and with only a bool the
frontend had one sentence for every blocked case, so a move from the Mac to a NAS was told Rollback "is not available
for same-volume moves". The enum makes each site name its reason, and the frontend's
`file-operations/reversal-wording.ts` (`rollbackBlockedTooltipKey`) maps it to copy that says why and what Cancel still
does; a new variant is a compile error there.

Both surfaces read it. The progress dialog blocks Rollback whenever the verdict isn't `Supported`; its own props-only
same-volume-move rule blocks too, for the frames before the first snapshot lands, and never contradicts it.

The verdict is a property of the op's STRATEGY, so it never changes over the op's life. Whether Rollback is offered
_right now_ is the UI's call on top of it, and both surfaces ask the same three questions: a running/paused op, not
already rolling back (read from the live `write-progress` phase, since rollback is an `OperationIntent`, not a
`LifecycleStatus`), and not past the point where a reversal can still do anything. That last one is the local cross-FS
move: `cross_fs.rs` commits its transaction before Phase 4, so a Rollback pressed during the `Deleting`-phase source
sweep reverses nothing and only stops the sweep. The frontend names that window in one place,
`file-operations/reversal-wording.ts`'s `reversalWindowClosed`, which the dialog and the queue row both call.

### IPC

`list_operations` (the thin snapshot), `cancel_operation(id)`, `cancel_operations(ids)` (the queue window's "Cancel
selected"), `pause_operation(id)` / `resume_operation(id)`, and `pause_all` / `resume_all`. Cancel routes through
`cancel_operation`: a Queued op is dropped from the registry without ever spawning (`cancel_if_queued`); a
Running/Paused op falls through to the existing `cancel_write_operation(id, rollback=false)` keep-partials path.
Pause/resume flip BOTH the live `WriteOperationState` pause gate (so the driver parks) AND the manager record's
`LifecycleStatus` (so the UI shows Paused), via `set_paused`, and both RETURN a `PauseOutcome` (`Applied` /
`AlreadyInState` / `NotApplicable`) that rides out through `bindings.ts` ([Pause / resume](#pause--resume)). Plus
`dismiss_failed_operation(id)` / `dismiss_all_failed_operations()`, which drop retained failures and re-emit, and
`retry_failed_operation(id)`, which starts one again and answers a `RetryOutcome`
([Retained failures](#retained-failures)). And `move_queued_operation(id, direction)`, which reorders the line and
returns a `QueueMoveOutcome` ([Reordering the line](#reordering-the-line-move_queued)). And
`latest_operation_progress()`, the newest `write-progress` tick of every live operation that has emitted one
(`state/registry.rs`), its wait classified at read time: a queue window that opens while an operation is paused seeds
its rows from it, since a parked operation emits nothing. Registered in the `ipc.rs` manifest; `OperationSnapshot` /
`LifecycleStatus` / `OperationsChanged` ride into `bindings.ts`. No capability change: manager commands go through the
invoke handler, not the ACL.

`get_transfer_tree_view(id, request)` answers one window of a running copy's, move's, or compress's live tree
(`tree_view.rs` over `transfer_tree/`; no new event: views pull it on each `write-progress` tick). Its typed refusal is
`TransferTreeViewError` (`unknown_operation`, `not_a_transfer`); a queued or scanning operation answers `ready: false`.

### Pause / resume

The paused bit has TWO homes, kept in sync by the IPC layer: a `PauseGate` on `WriteOperationState` (the runtime gate
the drivers honor) and the manager record's `LifecycleStatus::Paused` (what the UI sees in `operations-changed`). Pause
is **orthogonal to `OperationIntent`** (which stays the cancel/rollback machine) — it never perturbs the validated
`Running → RollingBack/Stopped` transitions — and it is **not a `WriteOperationPhase`** (a paused op may be
mid-`Copying`).

- **`PauseGate`** (`operation_intent.rs`): a `paused: AtomicBool` plus a `std::sync::Condvar` (for the sync driver,
  which parks inside `spawn_blocking`) and a `tokio::sync::Notify` (for the async volume drivers). `pause()` sets the
  flag and opens the operation's human-wait clock; `resume()` clears the flag, closes the clock, and wakes both waiters;
  `wake()` wakes both WITHOUT clearing the flag (the cancel path uses it) but DOES close the clock — the operation is
  winding down, so nobody is being waited on any more, and a clock left open would make the rollback that follows
  measure no rate at all. `wait_while_paused_sync(&intent)` parks while `paused && !cancelled`; the
  `*_until(&should_stop)` pair (sync and async) parks while paused and the caller's predicate is false. Ordinary loops
  reach them through `stop_or_park_*` below (which stop on `work_stopped`, so a failure drain stops them too), and a
  mid-file checkpoint through `park_while_paused`.
- **One question per boundary: `WriteOperationState::stop_or_park_sync()` / `stop_or_park_async()`.** `true` means stop,
  `false` means carry on with the next item. It owns the whole contract, so no loop can spell it wrong: cancel is read
  FIRST (a stopping op never parks, and nothing destructive runs between the two reads), only a live op parks, and the
  intent is re-read after the wake, so a cancel landing WHILE parked is answered at that same boundary instead of one
  item later. A caller whose reading of "stop" ISN'T `work_stopped` — a reversal running under `RollingBack`
  (`WindDown`), the history reversal (`rollback.rs`), a detached scan preview watching its own flag — drives
  `PauseGate`'s `*_until` helpers instead and names its own predicate.
- **Gate placement: exactly where the loop already observes cancel, and only there** ([The park](#the-park) has the
  reasoning). Every serial loop that can spend real time asks: both transfer drivers' per-source loop tops
  (`transfer_driver/{sync,async}_driver.rs`), both delete walkers' delete-phase file and dir loops (`delete/walker.rs`,
  `delete/volume_walker.rs`), trash's per-item loop (`delete/trash.rs`), all of `move_op/`'s per-item loops
  (`src/transfer/DETAILS.md` § "Pause in the local move engine"), the copy's scanned-dirs pass
  (`transfer/copy/scanned_dirs.rs`), the two cross-volume merge walks and the sequential extractor's member loop
  (`src/transfer/volume/DETAILS.md` § "Pause in the volume walks"), the archive mutator through
  `MutationHooks::wait_if_paused`, and every SCAN boundary that already observes cancel. The cross-volume streaming copy
  path ALSO parks BETWEEN CHUNKS via the `CheckpointStream` wrapper in `transfer/volume/stream_pipe.rs` (the sync
  per-chunk `on_progress` callback can't `.await`, so the async stream decorator owns mid-file parking + a `yield_now`),
  so a paused single large file (e.g. MTP→local) stops mid-stream holding only its `.cmdr-tmp-<uuid>`. Two paths still
  pause only at a file boundary, both for the same reason — no stream to park between chunks and nothing freed by
  parking: the local-FS sync chunk loop (`chunked_copy.rs`, which receives the cancel atom, not the `PauseGate`) and a
  server-side copy made in one call, such as SFTP's `copy_within` (`transfer/volume/server_side_copy.rs`'s
  `try_server_side_copy`); one made in pieces (S3's parts) parks between them, at `ServerCopyProgress::checkpoint`. Full
  rationale + scope: `src/transfer/DETAILS.md` § "Pause reaches between chunks".
- **Cancellation always wins over pause.** `cancel_write_operation` / `cancel_all_write_operations` flip the intent AND
  call `pause_gate.wake()`, so a paused, parked op unblocks, observes the non-`Running` intent, and bails through the
  existing keep-partials path (keeping already-copied files, deleting only the last partial). Without that wake a paused
  op parked on the condvar would never see the cancel.
- **A paused Running op keeps its lane slots** (`set_paused` never touches lanes), so a same-lane Queued op can't start
  and then fight it on resume. Resume runs NO admission pass (the op never freed its lanes). Pausing a Queued op is a v1
  no-op (it isn't touching a device yet; it stays Queued and admits normally when its lanes free). Pinned by
  `manager::tests::{set_paused_flips_running_op_to_paused_and_keeps_its_lane, paused_running_op_does_not_admit_a_queued_same_lane_op}`.
- **The request reports what it did**, as a `PauseOutcome`: `Applied` (the record flipped), `AlreadyInState` (asked for
  what it already is, so the caller's intent holds and a retry isn't a refusal), `NotApplicable` (queued, over, or
  unknown — nothing changed and nothing is remembered). It travels the whole way out: `set_paused` → `pause_operation` /
  `resume_operation` → the IPC commands → `bindings.ts`. The MCP `queue` tool is the consumer that needs it, since an
  agent acts on the answer; the queue window ignores it and reads the live status from `operations-changed` instead.
- **A sweep reports counts, not a verdict.** `pause_all` / `resume_all` ask every eligible op, so they fold the per-op
  outcomes into a `PauseAllOutcome` (`applied` / `already_in_state` / `not_applicable`) and hand that out the same way,
  through the IPC commands into `bindings.ts`. "No operation was running", "three parked", and "two were already paused"
  are three different situations, and a flat "OK" for all three is what sent an agent on believing a device had gone
  quiet. `took_effect_anywhere()` is the one derived question worth a name. From a sweep, `not_applicable` can only be
  the settle race (the snapshot named an op that finished before the call landed) — a queued op never enters the walk.
  The fold is `impl FromIterator<PauseOutcome> for PauseAllOutcome`, which is what makes it testable without touching
  the process-global manager (`manager::tests::a_sweep_counts_every_outcome_it_collected`); ❌ never drive a real
  `pause_all()` from a test, it would park a sibling test's operation.
- **Concurrent copy path.** `copy_volumes_with_progress`'s concurrent path has no single between-files boundary, and its
  per-file `on_progress` callback stays cancel-only (pinned by
  `transfer_driver::concurrent_tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`). But its in-flight
  files stream through the shared `stream_pipe_file`, so each parks between chunks via `CheckpointStream` when paused;
  the admission loop adds no new files while everyone is parked, so the batch effectively halts. Serial paths (local
  copy/move, cross-volume serial, delete) honor pause between files; the cross-volume paths additionally park between
  chunks. See `src/transfer/volume/DETAILS.md` § "Pause and the concurrent copy path".
- **Accepted resource asymmetry** (principle 5): `wait_while_paused_sync` parks the op's `spawn_blocking` pool thread
  for the whole pause — the same thing the deferred-start design avoids for _queued_ ops. A paused Running op
  legitimately holds its lane and is rarer than queued ops, so v1 accepts this; many simultaneously-paused local ops
  could pressure the blocking pool. v2 may bound concurrent paused-and-parked ops if it proves real.
- **Connection-idle caveat** (document, don't fully solve in v1): a long pause holds SMB/MTP connections idle and may
  hit server/USB timeouts. v1 accepts that resume may surface a normal transient error (SMB already reconnects; MTP
  stale-handle has a one-shot retry). v2 adds keep-alive / explicit reconnect-on-resume.

### Existing single-op flow is unchanged

When nothing else touches the op's lanes (the common case), `spawn_managed` admits it on the registration's own
admission pass — effectively an immediate spawn. The "register + return `operationId` immediately" contract holds:
registration and the id return happen before any I/O, so the dialog opens even on a stalled mount. Every pre-existing
write-op test passes through the manager path unchanged.

### Managed instant ops (`run_instant`)

Rename, make-folder, and make-file (`WriteOperationType::Rename` / `CreateFolder` / `CreateFile`) are **scan-free,
near-instant, result-returning** metadata ops. They flow through `OperationManager::run_instant(descriptor, op)` instead
of `spawn_managed`, so the "every write op goes through `spawn_managed`" framing above applies only to the streaming
transfer/delete ops.

- **No lane, no admission queue — deliberate.** `run_instant` registers a `Running` record and marks its volumes busy,
  but reserves NO lane and runs NO admission pass. Lanes exist to stop two big _transfers_ thrashing one device; a
  metadata syscall must never queue behind a multi-minute copy. An inline rename that hangs until its IPC timeout is
  worse than useless, and the MTP/SMB connection layer already serializes physical device access. It even ignores any
  lanes in its own descriptor (pinned by `manager::tests::run_instant_does_not_reserve_a_lane`). **Don't "clean this up"
  into `spawn_managed`** — that silently reintroduces lane-queuing for metadata syscalls, the regression this design
  forbids.
- **Runs inline and returns the op's result.** Unlike the fire-and-forget spawn path, `run_instant` awaits `op` inline
  and returns its `T` to the caller. The inline-rename editor and the new-file/new-folder dialogs need the result
  synchronously (new path for cursor placement / editor-open; conflict/timeout/success for the rename flow). The command
  layer wraps `run_instant` in its own IPC timeout; nothing inside spawns. Instant ops emit no `write-progress` /
  `write-complete` / `write-error` (the command return is the result channel) and no completion analytics (explicit
  no-op arms in `analytics::emit_completion_analytics`).
- **RAII cleanup on drop/panic is mandatory, not happy-path only.** The command wraps `run_instant` in a
  `tokio::time::timeout`, so a slow op that exceeds it makes the timeout **drop the `run_instant` future
  mid-`op.await`**; the async volume path can also panic. Either exit MUST still free the record AND unregister the busy
  status — else the eject guard sticks ON forever (the volume can never be ejected again) and a phantom `Running` row
  lingers. An `InstantTaskGuard` held across the `op.await` guarantees this: its `Drop` calls `free_and_remove` (record
  removal + `StatusCache::unregister` → `recompute_and_report_busy_volumes`) and re-emits `operations-changed`. The
  happy path calls `free_and_remove` + `emit_changed` explicitly, then `guard.disarm()`s so the Drop is a no-op. No
  admission pass on completion (instant ops reserve no lanes, so nothing waits on them). Pinned by
  `manager::tests::run_instant_releases_busy_and_record_when_{dropped_midflight,op_panics}`.
- **No `WriteOperationState`.** Instant ops have no intent/pause/conflict oneshot, so `run_instant` inserts none.
  Consequence: `cancel_operation` on an instant op is a safe no-op — `cancel_if_queued` is false for a Running op, then
  `cancel_write_operation` finds no state. Acceptable: instant ops finish before a human can cancel.
- **Queue surfacing.** They appear as a `Running` snapshot row that goes away almost immediately (the store prunes
  terminal/removed rows). A ~50 ms local rename may never render before it's pruned; a slow MTP rename shows a label +
  spinner with no progress bar (`fraction` is null). Local `root` ops cause NO busy-set churn (`root` is excluded), so
  inline-renaming local files won't flicker the eject menu; only volume ops mark busy.

## The scan-wait

A confirmed transfer is registered with the manager immediately, before its `TransferDialog` preview has finished
walking. That is what gives it an `operationId`, a queue row, its lanes, a busy-volume entry, and a place in the quit
gate from the moment the user confirms — the whole point, because before this a user could not background a scanning
transfer at all and the scan died with its dialog. The wait itself moved into the operation's own task
(`scan_bridge::await_claimed_preview`), which every deferred start calls first, BEFORE its journal open: an operation
that never got past its scan wrote nothing, so journaling one would record work that didn't happen.

**Claims, and why exactly one.** An operation claims its `preview_id` inside `spawn_managed`, before the record is even
inserted. The claim is what lets the progress bridge forward the walk's counts under the operation's id, what exempts a
settled result from TTL eviction while its owner sits Queued behind a busy lane (with `LANE_BUDGET = 1` that can be well
past five minutes), and what stops `cancel_scan_preview` from freeing a result an operation is about to read. A SECOND
operation naming the same id is refused and falls back to its own walk: `take_cached_scan_result` REMOVES what it reads,
so two claimants would race for one consumable result and the loser would silently get nothing. Not hypothetical — the
archive-password retry re-dispatches a new operation over the same sources, which is why `dialog-state.svelte.ts` clears
`previewId` on that path.

**The terminal outcome, not a completion pulse.** Both preview workers used to remove their in-flight state before
publishing a result, so an operation looking the preview up could find nothing and had four indistinguishable reasons:
complete-and-consumed, errored, cancelled, and never-existed. With `LANE_BUDGET = 1` "nothing there" is the common case,
not the rare one. `settle_preview` replaces the entry in one write with a `ScanOutcome`, readable afterwards. ⚠️
`Cancelled` comes from the worker's own cancel FLAG at its exit, ❌ never from which event fired: a genuinely cancelled
walk returns an error (the local walk's `on_cancelled`, the volume path's `VolumeError::Cancelled`), so classifying on
the event would reach the operation as a failure, and recovering the truth from it would be guessing on the control
path. Both workers' arms match.

**A failed walk hands its operation a typed failure that names the item.** `ScanOutcome::Error` carries the
`WriteOperationError` the waiting operation reports unchanged: the local walk classifies each `io::Error` with
`classify_io_error` against the entry it stopped on, and the volume walk maps the backend's `VolumeError` through
`map_volume_error` (a `NotFound` keeps its own path; anything else names the one source, or the selection's folder). The
watchdog's timeout names the first source (`ScanTarget`). ❌ Don't flatten it back to a string: that's what sent
`ERR-VETBX` to the user as `Copy error: Path: ; Error: No such file or directory`, a missing file reported as a nameless
I/O failure. The dialog's `scan-preview-error` reads only `timed_out`; its `message` is the failure's technical form,
for the log. Pinned by `scan_failure_tests.rs`.

## The park

Pausing during the scan is the case Pause exists for: the scan is the minutes-long part of a big transfer, so it is when
somebody realizes they picked the wrong destination. `scan_bridge::ScanPause` is the one primitive that makes it real,
for both scans — the detached preview worker, and the scan an operation runs for itself when it has no result to
consume.

**Where it parks: exactly where the walk already observes cancel, and only there.** A walk that stopped less often than
it can be cancelled would make Pause the weaker of two buttons that mean the same thing to a person; one that stopped
more often would need park points the volume backends don't offer. So every gate sits immediately after the existing
`is_cancelled` check (cancel outranks pause, and a cancelled walk must never park on its way out), and pause inherits
cancel's granularity for free:

- **Local walk** — per entry, via `WalkContext::park_while_paused` in `walk_dir_recursive` and `walk_cached_entries`.
- **Oracle-aware volume walk** — per entry, in `scan_subtree_with_oracle`.
- **Cold-cache volume group** — per source group in `run_oracle_aware_batch_scan`, and then per entry INSIDE the
  backend's own walk, through the `ScanBoundary` that call hands it.
- **An operation's own VOLUME scan** (`transfer/volume/preflight.rs`, the path a programmatic or MCP-started transfer
  takes when it has no preview to consume) — inside the backend's walk, same way, through a boundary armed with the
  operation's own stop.

**How a stop reaches inside a backend.** A backend crate can't name a `WriteOperationState` (`crates/` carry no
`tauri`), so the vocabulary is `cmdr_fs::volume`'s: a `ScanStop` handle over a `ScanStopSignal` the owner implements.
`WriteOperationState`'s impl forwards to `stop_or_park_sync` / `_async`, and `ScanPause`'s carries the preview's two
routes to "stop" — so neither can end up with its own reading of cancel-outranks-pause. The walk never touches the stop
directly: it threads a `ScanBoundary`, where reporting an entry and asking the boundary are one call
(`boundary.file(size).await?`), which is what makes a walk that counts entries without ever asking unwritable.
Per-backend granularity, what a stopped scan returns, and why it's an error rather than a partial result:
`crates/cmdr-fs/src/volume/scan_stop.rs` and `Volume::scan_for_copy_batch_with_boundary`'s doc. Cost measured at ~2 ns
per entry for the stop check, against ~1–3 µs for the syscall or round trip the entry already costs.

Two shared assertions in `volume::conformance` keep it honest per backend, since a scan that ignores its boundary still
returns the right numbers and nothing else in the suite would notice: `assert_batch_scan_stops_when_told` (all backends)
and `assert_batch_scan_asks_inside_the_walk` (the ones that walk a real tree).

**Why the owner is resolved lazily.** A preview walks detached from the operation that will consume it: the walk holds a
`preview_id`, the gate hangs off the operation, and the claim joining them lands when the user confirms — possibly after
the walk started. So `ScanPause::resolve_owner` runs at the walk's PROGRESS TICK, which is already off the per-entry
path, and a `OnceLock` keeps the answer for life (a claim is one-shot, and the `Arc` outlives the record). ❌ Don't move
the lookup onto the per-entry path: what makes this free is that an unpaused entry costs one atomic load, the same as
the cancel check beside it.

**The park's exit names every way the walk can be told to stop**: the operation's intent (which `cancel_write_operation`
sets BEFORE it wakes the gate) and the preview's own `cancelled` flag (which `abandon_claim` sets). ❌ Dropping either
leaves the thread on a gate nobody will open — the async volume walk parks on `wait_while_paused_until`, the local one
on the sync twin `wait_while_paused_sync_until`, and neither re-checks on a timer.

**The watchdog has to know.** A paused scan counts nothing, which is precisely what `ScanWatchdog`'s 60 s inactivity
bound fires on, so holding Pause would have ended the scan with "stopped responding" — the user's own click reported
back as a dead share. `note_parked` suspends the budget; `note_resumed` restarts the clock from now, so the pause is
never charged against it.

## Bounding the scan

A preview had no bound of any kind. That is survivable while a walk can only end, and it stops being survivable the
moment the volume under it can stop answering: a `stat` on a wedged kernel mount blocks until the mount is forced down,
observing no cancel flag and returning nothing, so the preview stayed in flight forever. The pre-confirm dialog spun on
`0 bytes / 0 files / 0 dirs`, and a transfer already confirmed waited with it, because it parks on the same preview.

**Why inactivity, not duration.** A legitimate scan of a large tree over SMB runs for minutes, so any total-duration cap
either cuts real work or is too generous to catch a wedge. What separates "slow but working" from "dead" is whether the
walk is still COUNTING: every entry a backend hands back is proof the far end answered. `ScanWatchdog::note_progress`
records that proof from both workers' progress paths (fed BEFORE the emit throttle on the volume path — the throttle is
a UI rate limit, not evidence about the device), and the watchdog fires only after `SCAN_INACTIVITY_LIMIT` (60 s) with
nothing counted.

**Why 60 s.** It sits above every bound the layers below own, so their better message wins whenever they have one: a
direct-SMB request gives up after ~50 s (20 s to the socket, then 30 s of server silence), and the IPC scan deadlines
are 30 s. What's left underneath is the case with no bound at all, and that's what this catches.

**Why the watchdog publishes.** It can't ask a wedged walk to stop and report, so it settles the preview
(`ScanOutcome::Error`) and emits `scan-preview-error` with `timed_out: true` itself, leaving the walk detached behind
it. It also sets the cancel flag as a courtesy: a walk that is merely slower than we believed reads it and stops working
for a dialog that has moved on. `claim_outcome` is the one-shot CAS both the watchdog and the worker pass through, so
exactly one publishes and a late walk can't contradict a timeout the user has already been shown. ❌ A new terminal path
in either worker owes that claim: skip it and two publishers race, which is the whole failure this CAS exists to make
unrepresentable.

**What it reaches.** The pre-confirm dialog renders an honest notice with a retry
(`src/lib/file-operations/transfer/DETAILS.md` § "When the dialog can't find out"). A CONFIRMED transfer's wait already
turns `ScanOutcome::Error` into that operation's failure (`await_claimed_preview`), so the same bound also ends the
post-confirm hang — with the watchdog's message, which is why that message is written for a person to read.

**The log is the other half.** `grep scan_preview` over the reporting user's multi-megabyte log returned nothing: the
module had two `debug!` calls, neither at scan start. Every preview now logs one INFO at start (sources + volume), a 5 s
DEBUG heartbeat with counts and time since the last one, and an INFO (or WARN, on a timeout) at the end, all under the
`scan_preview` target, so the next hang answers "did it start, did it progress, how did it end" without guessing. Those
lines name the first source as a quoted `path=` field (`ScanTarget::log_label`), while the timeout message keeps the
plain `label`: bare, a report's redactor read the ` trip` of `Summer trip on volume root` as prose and shipped it.

**A completed preview logs the WALK's totals, not the watchdog's counters** (`note_completed`, which takes a
`ScanTally`). The counters only move on the progress tick, and that tick is on the
`fileOperations.progressUpdateInterval` timer, so a scan finishing inside one interval never feeds them: the settle line
read `0 files, 0 dirs, 0 bytes` over a fully walked tree, which is what every preview in a production log said,
three-file folders included, and it sends whoever reads it chasing a phantom scan bug. ❗ The cancelled and gave-up arms
keep reading the watchdog's own counters on purpose (`note_settled`): there are no totals, and "how far we got before
stopping" is the number worth having.

**Testability.** `ScanPreviewEventSink` (mirroring `OperationEventSink`) keeps both workers off `tauri::AppHandle`, so
`scan_watchdog_tests.rs` can point a real walk at `test_support::WedgedVolume` — every future parks forever — and watch
the preview settle anyway. A network drop isn't repeatable; a volume that never answers is exactly repeatable and
reaches the same code.

**Trash is the one operation that doesn't wait.** `trashItemAtURL` is atomic per top-level item, so a trash walks
nothing: there is no second walk to serialize against and no cached result to consume, and waiting would be pure delay —
a long one on a big tree. `trash_files_start` therefore passes no `preview_id` and frees the preview outright, because
nothing downstream will read it and the dialog deliberately skips its own cleanup after a confirm (on the DELETE path
the operation DOES consume it). Pinned by `a_trash_frees_its_preview_instead_of_waiting_on_it`.

**Misses are always a re-walk, never a hang.** An unknown `preview_id` (evicted, or stale from a reloaded window) and a
refused claim both leave the operation with no claim at all, so it proceeds straight to its own foolproof scan.

**The progress bridge.** `scan-preview-progress` is keyed by `previewId` and carries no `operationId`, and nothing else
emits for an operation that is only waiting, so without a bridge every scan-phase surface would render zeros rather than
live counts. Each preview tick for a CLAIMED preview is republished as the owner's `write-progress` in
`phase: 'scanning'` (`scan_bridge::forward_scan_progress`), alongside — never instead of — the preview event, since a
pre-confirm dialog may still be watching the same preview by id. `files_total` / `bytes_total` stay 0 through the scan;
the index expectation rides `expected_*`, which every surface already treats as a hint.

⚠️ **The opening tick's ORDERING is load-bearing.** `row.progress` must stop being `null` immediately rather than at the
next `progress_interval_ms` boundary, which on a preview near its end may never arrive. But `spawn_managed` inserts the
record, runs admission, and only THEN calls `emit_changed()`, while the frontend store's `applyProgress` early-returns
for an id with no snapshot row yet. A tick that beats its own `operations-changed` is discarded and the row stays blank
— exactly the case the tick exists for. So `emit_initial_scan_tick` fires AFTER `emit_changed`, for a `Queued` row as
much as a `Running` one (the queued row renders the scan line too, and its task may not spawn for minutes).
`scan_bridge_tests` asserts the ordering against the manager's broadcast counter, not merely the tick's existence.

**Pause is NOT a special case here.** A scan-waiting operation is `Running`, and it pauses like any other `Running`
operation: the record flips to `Paused`, the gate is set, and the walk it is waiting on parks on that same gate
([The park](#the-park)). ❌ Don't reintroduce a refusal. The refusal that used to live here answered "it pauses the
moment it starts writing" and then let an 80,000-file move run to completion, because the scan is the minutes-long part
of a big transfer and nothing parked during it. The gate also carries the request across the handover: whatever sets it
during the walk is still set when the driver takes over, so `pause_all` issued mid-scan cannot be lost.

**Two leaks the wait creates, and their hooks.** (1) Every exit from the scan-wait has to reach `on_settled` or the row
leaks and its lane stays reserved; the operation's task owns that, not the detached scan worker, and `free_and_remove`
also abandons any claim still held (the panic / quit-drop net). (2) `cancel_if_queued` removes a Queued record WITHOUT
ever running its `DeferredStart`, which is where the wait and its cleanup live — so it calls `abandon_claim` explicitly.
A `Queued` op cancelled before admission is the ordinary case on a busy lane, and without the hook its walk would keep
going for an operation that no longer exists while its result sat until a TTL sweep. These are separate leaks and no
single test catches both.

**The lanes are reserved from confirm**, and this is the one genuinely contested decision here. For: with
`LANE_BUDGET = 1` a transfer confirmed while another runs on the same lane is admitted as `Queued` and the existing
auto-queue path backgrounds it with no new code; and admission is oldest-first, so operations run in the order the user
confirmed them, which they did NOT before (A dispatched only after its scan, so B, confirmed second with a finished
scan, took the lane first). Against: an operation scanning for three minutes holds a lane it is not writing to, so a
destination device can sit idle. Matching confirm order is a correctness property and better utilization is an
optimization, so reserve from confirm. Two-phase reservation (scan without lanes, request them at write start) costs the
invariant that `Running` means "admitted and holding its lanes" and adds a second admission point that can leak a lane:
reach for it only if the idle-device cost shows up in practice.

**Two behavior changes worth naming.** Eject is disabled from confirm rather than from first byte (the operation is
committed, so that is right, but it is new). And an operation confirmed onto a busy lane is `Queued` from registration,
so the progress dialog's one-shot `listOperations()` seed sees `queued` immediately and the auto-queue path mounts and
unmounts the modal within a frame or two, with a toast and a queue window. That flash is not new, only more frequent;
fixing it properly means moving dispatch out of the dialog.

**Archive routes await but do not reuse.** `copy_into_plan.rs` plans its changeset with its own `WalkDir` and never
calls `take_cached_scan_result`, so `preview_id` is threaded through `route_archive_copy_into`, `compress_start`, and
`route_archive_delete` for the WAIT alone: it restores the serialization the frontend's scan-wait used to provide.
Without it, ⌥F5's sampling preview and the planner's walk would run down the same tree at once, which is what costs on
MTP and SMB. The duplicate walk itself is pre-existing and out of scope; teaching the archive changeset planner to seed
from a `CachedScanResult` is real work with its own correctness questions.

**Testing the scanning window.** E2E fixture trees are deliberately tiny and `data-scan-state` signals "counting done",
the opposite of what a scanning-phase test needs to hold, so `set_test_scan_preview_delay` (an IPC override behind the
`playwright-e2e` feature, falling back to `CMDR_E2E_SCAN_PREVIEW_DELAY_MS`) holds every worker at its starting line for
a set number of milliseconds. Per-test rather than per-process, so one spec's window doesn't slow the whole run.

## The pre-flight conflict check

`conflict_preflight.rs` is the transfer dialog's "which of these would already collide at the destination?" check:
`VolumeScanError` (the refusal vocabulary), `SourceItemInput` (the FE's per-item input), and
`scan_volume_for_conflicts_within`, the whole budgeted check. `commands/file_system/volume_copy.rs`'s
`scan_volume_for_conflicts` is a thin `#[tauri::command]` wrapper that calls it with the production
`CONFLICT_CHECK_BUDGET` (30 s); the split exists so a test can hand the inner function a budget it can wait out
(`conflict_preflight_tests.rs`, `#[path]`-attached the way `conflict.rs`'s test suites are).

Distinct from `conflict.rs` (which resolves a clash a write has already hit) and from `scan.rs` / `scan_preview.rs`
(which size a transfer rather than check for collisions).

**One budget for every leg, not one per leg.** Two volume resolves (a `.zip`-crossing path probes the network), the
source batch stat, and the destination scan can each reach a device that has stopped answering, and each takes what's
LEFT of `deadline` rather than a fresh timeout, because four legs with their own timeouts add up to a promise nobody
wrote down.

**The optional source-stat leg gets a sub-budget, never the whole deadline.** When `source_volume_id` + `source_paths`
are supplied, `stat_source_paths` resolves each item's real `is_directory` + size. A watched pane listing answers first
(`Listings::authoritative_listing`, one lookup per parent folder), because a paste's sources almost always sit in a
folder a pane shows and a stat isn't always cheap: MTP lists the whole parent per stat, so 16 concurrent stats of a
101-photo paste queued sixteen 818-entry listings of `/DCIM/Camera` (`ERR-44S2Q`). The rest take one
`Volume::get_metadata` per top-level path, `SOURCE_STAT_CONCURRENCY` (16) in flight, strictly O(top-level items) and
never a subtree walk. ❌ Don't swap this back to `scan_for_copy_batch`: a batch of exactly one path takes a fast path
straight into `scan_recursive` on SMB and SFTP, so a single directory source would walk its whole subtree (a 119k-file
folder ate the entire 30 s check budget, `ERR-AYVM4`). This leg runs on `deadline.fraction(SOURCE_STAT_BUDGET_DIVISOR)`
(3, so it gets 10 s of a 30 s budget) because its failure is non-fatal — a fallback to the FE's name-only items — while
the destination scan after it is not: sharing one deadline would let a wedged source spend everything, so the
destination scan then fails instantly and the user reads a timeout blamed on a device the check never asked.
`merge_source_types_from_stats` folds the stats over the name-only placeholders so dir-vs-dir collisions classify as
silent merges; back-compatible when the caller omits the source volume. For a single named transfer it binds the
destination-name hint to the explicit original source path, taking type and size from that source's stat; batch hints
match by basename. The destination leaf contract: `src/transfer/DETAILS.md` § "Named destinations".

**`drop_self_collisions` removes the collisions naming a source itself**, so a same-folder paste (a duplicate, which
both engines auto-rename silently instead of asking about) doesn't announce every item as its own conflict. It answers
with `transfer_would_land_on_its_source`, the engines' own predicate, which is what keeps the dialog and the write
agreeing about which clashes are real (canonical: `src/transfer/DETAILS.md` § "Self-collision (duplicating in place)").

## Routing a transfer

`routing.rs` answers the two questions every cross-volume transfer asks before it starts, and owns the fork it takes
from the answers. It exists because that whole shape used to live inside the three `#[tauri::command]` bodies in
`commands/file_system/volume_copy.rs`, which build a `TauriEventSink` at the edge — so it was reachable only from a
window, and a backend caller (an approved suggestion group starting an op with its own injected sink) could reach it
only by duplicating it.

- **`resolve_source_volume(volume_id, first_path)`** routes a batch that a ROUTE serves to that read-only volume, and
  returns which route it was as an `Option<RoutedKind>`. Two routes qualify: an archive-inner batch goes to its
  `ArchiveVolume`, and a path inside a repo's virtual `.git` trees goes to the repo's `GitPortalVolume`. One
  `source_volume_id` per batch means no straddle risk, so the first path decides. A cheap string pre-filter
  (`volume::manager::path_routes_over_its_parent`) keeps a plain path off the parent-aware resolve entirely, and it is
  deliberately narrow at the edges: the `.zip` FILE itself is a plain file copied through its parent volume, and a
  `.git` path counts only while the portal is switched on.
- **A routed source is the whole point of the routed shape.** Neither namespace has an inode, so a source left on its
  parent volume takes `copy_between_volumes`'s both-local fast path against a path `std::fs` can't stat: the scan finds
  nothing, the preview caches zero files, and the transfer dialog sits on "Verifying before copy" until it ends in
  "Couldn't finish copying". Routed, both read with `open_read_stream` and walk with `scan_for_copy` on the cross-volume
  engine. The same generalization runs in `commands/file_system/write_ops.rs::scan_preview_source_volume`, which sizes
  the transfer before it starts.
- **`RoutedKind` rather than a bool, because the two routes part ways on a MOVE.** An archive can delete its source
  entries after the extract lands (the compound move-out op), a snapshot cannot: a file in a branch snapshot has no
  inode to remove. So `start_volume_move` refuses a `GitPortal` source with the typed `ReadOnlyDevice`, named for the
  source volume, before the destination is even resolved. A `GitPortal` DESTINATION is refused the same way in both
  `start_volume_copy` and `start_volume_move`: every mutation on that volume answers `NotSupported`, so a drop onto a
  snapshot would otherwise start and die on its first write. A `.zip` destination is NOT this case; it routes to the
  archive-edit driver, which really does write.
- **`resolve_dest_path(dest_volume, dest_path)`** expands `~` for LOCAL volumes only (on a share, `~` is an ordinary
  folder name) and then root-anchors. The transfer dialog's box is volume-relative (`/photos`, with the volume in a
  dropdown beside it) while a pane sends the absolute path it displays; `root_anchored` is idempotent, so neither caller
  has to know which it holds. Skipping it is what made a 360 GB move into an SMB subfolder fail instantly (ERR-XCP5Q):
  `SmbVolume` read `/photos` as a path outside its mount and answered `NotFound`.
- **`start_volume_copy` / `start_volume_move` / `start_volume_compress`** are the three entry points. Between them they
  cover the plain cross-volume engine, copy/move INTO a zip (one `{ add }` changeset), move OUT of one (the compound
  extract-then-delete op), and compress (seed an empty archive, then copy into it). Each takes an injected
  `Arc<dyn OperationEventSink>`; the IPC commands build the sink and pass it in, nothing more.

**A route that cannot hold a binding refuses the transfer rather than running it unbound.** The archive-changeset routes
(copy or move INTO a zip, move OUT of one) plan their work from their own `WalkDir` instead of the per-source engine, so
they have nowhere to apply an `ExpectedSources`. Dropping one silently would be the exact failure a binding exists to
prevent, and nothing downstream would ever say so, which is why `route_cannot_hold_a_binding` turns it into an error the
caller sees. Extract itself is unaffected: it resolves to an `ArchiveVolume` SOURCE and runs through
`copy_between_volumes`, which is bound.

**Decision: an extract has no operation type of its own.** **Why**: pulling entries out of a `.zip` is a copy whose
SOURCE volume happens to be an `ArchiveVolume`, which `resolve_source_volume` arranges. `ArchiveSubkind::Extract` is an
operation-log label describing what a copy was for, not a second driver. A `WriteOperationType::Extract` would need its
own lane rules, its own conflict handling, and its own progress accounting, all of which already exist and already work.

**Decision: a compress over an existing archive overwrites it, and this module does not refuse.** **Why**: the seed is
unconditional and the prior bytes aren't retained, which makes it the one transfer here that can't be reversed at all.
That is a fact to DISCLOSE before someone commits to it (the review surface's job), never a refusal in the engine: if a
person can do it from the dialog, an operation they approved does the same thing. Same rule as § "Binding the sources".

## Binding the sources

`source_binding.rs`. An operation decided against files as they looked at review time can execute much later — an
approved op sits queued behind a forty-minute copy on the same lane, and a suggestion waits until somebody clicks
Approve. In between, a source can be edited, replaced, or swapped for a different file under the same name.

**`SourceFingerprint` is what lets an operation notice.** `Local { device, inode, content }` uses the identity the
kernel maintains; `Remote { normalized_path, content }` stands in for it where no inode exists (SMB, MTP, an archive).
Comparison is derived equality against a fresh capture, so a caller never writes the comparison itself.

**Decision: `LocalContent` / `RemoteContent` are `File { size, modified }` or `Directory`, not `Option`s.** **Why**: a
directory's own size and mtime move with every child write, so binding a proposed `delete ~/projects/cmdr/target/` to
yesterday's directory mtime would refuse it after any build — while `(device, inode)` plus "still a directory" is a real
identity worth holding it to. Two variants also make the two reasons a field could be empty impossible to confuse: "it's
a folder, so there are no bytes" and "the backend didn't report a size" are different facts, and an `Option<u64>` doing
both jobs is the same anti-pattern as `per_path`'s empty `Vec` (§ "Key patterns", the `scan_sources_internal` decision).
A directory that became a file mismatches on the variant alone.

**Decision: the EXPECTATION picks the namespace a source is checked in, not the call site.** **Why**: one entry point
can take two routes. `copy_between_volumes` hands a both-local transfer to `copy_files_start` and everything else to the
Volume engine, and a caller that fingerprinted its sources a week ago cannot know which route the operation takes when
its turn finally comes. Dispatching on the variant the caller actually holds keeps one binding correct on both routes,
and leaves the capture rule a single sentence: **fingerprint with `capture_local` when the source volume has a
`local_path()`, and with `capture_remote` when it does not.** The asymmetry that remains is honest: a `Local`
expectation is settleable anywhere, while a `Remote` one needs the backend that owns it, so the sync local-only path
drops it rather than guessing.

**The pre-flight runs INSIDE the operation's own task, not at approval.** `retain_bound_sources` (local, sync),
`retain_bound_sources_on` (Volume-backed, async), and `retain_bound_sources_with_sizes` (trash, whose `item_sizes` is
positional against the sources and has to be filtered in lockstep) sit at the top of the four starters' handlers, after
admission. The whole point of a fingerprint is to close the window between checking and acting; checking at approval and
acting an hour later would reopen it as wide as it goes.

- **Binding is all-or-nothing.** A source the binding doesn't name is dropped, not waved through: a caller that supplies
  a partial map has a bug, and guessing "probably fine" means acting on a file nobody reviewed. A caller that wants no
  checking passes no `ExpectedSources` at all rather than an empty one.
- **An operation the binding empties completes; it does not fail.** Every source already went out as a `Skipped` item,
  so the caller has its per-source answers, and a failure dialog on top would be the engine editorializing about a
  decision the person is entitled to make differently.
- **`journal_snapshot()` lives on the fingerprint** because the journal's mtime column is Unix SECONDS while a local
  fingerprint holds nanoseconds. Journal the nanoseconds and every undo reports drift and refuses (silently disabling
  undo); journal nothing and identity rests on size alone, so a same-size replacement gets renamed back in place of the
  original. A directory answers `(None, None)`: it has no bytes of its own to be held to.

**Nothing in the engine asks who started an operation**, and a test holds it there. `expected_sources` is an `Option` a
caller may supply; an unbound operation gets its sources back untouched, which is every user-started copy, move, delete,
and trash. `approved_op_parity_tests.rs` pins the consequence: the same transfer run bound and unbound agrees on the
destination tree, the completion counters, and every per-source event — a missing destination folder is created either
way, an Overwrite overwrites either way, a Skip skips in the same place either way.
`only_a_source_that_changed_is_treated_differently` states the one permitted difference positively, so the parity cases
can't be read as "the binding does nothing".

## Sources that left while an operation queued

`queued_sources.rs`. An operation that auto-queued behind a long one on the same lane can wait many minutes, and the
person keeps working meanwhile. ERR-962JD: a queued move of 22 mp3s whose files the person had moved by hand was
admitted four minutes later and stopped on the first one with a bare `SourceNotFound`, nothing having said the queued
job still had plans for those files.

- **Only an operation that WAITED checks.** `spawn_managed` marks the state (`note_waited_in_queue`) when its own
  admission pass leaves the record `Queued`. One admitted at once runs against sources its dialog looked at a moment
  ago, and keeps the engine's own typed refusal (`a_move_admitted_at_once_is_not_checked`).
- **It rides the binding's seam.** `retain_bound_sources*` already sits at the top of every starter, after admission, so
  an UNBOUND operation runs the queue check there (`kept_after_queue_local` / `kept_after_queue_on`); a bound one
  already drops a changed or vanished source on its own terms. Archive-edit routes (copy into a zip, extract, compress)
  don't pass through it, so they keep the plain refusal.
- **One probe per top-level source, ❌ never a walk**, through the source `Volume` on a volume route. Only a POSITIVE
  absence counts (`symlink_metadata` NotFound locally, `VolumeError::NotFound` on a volume, ❌ never `Volume::exists`,
  which says `false` for an unreachable source too): a dead connection must never read as "all your files were moved". A
  source that can't be looked at stays for the engine to report.
- **Some gone: it asks.** `write-sources-gone` (`AwaitingAnswer`, with the honest `gone_count` / `total_count` and the
  first few paths), and the operation parks on `state.sources_gone` (`sources_gone_slot.rs`), holding its lanes like a
  conflict prompt. `continue_without_gone_sources` carries it on: each gone source is reported `Skipped` with
  `source_removed: true`, and the narrowed source list misses the stale scan cache, so the rest is re-scanned honestly.
  Cancel is the ordinary cancel path, which abandons the slot; the operation emits `write-cancelled` with nothing
  processed. The local starters block their `spawn_blocking` thread on the answer, the volume ones `.await` it.
- **All gone: it says so and completes.** Each source `Skipped`, `write-sources-gone` (`NothingLeft`) so the person
  learns why, and the empty batch's `write-complete`. ❌ Not a `write-error`: nothing went wrong, so nothing is retained
  as a failure.
- **No human-wait clock edge.** The question comes before the write phase's first progress sample, so no rate window can
  straddle it.
- **Observable and answerable by an agent**: `cmdr://state` renders the parked question as `pendingSourcesGone`, and the
  `queue` tool's `continue_without_gone` answers it (`cancel` is the other answer).
- `OperationEventSink::emit_sources_gone` has a default no-op so the many single-purpose test sinks needn't spell it
  out; ❗ a sink that WRAPS another (the suggestion bridge's `ProposalReportingSink`) must forward it, or nobody is
  asked and the operation waits until cancelled.

Pinned by `queued_sources_tests.rs`. The frontend half: `apps/desktop/src/lib/file-operations/DETAILS.md` § "Sources
that left while an operation queued".

## Moving Locked items

`locked_sources.rs` (#408). A move has to rename or delete the user's ORIGINAL, and Finder's Locked (`uchg`/`uappnd`) or
an ACL entry denying `delete` / `delete_child` refuses both (`EPERM` / `EACCES`). A copy only reads the original, so it
never asks. Before this, a same-drive move of a Locked file failed outright, and a cross-drive one copied everything and
then stopped in Phase 4 with every original kept: the files in both places and an error.

- **Asked once, before any byte moves.** Each engine classifies what it will take away
  (`cmdr_fs::metadata:: removal_lock`): same-drive, the sources plus, for a folder that MERGES, each child the merge
  renames on its own (`LockedScan::of_renames`; a folder that lands by one rename carries its contents untouched, so
  nothing inside it is asked about); cross-drive, every scanned item (`of_paths` over the scan result, a cached
  preview's included); cross-volume, a local walk of the source trees on the engine's blocking pool
  (`plan_locked_on_volume`), since the copy's walk lists through the `Volume` and carries no flags. Only a source with a
  `Volume::local_path` can wear Locked; any other plans nothing. Cost: an `lstat` plus one ACL read per item, beside a
  copy that already reads every file's ACL.
- **Some Locked: it asks.** `write-locked-sources` (the honest `locked_count`, the first few paths), the move parked on
  `state.locked_sources` (`locked_sources_slot.rs`) with the `LockedQuestion` human-wait clock open and announced like a
  conflict. `answer_locked_sources` resumes it with `MoveAnyway` or `Skip`; cancel is the ordinary cancel path, which
  abandons the slot, and the move emits `write-cancelled` with nothing processed. Local engines block their thread on
  the answer, the volume engine `.await`s it.
- **Move anyway** becomes a `LockedPlan` that unlocks each item right before the act that needs it, and only then:
  same-drive, before its `rename`, locked again where it landed (`Unlocked::landed_at`); cross-drive and cross-volume,
  right before the sweep deletes the original (a Locked folder before its children leave it, locked again if it stays).
  The copy already landed Locked, so nothing goes back on at the destination. An `Unlocked` that is dropped (the act
  failed, a `?` on the way) puts the blockers back where the item still is.
- **Skip** keeps each Locked item where it is with everything under it: same-drive like a conflict Skip, cross-drive by
  pre-seeding the copy's `skipped_subtrees` (and leaving those folders out of staging and the sweep), cross-volume by
  the driver's pre-skip set for a top-level item and `MergeCtx::locked` for one deeper down. `files_skipped` counts the
  skipped files, and `WriteCompleteEvent::locked_kept` counts the Locked items left in place.
- **System-locked** (`schg`/`sappnd`): only root can clear them, so no answer could move them. Never asked about, always
  skipped, counted in `locked_kept.system_locked`.
- ❌ **Clearing never happens inside `LocalPosixVolume::delete` / `rename`** or anywhere without a plan built from an
  answer: F8 and F2 run through those and must never silently unlock a user's file.
- **Accepted gap**: a same-drive move clears, renames, then re-locks. A crash between the clear and the re-lock leaves
  the item unlocked at whichever path it's at, the same gap a copy's landing accepts (`crates/cmdr-fs/DETAILS.md` §
  "`metadata`: what a copy keeps, and when it goes on").
- **Not covered**: a cancel-with-rollback of a same-drive move renames a re-Locked item back and is refused, so it stays
  at the destination and the reversal reports it (nothing lost). A move INTO a zip (`archive_edit/copy_into.rs`) sweeps
  with an empty plan, so a Locked original stays and the move reports it. A `0500` folder still refuses its children's
  removal on a cross-drive move after its lock is lifted: a mode, which no Locked answer covers.
- **Observable and answerable by an agent**: `cmdr://state` renders the parked question as `pendingLockedSources`, and
  the `queue` tool's `move_locked` / `skip_locked` answer it.
- `OperationEventSink::emit_locked_sources` defaults to a no-op; ❗ a sink that WRAPS another must forward it, or nobody
  is asked and the move waits until cancelled.

Pinned by `locked_sources_tests.rs`, the move cells in `src/transfer/metadata_matrix_tests.rs`, and
`src/transfer/volume/move_locked_tests.rs`. The frontend half: `apps/desktop/src/lib/file-operations/DETAILS.md` §
"Locked items in a move".

## Per-source outcomes

`write-source-item-done` carries a `SourceItemOutcome` (`Done` / `Skipped` / `Failed`) beside `source_removed`.

**Decision: the outcome rides on that event rather than a sibling one.** **Why**: "this source is finished with" and
"how it ended" are one fact. Split across two events, every consumer wanting a per-source verdict — the queue's gradual
deselection, the search-snapshot purge, the suggestion store writing `proposal_ops.status` — would have to join two
streams and decide what a missing partner means.

**The LAST event a source gets is the verdict**, and both directions are pinned. A cross-filesystem move speaks twice
for one source: `Done` when it finishes staging, then `Done` again when the source-delete phase removes it, or `Skipped`
when the rename phase left it standing. Staging succeeding says nothing about where the item ended up, so a consumer
recording a per-source status OVERWRITES rather than accumulating. `move_op_tests.rs`'s
`a_cross_fs_move_that_{skipped_the_item_ends_on_skipped_not_done, took_the_item_ends_on_done}` pin both directions, so
the rule can't degrade into "the second event is always a skip".

⚠️ **`source_removed` is a separate question from the outcome**, and it is the one the search-snapshot purge steers by
(`apps/desktop/src/lib/search/snapshot-purge.ts`). A source skipped BECAUSE it vanished under the operation reports
`Skipped` AND `source_removed: true`; a source that merely couldn't be stat'd reports `false`, because "we couldn't
look" is not evidence a file is gone and a wrong `true` drops a row for a file the user can still open.

⚠️ **Both move sweeps ANSWER it with an `lstat`, never by assuming the sweep took the item.** A directory move can leave
the source standing two ways: a same-FS MERGE whose child hit a conflict, and a cross-FS sweep that deletes the tree
around a skipped descendant (`delete_dir_preserving_skipped`). The skipped-source list only rules out a skipped
TOP-LEVEL item, so it can't answer for either. One `lstat` per top-level item is nothing beside the move, and it is the
only honest answer; the purge now drops every snapshot row UNDER a path this reports gone, so a wrong `true` on a
directory costs a whole subtree of rows rather than one. Pinned by `move_op_tests.rs`'s `a_{same,cross}_fs_merge_that_*`
cases.

Where each non-`Done` outcome comes from — every one a place the engine already knew and used to say nothing:

- **The binding's pre-flight** (§ above) reports `Skipped` per dropped source, for all four verbs.
- **Trash reports `Failed` per item**, on both failure paths (a source it can't stat, and `trashItemAtURL` refusing).
  Trash is per-item by nature: one failure leaves the batch running, so the operation's own terminal event says nothing
  about that item.
- **A bulk-skipped copy source reports `Skipped`.** Those leave `scan_result.files` before `SourceItemTracker` is even
  built, so nothing downstream would otherwise speak for them.
- **A cross-FS move's rename-phase skip reports `Skipped`** in phase 4, which is what makes the last-event rule
  load-bearing rather than decorative.
- **A skip decided MID-FLIGHT reports `Skipped`**, via `scan::FileVerdict` returned from `copy_single_item`. This is the
  one the pre-flight can't see: the local engine resolves per-file conflicts as it walks (a cross-type refusal, a Skip
  on a clash inside a folder, a source that already IS its destination), long after the bulk-skip pass has run.
  `copy_single_item` used to return `Ok(())` for both outcomes, so the driver counted every one of them as a transfer —
  the operation reported "Copied 2 files." having copied one, `files_skipped` stayed 0, and the `had_conflicts`
  analytics proxy (`files_skipped > 0`) missed the whole class.

**A source is `Done` if ANY of its files landed.** The skip is per file, so one refused child inside a folder doesn't
make the folder a skip; `SourceItemTracker` tracks "landed something" per top-level source and only reports `Skipped`
for one that landed nothing.

**The volume engines report it too** (the cross-volume copy's serial and concurrent drivers, the cross-volume move, and
the same-volume rename move), so a transfer to or from a share deselects gradually. Whoever DECIDES a verdict reports
it:

- **The async driver** (`transfer/transfer_driver/async_driver.rs`, `DriverConfig::report_source_outcomes`) reports the
  ones it decides: `Skipped` for a bulk pre-skip, a top-level Skip, or a closure's `Skipped`, and `Failed` for an error
  on that source. ❌ Never for a `Cancelled` (nothing was decided) or a `SourceNotRemoved` (the copy landed; the closure
  already said `Done`).
- **The closures** report `Done`, because only they know whether anything landed (a folder whose every file was declined
  is `Skipped`, as on the local engine) and whether the source is gone. The concurrent driver reports from
  `record_success` / `record_failure` and its own pre-skip and Skip paths (`copy_concurrent*.rs`).
- **A cross-volume move speaks twice**, like the local cross-FS one: `Done` (source kept) once the copy landed, then
  again after the sweep with `source_removed` answered by the sweep's own account of what it kept (a Skip, a newcomer,
  or a changed file keeps the source standing), ❌ never inferred from "it was a move" and ❌ never a fresh probe: on an
  object store every probe is a billed request the S3 estimate suites count. Pinned by
  `transfer/volume/copy_tree_rows_tests.rs`.
- **A same-volume rename move** reports `Done` per renamed item (`move_same.rs::landed_whole`). A plain rename that
  returned `Ok` took the item whole (`source_removed: true`); a rename-merge asks the volume, since it leaves the source
  folder standing around a skipped child.

⚠️ **`FileVerdict` says what the POLICY decided, never what is on disk**, and a destructive act must not read it. The
cross-FS move's phase-4 delete set is decided by the LEDGER (`transaction.created_files()` growing across the call),
because that records what actually got written; the verdict feeds reporting only, where being wrong costs a wrong label
instead of a file. Delete passes `CarriedOut` unconditionally: it has no Skip, so reaching the tracker means the entry
is unlinked.

The injected sink is the seam: a caller that wants per-source statuses wraps the sink it passes in, and the engine never
names the `agent` module at all. The crate boundary holds it: `cmdr-operations` has no `cmdr` dependency, so naming
`agent::` fails to compile instead of costing every session a line it might ignore.

## Archive edits

Editing a `.zip` (mkdir/mkfile/rename/delete inside, or copy/move INTO one) is an O(archive) temp+rename rewrite that
runs as a managed op, driven by the `archive_edit/` module: `src/archive_edit/CLAUDE.md` and its `DETAILS.md`.

## Busy-volumes set

Drives "disable Eject while an op reads from / writes to this device" so a disconnect can't truncate an in-flight file.
Lives in `status_cache.rs` (`StatusCache`, one per engine), alongside the cache it derives from: the recompute reads
that cache, and register / unregister are the only two places membership can change, so the two stay one struct.

The same two register/unregister functions also report `EngineEvents::transfer_started` / `transfer_finished`, which the
app feeds into `crate::priority::transfers` (the per-volume "a transfer is running" gauge indexing yields to — transfers
trump indexing). ONE lifecycle choke point on purpose: a new op kind that guards eject automatically also outranks
indexing, and the panic-safe unregister paths below cover both signals. Don't add a second feed site (see
`apps/desktop/src-tauri/src/priority/CLAUDE.md`).

- The manager registers an op's volume IDs busy (`StatusCache::register(op_id, type, volume_ids)`) **only when it admits
  the op (Running)** — a Queued op isn't touching the device, so it marks nothing busy. Source **and** destination go in
  (a download from a phone is as corruptible as an upload to it). The manager's `on_settled` / `ManagedTaskGuard` Drop
  unregisters on every exit (including panic), so a finished or panicking op can't leave a volume stuck busy.
- The busy set is the union of every Running op's `volume_ids` **∪ external registrations**, minus `root` (never
  ejectable). `status_cache.rs::recompute_and_report_busy_volumes` reports `volumes-busy-changed` only when membership
  changes — progress ticks don't churn it (the cache's last-reported set; pinned through a `cfg(test)` emit counter by
  `status_cache_tests::volumes_busy_changed_fires_once_per_distinct_change`, on an engine of its own). Every
  transfer-gauge start has exactly one finish on success, cancel, error, a panicking handler, and the panic net
  (`status_cache_tests::every_transfer_start_has_exactly_one_finish_however_the_op_ends`). Membership-by-union means two
  concurrent transfers to one device keep it busy until both finish, with no manual refcount.
- **Where `volume_ids` come from**: the `OperationDescriptor` each spawn site hands the manager. The cross-volume entry
  points (`copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) and the volume-aware delete carry
  the IDs; the both-local branch of `copy_between_volumes` (a local→USB / DMG copy) passes both IDs through
  `copy_files_start` / `move_files_start` so the ejectable destination is still marked. The plain `move_files` / `trash`
  commands pass an empty list — the unified transfer dialog only routes through them for same-`root` ops, where no
  ejectable volume is involved.
- **Consumers**: `busy_volume_ids()` backs the `get_busy_volume_ids` bootstrap command, the `eject_volume` server-side
  guard (refuses a busy volume — the real safety net, since the picker's disable is only UX), and the native
  breadcrumb-menu builder (renders the Eject item disabled with a ` (busy)` suffix). A poisoned status cache recovers
  (`*_ignore_poison`), because skipping the update on poison would drop a busy entry and let Eject run mid-transfer. The
  frontend `volume-busy-store.svelte.ts` subscribes to `volumes-busy-changed` and exposes `isVolumeBusy(id)` to disable
  the picker's eject controls. The engine reports each change through `EngineEvents::volumes_busy_changed`; the app
  emits it.
- **A wider question, `operations_need_volume(id)`** (`manager.rs`): every op the manager HOLDS (Queued too) plus the
  busy set. A queued op hasn't touched the device, so Eject ignores it, but it names the volume and will set out for it,
  so retiring a volume id for good (a saved server moving to a new address, `server_move.rs`) waits for it too.
- **External (non-write-op) seam**: the drag-out file-promise fulfillment service (`native_drag::fulfillment`) marks the
  source volume busy while it streams a promise to a Finder destination, but it isn't a real write op (no registered
  state, no progress events, no settle). The `pub(crate)` `register_external_volume_op(op_id, volume_ids)` /
  `release_external_volume_op(op_id)` pair (in `status_cache.rs`, `Operations` methods) is the seam: it touches only the
  status-cache half the busy set is derived from, registering under `WriteOperationType::Copy` (the type only affects
  `get_operation_status` diagnostics; the busy set is type-agnostic). The fulfillment side wraps it in an RAII guard so
  release fires on every exit path.

## Settle contract

`write-settled` fires exactly once per operation, after the spawned background task has fully torn down — including
in-flight USB / network teardown that may briefly outlive the `write-cancelled` emit. The FE uses it to gate the
"Cancelling…" dialog close so the user can't dispatch a new op against a still-tearing-down volume (the wedge mode that
cancel propagation already shortens but doesn't eliminate).

**Ordering**: `write-settled` always fires AFTER the terminal outcome event (`write-complete` / `write-cancelled` /
`write-error`) for the same `operation_id`. The BE guarantees this by placing the settle emit in a `WriteSettledGuard`
RAII struct whose `Drop` runs at the very end of the spawn-task scope, AFTER all the conditional terminal-event emits.

**Guard pattern**: every op's deferred start (the future the manager spawns from each of the five entry points)
constructs a `WriteSettledGuard` at the top, from the same injected `Arc<dyn OperationEventSink>` the rest of the op
emits through. The guard's `Drop` impl calls `sink.emit_settled(...)`. This makes the emit panic-safe: even if the op
body panics and the task exits via `JoinError`, the guard still drops during stack unwinding, so the FE never hangs
waiting for a settle that never comes. `emit_settled` is a required `OperationEventSink` method (no default no-op), so a
new sink can't silently swallow settle. See `settle_event_tests.rs::settled_fires_on_panic_unwind` for the safety-net
pin.

**Cache-cleanup panic safety**: removal from the state registry + the status cache must survive a panic, or the op
lingers forever in the status cache. The manager owns this: `on_settled` removes both maps on the happy path, and the
`ManagedTaskGuard` Drop (held by every spawned task, declared so it drops AFTER the `WriteSettledGuard`'s scope cleanup
runs but frees caches before the settle emit) does it on unwind. The guard NEVER spawns in Drop — see
[Operation manager](#operation-manager) § "Dequeue on settle". Pinned by
`manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

**Payload**: `{ operationId: String, operationType, volumeId: Option<String> }`. The `volume_id` is best-effort: filled
with the source volume's display name for volume-aware ops (copy/move between volumes, volume delete), `None` for pure
local-FS operations. The FE currently filters only by `operationId`; `volume_id` is for diagnostics and forward
compatibility.

**The full order, through a real managed op**: a handler that emits its own terminal event (`write-complete`, a
handler's `write-cancelled`) does so INSIDE the blocking handler, so the journal row still reads `running` at that emit;
then `journal::finalize_op`; then the safety net's `write-error` for a handler that returned an error without emitting
one (so that emit sees the row finalized `failed`); then `on_settled` (record removed, lanes freed, next op admitted,
which therefore reads the previous op's row finalized and eligibility computed); then `write-settled`.

**Tests**: `settle_event_tests.rs` pins the guard's invariants (single fire, panic safety, ordering relative to the
terminal event) and the full order above, probing journal row, record, and lane at each emit, plus an op queued behind a
copy reading that copy's row at the admission instant. `delete/volume_cancel_tests::volume_*_emits_write_settled_event`
pin the integration shape against the volume-delete handler.

## Key decisions (shared)

**Decision**: Copy and cross-FS move pre-flight a destination per-file-size limit (FAT32's 4 GiB cap) right after the
scan, before the first byte. `validation::validate_file_sizes_for_filesystem` classifies the destination via
`cmdr_local::filesystem_kind` (macOS `statfs.f_fstypename` / Linux `/proc/mounts` → `FilesystemKind` → `MaxFileSize`)
and, only when the cap is `Limited`, fails the whole operation with `WriteOperationError::FilesTooLargeForFilesystem`
(up to 10 offenders, largest first, plus the true count). **Why**: A FAT32 USB stick silently failed a 5 GB copy ~4 GB
in. The gate is all-or-nothing and runs alongside the free-space check (`copy/mod.rs`, `transfer/move_op/cross_fs.rs`).
It blocks **only** when certain: `Unlimited` (APFS/exFAT/NTFS/ext4/MTP) and `Unknown` (OS-mounted SMB, unrecognized)
never block, so a false positive — worse than the mid-copy failure because it stops a copy that would have succeeded —
can't happen. **exFAT must stay `Unlimited`** (it's the common big-USB format with no 4 GiB cap); only FAT32
(`msdos`/`vfat`) is `Limited`. Same-FS moves rename in place and never reach the gate. The kind → cap map in
`filesystem_kind::FilesystemKind::max_file_size` is the single source of truth (the write guard, the error prose, and
any future volume-picker display all read it). SMB FileSystemName detection (a `smb2`-crate `FileFsAttributeInformation`
query) and the volume-picker filesystem display are scoped follow-ups.

**Decision**: Every scan reports **two** byte totals — `total_bytes` (write footprint, un-dedup'd) and `dedup_bytes`
(`du`-equivalent, each inode once). Delete consumes `dedup_bytes`; copy/move consume `total_bytes`; the Copy dialog
shows both. **Why**: A hardlink contributes differently to the two operations. **Delete** frees an inode only when its
last link is removed, so the bytes-freed number is the dedup'd one — counting every link would claim to free 80 GB when
only 60 GB (cargo `target/`) actually frees. **Copy/move** materialize every hardlink as an independent file at the
destination (hardlinks don't survive a cross-volume copy, and even a same-FS `cp` doesn't relink), so the bytes-written
number — and the disk-space reservation — is the full write footprint. The earlier single-`total_bytes`-is-dedup'd
design got delete right but silently regressed copy: the space check under-reserved (risking ENOSPC mid-copy) and the
bar hit 100% early. Now `walk_dir_recursive` / `walk_cached_entries` / `scan_volume_recursive` /
`LocalPosixVolume::scan_for_copy` / `scan_subtree_with_oracle` all track both, using a `seen_inodes: HashSet<u64>`
(mirrors `indexing/scanner/mod.rs`, `nlink == 1` fast path, operation-scoped across source roots; **Unix-only**, where
non-Unix has no `nlink()` so `dedup_bytes == total_bytes`). Volume backends populate `FileEntry::inode` only for
`LocalPosixVolume` files with `nlink > 1` (MTP/SMB/InMemory leave it `None`, so dedup is a no-op and the two totals are
equal). The **scan-phase** progress bar reports the dedup'd running total (it's compared against the indexer's
inode-dedup'd `dir_stats` estimate, so reporting the write footprint would overshoot 100% on hardlink trees). The
**delete** active phase sums per-entry `progress_bytes`/`VolumeDeleteEntry::progress_bytes` (= dedup'd) against the
`dedup_bytes` denominator. The **copy** active phase credits full per-file `size` against the `total_bytes` denominator
(no chunk scaling). The Copy dialog surfaces the gap with a one-line note ("X will be written; source is Y; the extra is
hardlinked files…") via `dedup_bytes_total` on the scan-preview events — copy-only, since a same-FS move writes nothing.
Pinned by `delete/hardlink_progress_tests.rs`, `delete/volume_hardlink_progress_tests.rs`,
`transfer/hardlink_progress_tests.rs::copy_counts_write_footprint_for_hardlinks`, `scan.rs::tests::walker_dedupes_*`,
`local_posix_test::test_scan_for_copy_dedupes_hardlinks_for_source_size_only`, and
`transfer-dialog-utils.test.ts::shouldShowHardlinkNote`.

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`,
`dirs_done`, `expected_files_total`, `expected_bytes_total`). **Why**: 20+ emit sites construct `WriteProgressEvent`
literals for active-phase events. Adding four optional fields to the struct would force every site to spell out their
defaults, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan
meta (`None` / `0`); the scan emit sites in `scan.rs`, `scan_preview.rs`, and
`delete/volume_scan.rs::scan_volume_recursive` opt in via `.with_scan_meta(current_dir, dirs_done, expected)`. Future
scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the
builder pattern still composes cleanly on top.

**Decision**: All write operations go through `OperationEventSink` instead of `tauri::AppHandle`, and the sink is
constructed **only at the IPC edge** (`commands/file_system/write_ops.rs` + `commands/file_system/volume_copy.rs`), then
injected all the way down. **Why**: Decouples the copy/move/delete/trash orchestration from the Tauri framework. The
app's `TauriEventSink` (`apps/desktop/src-tauri/src/operations_host/sinks.rs`) wraps AppHandle for production;
`CollectorEventSink` stores events for test assertions. The whole managed layer — `start_write_operation`, the four
starters, the volume entry points (`copy_between_volumes` / `move_between_volumes` / `move_within_same_volume`), every
`*_with_progress` function, and `WriteSettledGuard` — takes `&dyn OperationEventSink` / `Arc<dyn OperationEventSink>`,
never an `AppHandle`. Each command builds `Arc::new(TauriEventSink::new(app))` once and passes it in; the engine names
no Tauri sink at all. This lets the full pipeline (multi-file copy, cancellation, conflict resolution, progress, the
managed spawn path, and settle) run end-to-end under a `CollectorEventSink` with no Tauri runtime — see
`tests.rs::injected_sink_receives_complete_and_settled_for_local_copy` and the trash unit tests
(`delete/trash.rs::tests::trash_*_via_sink`). `state.emit_progress_via_sink` is the only progress-emit method. The
write-error safety-net arms in each deferred also route through `sink.emit_error(...)` rather than a string-named
`app.emit("write-error", ...)`.

**Decision**: Scan preview reuses watched listings (the "fresh-listing oracle"). **Why**: Pre-flight scans for copy/move
on MTP (and to a lesser degree SMB and big local trees) used to duplicate work the backend already had in the listing
cache. Selecting 135 photos in a watched `/DCIM/Camera` (~15k entries) and pressing F5 would re-list the parent dir over
USB just to look up size by name — ~17 s of "Verifying before copy…" while the listing was already fresh on the pane
behind the dialog. `run_volume_scan_preview` now groups input sources by parent dir and consults
`try_get_authoritative_listing(volume_id, parent)` first. On hit, sizes and `is_directory` flags come from the cached
`FileEntry` for top-level files; top-level directories recurse via `scan_subtree_with_oracle`, which re-applies the
oracle at every level (so a subfolder open in another pane also short-circuits). On miss, the call falls through to
`volume.scan_for_copy_batch_with_boundary(paths_in_group, ...)` — same code path as before — so MTP's parent-grouping
and SMB's pipelined-stat optimizations still run for cold-cache parents. The local-FS walker (`walk_dir_recursive` in
`scan.rs`) also takes an oracle check at the top of each recursive call, with `volume_id = "root"` plumbed through from
`scan_sources_internal` and `run_scan_preview`. The freshness contract is bright-line at the watcher boundary: no "5
seconds is fresh enough" TTL, just "the volume's `listing_watch_coverage(path)` returned `EveryWriter`." See
`crates/cmdr-listing/src/caching.rs::try_get_authoritative_listing` for the per-backend debounce windows that contract
tolerates.

**Decision**: Copy and move are durable before they report complete: per-file `sync_data` (fdatasync) in chunked copy,
plus an end-of-op pass that syncs the data the strategy didn't and fsyncs every directory that gained an entry. A
cross-FS move deletes its sources only when that pass answers `Ok`. Delete and trash don't sync at all. **Why**:
"Complete" must mean "durable on disk," not "buffered in the OS page cache." Without it, a user who copies to a USB
stick / SD card and ejects (or the machine sleeps) right after "Copy finished" loses the file — and on a move it's gone
from both source and dest. The flush is targeted, not a whole-machine `libc::sync()`: that global sync also stalled
unrelated apps (AGENTS.md principle #5). The mechanism: (1) `transfer/chunked_copy.rs` calls `dst_file.sync_data()` per
file, so each file is durable as it completes — a crash mid-batch on a long transfer leaves earlier files safe. (2)
Before emitting `write-complete`, `durability::flush_created_destinations` emits a `Flushing`-phase progress event,
`fdatasync`s every created file the strategy didn't already make durable (it reuses `CopyTransaction.created_files` and
`created_dirs`, no parallel dest-tracking), and `fsync`s every directory that gained an entry. It answers a `Result`:
the cross-FS move gates its source delete on it, and the copy completes with `durability_unconfirmed` set so the person
hears it. Same-FS move (pure rename) writes no data at all, so it takes the sibling pass `flush_touched_directories`:
one `fsync` per DIRECTORY the renames touched, and none on the moved files, which would cost an `F_FULLFSYNC` device
barrier each for bytes nothing wrote. The rules (which errnos count as "not supported", the staged-to-landed path
mapping, what a failure keeps) and the measurements live in `src/transfer/DETAILS.md` § "Durability (flush before
reporting complete)". Pinned by `transfer/copy/copy_tests.rs::local_copy_emits_flushing_phase_before_complete` and
`transfer/move_op/move_progress_tests.rs::cross_fs_local_move_emits_flushing_phase_before_complete`; FE label by
`TransferProgressDialog.flushing.test.ts`. **Cross-volume copy/move landing on a local disk** (MTP → Local, SMB → Local,
USB import) doesn't go through this local-FS engine — it flows through `LocalPosixVolume::write_from_stream`, which
keeps the same promise by `sync_data`-ing each file (plus a best-effort parent-dir fsync for the directory entry) before
it returns, so each file is durable as it completes. That path doesn't yet emit the `Flushing` UI phase (the volume
copy/move handlers don't call `flush_created_destinations`); a follow-up could route them through the end-of-op pass for
UI consistency, but the per-file `sync_data` already makes them durable.

**Decision**: `TauriEventSink::emit_error` logs the whole typed error at **WARN**, and that is the only place a failed
write operation's technical reason is written down. **Why**: The variant travels to the frontend, which raises the
user-facing error and shows the detail string under the dialog's collapsed "Technical details". Nothing wrote it to the
log, so an error-report bundle recorded the frontend's `Trash error: io_error` and nothing else — ERR-BPAPM (2026-09-08)
lost the OS's own words for a `trashItemAtURL` refusal, unrecoverably. The all-items-failed branch of
`trash_files_with_progress` was the specific hole (its partial-failure sibling below it does log per-item details), but
the gap was every operation's, so the line sits at the one shared emit site rather than in each engine. **WARN,
deliberately, not `log_error!`**: the frontend already raises the same failure at error level, and `log_error!` trips
the auto-dispatcher, whose Flow B window opens at two errors within 60 s — a second error-level line would let one
user-visible failure auto-send a report on its own. The file target is always Debug, so a warn still reaches every
bundle. It logs `{:?}` of the variant rather than a hand-written sentence, because the variant already carries the path
and the message in named fields and Debug can't drift from them. The frontend side of the same lesson is
`cmdr/no-unrendered-log-fields` (`src/lib/logging/DETAILS.md`).

**Decision**: `types.rs` is the floor of `cmdr-operations` and imports no sibling. `state.rs` keeps its
`operation_intent` + `scan_cache` + `status_cache` re-export facade, and `lib.rs` keeps its private root `use`s of the
transfer and delete entry points. **Why**: See § "Why `types` imports nothing" for the floor. The two surviving facades
sit ABOVE the floor and point down, so neither can close a circle: `state` and `lib.rs` already depend on everything
they re-export. Both front a broad name surface (`operation_intent` at ~35 sites across ~20 files, every cancellation
check; the `scan_cache` types across `scan.rs`, `scan_preview.rs`, `validation.rs`, and two test files), which is a
legitimate shape for a facade that costs nothing structurally.

## The free-space pre-flight

A copy checks the destination's room after its scan and before anything is written:
`free_space.rs::check_local_copy_space` for a local copy, `transfer/volume/copy.rs` Phase 2 for the volume engine
(`src/transfer/volume/DETAILS.md` § "The destination free-space pre-flight"). A local-FS move has no space check.

- **What the destination has**: `statvfs` first, the purgeable-aware NSURL figure only on a shortfall, the larger of the
  two winning (§ "Shared gotchas" has why). No figure at all lets the copy go ahead.
- **What the copy can need**: the full write footprint (`total_bytes`), unless that doesn't fit AND the destination is a
  local disk (`cmdr_local::filesystem_kind::path_is_on_network_mount` says no). Then `local_copy_need` stats each source
  file at its destination name, and a file landing on an existing file counts per policy: `Skip` nothing, the three
  overwrite policies `max(0, size - existing)` plus headroom for the largest `min(size, existing)` (the new bytes stage
  beside the old ones before the rename), and `Rename` / `Stop` the whole file (both copies can stay). That bound stays
  above the real peak whatever the order (proof on `SpaceNeed`). A source already AT the destination is a duplicate,
  counted in full. ❌ Never run that probe against a network destination: it's a round trip per file, so a share keeps
  the full count and relies on the next point.
- **A shortfall is the person's call.** The figure is an upper bound (it can't see clashes on a network share or inside
  the volume engine), so `InsufficientSpace` is a question: the error dialog offers "Copy anyway", which starts the same
  copy again with `SpaceShortfall::Proceed` (`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "Copy
  anyway"). A destination that really fills up mid-copy stops the copy as `DestinationFull` (`ENOSPC` / `EDQUOT` via
  `error_classification.rs`, `StorageFull` via `transfer/volume/transfer_error.rs`), and a failed copy keeps what
  landed. #351 is the case: re-syncing 1.5 GB onto a 2 GB SMB share that already held most of it was refused outright.

Pinned by `free_space_tests.rs` (the figure order, the per-policy need, the real-folder probe, the verdict) and
`transfer/volume/copy_space_tests.rs::copy_anyway_goes_past_a_destination_that_says_it_is_too_small`.

## Shared gotchas

**Gotcha**: On macOS, never refuse a copy on `statvfs` alone, and never ask NSURL first. **Why**: `statvfs` reports only
physically free blocks. On APFS, purgeable space (iCloud caches, APFS snapshots) can account for tens of GB that macOS
reclaims on demand, so `statvfs` alone rejects copies that would succeed.
`NSURLVolumeAvailableCapacityForImportantUsageKey` counts it, but it walks the volume in the kernel and serializes
across callers (0.6 s for one copy, over 5 s for 40 at once, 2026-10-02, #350). So `free_space.rs::available_space_from`
asks `statvfs` first and NSURL only when `statvfs` says the copy won't fit. Pinned by `free_space_tests.rs`.

**Gotcha**: Volume-side `on_progress` callbacks report counts LOCAL to the current scan operation, not cumulative.
**Why**: `Volume::scan_for_copy_batch_with_boundary` and `scan_subtree_with_oracle` both invoke `on_progress(count)`
with a count local to the current `list_directory` call / subtree (starts at 1 each time). Forwarding that unchanged
through `run_volume_scan_preview`'s closure made the FE's running tally drop visibly between parent groups, between
sibling top-level dirs in a cache-hit branch, and between recursion frames inside `scan_subtree_with_oracle`.
`run_oracle_aware_batch_scan` now wraps `on_progress` with a `baseline = aggregate.file_count` shift before each scan
call (cold-cache batch + cache-hit subtree), and `scan_subtree_with_oracle` does the same at its own recursion site
(`baseline = totals.file_count`). The visible FE count stays cumulative across the entire scan. Direct
`on_progress(aggregate.file_count)` emit sites in `run_oracle_aware_batch_scan` (cache-hit per-file paths, fallthrough
`scan_for_copy` after a name miss) stay unwrapped — they're already cumulative. Future scan call sites that delegate to
a volume backend or to `scan_subtree_with_oracle` need the same baseline wrap.

**Gotcha**: Copy's bar/space-check use the write footprint (`total_bytes`), not the dedup'd source size — by design.
**Why**: A copy of a hardlink-heavy tree writes every link in full, so the bar fills against the write footprint and the
disk-space check reserves it (the headline can legitimately read "80 GB" for a 60 GB-`du` `target/`). This is correct,
not a bug — `scan_subtree_with_oracle` and `copy_volumes_with_progress` both carry the un-dedup'd `total_bytes` for
copy, while the dedup'd `dedup_bytes` rides alongside purely to drive the dialog's clarifying note. Don't "fix" copy to
show the dedup'd number: that would under-reserve disk space and stall the bar on dupes. The cross-volume copy path
(`copy_volumes_with_progress` → `volume::strategy::copy_directory_streaming`) credits raw streamed bytes per file, which
already equals the write footprint, so no dedup wiring is needed there. The one residual approximation: `dedup_bytes`
over the cross-source-hardlink case (a file hardlinked into two separately-selected sources) counts twice, slightly
understating the dedup savings shown in the note — safe direction, documented on `CopyScanResult::dedup_bytes`.

**Gotcha**: Volume disconnect mid-walk races with the oracle. **Why**: The oracle returns `Some(entries)` when
`listing_watch_coverage` reports `EveryWriter` at the moment of the check. Between that read and the recursive walk
consuming the entries (and then issuing real `list_directory` calls for any sub-subfolders that aren't cached), the
watcher can die (cable yanked, network drop). The synthesized totals for the cached level are correct — they reflect
what the listing held — but recursion into now-disconnected sub-subfolders fails per-call, and the per-file copy/delete
later then hits `DeviceDisconnected`-shaped errors instead of a single "device gone" message at the scan level. Same
race that `scan_for_copy_batch` already had; the oracle doesn't widen it. Documented here so future investigation knows
where to look.

## Dependencies

Each with its reason in `Cargo.toml`. The workspace crates:

- `cmdr-fs`: the `Volume` trait every operation goes through, `FileEntry`, the host seams (`VolumeHost`, `ListingHost`),
  staging names, `ignore_poison`, and `deadline`.
- `cmdr-local`: the `LocalPosixVolume` the engine mints for local operations and pull scratch, its reader, and
  `rename_local_exclusive`, the one no-replace rename.
- `cmdr-operation-log`: the journal every mutation records into, `OperationId`, and the rollback planner an undo runs
  through (the engine is its `RollbackRunner`).
- `cmdr-archive`: the zip mutator, the archive volume a fresh zip is parsed through before it publishes, and the
  formats.
- `cmdr-collation`: `NameCollator`, the panes' order the transfer tree sorts with, named here so the engine never
  depends on the listing engine. `cmdr-listing` is a `testing`-only dependency, for `host::testing::PaneListings`.

No `tauri`, `tauri-specta`, `cmdr-index`, or app crate, which `index-crate-isolation` enforces. `specta` is pinned to
the app's exact version, because the wire types' `Type` impls must be the ones `tauri-specta` collects. `tokio` comes
without `rt-multi-thread`: the engine builds no runtime of its own and spawns on its host's.

**Decision: `cmdr-listing` stays off the normal dependency path.** With it there, every listing edit would recompile the
engine, and the build's long pole would become `cmdr-fs → cmdr-listing → cmdr-operations → cmdr`; the one type the
engine needed from it, the collator, sits in `cmdr-collation` instead. `cmdr-index` stays out the same way: the engine
asks the index through `OperationIndex`.

## Testing bar

This crate's state machine (`state.rs`) is the spine of the cancel UX. Past investigations found one real production bug
here ([commit `1de4255d`](../../docs/notes/speed-up-e2e-tests.md), lost-rollback on `Ok(())` arm) plus 30+
mutation-testing gaps that have since been pinned. New transitions or new cancel paths must:

1. **Drive the state machine through the public interface in tests.** A direct `state.intent().set_for_test(...)`
   bypasses the validation guard and effectively dead-tests it. Pattern to copy:
   `state_tests.rs::cancel_write_operation_flips_backend_cancel_to_stopped`.
2. **Cover both the happy path and the cancel-during-X race** for any new write-side operation. The Cancel-copy bug was
   specifically the `Ok(())` arm of the loop not re-checking intent.
3. **Add at least one E2E test** for user-visible flows (transfer dialogs, conflict policies); use `dispatchMenuCommand`
   for keyboard-shortcut triggers, see `docs/testing.md` § "❌ Synthesized F-key dispatches".
4. **Run `cargo mutants -p cmdr-operations --file crates/cmdr-operations/src/<file>.rs`** after substantial changes; the
   engine has ~85-90% mutation score per file and shouldn't regress. See `docs/testing.md` § "Process".

### Test isolation: an engine of the test's own

`cargo test` runs the crate's tests as threads in ONE process, so anything they share is shared by every write-op test
at once. **A test builds its own engine**: `testing::TestOperations::new()` (`testing/engine.rs`), an `Operations` over
one fake per seam it reads back. Two engines share nothing, so its registry, status cache, busy set, manager counters,
and line are that test's alone, under plain `cargo test` as under nextest:

- **Getting one.** `TestOperations::new()` journals nothing;
  `TestOperations::journaling_into(&TestOperationLog::new(label))` journals into a scratch database the test reads rows
  back from (`log.db()`); `TestOperations::with(|builder| …)` replaces any other seam (a volume host, pacing).
  `TestOperations::seeing(&panes)` is one whose host sees `host::testing::PaneListings`, panes over a listing engine of
  the test's own, for a cell that seeds the fresh-listing oracle (on `new()`'s detached host every oracle question
  misses). It derefs to the engine, so a test calls `engine.manager()`, `engine.cancel_write_operation(…)`, or
  `engine.status()` directly.
- **Reading back.** `engine.events()` (`RecordingEngineEvents`: the busy set, the transfer pair, `operations-changed`,
  error reports), `engine.volumes()` (`FakeVolumes`, which the test fills before an op names a volume id;
  `engine.register_local_root()` puts the local `root` volume there), and `engine.own_writes()` (`RecordingOwnWrites`).
- **States.** `engine.new_state()` builds a state on THIS engine (`engine.new_operation_state(interval)` for another
  interval; `testing::new_operation_state(engine, interval)` from outside the engine); `engine.register(id)` /
  `engine.register_state(id, state)` put one in the registry as a running op's is, returning a `RegisteredOperation`
  (`id()`, `state()`). No guard: the entry dies with the engine. Ids, lanes, and volume names only need to be distinct
  within the test.
- **The stores an op reads, on the same engine.** Seed a scan preview with `engine.stores().previews()`, photograph a
  live transfer's in-flight table with `engine.render_live_transfer_dump(id, reason)`, reverse what the engine journaled
  with `log.writer()`, and drive a reversal with `rollback::Reversal::on(&engine, tag)`. A fixture that cancels from
  inside a seam (a sink, a volume) holds an `Operations::clone(&engine)`. A state on one engine and a preview, probe, or
  registration on another is a test of nothing: the op reads its own engine's stores.
- **No engine test names the app.** A backend a test builds (a `LocalPosixVolume`, an archive volume) takes
  `host::testing::pane_order_host()`: no panes, but the panes' name order, which the local reader sorts a listing by and
  the walkers take as is (`operations_host::name_order`'s tests pin it against the app's). An oracle cell opens its
  panes with `PaneListings::new()`, inserts a pane's listing with `TestListing::insert(panes.panes(), …)`, and shows the
  panes its volume with `panes.show_volume(…)`, since the listing engine asks the volume about watch coverage.
  `FakeVolumes` announces each registration to the engine's arrival listeners, as the app's registry does, so the
  in-flight sweep's deferred records wake on `engine.volumes().register(…)`.
- **Gotcha: what the engine spawns runs on ITS runtime, not the test's.** `new()`'s detached host resolves to
  `cmdr-fs`'s shared fallback runtime, so anything spawned through `host.runtime()` (the concurrent copy driver's
  per-source tasks, the stall watchdog, an admitted op) runs on another runtime's threads. A thread-local test override
  (`AutoYieldTuningGuard`, `durability`'s `DirSyncLog`, the mount-table hook, `cmdr_fs`'s `helper_calls_on_this_thread`)
  doesn't reach it, a paused clock doesn't drive it, and on a pool two tasks start in either order. **Why**: the copy
  tasks used to be futures the driver polled on the test's own task, and moving them to spawned tasks (ERR-8MEX6)
  changed what the test thread can see. A cell that needs any of those three gives the engine its own runtime:
  `TestOperations::with(|b| b.volume_host(VolumeHost::builder().runtime(Handle::current()).build()))` (`VolumeHost` is
  `cmdr_fs::volume::host::VolumeHost`), on a `current_thread` test when it needs spawn order to be start order
  (`copy_tree_order_tests.rs::the_concurrent_driver_starts_its_sources_in_tree_order`). Overrides the DRIVER reads
  (`CancelDrainGuard`, `StallAbortGuard`) are fine either way: the driver runs on the test's task.
- **A walk that never touches an engine** (a `WalkContext`, `run_oracle_aware_batch_scan`) takes
  `&host::testing::PANE_ORDER_LISTINGS`, or a `PaneListings` when it reads the oracle.
- **What `FakeVolumes` can't route stays app-side**: the cells that need the app's registry (an archive or the git
  portal inside a parent, an id nothing knows) live in
  `apps/desktop/src-tauri/src/operations_host/registry_routing_tests.rs` and
  `apps/desktop/src-tauri/src/operations_host/copy_snapshot_out_tests.rs`, and the report redactor's cell in
  `apps/desktop/src-tauri/src/operations_host/log_redaction_tests.rs`.
- **Walks and counters are honest.** `engine.cancel_all_write_operations()`, `emit_count()`, `admission_pass_count()`,
  and `busy_emit_count()` see only this test's operations, so an equality assertion on one holds under plain
  `cargo test` (`retained_failure_stays_hidden_until_the_record_settles` once passed only because nextest forks per
  test). A synthetic op's deferred start settles on the engine it's handed (`manager::tests::gated_deferred`).

**App-side cells share the app's one engine** (`operations_host::app_engine()`, the engine `operations()` hands out in a
test binary): an IPC-level entry point, the MCP `queue` tool, and the backend suites reach it the way the app does, so
each such cell keeps its operation ids, lanes, and volume names unique
(`QueuedOperationFixture::park_on(app_engine(), tag)` mints them). A cell that only needs a registered state or a probe
table builds an engine of its own instead, and a cell that reads its journal back builds a log of its own and an engine
journaling into it (`operations_host::test_engine_journaling_into`). ❌ Never drive a walk-everything mutator
(`cancel_all_write_operations`) on the app's engine: it stops whatever the cells beside it have in flight, and the
failure lands in the victim, reading exactly like environment flake (mis-blamed on load starvation once and on a
dependency upgrade once). The fix is always a scope the test owns, never a mutex around the suites, an `#[ignore]`, or a
looser assertion.

**New subsystem state hangs off a struct, not a `static`.** A handle threaded through its callers gives every test its
own with no guard; the guards elsewhere (`cmdr_listing::testing::TestListingGuard` over the listing cache,
`indexing::tests::stress_test_helpers::TestInstanceGuard` over `INDEX_REGISTRY`) are the retrofit cost of a
process-global.

See also: `docs/testing.md` for the project-wide testing playbook.

## Bulk rename's hop log

`BulkRenameRecorder` (`rename/bulk.rs`) journals every filesystem hop the moment it lands, and the rows that never moved
once the run settles.

**Why per-hop rather than one pass at the end.** Journaling used to run after the whole batch, so a crash or force-quit
mid-batch left the renames done on disk, `operation_items` empty, `finalize_op` never run, and nothing for undo to
reverse (`restore_move` needs `ItemOutcome::Done` rows). The operation log's startup reconciliation only covers
`RollingBack` ops, so a crashed original stays `Running` with zero item rows forever. Recording as it lands is what
makes a partially-applied batch reversible.

**Why a rotation's temp hop gets its own row.** A cycle rotates through one same-directory temporary, and a case-only
rename does the same two-hop. Mid-rotation, one file's real name exists only at a `.cmdr-bulk-rename-*` path. Recording
`source → temp` as its own rollback unit means a crash there leaves the file findable by name and reversible in the
ordinary reverse-order replay. Consequence to expect: a two-file swap journals three rows, and a case-only rename two.
An in-process restore records its reversal hops too, so the log stays a faithful list and the reversals cancel out under
replay.

**The temp is NOT an `in_flight_temps` entry**, tempting as the shape looks. That ledger's next-launch sweep calls
`remove_file` on what it holds, which is right for the half-written `.cmdr-tmp-*` partials it exists for and destroys
data here: a rotation temp holds a COMPLETE user file under a private name.

**Skips are log entries, not rollback units.** Non-landing rows are journaled with `ItemOutcome::Skipped` / `Failed` so
the operation log stops claiming a smaller batch than the one that ran (they were dropped entirely before). They never
reach undo: `read_rollback_units_page` streams only rows that `RollbackUnit::awaits_reversal` (`Done`, or `Skipped` with
a rollback's own `rollback_skip_reason`), and `restore_move` refuses the rest defensively. Pinned by
`a_skipped_row_is_logged_but_never_offered_to_undo_as_a_rollback_unit`.

## What the sweep does with each kind

The persisted ledger records four shapes of thing, and exactly one of them is disposable. The rules run from two places
— the engine's ledger claim at launch, and the volume registry's arrival announcement (a registration, or a registered
volume reconnecting in place) — on the claiming engine's host, and ❗ they must stay ONE set: a leftover on a USB stick
meets whichever happens first, so rules that drifted apart would make its fate depend on whether the drive was plugged
in at launch.

- **A temp** (`.cmdr-tmp-*`) holds bytes on their way in. Nothing else has them, so a leftover is garbage and goes. A
  local one that refuses its `unlink` is cleared (`uchg`, ACL) and retried once (`Surface::remove_temp`): an APFS clone
  copies the source's Locked flag and ACL onto the temp in the same call, so a crash before the landing clears them
  leaves one. ❌ Only a temp: an aside is the user's own file, and its flags stay theirs.
- **A landing** (`ItemKind::Landing`, also `.cmdr-tmp-*`) is a cross-volume staged write's complete temp whose way to
  its name may have been cleared (`transfer/staged_write.rs::land`), so it can be the destination's only copy. ❌ Never
  removed. The temp is read FIRST: gone means the landing rename went through (a crash before its record retired), so
  the record retires rather than deferring forever. A temp of exactly `expected_size` takes its name when that's free
  (finishing the landing), or a ` (recovered)` name beside whatever holds it. Any other size isn't provably the file, so
  it never takes the name: it gets a ` (recovered)` name too, and raises `leftovers-kept`
  (`KeptLeftover::UnfinishedLanding`) naming where. Left on its scratch name, it would be retried forever while recorded
  and reaped by name and age once not. The kind change writes the landing BEFORE it retires the temp record
  (`in_flight_temps::hold_for_landing`), so a crash between leaves both, and the sweep settles that path by the landing
  (`without_superseded_temps`); the other order would leave a moment where only the record the sweep deletes is on disk.
  ❗ The hold answers whether the landing's line really reached the log (`OnDisk`: the writer reports each confirmed
  line back, `in_flight_log_writer.rs::append_then`), and a ledger that keeps a log but lost the line (its writer gone,
  the write refused, no path space to state it in) answers `NotOnDisk`, so the landing refuses to clear the way
  (`transfer/DETAILS.md` § "File writes are staged"). A ledger no claim set out to keep on disk (a test's engine)
  answers `NotKept`, and the landing goes ahead in memory.
- **An aside** (`.cmdr-temp-*`) holds bytes that were ALREADY the user's, renamed out of the way so a replacement could
  take their name. It goes back to its own name when that name is free; it is removed ONLY when the replacement is
  provably complete, which for `FileAside` means a regular file of exactly the recorded size; in every other case its
  bytes keep a ` (recovered)` name beside where they belong (the same convention
  `DisplacedEntry::keep_as_recovered_sibling` and the volume engine's rescue use). A cross-volume swap's aside
  (`transfer/staged_write_landing_aside.rs`) is a `FileAside` too, and ❗ a volume's records settle in `ItemKind`'s
  variant order, so a landing that same swap left (its new bytes rescued onto a `Landing`) finishes first and the aside
  then sees the replacement whole. The aside is read FIRST, as a landing's temp is: one that isn't there retires
  (`AlreadyGone`). A record can name an aside that never happened (a rename aside the transport cut off is recorded
  because it MAY have, `transfer/volume/displaced_destination.rs`), and without the read its restore would refuse and
  defer at every arrival.
- **A staging directory** (`.cmdr-staging-<op>`) is a cross-filesystem move's half-built tree. ❌ Only ever
  `remove_dir`, never recursively: a destination that left the mount table returns before Phase 5, and if it left before
  Phase 3 that folder holds the whole staged tree on a drive that comes back later, with the originals possibly already
  gone. A non-empty one stays, warns, and raises `leftovers-kept`.

❌ **Nothing removes something it hasn't looked at.** Every removal is gated on a presence read, and a read that FAILS
is never treated as "nothing there" (`Standing::Unknown` defers). Same rule one level up: `discard_temp` retires a
record on a `NotFound` only while the destination is still listed, because on a pulled drive "not found" is the mount
being gone and the partial is still on the drive.

❌ **Neither entry point runs on a runtime worker.** The rules are `async` so the `Surface::Volume` arm can await a
backend, but every `Surface::Local` arm is plain blocking `std::fs` aimed at a removable drive — the one place a `stat`
or an `unlink` sits for 30-120 s on a wedged mount. The launch sweep gets its own thread, the arrival sweep a
`spawn_blocking`, and both `block_on` the host's runtime from there (`Handle::block_on` is legal off a runtime worker).
❌ Never `runtime().spawn` for either.

**A file→folder overwrite sets a whole DIRECTORY aside.** Even with the replacement provably complete it gets a real
name rather than a recursive delete; the person decides what to do with the folder.

**Four homes' worth of one distinction.** `ItemHome::Local` is a path that's always there (the boot volume), swept with
`std::fs` before the volume registry even exists. `Mount` is a local mount that can go away: the path is stored RELATIVE
to the mount root, so a remount at `/Volumes/Backups 1` still finds it, and the sweep uses `std::fs` once the registry
has the volume. `VolumeSpace` is a volume's OWN namespace, swept through `Volume`. ❗ The `Mount` / `VolumeSpace` split
is stated by the producer and ❌ never re-derived: a direct SMB session roots at `/` in its own namespace, so guessing
from the path would point `std::fs::remove_file` at the user's Mac.

**The ledger sits BELOW `overwrite`.** `overwrite` tracks every temp and aside through `in_flight_temps`, so the ledger
and its sweep ❌ never `use` `overwrite`: the sweep's local no-clobber rename calls `cmdr_local::rename_local_exclusive`
directly, the one primitive `overwrite::rename_no_replace` delegates to. Reaching up for the alias welds the two modules
into a cycle `module-cycles` reports.

**Why the kinded records got their own op bytes** (`A`/`a`) rather than widening the `+` line: a build without them
skips an op byte it doesn't know and truncates the log at launch, so a rollback FORGETS these records. A widened `+`
would have been read by that build as a temp, and a temp is the one kind it removes on sight — which is exactly the
aside it must never touch. `in_flight_sweep.rs::name_matches_kind` holds each kind to its own name shape, so a `Temp`
record never matches a `.cmdr-temp-` aside, for the same reason.

**The notice.** A kept staging folder raises `leftovers-kept` (drive name + `KeptLeftover::StagingFolder`) as an
engine-wide fact (`EngineEvents::leftovers_kept`); the sweep belongs to no operation and has no event sink. A
Mac-internal one gets the `warn` alone — the copy names a drive, and the boot disk never goes away, so it meets the rule
again next launch. `in_flight_sweep_tests.rs` pins both halves, each cell's sweep on a host of its own (`FakeVolumes`
holding the drive, `RecordingEngineEvents` hearing the notice).

## Who claims the ledger

The ledger is what lets a crashed copy's temps be found and removed, and a second ledger on one data dir would replay
the first one's LIVE records and delete its temps mid-copy. So exactly one claim, on exactly one engine, ever touches a
ledger file:

- **Every engine has a `Ledger`** (its `EngineStores`), which its ops record into through their scope. An engine that
  never claims (a test's, a detached one, a duplicate that lost the install) records into memory only and never reads,
  truncates, or sweeps a file.
- **`Operations::claim_ledger(data_dir)`** (`in_flight_claim.rs`) is the one path to a file: it opens and truncates
  `data_dir`'s log, replays it, and starts the sweep. It consumes the engine's one claim; a second is
  `Err(AlreadyClaimed)` and leaves the file alone. ❌ Never call it from `build()`.
- **The app claims through `operations_host::claim_ledger`**, which goes through `operations()`, so only the `OnceLock`
  winner can be claimed. It runs where the launch sweep always ran: after the instance lock (`instance_lock.rs` keeps a
  second process off the data dir whenever its lock can be taken), before `init_volume_manager`, unconditionally.
- **No data dir, no file**: the claim runs unpersisted and raises `OperationErrorReport::LedgerUnpersisted`. That is the
  only unpersisted production path.

Pinned by `in_flight_temps_claim_tests.rs` (a second claim, an unclaimed engine beside another run's ledger, no data
dir) and `operations_host::tests::the_app_claims_the_installed_engines_ledger_once`.

## Testing the in-flight temp ledger

**Every test that records or sweeps owns its ledger.** `in_flight_temps::Ledger` is a handle; each engine has one (which
a `WriteOperationState` records into, through its scope, unless it carries another), and a test builds its own and hands
it to the states it drives with `WriteOperationState::with_in_flight_ledger`. The state holds only that `Option<Ledger>`
and `Ledger::of` resolves it, so `state` never calls into `in_flight_temps`: that call closed a `state` →
`in_flight_temps` → `sweep` → `transfer_sides` → `state` module cycle. `Ledger::recording_in(data_dir)` records into a
fresh log there; `die()` is the crash (it waits for the writer thread, then stops writing, so a state still holding a
clone can't add to the log the next run replays); a cell that reads the log while its ledger lives calls `flush_log()`
first, since lines reach the file from the writer thread; `hold_writer()` parks that thread, which is how the ordering
cell proves a temp only queues while an aside waits to be on disk (seams: `in_flight_temps_test_support.rs`);
`Ledger::for_test().launch_in(data_dir, &engine)` is the next launch, replaying that log, its sweep on the host of the
engine it's handed (each launch on an engine of its own, whose `FakeVolumes` holds the cell's volume); `live_paths()` is
what that ledger alone believes is on disk. So a cell's log, tally, and live set hold its own records and nothing else,
and it can assert on all of them whole.

- ❌ **Don't reintroduce a singleton that tests install a log into.** That's how a sweep cell replayed the records of
  every transfer test running beside it and deleted their live temps mid-copy under plain `cargo test` (#162; the check
  runner's one-process-per-test runner hid it). A test whose states use their engine's ledger records only in memory,
  since nothing claims a test engine's ledger.
- **A volume-borne cell's arrival is `engine.volumes().register(…)` on its own engine.** Each engine's `FakeVolumes` is
  its own registry, so one cell's registration never reaches another's ledger.

**The sweep signals completion, so no test needs a deadline.** A claim (and `launch_in`) returns a `SweepHandle`; the
launch path drops it (waiting there would block on a dead mount for minutes), and a test calls `.wait()` to join the
sweep thread, which also answers a `SweepTally`: assert on that rather than on a log line. Joining keeps the sweep from
outliving the `TestDir` it is walking. The volume-arrival half runs on a task instead, so a cell waits for it with
`wait_until_async`.

The cells live in `in_flight_temps_tests.rs` (what replays, what defers, what a producer records) and
`in_flight_sweep_tests.rs` (one cell per rule, driving `settle_kind` against a `TestDir`), each included as its module's
own `tests` so they reach the private record types. The real-detach half is `transfer/real_image.rs`: a drive pulled and
plugged back in, on a live HFS+ image.

**A `Mount`-homed cell needs the mount table answered.** `transfer_sides::test_hook` is THREAD-LOCAL, and the launch
sweep runs on its own thread, so a cell that wants a fixture directory to read as a mounted drive calls `settle` (or
`settle_kind`) on its own thread rather than going through a launch. `engine.volumes().register(…)` stands in for the
arrival: `FakeVolumes` announces each registration, which is what wakes the ledger's listener.
