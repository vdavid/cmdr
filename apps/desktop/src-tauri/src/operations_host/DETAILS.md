# Operations host: details

The app side of the write engine's seams. The engine declares what it needs in `crates/cmdr-operations/src/host/`
(each seam's contract: `crates/cmdr-operations/src/host/DETAILS.md`); this module answers, and this doc is the one
place the side effects of those answers are written down.

## Startup

`install(app, operation_log)` runs in `setup()` right after `operation_log_host::install()`, which follows
`volume_host::install`, and runs unconditionally. It builds the app's one `Operations` with `Operations::builder` over
`volume_host::host()` (the engine's runtime), a `TauriEngineEvents` holding the app handle, the operation log
`operation_log_host::install` returned (passed by value, so the engine's log IS `operation_log()`), the app's answer
to every optional seam (`app_answers`, shared with the test binary's `test_engine`), and a sink factory that builds a
`TauriEventSink`, and keeps it behind `operations()`. That's before anything can start an operation, before the
ledger claim, and before `init_volume_manager`. Every app caller reaches the engine through `operations()` and calls a
method on it; the engine's root `pub use` list is all it may name.

`claim_ledger(data_dir)` runs later in `setup()`: after the instance lock (no other
process holds this data dir) and before `init_volume_manager`, unconditionally. It claims the installed engine's
in-flight ledger through `operations()`, so a duplicate that lost the install is never the one claimed; a second claim
is refused and logged. With no data dir the engine runs the ledger unpersisted and reports it
(`crates/cmdr-operations/DETAILS.md` § "Who claims the ledger").

- **Decision: a call before `install` is refused, never run detached.** `operations()` answers `Err(NotInstalled)`
  and raises a `log_error!`. A start maps it through `refuse_not_installed` to the frontend's generic `IoError` (its
  dialog renders that without naming a file), an instant op through `refuse_instant_not_installed` to
  `ArchiveEditNotReady` ("Cmdr is still starting up"), a scan preview through `refuse_scan_preview_not_installed` to an
  immediate error event; a query answers what an engine with nothing running would (an empty list, nothing busy, an
  unknown id). No new variant reaches `bindings.ts`. Unreachable in the app, which installs at the top of `setup()`;
  the engine knows nothing about installs.

- **Decision: the runtime comes from the volume host, with no second handle.** Why: the engine and the backends it
  drives then share one pool, and the two can't disagree about which runtime is the app's. The admission pass
  depends on it being the app's (`crates/cmdr-operations/DETAILS.md` § "The admission pass spawns admitted ops on the APP
  runtime").
- **Decision: the sink factory is the engine's only way to a Tauri sink.** Why: an instant op routed into a zip becomes
  a managed archive edit the ENGINE starts, with no caller-built sink to hand it; and the MCP rollback tool is generic
  over the Tauri runtime, so it can't build the concrete `TauriEventSink<Wry>` from the handle it holds. Both ask the
  factory, and a test binary (no factory) gets `None` and refuses plainly.

## The per-op sinks (`sinks.rs`)

`TauriEventSink` (`OperationEventSink`) and `TauriScanPreviewSink` (`ScanPreviewEventSink`) turn each event into its
Tauri event. Every IPC command that starts an operation or a preview builds one and injects it. Engine and agent
wrappers sit in front of it (archive move-out's `SuppressTerminalsSink` withholds the inner extract's terminals; the
agent's `ProposalReportingSink` adds a write and forwards everything, the defaulted `emit_sources_gone` and `emit_locked_sources` included), so a
side effect added here reaches the person only through what each wrapper forwards.

The four terminal emits carry app side effects in a pinned order (`terminal_effects.rs`, pinned by
`terminal_effects_tests.rs` with a recorder):

- `write-complete`: completion analytics (`analytics.rs`), then the MCP terminal ring, then the emit.
- `write-cancelled`: the ring, then the emit.
- `write-error`: the ring, the failure `warn!` (target `write_operations`, the one place the technical reason is
  written down; `crates/cmdr-operations/DETAILS.md` has the WARN-not-error decision), the engine's
  `Operations::retain_failure`, then the emit.
- `write-settled`: the free-space hint (`cmdr_local::live_space::expect_space_change`, macOS), then the emit.

- **Decision: the terminal side effects live in this sink, not behind an engine-level "operation ended" observer.** Why:
  the MCP ring, the completion analytics, the failure `warn!`, and the space hint are app concerns, and the sink is
  already the app's per-op voice. An engine observer would double-fire under archive move-out's `SuppressTerminalsSink`
  (which swallows the inner extract's terminals on purpose) and would make every collector-sink test retain failures.
  So the engine keeps only the fact it owns, `retain_failure`, and test sinks stay side-effect-free.

The ring and the retained failure land BEFORE the frontend hears: MCP's `await operation_complete` answers the instant
the event arrives, and a backgrounded op that fails while the queue window is closed can still say why. Completion
analytics are categorical only (op kind, a count bucket, a bool), and an instant op has no completion category: it never
emits `write-complete` and reports an `InstantMutationFact` instead.

## The wire (`wire.rs`)

The engine's payloads (`crates/cmdr-operations/src/types/events.rs`, plus `OperationsChanged` and `MutationSettled`) derive only
`serde` + `specta::Type`; the Tauri event, with its wire name, is the app's. `wire_event!` gives each a newtype that
derives `tauri_specta::Event` with `#[serde(transparent)]`, `ipc.rs`'s `collect_events!` registers the newtypes, and
every emit site writes `wire::WriteProgress(event).emit(&app)`. A payload emitted unwrapped doesn't compile.

- **Decision: wrappers, not app-side mirror structs.** Why: the orphan rule forbids implementing `tauri_specta::Event`
  on another crate's type, and once the engine is a crate its payloads are foreign. A newtype is one line per payload;
  mirrors would be 20 structs plus conversions, and the collector sink would assert on a second shape (verified across
  a real crate boundary by the event-wire spike, commit `b10cc9821`, 2026-10-16: `bindings.ts` regenerated identical).
- **Decision: the wrapper's `specta::Type` is hand-written**, returning the payload's own definition. Why: a derived
  one, even with `#[specta(transparent)]`, exports `type WriteProgress = WriteProgressEvent` and renames the events
  object's generic, so `bindings.ts` changes. The hand-written one keeps it byte-identical, and a command returning the
  payload (`latest_operation_progress`) still names the payload itself.
- **A bad wrapper fails loudly**: `register_event` panics on a type whose definition isn't a reference, at
  `ipc::builder()`, which the bindings regen calls, so `desktop-bindings-fresh` goes red. The wrappers derive only
  `Serialize` + `Clone`, since nothing in Rust listens to an engine payload; one that ever does adds `Deserialize` to
  the macro.
- **Wire names live only here**, one literal per wrapper. The payload's doc comments and `#[serde]` attributes still
  reach `bindings.ts` from the engine, so they stay part of the wire.
- A settle callback the engine hands back (`MutationSettled`) goes out through `commands/settled.rs::broadcast_settled`,
  which takes the wrapper constructor (`wire::MutationSettled`) as its `wire` argument.
- A pin: `wire::tests::a_wrapped_payload_reaches_the_wire_unchanged` mounts a mock app and checks the raw JSON under
  `write-settled` equals the payload's own.

## What each fact becomes

- `operations_changed` → the `operations-changed` event (a failed emit `warn!`s under `op_manager`).
- `volumes_busy_changed` → `volumes-busy-changed`, wrapped in `VolumesBusyChanged`, an event struct this module owns
  outright since the engine only reports the ids (a failed emit is a `log_error!` under `eject`).
- `transfer_started` / `transfer_finished` → `priority::transfers::note_transfer_started` / `_finished`.
- `leftovers_kept` → `leftovers-kept` (a failed emit `warn!`s under `copy`).
- `instant_mutation_settled` → `rename_used` / `folder_created` / `file_created`, with the `initiator`, `target`, and
  `outcome` tokens `analytics/DETAILS.md` § "Starter event set" catalogs.
- `error_report` → `log_error!`, one arm per `OperationErrorReport` variant, each with the message and target the
  engine's code always raised it with. A report that had no explicit target carries the engine module's path as a
  literal (`cmdr_operations::overwrite`, …), the target the engine's own `log` lines under that module carry, so
  `RUST_LOG` recipes and the report dispatch's per-chain filters match both.
- `scan_wait_progress` → `write-progress` (a failed emit `warn!`s under `op_manager`).

## The volumes (`volumes.rs`)

`AppOperationVolumes` answers the engine's `OperationVolumes` (and so the rollback planner's `VolumeLookup`) from
`get_volume_manager()`, reached per call: the registry is where volumes come and go, so a host that captured it at
`install` would answer from a stale picture, and one built before `init_volume_manager` from nothing. `unregistered`
maps `unregistered_volumes::why_unregistered`'s answer onto the engine's `UnregisteredVolume`, one variant each.
`routes_over_its_parent` is `path_routes_over_its_parent`, which reads the git portal's switch, so it stays app-side
even though it's pure string work. Pinned by `volumes::tests::the_adapter_answers_from_the_live_registry`.

## Own writes (`own_writes.rs`)

`AppOwnWrites` answers `OwnWrites::announce` with `downloads::note_pending_write_for_cmdr`, which no-ops while the
watcher sleeps and for a path outside the Downloads root (`downloads/DETAILS.md` § "Cmdr-own-write hook contract"). It's
called beside a syscall on every file landing, so it stays one prefix-checked insert. Under `cfg(test)` the hook feeds
`downloads/own_write_recorder.rs`, which is how `own_writes::tests::an_announce_reaches_the_downloads_hook` pins that an
announce reaches the hook. The engine's own announce order is pinned against its seam, in
`crates/cmdr-operations/src/own_write_announce_tests.rs`.

## The drive index (`index.rs`)

`AppOperationIndex` answers `OperationIndex` from `index_host::index()`, reached per call (the index is built in
`setup()`, and an adapter that captured it could win its slot with an unconfigured one). `expected_totals` maps the
index's count onto the engine's own `ExpectedTotals`; `recursive_size` reads `dir_stats` the way the listing does
(`prompt_size`): `Exact` only for a complete, fresh, not-pending total, `AtLeast` for a partial count, `None` for a stale
or in-flux one or nothing counted yet, because the conflict prompt shows it to a person deciding whether to replace that
folder (a 40 GB tree counted to 12 KB must never read as 12 KB, #528); `search_leaves`
is the honest-coverage read behind the operation log's search leaves (`crates/cmdr-operation-log/DETAILS.md` §
"Search-leaf enumeration"): the Live gate, path resolution, the epoch gate (`enumerate_from_id`), and the cap, each miss
a typed downgrade. Its cells (the coverage honesty, the cap's edges, an ignored latency bench) run against a hand-built
index DB; `an_unindexed_volume_answers_uncovered` pins the adapter itself.

## E2E pacing (`pacing.rs`)

`AppPacing` answers `Pacing` from `test_mode`: the IPC overrides a spec sets, then the `CMDR_E2E_*` variables, then
nothing. Each read is live, since a spec changes a throttle mid-run, and a zero throttle reads as no pause (the seam
never hands back a zero-length sleep). The scan-preview delay is gated on E2E mode inside `test_mode`, so a stray
variable can't slow a real build. Pinned by `pacing::tests::the_rollback_pace_follows_test_mode`.

## Name order (`name_order.rs`)

`AppNameOrder` answers `NameOrder` with `listing_host::listings().collator()`, asked per call: the reading language can
change between two transfers, and the listing engine already keeps one collator per language, so a transfer's tree and
the panes rank names identically. Pinned by `name_order::tests::the_engine_sorts_with_the_panes_collator`. The
engine's own tests build their local volumes on `host::testing::pane_order_host()`, whose listings sort like
`AppListings` does in English; `the_engine_test_host_lists_in_the_order_the_apps_host_does` pins the two together.

## A test binary

Nothing installs in a test binary, and app-side cells reach the engine the way the app does. So the first read of the
engine's slot builds `test_engine()`, and `operations()` (and `app_engine()`, its test-only unwrap) hands out that same
engine to every app-side cell: `TauriEngineEvents` with no app handle (events go nowhere; the gauge, the analytics
client, and the error reporter work as in the app), the real `AppOperationVolumes` (cells register their volumes in the
real registry) over `volume_host::host()`, whose runtime is `cmdr-fs`'s shared fallback, the test binary's shared
operation log, the same optional seams as the app (own writes reach the downloads hook and its recorder), and no sink
factory. The cells sharing it keep their ids, lanes, and volume names unique. A cell that needs
the app's answers on an engine of its own builds `test_engine()` itself, or `test_engine_journaling_into(&log)` to read
its journal back.

The engine's tests never name the app, so the few cells that need the app's answers live here:
`registry_routing_tests.rs` (an archive or the git portal inside a parent drive, an id nothing knows, a pane open
inside a zip the edit rewrote), `copy_snapshot_out_tests.rs` (copying out of the git portal end to end), and
`log_redaction_tests.rs` (an engine log label through the report redactor). Each reaches the engine through its public
list plus `cmdr_operations::testing`.

## Backend suites (`backend_suites/`)

Every backend's cells through the app's real engine, and the backend-blind scenarios they share. They sit here, beside
the host, because each needs the app around the engine: a Docker fixture, the real volume registry over
`volume_host::host()`, virtual MTP, or the ADB fake with the downloads watcher. They drive the engine through its
public list plus `cmdr_operations::testing` (the volume drivers they run with a state and sink of
their own, the fault injector, the safety oracle), ❌ never an engine module by path, so they compile against the crate's promised surface
and nothing else. Moving the pure network suites into `cmdr-operations`, so an engine edit tests them without building
the app, is #468.

### The network transfer suites

`webdav_transfer_integration_test.rs` and `sftp_transfer_integration_test.rs` copy real bytes between local disk and a
live fixture server through `cmdr_operations::testing::copy_between_volumes`, which is the seam neither backend crate
can reach: both directions of an actual copy were once broken in the app while `cmdr-sftp`'s own Docker suite was fully
green (a `supports_export` predicate the crate never states, and a free-space pre-flight reading `NotSupported` as "no
room").

- **The scenarios are backend-blind and live in `network_*_test_support.rs`.** Everything they touch is `dyn Volume`,
  taken as `(remote: Arc<dyn Volume>, dir: PathBuf)` (a live volume plus a scratch dir it owns and removes), so a claim
  proved against one backend is proved in the same words against the others and the suites can't drift. Each backend
  file connects its own fixture (`sftp_test_support::fixture`, `smb_test_support::fixture`, `webdav_test_support::fixture`) and
  delegates. Six files, by what they prove:
  - `network_transfer_test_support.rs`: the byte path (below).
  - `network_semantics_test_support.rs`: merges under Skip / Overwrite / Rename / OverwriteSmaller, a move-merge that
    spares what it skipped, whole-folder moves both ways, same-server move-merge, move, and copy, a missing nested
    destination, a 6 MiB odd-length round trip, and 40 files at full concurrency. Every merge asserts through
    `testing::assert_operation_was_safe`. Also the shared seeding and driving helpers (`seed`, `transfer`).
  - `network_safety_test_support.rs`: a failed merge copy or move onto the user's folder, a delete bound to a
    local-shaped preview, a recursive delete that takes exactly the selection, an unanswerable source-type probe, a
    download cancelled mid-file, and a name another writer takes mid-upload (never replaced, never deleted by the
    failed copy's cleanup; SMB's single-shot size is a known red gap, `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "The single-shot
    exemption"). Faults go on the LOCAL side (`FaultyVolume`); the server is what users run.
    `Registered` puts a volume in the registry for the ops that look it up, and cleans BEFORE unregistering, because
    unregistering retires the volume and a retired network volume drops its session.
  - `network_look_alike_test_support.rs`: an NFD name onto its NFC twin (Skip, Overwrite, folder merge, same-server
    move, bulk rename), new names landing composed, an existing decomposed entry read, downloaded, overwritten, and deleted under its own bytes, and an inline rename that never
    lands on a taken name or its other spelling.
  - `network_archive_test_support.rs`: a zip on the server browsed and extracted through the copy engine, the routing
    predicate, a remote edit and its cancel before the swap, files copied into a remote zip, a compress, and a
    compress onto a look-alike name.
  - `network_move_drift_test_support.rs`: a folder moved off the server while a file in it is saved over or a new one
    appears (#139). A local destination that edits the server as the first file lands opens the window; the edit
    stays with its new bytes, the rest goes, and `AppearedDuringMove` counts it. The saved-over file changes SIZE,
    since a server's whole-second mtime can't tell a same-size save apart.
- **Which backend drives what.** SFTP, SMB, and WebDAV drive all six; ADB drives `network_transfer_test_support.rs`.
  S3 drives the transfer, semantics, safety, and move-drift scenarios (`s3_transfer_*`), minus the name-taken-mid-upload
  cell, whose guard is a staged landing S3 never makes. Its scenarios take a `cmdr_s3::volume::testing::S3Target`, so
  `s3_live_engine_test.rs` runs the same bodies against real accounts by hand (`live-engine.sh`; each scenario's waits
  stretch to ten minutes under `CMDR_S3_LIVE=1`, `network_transfer_test_support::budget`).
  SFTP also points the same-server move and copy, the inline rename, and the remote zip edit at
  `sftp-fixture-noposixrename`, where the server can't copy for itself and a rename has no atomic replace. WebDAV points
  the zip browse at `webdav-fixture-norange`, whose whole-file answer to every ranged GET is what a zip reader's many
  small windows have to be cut out of locally. SFTP alone holds a link, so its cell for a moved folder holding a link
  to a folder outside the selection (#140) is its own, made through the test-only `SftpVolume::create_symlink`. It pins
  that the target keeps its files; the move itself fails on the link today (SFTP lists a link with lstat attributes,
  so the walk streams a folder link as a file). The dialog-addressed destination cells are SFTP's and SMB's own: the
  WebDAV fixture's remote root is `/`, where a volume-relative and a server-absolute path are spelled alike, so a
  doubled root can't be told from a right one there (`cmdr-webdav`'s `paths_test.rs` pins the refusal instead).
- **A server that says no mid-operation: `webdav_refusal_test.rs`.** 507 on an upload (answered after the body, and
  before it with the body unread, the way a quota'd Nextcloud answers), 403 on the upload and on the DELETE a
  safe-replace needs, and refusals of the server's own `COPY` and `MOVE`. Each arrives typed (`DestinationFull`,
  `PermissionDenied`), loses nothing, and ❗ leaves the volume connected, since a refusal is an answer. The refusal
  comes from `webdav_refusing_proxy_test_support.rs`, an HTTP proxy in front of the real Apache that answers one
  request itself; ❌ never reconfigure the shared container for a cell. A refused `COPY` falls back to streaming by
  design (`crates/cmdr-operations/src/transfer/volume/server_side_copy.rs::try_server_side_copy`).
- **`adb_transfer_test.rs` runs the same scenarios against a phone on `cmdr-adb`'s in-process fake server**, so it
  needs no Docker, runs in the unit lane, and has no name prefix to keep. Its own cells start a copy from two registered
  ids (`start_copy_by_id`, through `start_volume_copy`), check that a copy onto the phone lands through the writer's own
  staging `mv`, and read the pane patch a mkdir, a move, and a delete each owe from a `RecordingListings` host.
  ❗ A transfer onto a phone runs at width 1 (the `"adb"` row in `MAX_CONCURRENT_OPERATIONS_SOURCES`), so the serial
  driver runs it: the pre-existing-folder scenario there holds the per-name probe (`landing::top_level_precheck`),
  never the concurrent driver's skip on a `Created` answer. The ADB cells need nextest's process per test: the device
  list they register phones in is process-wide (`adb/device_provider.rs`), so plain threaded `cargo test` fails them.
- **❗ The cells themselves must stay in the backend files, on the `webdav_integration_` / `sftp_integration_` / `smb_integration_` name
  prefix.** The integration lane selects the app crate's Docker cells by NAME (`scripts/check/checks/fixture-lane-coverage.go`,
  enforced by `desktop-fixture-lane-coverage`), so a scenario promoted to a `#[tokio::test]` in the shared file would
  compile, look like coverage, and never run anywhere.
- **What the shared scenarios pin**: a nested directory tree lands intact in both directions (structure and bytes, as
  one `tree_fingerprint` comparison); a copy cancelled mid-upload leaves neither the user's filename nor a
  `.cmdr-tmp-*` sibling; an Overwrite answer travels out as a `write-conflict` and back through
  `resolve_write_conflict` and lands the source bytes; a PRE-EXISTING destination folder still probes each name under
  the concurrent driver (what a wrongly-`Created` `DirectoryCreation` would turn into a silent overwrite of every
  clashing file); and awkward names (`&`, `+`, `%`, `#`, non-ASCII) plus a zero-byte file survive a full round trip.
- **Nothing here waits on a stopwatch to decide a verdict.** The fixture stack is machine-wide and shared with every
  sibling worktree, so a wait that expires says "the containers were busy" at least as loudly as it says "the code is
  wrong". Two consequences to keep: a wait for a clash also ends when the operation SETTLES, so a destination probe that
  never happened fails as a sentence rather than as a timeout; and the cancel cell's premise is a chunk COUNT, not a
  duration.
- **The cancel cell holds the source at a chunk boundary rather than racing a timer.** Its `GatedUploadSource`
  (`network_gated_source_test_support.rs`, split out so neither file crosses the `file-length` warn threshold) hands
  out one chunk per semaphore permit, so the destination provably holds an open, incomplete staging sibling when the
  cancel lands; it then insists a `write-cancelled` was emitted and no `write-complete` was, so neither a copy that
  finished nor a run the host starved can satisfy the later claims for the wrong reason. It waits for TWO chunks, not
  one: the stream is polled again only once the destination has taken the previous chunk, so the second hand-out is what
  proves the first reached the server. That also keeps the cell inside the workspace-wide 8 s nextest cap,
  which a payload big enough to outrun a stopwatch would not. The download twin gates a LIVE server's own read stream
  (`gated_reads`, a `forward_volume_methods!` double), with a payload over two of SMB's 8 MiB read chunks.
- **❗ The SFTP cells share one stock server, one SSH session per cell** (nextest runs each test in its own process).
  That server gets two CPUs where the others get half of one; at half, two concurrent lanes pushed every SFTP cell past
  the 8 s cap. `apps/desktop/test/sftp-servers/README.md`.

### The SMB app-side suites

Every SMB cell whose other half is this app rather than the protocol lives here: `smb_transfer_semantics_test.rs`,
`smb_transfer_safety_test.rs`, `smb_full_concurrency_test.rs`, `smb_stress_test.rs`, `smb_soak_test.rs`,
`smb_archive_integration_test.rs` (remote-backed zips: browse, extract-out, and the pull-apply-upload-swap edit), and
`smb_stream_write_integration_test.rs` (a `LocalPosixVolume` source streaming onto the share). They connect through
`cmdr_smb::volume::testing`, wrapped by `smb_test_support.rs` so the volume they get carries the app's real
`VolumeHost` and the listing cache, index, and activity tracker see what the share reports. Two cells that assert on
other modules reach `smb_test_support` from there: `listing_host/tests/smb_pane_close_watch_integration_test.rs` and
`file_system/volume/smb_media_fetch_integration_test.rs`.

- **The semantics, safety, look-alike, and archive cells delegate** to the shared `network_*_test_support.rs`
  scenarios through `smb_test_support::fixture`; only the dialog-addressed move and the replaced-volume copy in
  `smb_transfer_semantics_test.rs` are SMB's own bodies.
- **Docker SMB integration tests**: `#[ignore]` tests that require Docker SMB containers (start with
  `apps/desktop/test/smb-servers/start.sh`). Run with `cargo nextest run smb_integration --run-ignored all`. Connect via
  `smb2::testing::guest_port()` (10480, guest/no-auth), `auth_port()` (10481, `testuser`/`testpass`), `readonly_port()`
  (10488), `slow_port()` (10493, 200ms latency). See `apps/desktop/test/smb-servers/README.md` for the full container
  list and env var overrides.
- **❗ A cell must carry the `smb_integration_` name prefix**, the same rule the WebDAV and SFTP suites above follow:
  the integration lane selects the app crate's Docker cells by NAME
  (`scripts/check/checks/fixture-lane-coverage.go`, enforced by `desktop-fixture-lane-coverage`), so anything else
  compiles, reviews clean, and runs nowhere.
- **Full-concurrency copy** (`smb_full_concurrency_test.rs`): the automated net under the 2026-07-31 transfer wedge
  (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). 400 local sources onto the share through
  `copy_volumes_with_progress` at the driver's own concurrency, with sizes on BOTH SMB write paths: the large ones are
  sized off the session's negotiated `max_write` at runtime, not hardcoded, so they always land on the staged streaming
  writer. Beyond byte-exactness it asserts three things a content check alone would miss: the concurrency window really
  filled (peak `TransferActivity::in_flight` off the progress events, against a floor rather than the driver's own
  formula, so a change to it can't fail this suite for the wrong reason), a `.cmdr-tmp-*` really appeared during the
  copy (else the "no leftovers" check passes vacuously), and none survived it.

  Both tests there bound their own wait and, on expiry, panic with `transfer_probe`'s LIVE in-flight table via
  `render_live_transfer_dump` — a `testing`-gated `Operations` method over the probe registry. They time out on the
  copy's `JoinHandle`, never on the copy future: timing out the future DROPS it, which drops the probe guard and empties
  the registry before there is anything to dump. `smb_integration_a_wedged_copy_is_caught_and_names_its_phase` is the
  test of that mechanism — it parks a copy on the pause gate and asserts the bound fires with a dump naming the
  operation, the driver phase, the window fill, and `parked(pause)`. Without it the deadline and the dump are untested
  scaffolding, and a suite meant to catch a hang becomes one. The wedge is staged through the pause gate rather than a
  silenced server because what is under test is the harness at expiry, and a pause reaches that state deterministically
  without holding the shared Docker stack hostage. `.config/nextest.toml` grants the big test a 75 s cap so its own 45 s
  deadline stays authoritative; a cap kill would leave no diagnostic.
- **A dropped network, end to end** (`smb_outage_test.rs`): a folder copy through `outage_proxy_test_support.rs`'s
  `OutageProxy` (a TCP proxy of the test's own between the share and the fixture) pauses part-way, the proxy cuts every
  connection and refuses new ones, the copy resumes into the outage and must WAIT (named, not failed), then the proxy
  restores and every file must land byte-exact with no `.cmdr-tmp-*` left. A sibling cell cuts mid-file, cancels while
  the share is out, and pins that `write-cancelled` arrives at once naming the temp `removedWhenBack`, and that the
  background cleanup removes it once the share reconnects. ❌ Never stage an outage by stopping a
  fixture container: the stacks are leased machine-wide, so that fails every other worktree's running suite. Their
  waits are paced by the copy's progress (`wait_until_async_while_moving`, `QUIET` of no progress, one `CELL_BUDGET`
  under a 120 s cap), ❌ never a stopwatch: a healthy run on a starved shared Samba outlasted the general 30 s cap.
- **The stress test's hung-run diagnostic gates on the log TARGET** (`MutexCaptureLogger::enabled` in
  `smb_stress_test.rs`): `cmdr_smb::*` for the `client-mutex:` records and `smb2::*` for `recv:`. Without that gate the
  whole process-wide trace stream flows through `log()` and gets `format!`-ed per record, which pushed
  `smb_integration_concurrent_streaming_writes_no_deadlock` past its own deadline. ❗ The target is the EMITTING crate's
  module path, so moving where the SMB client lives silently empties the capture rather than failing anything.
- **SMB soak test** (`smb_soak_copy_loop` in `smb_soak_test.rs`): repeats the SMB→Local copy pipeline for hundreds to
  thousands of iterations and watches RSS, open FDs, SMB credits, and per-iteration wall-clock drift. Catches
  accumulating bugs the single-shot integration tests can't see (credit leak, FD leak, memory growth, slowdown).
  Default mode: `CMDR_SOAK_ITERATIONS=100` (≈5 s against Docker). Long mode: `CMDR_SOAK_DURATION_SECS=1800` (30 min,
  via `./scripts/soak-smb.sh`). CI has a `workflow_dispatch`-only job in `slow-checks.yml`. It is deliberately outside
  the gating lane, so it carries no `smb_integration_` prefix.

