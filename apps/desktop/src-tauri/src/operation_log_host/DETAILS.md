# Operation log host: details

The app side of the operation log. The log (`crates/cmdr-operation-log/`) owns its state in an `OperationLog` handle
and knows nothing about Tauri, settings, or the data dir; this module supplies them. Read this before any non-trivial
work here: editing, planning, reorganizing, or advising.

## Startup

1. `install()` runs in `setup()` right after the volume host, before the write engine is built
   (`operations_host::install` takes the handle), and returns the handle, not yet open. Until `start` opens it, it
   journals nothing: an operation opened before then stays unjournaled for its whole life.
2. `start(app, &log)` runs late (beside `agent::start`), once the data dir resolves: it seeds the recording switch from
   `settings.json`, calls `OperationLog::open` (store, writer, reconcile, then publish), and starts retention on the
   app's runtime. A data-dir or open failure logs a warning and leaves the app running without history.
3. On a good open, `start` also asks the log's registry to say when the writer's build of the item tree's folder
   indexes settles, built or not (`when_indexes_settled`), and emits `OperationLogTreeIndexesSettled`
   (`operation-log-tree-indexes-settled`) then, once. The writer builds them right after open (`crates/cmdr-operation-log/src/store/tree_indexes.rs`); the Operation
   log's tree, which answers `indexesBuilding` until then, reads again on the event instead of polling. The callback
   runs on the writer thread, so it only emits.

- **Decision: `start` takes the handle `install` returned.** Why: a re-read through `operation_log()` could, in a
  mis-ordered startup, open a detached placeholder while the engine journals into the installed one. Passing it makes
  "the handle that opens is the handle everyone reads" true by construction.
- **Decision: a pre-install read answers from a separate detached handle and is reported**, the listing host's
  pattern (`../listing_host/DETAILS.md`). Why: claiming the slot with an early placeholder would make `install` lose
  silently, and every later journal call would go nowhere. The test binary never calls `install`, so there the first
  read builds the shared handle.

## The write engine's journal

`install` returns the handle, and `operations_host::install` hands that same handle to the write engine, which journals
into it (`Operations::operation_log`) and never reads `operation_log()`. The test binary's shared engine is built over
the shared handle too, as the app's engine is over the installed one.

## Answers passed in

- **Volumes**: the planner's `VolumeLookup` is the write engine's `OperationVolumes` (its supertrait), so a rollback
  gate and each reversed item look volumes up through the engine's host, which is the real registry in the app.
  `volumes.rs` implements it for a bare `VolumeManager`, test-only, for the backend suites' rollback cells
  (`operations_host/backend_suites/s3_engine_integration_test.rs`, `smb_flags_test.rs`), which reverse an op against the
  real registry with no engine in hand.
- **Retention limits**: a closure over `settings::load_operation_log_retention_limits`, read on every tick so a
  settings change applies on the next one.
- **Recording**: seeded here at `start`; live changes arrive through `commands/settings.rs::set_operation_log_enabled`,
  which calls `operation_log().set_recording`.
- **Pacing**: not here. The rollback planner asks its runner (`RollbackRunner::pace_between_items`), and the engine's
  runner answers from its host's `Pacing`, which `operations_host/pacing.rs` answers from `test_mode`.

## Tests

Tests leave the shared handle's journal slot alone (one cell,
`operations_host::tests::the_shared_test_engine_journals_into_operation_log`, probes its wiring): a write-pipeline test
builds a log of its own (`cmdr_operations::testing::TestOperationLog`) and an engine journaling into it, app-side
through `operations_host::test_engine_journaling_into`, so plain `cargo test` needs no lock around the slot.
