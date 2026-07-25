# Extract the index into a `cmdr-index` crate

Status: planned, 2026-07-25. Not started. Revised after three review rounds; every count below is measured, not
estimated.

Move `indexing/`, `media_index/`, and `importance/` out of the app crate into a standalone, Tauri-free workspace crate
with a designed public API: typed errors, no user-facing strings, real cancellation, structured progress, and a
first-class ingest side alongside the query side.

**One thing needs David's sign-off before M6**: the path-keyed allowlists (`file-length-allowlist.json`, 16 entries
under the three subsystems) reset when the files move, and `.claude/rules/file-length-allowlist.md` forbids re-adding
entries without explicit consent. The numbers don't change, only the paths. See M6.6.

## Why (the intent behind everything below)

Three motivations, in priority order. When a decision below is ambiguous, resolve it toward the higher one.

1. **Encapsulate the hardest code in the codebase behind a boundary you can reason about.** These three subsystems are
   89,540 of 318,142 `src-tauri/src` lines (28%), and they hold the gnarliest concurrency and lifecycle logic we have.
   Today their "API" is 65 named re-exports plus a glob in `indexing/mod.rs` (26 `pub` / 39 `pub(crate)`), on top of
   three fully public modules (`aggregator`, `store`, `writer`), backed by ~50 process-wide mutable statics. There's no
   line between "what the app may rely on" and "internals", so every app change can reach into indexing internals and
   every indexing change has an unbounded blast radius. A crate makes that line real and compiler-enforced.
2. **Build-time separation.** David works on app backend without touching indexing, and on indexing without touching
   the app. Today either edit rebuilds all 318k lines.
3. **Make the index a thing that could later be a product.** "Cmdr, plus a smart file+image index any agent can tap
   into" needs a documented, stable, self-contained API. This plan builds that API. It does NOT build a daemon.

Two features David wants next, which the API must admit without redesign:

- **Listings auto-enrich the index.** A directory listing already paid for the syscalls; feed the result back so the
  index self-corrects exactly where the user is looking.
- **Space-to-size on any folder, including unindexed drives**, persisted and kept fresh.

Both are app → index **writes**. So the crate's API is bidirectional from day one, not a read-only query surface with
ingest bolted on later. See Decision 10.

## Non-goals

- **No separate process, no daemon, no login item.** The deferred escalation is captured in
  `docs/specs/later/out-of-process-indexing.md`; this plan is its prerequisite, not its start.
- **No behavior change.** Zero user-visible difference at the end of this plan.
- **`search/` stays in the app.** It's a product surface with ranking choices and UI copy. (One constant moves the
  other way: see Decision 6.)
- **No `pub(crate)` laundering.** Making everything `pub` to compile is failure, not progress. See Decision 3.
- **No de-globalization of internals.** The ~50 statics move; they get hidden, not removed. See Decision 4.

## What "a nice crate" means here (the API contract)

The acceptance bar. Every milestone is judged against it.

1. **No `tauri` in the dependency tree.** Enforced by a check, not a convention (Decision 7).
2. **No user-facing strings.** The crate emits typed values; the app renders every word a human reads. Diagnostic
   strings for `log::` are fine and stay English.
3. **Typed errors everywhere.** No `Box<dyn Error>`, no stringly-typed failure. Every variant carries the data a caller
   needs to decide what to do, so the app never string-matches (`.claude/rules/no-string-matching.md`).
4. **Everything long-running is cancelable**, through one primitive, with cancellation observable from outside.
5. **Everything long-running reports progress** as structured values through a caller-supplied sink.
6. **A handle, not a global.** The public API is methods on an `Index` value the app constructs and owns.
7. **The crate-root lint block is replicated, not weakened** (Decision 15), `#![deny(missing_docs)]` added.
8. **Ingest and query are equal citizens.**

## Target crate graph

```
crates/cmdr-fs/          NEW. Leaf vocabulary + host primitives. No index deps.
crates/cmdr-index/       NEW. indexing + media_index + importance.
crates/index-query/      exists. Keeps a `cmdr` dep (see M7.5); loses its index-internals reach.
apps/desktop/src-tauri/  the app. Depends on both. Owns all real-storage Volume backends.
```

### The cycle, and how it actually breaks

`file_system/` sits on **both** sides: the three subsystems reference it (69 `crate::file_system` refs), while
`file_system/*` references them back (46 refs across 18 files, including `file_system/watcher.rs` and
`volume/backends/archive/watch/mod.rs`).

The `file_system → indexing` direction has exactly **two** causes, and both are removable:

1. **Dead speculative API.** `file_system/volume/mod.rs:20-22` imports `ScanConfig`, `ScanError`, `ScanHandle`,
   `ScanSummary`, `DriveWatcher`, `FsChangeEvent`, `WatcherError`, and `IndexWriter` — solely to define the
   `VolumeScanner` (`:101`) and `VolumeWatcher` (`:123`) traits and the `Volume::scanner()` (`:575`) /
   `Volume::watcher()` (`:583`) methods. **All of it is dead.** `grep -rn '\.scanner()\|\.watcher()' src/` returns
   nothing; the only implementor is `LocalPosixVolume` (`backends/local_posix.rs:769,773`), whose
   `LocalPosixScanner`/`LocalPosixWatcher` (`:781,803`) wrap `scanner::scan_volume`, `scanner::scan_subtree`, and
   `DriveWatcher::start` — which the lifecycle layer calls **directly** (`lifecycle/manager.rs:494,732,797`,
   `reconcile/verifier.rs:385`, `watch/event_loop/verification.rs:95`). Volume-kind dispatch runs through
   `VolumeKind::uses_local_scanner()` (`lifecycle/manager.rs:138`), not the trait. The whole mechanism survives only
   under `#![allow(dead_code, reason = "Trait API surface and test-only scaffolding")]` (`volume/mod.rs:17`), whose
   own comment says it's "part of the public API for future backends but aren't all called from production code paths
   today". See Decision 1.
2. **`VolumeError::FriendlyGit`.** `volume/types.rs:282` carries
   `FriendlyGit(crate::file_system::git::friendly::FriendlyGitError)`, so `VolumeError` reaches into a 6,614-line git
   subsystem that isn't moving. See Decision 2.

What lands where, derived from the import census:

- **`cmdr-fs`**: `Volume` (post-deletion), `VolumeError`, `ListingProgress`, `FileEntry`, `TagRef`, `InMemoryVolume`
  (`volume/backends/in_memory.rs:32`, clean deps), `filesystem_kind`, `ignore_poison`, `pluralize`, `thread_qos`,
  `process_memory`, `wait_until`/`wait_until_async` (behind a `testing` feature), and the Decision 2 error types.
- **`cmdr-index`**: the three subsystems, plus `SYSTEM_DIR_EXCLUDES` (moved down out of `search/`) and
  `sqlite_util::run_incremental_vacuum`.
- **stays app-side**: every *real-storage* `Volume` backend (`LocalPosixVolume`, SMB, MTP, archive) with their `smb2` /
  `mtp-rs` / `file_system::git` / mount-detection dependencies.

`VolumeId` is `pub(crate) type VolumeId = String` (`indexing/lifecycle/state.rs:50`), a bare alias. It moves to
`cmdr-index`; making it a newtype is a separate, optional cleanup.

## Design decisions

### 1. Delete the dead `VolumeScanner` / `VolumeWatcher` apparatus

Not an inversion, a deletion: `VolumeScanner`, `VolumeWatcher`, `Volume::scanner()`, `Volume::watcher()`,
`LocalPosixScanner`, `LocalPosixWatcher`, and the three `crate::indexing::*` imports at `volume/mod.rs:20-22` and
`local_posix.rs:11-13`. Roughly 90 lines.

**Why deletion beats inversion:** an earlier draft proposed moving the two traits into `cmdr-index` and adding
`scanner_for` / `watcher_for` to `VolumeProvider`. That would put two never-called methods into the crate's public
plugin interface on day one, which is exactly what Decision 3 exists to prevent. The abstraction was written for
"future backends" that arrived and chose a different shape: SMB and MTP index through
`network_scanner::scan_volume_via_trait`, a BFS over `Volume::list_directory`, which is unaffected by this deletion and
is the live volume abstraction the indexer actually uses.

**Consequence for the rest of the plan:** the "touches every backend" risk disappears, and so does Decision 14's
trait-signature fan-out. This is the cheapest blocker removal in the plan; it just needed someone to check whether the
coupling was live.

### 2. `VolumeError::FriendlyGit` moves down with its payload

`FriendlyGitError` (`file_system/git/friendly.rs:95`) is a leaf error type, but it references
`volume::friendly_error::ErrorCategory`, and `volume/friendly_error/` is seven files (`volume_error.rs`, `provider.rs`,
`errno.rs`, …) the earlier draft never mentioned.

**Decision:** `git/friendly.rs`'s error type and `volume/friendly_error/` move to `cmdr-fs` alongside `VolumeError`.
Verify at M2 that they're genuinely leaf; **fallback if `friendly.rs` turns out to drag git internals**: box the
variant behind a small `cmdr-fs`-owned trait so the payload stays app-side. Decide by reading, not by guessing, and
record which one you took.

**Why it needs a decision at all:** it's the second half of the cycle, and it's the kind of thing that surfaces as a
compile error three days into M2 if nobody looked.

### 3. `pub` is a design act, not a compile fix

The starting surface: 65 named re-exports, a `pub use events::*` glob (16 structs + 3 enums), and three `pub mod`s.
Mechanically all of it has to become `pub`. **That is the thing to resist.** For each item, one of:

- **Facade method** on `Index`, named for what the caller wants, not for the internal behind it.
- **Fold into another call**, where an export exists only because callers do three calls in sequence.
- **Delete** (test-only, or a caller that's since changed).
- **`#[doc(hidden)] pub` behind a feature.** Two gated buckets: `testing` (test reach-through) and `tooling` (the
  `importance/evals/` corpus machinery that `index-query`'s three importance binaries need; see M7.5).

**Target: no more than ~25 public items on `Index`,** each with a doc comment naming the user-visible behavior it
serves. Expanding the glob is step one of the audit; the real surface isn't readable from the file today.

14 current re-exports are `#[cfg(any(target_os = "macos", target_os = "linux"))]`-gated (`indexing/mod.rs:63-80`), so
the API needs a deliberate platform-conditional story and `#![deny(missing_docs)]` must hold on every platform.

### 4. Handle-first API, globals hidden behind it

The crate carries ~50 process-wide statics plus five thread-locals: `INDEX_REGISTRY`, `APP_HANDLE`, `READ_POOL`,
`PENDING_SIZES`, `ENRICH_RESULT_MEMO` (on the enrichment hot path), the three lifecycle buses, `RECOMPUTE_BUS`,
`MASTER_ENABLED`, `WRITER_GENERATION`, `VERIFIER_STATE`, `STOP_HOOKS`, two `SCAN_CHANGE_BUFFER`s, `media_index/gate.rs`
(seven atomics), `media_index/network/config.rs` (`CONFIG` + `PAUSED`), `media_index/coverage.rs`'s four maps, the CLIP
`WORKER` / `MODEL_DIR`, three ANN caches, and more. They *are* the current architecture.

Removing them is a lifecycle rewrite and is not in scope. But a crate whose API is "call `init()` first or panic" fails
the contract. **So separate the two problems.** The public API is handle-based from day one:

```rust
let index = Index::builder()
    .data_dir(dir)
    .volumes(Arc::new(AppVolumeProvider::new(...)))   // Decision 6
    .events(Arc::new(TauriEventSink::new(app)))       // Decision 6
    .host(Arc::new(AppHostPolicy))                    // Decision 6
    .config(IndexConfig { ..Default::default() })     // Decision 9
    .runtime(handle)                                  // Decision 8
    .build()?;
```

Internally `Index` is initially a thin token whose methods resolve to the same statics, now `pub(crate)`.

**Why:** callers get written against the good API immediately, so de-globalizing later is a pure internal refactor with
no call-site churn. And this plan carries no lifecycle-rewrite risk, which is where the bugs would be.

**Cost, stated honestly:** `Index::build()` twice is not independent; it returns `IndexBuildError::AlreadyBuilt` rather
than pretending. Honest for a single-instance system, and the variant disappears when the statics move into the handle.

### 5. De-Tauri **in place**, then move

The risky work happens while the code still lives in `src-tauri/src/`, with the full existing test suite watching. Only
once the subsystems reference nothing app-side does anything move directories.

**Why:** moving and de-coupling together makes every failure ambiguous and puts a rename boundary exactly where the
semantic changes are. Sequenced this way, M6 is mechanical enough to review in one pass, and each de-coupling step is
independently revertable. It's also what makes several problems below catchable at a milestone boundary instead of
after the move.

### 6. Three injected traits, five moved primitives, and a disposition for every back-edge

An earlier draft claimed the back-edges "reduce to three traits". Measured, the `crate::<mod>` census across the three
trees is: `file_system` 69, `priority` 18, `config` 18, `mtp` 9, `volumes` 2, `volumes_linux` 1, `network` 1,
`location` 1, plus `thread_qos` 14, `process_memory` 8, `test_support` 9, `log_error!` 5, `sqlite_util` 2, `fda_gate`
2, `search` 1, `restricted_paths` 1, `commands` 1. Every one needs a disposition, because an undisposed back-edge is
what blocks M6.

**Injected (the crate asks; the app answers):**

- **`EventSink`** — `fn emit(&self, event: IndexEvent)`. Precedent to copy:
  `file_system/write_operations/event_sinks.rs` (`OperationEventSink` + `TauriEventSink` + `CollectorEventSink`), a
  closer analogue in scale than `downloads/watcher.rs:109`. Also absorbs `log_error!` and
  `restricted_paths::record_denial`.
- **`VolumeProvider`** — the 18 `get_volume_manager()` sites, volume identity (absorbing `mtp::identity` /
  `mtp::connection`, including the two core sites at `paths/routing.rs:147` and `lifecycle/state.rs:885`), and
  **mount classification**: `volumes::is_network_fs_type` (`transports/local_external/index.rs:80`),
  `volumes::get_smb_mount_info` (`transports/smb/index.rs:76`), `volumes_linux` (`:78`), and
  `file_system::linux_mounts::is_network_fs_type`. Decision 11 says mount detection reaches the crate through this
  trait; that only works if the trait actually answers "is this a network fs", so it gets an explicit method.
  Note: **no `scanner_for` / `watcher_for`** (Decision 1).
- **`HostPolicy`** — "may I / should I do work right now?" Covers `priority::foreground::idle_for` and
  `priority::transfers::transfer_active` (four production sites across `network_scanner/scan_pace.rs`,
  `media_index/scheduler/mod.rs:566`, `media_index/scheduler/lifecycle.rs:437`) **and** `fda_gate::is_fda_pending` (a
  runtime query, not the static config value). One trait because they're one question.

**Moved down into `cmdr-fs`** (host primitives that can't sensibly be injected):

- **`thread_qos`** (14 refs). `set_current_thread_qos(QosClass::Utility)` on `std::thread`-spawned workers
  (`scanner/mod.rs:258`, `scanner/walker/mod.rs:384,591`, `writer/mod.rs:559`, `reconcile/local_reconcile.rs:130,235`,
  `reconcile/reconciler/rescan.rs:399`). **This is the property that kept indexing in-process at all**; injecting a
  tokio `Handle` does nothing for it.
- **`process_memory`** (8 refs, mostly `resources/memory_watchdog.rs`). Carries `libmimalloc-sys` (macOS-only) and Mach
  FFI; the target-conditional dep moves with it.
- **`pluralize`** (49 refs) and **`ignore_poison`** (39 refs). Leaves.
- **`wait_until` / `wait_until_async`** (7 of the 9 `test_support` refs), behind a `testing` feature. See Decision 16
  for why the rest of `test_support` can't come along.

**Moved down into `cmdr-index`** (misfiled today): `search::SYSTEM_DIR_EXCLUDES` (exclusion policy is the indexer's;
`search/` imports it back) and `sqlite_util::run_incremental_vacuum` (index-DB maintenance).

**Config (Decision 9):** `config::resolved_app_data_dir` (18 refs, 5 after the command relocation), `settings` (3),
`media_index/gate.rs`, `media_index/network/config.rs`.

**Routed through `EventSink`:** `log_error!` (5) is an app `macro_rules!` (`error_reporter/mod.rs:86`) feeding
`error_reporter::auto_dispatcher::on_error_logged`, the live Discord error pipeline. A crate can't invoke a crate-root
macro across the boundary, and silently dropping it is a feedback-loop regression, so it becomes a typed
`IndexEvent::Error { … }` the app re-raises. Same for `restricted_paths::record_denial` (1).

**Still needing a call at M2** (the residue of `crate::file_system` that is *not* in the `cmdr-fs` set):
`file_system::listing::caching::{DirectoryChange, ListingSummary}` (4 refs — the app's listing cache, used by
`transports/smb/watch.rs:54`, `events/partial_agg.rs:10`, `events/progress_reporter.rs:28`),
`file_system::file_provider::domain_id_for_dir` (2, macOS NSURL), and `file_system::volume::backends`
(`get_space_info_for_path`, 2). Each is small; each needs to move down, get injected, or get inlined. **Do not start M2
without deciding these three**, because they're the ones that would otherwise be discovered mid-move.

**Resolves for free:** `crate::ai` (CLIP model download) and `crate::file_viewer` (3) live entirely inside the three
files M4 relocates to `commands/`. **Covered by Decision 11:** `commands::network::upgrade_to_smb_volume_inner`.

**Dispatch rule:** `EventSink` and `VolumeProvider` are `Arc<dyn …>`, called at human-perceptible cadence. `HostPolicy`
is consulted inside scan loops, so it returns a cheap `Copy` value and callers cache it per batch. **No trait may be
introduced on a per-entry path.** Wanting one is a signal to restructure the call, not to add the trait.

### 7. Isolation is a check, and `specta` is not part of it

Add `index-crate-isolation` to the Go check runner (error-level): `crates/cmdr-index` and `crates/cmdr-fs` must not
depend on `tauri`, `tauri-specta`, or `cmdr`, verified against `cargo metadata` so it catches transitive creep.

**`specta` is a plain, unconditional dependency of both crates.** 58 data types derive `specta::Type` (`DirStats`,
`IndexStatus`, `IndexFailure`, `OcrHit`, `TagHit`, `SemanticHit`, the whole `importance/scorer/types.rs` set), plus
`FileEntry` and `TagRef`; `importance/CLAUDE.md` states outright that `FolderSignals`'s serde shape is load-bearing.
An earlier draft made it an optional feature, which is worse than either alternative: the app is the only consumer and
always enables it, nothing in the check runner builds `--no-default-features`, so the specta-off configuration would be
compiled zero times and rot immediately. Unconditional costs nothing once `tauri-specta` is out (Decision 12).

Two mechanics that must hold: the version stays pinned identically to the app's `=2.0.0-rc.24`
(`src-tauri/Cargo.toml:206`) or two `specta` crates coexist and the `Type` impls stop satisfying `tauri-specta`; and
bindings collection is unaffected, because `ipc.rs:711-712` collects types transitively through command signatures and
cross-crate `specta::Type` impls collect normally.

**Why a check for `tauri` at all:** it's the load-bearing property of this plan, and exactly the kind that erodes one
convenient import at a time. It's also trivially machine-checkable, unlike "no user-facing strings", which stays a
review-time judgment.

### 8. One runtime, injected

`tauri::async_runtime::spawn` **is** `tokio::spawn` on Tauri's runtime; 61 calls plus 5 `JoinHandle` references.
Replace with a `tokio::runtime::Handle` supplied at build time. Feasible as described: `RuntimeHandle::inner() ->
&tokio::runtime::Handle` exists (tauri 2.11.5, `async_runtime.rs:181`).

**Why injected rather than crate-owned:** a crate-owned runtime means two thread pools competing for the same cores,
and thread QoS silently stops applying to half the work. A case where the "cleaner" design would be a regression.

### 9. Config in, no settings reads

`IndexConfig` is a plain struct passed at build time and updatable through `index.reconfigure(cfg)`. The crate never
reads settings, env vars, or the FDA choice for itself. Absorbs `settings::FullDiskAccessChoice`
(`lifecycle/state.rs:47`), the `media_index/gate.rs` atomics, `media_index/network/config.rs`'s `CONFIG` and `PAUSED`,
`MASTER_ENABLED`, and the data dir. The atomics stay as internal storage, now written only by `reconfigure`.

**Why:** policy belongs to the product. It kills a class of test setup pain and makes "controlled from Cmdr's UI" one
concept instead of scattered setters. The `CMDR_*` debug knobs are the deliberate exception: developer diagnostics,
they stay `std::env::var` reads inside the crate, documented as such.

### 10. Ingest is designed now, implemented later

Signatures written and compiled against, proving the API admits the two future features without reshaping:

```rust
/// Fold an already-performed directory listing back into the index.
/// Non-blocking: takes ownership and returns immediately. Never touches a DB lock
/// on the caller's thread. Drops the oldest queued batch under pressure rather than
/// applying backpressure, because the caller is the listing hot path.
pub fn observe_listing(&self, observation: ListingObservation) -> Result<(), IngestError>;

/// Recursive size of a subtree, on any volume, indexed or not.
/// Progressive (a climbing total) and cancelable.
pub fn size_of(&self, req: SizeRequest, cancel: CancellationToken) -> Result<SizeStream, SizeError>;
```

Bodies return `Err(IngestError::NotImplemented)` / `Err(SizeError::NotImplemented)`, **not `todo!()`**: `lib.rs:11` is
`#![deny(clippy::todo, clippy::unimplemented)]` with the comment "No leftover `todo!()` / `unimplemented!()` stubs
reaching a build", and M5 happens in place, so a `todo!()` fails clippy at the milestone boundary. A typed error keeps
all the "does the shape fit?" value and doesn't defeat a deliberate guardrail.

Three things fall out that are hard to retrofit and cheap to reserve:

- **`observe_listing` carries `observed_at` and covers direct children only.** A listing can freshen what's visible; it
  cannot fix a recursive size or detect deletions deeper down. The store must express "these rows were confirmed at T"
  without implying the subtree was.
- **`ListingObservation` includes a match/mismatch tally** against what the index currently claims. Every listing is a
  free correctness audit at exactly the paths the user looks at. This is the evidence that would one day justify
  `later/db-first-listings-plan.md`, and it's unbuildable retroactively.
- **A volume can be "watched for size invalidation but not indexed"** (a third lifecycle state), and persisted sizes
  carry an as-of stamp, because FSEvents has no coverage on SMB or MTP and can drop history.

### 11. `transports/` splits; "stays app-side" was not a disposition

`indexing/transports/` is bidirectional. The app calls **in** through 14 exported entry points
(`indexing/mod.rs:63-80`), while `transports/smb/index.rs:148` calls **out** to
`commands::network::upgrade_to_smb_volume_inner`, and `paths/routing.rs:51` reaches into
`transports::smb::index::smb_volume_id_for_path`.

**The cut line:** the buffered-change replay state machines (the two `SCAN_CHANGE_BUFFER` statics and their
replay/discard logic) are index internals and go in the crate, surfaced as `Index::apply_change(volume, change)` and
`Index::on_watch_gap(volume, reason)`. Mount detection, `smb_upgrade`, and connection management stay app-side and
reach the crate through `VolumeProvider` (which is why Decision 6 gives it a mount-classification method).

### 12. Pure-Rust events; the app owns the wire format

The crate's events are a plain Rust enum with no `serde`, no `tauri_specta::Event`, no `#[tauri_specta(event_name)]`.
There are **15** `Event`-deriving structs to relocate app-side: 12 in `indexing/events/mod.rs`, 2 in
`media_index/events.rs`, and one in `indexing/writer/mod.rs:52` (`AggregationProgressEvent`) — inside a module this
plan otherwise calls a pure internal, which is exactly why it's easy to miss. `importance` emits none.

**Relocating those 15 structs is an explicit M3 step**, not a side effect of "the sink owns the mapping". If they don't
move at M3, M6 drags them into the crate and this decision is silently abandoned.

**Why, given the mapping costs a few hundred lines:** it makes "no user-facing strings in the crate" enforceable rather
than aspirational, copy gets exactly one home, and the FE wire format can change without touching the crate. This is
consistent with Decision 7 keeping `specta` on data types: **schema derives on data are fine; presentation decisions
are not.** Events are where presentation lives.

**Watch for:** `IndexRescanNotificationEvent.details: String` is free-text English, verified log-only (the FE handler
at `index-state.svelte.ts:370` reads `payload.reason` and maps it to an i18n key; `details` is never rendered). It
stays a diagnostic, typed as `Diagnostic(String)` so the boundary is explicit. Separately, `pluralize` output reaches
`PhaseRecord.trigger`, which **is** rendered at `DebugDriveIndexPanel.svelte:234` — the developer debug panel, not
product copy, so it's acceptable, but "pluralize is purely log-only" shouldn't be claimed.

### 13. Errors

Keep the house style (hand-rolled enums, manual `From`; no `thiserror`/`anyhow` as a direct dependency anywhere in the
workspace). Existing per-area errors stay: `ScanError`, `WalkReadError`, `WatcherError`, `IndexStoreError`,
`VolumeScanError`, `MediaStoreError`, `ImportanceStoreError`, `ClipError`, `InstallError`, `FetchError`, `VisionError`,
`AnnError`. Add `IndexError` (crate-level facade), `IndexBuildError`, `IngestError`, `SizeError`. Every variant carries
structured data (volume id, path, errno, counts), never a pre-formatted sentence.

### 14. Cancellation: one primitive

`tokio_util::sync::CancellationToken` everywhere. Already a dependency (`Cargo.toml:90`) and already used at 53 sites
in the agent subsystem. Today indexing cancellation is `Arc<AtomicBool>` plus bespoke stop paths.

Child operations get `token.child_token()` so stopping a volume stops its scan, aggregation, and media enrichment
together. Cancellation is **observable**: a cancelled operation returns a distinct error variant rather than a silent
early return. (Decision 1's deletion removes the `VolumeScanner::scan_subtree` trait signature from this change, so the
blast radius is internal to the crate.)

### 15. Lint and format config move up to the workspace, before any code does

`rustfmt.toml` and `clippy.toml` exist **only** at `apps/desktop/src-tauri/`. rustfmt resolves config by walking up
from each source file, so `crates/cmdr-index/src/**` would find nothing and fall back to `max_width = 100` instead of
the project's `120`. `cargo fmt --all` would then rewrite essentially every one of the 89.5k moved lines, destroying
both the "mechanical, reviewable in one pass" property and the `git mv` rename detection M6.1 depends on. Losing
`clippy.toml` is worse: `allow-unwrap-in-tests = true` is what keeps `#![warn(clippy::unwrap_used)]` from detonating
across the 1,194 moved tests, and `cognitive-complexity-threshold = 15` would revert to clippy's default.

**Decision:** promote both to the workspace root at M2, verify `cargo fmt --all --check` is a no-op, and only then let
code move. Likewise the crate-root lint block: `lib.rs:2-19` sets `#![deny(unused)]`,
`#![warn(unused_crate_dependencies)]`, `#![warn(unused_qualifications)]`, `#![deny(clippy::print_stdout,
clippy::print_stderr, clippy::dbg_macro)]`, `#![deny(clippy::todo, clippy::unimplemented)]`,
`#![warn(clippy::allow_attributes_without_reason)]`, `#![warn(clippy::undocumented_unsafe_blocks)]`, and
`#![warn(clippy::unwrap_used)]`. A new crate without these is a lint-free zone over a quarter of the codebase. Use
`[workspace.lints]` + `lints.workspace = true` so it can't drift, rather than copying the block.

(Noted, not fixed here: `rustfmt.toml` says `edition = "2021"` while the packages are `edition = "2024"`. Pre-existing;
changing it would churn formatting, so it's a separate decision.)

### 16. `test_support` splits; it cannot move wholesale

`src/test_support.rs:133-134` declares `COUNTING_ALLOCATOR` as a `#[global_allocator]`, and the module is
`#[cfg(test)] pub(crate) mod test_support` (`lib.rs:157-158`) — it exists only in test builds, where it *replaces*
mimalloc (`main.rs:4`). Moving it into `cmdr-fs` as an ordinary module gives every binary linking `cmdr-fs` a second
global allocator: a hard compile error. Feature-gating doesn't save it either, since dev-dependency features unify with
normal ones for the same package under a workspace test build.

**Decision:** `wait_until` / `wait_until_async` (7 of the 9 refs, and the module `desktop-rust-test-sleep` exists to
enforce) move to `cmdr-fs` behind a `testing` feature. `count_allocations` / `heap_bytes_held` and the allocator itself
become a `#[cfg(test)]` module *inside* `cmdr-index`, since `enrich_memory_tests.rs` is the only consumer and a global
allocator has to be per-test-binary.

**Consequence for M0:** today's Rust memory baselines are measured under the counting allocator, not mimalloc. Say so
in the baseline note, or the M7 comparison is apples-to-oranges.

## Milestones

Sequential unless stated. Each ends green on the named checks and is its own commit (usually several).

### M0 — Baseline, benchmark harness, LTO

**Intent:** make "did we regress?" answerable. Without this, every later perf claim is opinion and M7 has no premise.

1. Add `[profile.release] lto = "thin"` at the workspace root. There is currently **no** `[profile.*]` section in any
   of the six manifests and no `.cargo/config.toml`, so we're on Cargo defaults (`lto = false`, `codegen-units = 16`).
   Post-split, only `#[inline]` and generic functions inline across the boundary; thin LTO restores it. Landing it
   first makes any later delta attributable to the extraction.
2. **Build the benchmark harness. It does not exist today** (the only bench is `benches/icon_benchmarks.rs`). Criterion
   benches for `enrich_entries_with_index` (`read/enrichment.rs:229`, the sub-ms hot path the directory-size feature
   rests on), `get_dir_stats_batch` (`read/queries.rs:360`), and scan throughput on the disk-image fixture. **Put them
   in `crates/cmdr-index/benches/` from the start** (they can reach a `testing`-feature-gated fixture there). A
   `src-tauri/benches` home would pre-commit two items of the ~25 public budget for benchmarking reasons, and the scan
   fixture (`indexing/tests/external_drive_fixture.rs`) is `#[cfg(test)]`, so it becomes invisible outside the crate
   entirely. Until M6 they live in `src-tauri/benches` and move with the code.
3. Record baselines in `docs/notes/index-extraction-baseline.md`: the three above, plus clean build time, incremental
   rebuild after a one-line change in `indexing/`, and the same in `commands/`. Those last two are goal 2's scoreboard.
   **Note the allocator** (Decision 16) so M7 compares like with like.
4. Record **release build time** alongside binary size and startup. Thin LTO makes release builds meaningfully slower,
   including the notarized pipeline; the tradeoff belongs on the record.

**Tests:** the harness is the deliverable; it must run green and produce stable numbers across two runs before its
baseline means anything. **Docs:** the baseline note, linked from `docs/notes/README.md`.
**Checks:** `pnpm check rust`, plus a release build.

### M1 — Delete the dead indexing hooks on `Volume`

**Intent:** remove half the cycle for free (Decision 1).

1. Delete `VolumeScanner`, `VolumeWatcher` (`volume/mod.rs:101,123`), `Volume::scanner()` / `Volume::watcher()`
   (`:575,583`), `LocalPosixScanner` / `LocalPosixWatcher` (`local_posix.rs:781,803`) and their `Volume` impl methods
   (`:769,773`).
2. Remove the now-unused `crate::indexing::*` imports at `volume/mod.rs:20-22` and `local_posix.rs:11-13`.
3. Narrow the `#![allow(dead_code)]` at `volume/mod.rs:17` if the remaining scaffolding no longer needs a
   module-wide relaxation, and update its reason comment either way.

**Tests:** no new test. There is nothing to TDD: the deleted code has zero callers, so a test asserting its behavior
would pass forever and protect nothing. The guard is the existing suite plus `--include-slow`, which exercises the
real scan paths (`scanner::scan_volume`, `scan_subtree`, `network_scanner::scan_volume_via_trait`).
**Docs:** `file_system/volume/CLAUDE.md` (drop the scanner/watcher mention).
**Checks:** `pnpm check --include-slow`.

### M2 — `cmdr-fs`, and the tooling that must exist before any crate does

**Intent:** create the foundation crate **and** fix the check runner in the same milestone. A second crate the checks
don't cover is worse than no second crate. `cmdr-fs` is the canary: small enough that a gap is obvious, real enough to
prove the fix.

The tooling problem, measured. `desktop-rust-tests.go:13,29` runs `cargo nextest run --locked --features virtual-mtp`
with `cmd.Dir` at the package and **no `--workspace`**. The three subsystems hold **1,194 of the app crate's 4,918
tests (24%)**. Moved to a crate without this fix, they stop running and every later milestone goes green vacuously.
Same shape in `desktop-rust-tests-linux.go`, `desktop-rust-integration-tests.go:44`, `desktop-rust-rustfmt.go`,
`desktop-rust-clippy.go:38` (`--all-targets` applies to the *selected* package, so the moved test targets go unlinted),
`cargo-machete` (`:22`), and `cargo-udeps` (`:57`). Nine Go scanners hardcode `apps/desktop/src-tauri/src`:
`lock-poison.go:48`, `desktop-rust-error-string-match.go:62`, `desktop-rust-log-error-macro.go:33`,
`desktop-rust-ipc-enum-camelcase.go:16`, `desktop-rust-test-sleep.go:48`, `desktop-rust-cfg-gate.go:15`,
`desktop-rust-jscpd.go:21`, `pluralize-noun.go:106`, `claude-md-length.go`.

1. **Promote `rustfmt.toml` and `clippy.toml` to the workspace root** and add `[workspace.lints]` (Decision 15). Verify
   `cargo fmt --all --check` is a no-op **before** anything moves.
2. Decide the three undisposed `file_system` residue items from Decision 6 (`listing::caching`,
   `file_provider::domain_id_for_dir`, `volume::backends::get_space_info_for_path`).
3. Create `crates/cmdr-fs`: `version = "0.0.0"`, `publish = false`, `lints.workspace = true`,
   `#![deny(missing_docs)]`, a `testing` feature, unconditional `specta` pinned to `=2.0.0-rc.24`.
4. Move the `cmdr-fs` set, including the Decision 2 error types and the Decision 16 `wait_until` split. Carry the
   macOS-only `libmimalloc-sys` dep with `process_memory`. The app re-exports every moved item from its original path,
   so no other app file changes.
5. Switch the cargo-driven checks to `--workspace`: `nextest`, `fmt --all`, `clippy`, integration, linux, plus
   `cargo-machete` and `cargo-udeps`. Pin the feature spec as `cmdr/virtual-mtp` (a bare `--features virtual-mtp`
   changes meaning under multi-package selection, and `desktop-rust-tests-linux.go` passes none at all). Decide where
   `deny.toml` lives now that the graph has more roots.
6. **Green up `crates/index-query` under the new lanes.** Nothing compiles it today (all lanes are `cmd.Dir`-scoped to
   `src-tauri`), so `--workspace` + `fmt --all` exposes it to `-D warnings` and default-width rustfmt in one step, at a
   boundary that must ship green.
7. Generalize the nine scanners from a hardcoded path to a list of Rust source roots.
8. **Fix `desktop-rust-cfg-gate` first-class.** It derives the macOS-only crate list from
   `[target.'cfg(target_os = "macos")'.dependencies]` in the app manifest and scans only `src-tauri/src`
   (`cfg-gate.go:15-16`). It must take (manifest, srcRoot) pairs, or it goes blind over 104 `#[cfg(target_os =
   "macos")]` and 15 `#[cfg(target_os = "linux")]` sites once M6 moves the CLIP stack — surfacing as a broken Linux
   build, not a red check. Commit `aabc4cb11` ("declare `libmimalloc-sys` as macOS-only so Linux builds again") is the
   evidence this breaks in practice.
9. **Fix `bindings-fresh`.** `desktop-bindings-fresh.go`'s `hashBindingsInputs(rustDir)` walks only
   `src-tauri/src/**.rs` plus `src-tauri/Cargo.toml`, so after M6 a `specta::Type` edit in `crates/cmdr-index` leaves
   the marker hash unchanged and the check reports "in sync (cached)" over stale bindings. The check is `NotInCI`, so
   nothing catches it downstream. Add the new source roots. While in there, two pre-existing bugs: it hashes
   `filepath.Join(rustDir, "Cargo.lock")`, which doesn't exist (the lock is at the workspace root), and silently
   `continue`s on `IsNotExist`, so lockfile changes have never invalidated the marker.
10. Add a meta-check: every workspace member is covered by the test **and** lint lanes (nextest, clippy, fmt, machete,
    udeps). This is what stops the next crate from re-opening the same hole.
11. `pnpm check` scoping: give `crates/` its own `index` scope. Note the two genuinely new checks (this meta-check and
    M7's `index-crate-isolation`) each need a workflow reference **or** a `NotInCI` reason; a reason *plus* a reference
    also fails `ci-coverage`. A new `App` scope value on its own doesn't trip it. `rustInputs` already globs
    `crates/**` (`inputs.go:20`), so no fingerprint wiring is needed.

**Tests, TDD:** the meta-check and the cfg-gate change each get a Go unit test with a fixture workspace that omits a
member / hides a macOS-only import, asserting failure. Red first; they're checks, and an untested check is worthless.
**Verify empirically:** `cargo nextest run --workspace` reports a count that includes `cmdr-fs`'s moved tests.
**Docs:** `crates/cmdr-fs/CLAUDE.md` + `DETAILS.md`, wired into `docs/architecture.md`;
`scripts/check/checks/DETAILS.md` for the generalized scanners.
**Checks:** `pnpm check` (full), `pnpm check go`.

### M3 — `EventSink`: cut the `AppHandle` cord (in place)

**Intent:** the biggest coupling, done where the existing tests can see it.

1. Define `IndexEvent` covering the 15 emitted events plus `Diagnostic(String)` and the `Error { … }` variant
   replacing `log_error!` (Decision 6).
2. Define `trait EventSink`, modeled on `file_system/write_operations/event_sinks.rs`.
3. **Relocate the 15 `tauri_specta::Event` structs** to an app-side `events/index_mapping.rs`: 12 from
   `indexing/events/mod.rs`, 2 from `media_index/events.rs`, and `AggregationProgressEvent` from
   `indexing/writer/mod.rs:52`. This is the step that makes Decision 12 real.
4. Two sink impls: `TauriEventSink` (app-side, owns the mapping) and `RecordingSink` (test-side).
5. Thread it through the **31** `AppHandle`-carrying files (130 lines). `IndexManager.app: AppHandle` becomes
   `events: Arc<dyn EventSink>`; the `APP_HANDLE` `OnceLock` at `indexing/lifecycle/state.rs:133` is deleted. There is
   a *second*, separate `APP_HANDLE` at `commands/indexing.rs:340` and 11 across `src/` in total; only the
   `lifecycle/state.rs` one is in scope.
6. `events/progress_reporter.rs` keeps its 500 ms tick, emitting `IndexEvent::ScanProgress`.

**Tests, TDD (red→green):**
- Exhaustiveness: every `IndexEvent` variant maps to a non-empty Tauri event name through the app-side mapper.
- `RecordingSink` ordering: a fixture scan emits `ScanStarted` → progress → `ScanComplete` for the right volume id.
- Per-volume isolation: two concurrent fixture volumes produce two independent streams. This is the crate's one
  cross-area invariant (`indexing/CLAUDE.md`) and deserves an explicit test at the new boundary.
- The `Error` variant reaches `error_reporter::auto_dispatcher::on_error_logged` through the app sink, so the Discord
  pipeline is provably intact.

**Tests, after:** the existing suite; `bindings-fresh` must stay green through the struct relocation.
**Docs:** `indexing/events/CLAUDE.md` + `DETAILS.md`. **Checks:** `pnpm check --include-slow`.

### M4 — Runtime, host seams, config, cancellation (in place)

**Intent:** dispose of every remaining back-edge so the subsystems reference nothing app-side.

1. Replace the 61 `tauri::async_runtime::{spawn, spawn_blocking, block_on}` calls and 5 `JoinHandle` references with an
   injected `tokio::runtime::Handle` (Decision 8). **Verify thread QoS still applies to the spawned work**; that's the
   property that kept indexing in-process at all, and `thread_qos` moving to `cmdr-fs` is necessary but not sufficient.
2. `VolumeProvider`: the 18 `get_volume_manager()` sites, volume identity, and mount classification (Decision 6).
3. `HostPolicy`: four `priority` sites across three files in two subsystems, plus `fda_gate::is_fda_pending`.
4. `IndexConfig` (Decision 9), including `media_index/network/config.rs`'s `CONFIG` and `PAUSED`.
5. Cancellation on `CancellationToken` (Decision 14).
6. Relocate the 27 `#[tauri::command]` functions to `commands/` (`media_index/commands.rs` 17,
   `media_index/commands/policy.rs` 9, `importance/commands.rs` 1). Also resolves the `ai` and `file_viewer`
   back-edges, which live entirely in these files.
7. Apply the Decision 11 transport cut.
8. **Gate:** no reference to *any* app module from the three subsystems, not merely no `tauri::`. Verify with a scripted
   `crate::` census; a `grep tauri::` gate would pass with ~20 back-edges still open.

**Tests, TDD:** `IndexConfig` round-trip (a value set via `reconfigure` is the value the scan reads, proving nothing
still reads a global); a cancelled scan returns the distinct cancellation variant rather than `Ok`; `HostPolicy` is
consulted per batch not per entry (counting fake on a fixture scan, pinning Decision 6's dispatch rule).
**Tests, after:** full suite with `--include-slow`; this milestone touches concurrency.
**Docs:** `indexing/CLAUDE.md` + `DETAILS.md`, `media_index/CLAUDE.md`. **Checks:** `pnpm check --include-slow`.

### M5 — The `Index` handle and the public-API audit

**Intent:** the design milestone. Where the crate stops being a directory and becomes an API.

1. Expand the `pub use events::*` glob so the real surface is visible, then audit all 65 named re-exports, the three
   `pub mod`s, and `media_index`/`importance`'s surfaces against Decision 3's buckets (facade / fold / delete /
   `testing` / `tooling`). Record the mapping as a committed artifact; the reasoning is the deliverable.
2. Build `Index` + `Index::builder()` (Decision 4), resolving to the existing statics.
3. Move every app call site onto the handle: `ipc.rs` (29), `ipc_collectors.rs` (26), `mcp/resources` (23),
   `mcp/executor` (16), `file_system/*` (46 refs across 18 files), `agent/tools` (11), `search/*` (15),
   `mtp/connection` (8).
4. Write the `observe_listing` / `size_of` signatures and types (Decision 10) with `Err(…::NotImplemented)` bodies.
   **Compile them. Do not implement them.** If either doesn't fit the handle's shape, the shape is wrong and this is
   the cheapest moment to learn it.
5. Settle the platform-conditional API story for the 14 cfg-gated exports; `#![deny(missing_docs)]` must hold on both
   platforms.

**Tests, TDD:** `Index::build()` twice returns `AlreadyBuilt`; **a full scan driven by a handle built with a
`RecordingSink` and an `InMemoryVolume`, with no app types present.** That second one is the real acceptance test for
the whole extraction, written before the handle exists, and it's what smokes out a hidden global that only works by
accident.
**Tests, after:** the whole suite. **Docs:** the public API doc page; the audit mapping in the crate's `DETAILS.md`.
**Checks:** `pnpm check --include-slow`.

### M6 — The move

**Intent:** by now, mechanical.

1. `git mv` the three module trees into `crates/cmdr-index/src/`, so rename detection survives. (M2.1 is what keeps
   this true: without the promoted `rustfmt.toml`, `cargo fmt --all` rewrites every moved line.)
2. `Cargo.toml`: `version = "0.0.0"`, `publish = false`, `lints.workspace = true`, path dep on `cmdr-fs`,
   unconditional `specta` at `=2.0.0-rc.24`. Carry over `rusqlite`, `tokio`, `tokio-util`, `notify`,
   `notify-debouncer-full`, `walkdir`, `rayon`, `image`, `serde`, and the macOS `objc2-vision` / `objc2-core-ml` CLIP
   stack **under `[target.'cfg(target_os = "macos")'.dependencies]`**, with the M2 cfg-gate now watching the new
   manifest.
3. **Move the deps, don't copy them.** After 89.5k lines leave, `rusqlite`, `image`, `rayon`, `notify`,
   `notify-debouncer-full`, `walkdir`, and the `objc2-vision` / `objc2-core-ml` stack become unused in the app
   manifest. `#![warn(unused_crate_dependencies)]` fires and `cargo-machete` / `cargo-udeps` are error-level. Expect
   churn in `lib.rs`'s `use foo as _;` block (`lib.rs:26-60`) too.
4. Fix imports; app side switches to `use cmdr_index::…`.
5. Docs: 34 `CLAUDE.md` / `DETAILS.md` files move with their code, and **neither existing crate has any `.md` at all**,
   so `crates/cmdr-index/{CLAUDE,DETAILS}.md` and `crates/cmdr-fs/{CLAUDE,DETAILS}.md` must be *created*
   (`claude-md-details-sibling` + `docs-reachable`). Path references across the repo update; `docs-dead-links` catches
   stragglers.
6. **Path-keyed allowlists.** `file-length-allowlist.json` has 16 entries under the three subsystems;
   `claude-md-length.go:35` hardcodes a per-file override (`indexing/CLAUDE.md`: 1000) that silently stops applying
   after the rename; `docs-reachable-allowlist.json` is path-keyed too. Shrink-wrap **drops** gone paths but never adds
   new ones, so the 16 files return as fresh warnings. Per `.claude/rules/file-length-allowlist.md` these can't be
   re-added without David's explicit consent — **get it before starting M6**, since the numbers are unchanged and only
   the paths move.
7. Check the `.taurignore` dev-watcher shield. It lives at `apps/desktop/src-tauri/.taurignore` with gitignore-style
   patterns rooted there, so it will **not** reach `crates/`, and Tauri's dev watcher also watches local path
   dependencies. Without a fix, every colocated-doc edit restarts `pnpm dev` — the exact regression
   `apps/desktop/CLAUDE.md` calls a must-not-delete shield. Fix in the watch config or `scripts/tauri-wrapper.ts`.

**Tests:** the full suite including `--include-slow` and `desktop-e2e-playwright`. Confirm the workspace test count
matches pre-move (M2 made this meaningful). **If a test needs an assertion changed here, something in M1–M5 was
wrong**; fix it there.
**Checks:** `pnpm check --include-slow`, `pnpm check desktop-e2e-playwright`.

### M7 — Isolation check, measurement, and cleanup

**Intent:** convert the claims in "Why" into measurements, and make the boundary self-defending.

1. Land `index-crate-isolation` (Decision 7), with a Go unit test using a fixture manifest that depends on `tauri`, and
   a workflow reference or `NotInCI` reason (M2.11).
2. Regenerate the `file-length` allowlist by running the check and committing its rewrite (never hand-edit).
3. Re-run every M0 baseline, comparing like allocator with like (Decision 16). The enrichment hot path must be within
   noise; if it isn't, suspect a missing `#[inline]` on a small cross-boundary function or a trait that slipped onto a
   per-entry path.
4. Record incremental rebuild times for the two M0.3 scenarios. That's goal 2's actual answer.
5. **`index-query`: reduce, don't eliminate, the `cmdr` dep.** An earlier draft claimed it was "one call". It's 8
   import sites across 5 files: `main.rs:22` (`register_platform_case_collation`), `bin/importance-measure.rs:14,15`,
   `bin/importance-snapshot.rs:31,32`, `bin/importance-tune.rs:16` (all deep `importance` internals including the
   `evals/` corpus machinery), and `bin/operation-log-dump.rs:18` (`cmdr_lib::operation_log::store`). That last one is
   fatal to the elimination claim: `operation_log` is an app module that isn't moving. So: split `operation-log-dump`
   off (its own crate, or an app `examples/` bin), point the three importance binaries at the `tooling`-gated surface
   from M5.1, and let the rest depend on `cmdr-index` alone.
6. Update `docs/architecture.md`, `AGENTS.md` (file structure gains `crates/`), and
   `docs/specs/later/out-of-process-indexing.md` (note which seams this plan built, and correct its `APP_HANDLE`
   location: it names `commands/indexing.rs`, which is real, but the in-scope one is `indexing/lifecycle/state.rs:133`).
7. Wipe this plan from `docs/specs/` per the folder's convention, once its durable intent lives in the crates'
   `CLAUDE.md` / `DETAILS.md`.

**Checks:** full `pnpm check --include-slow` plus E2E.

## Parallelization

Mostly sequential by design; the milestones are a dependency chain and we're not in a hurry. The genuinely safe
overlaps:

- M0's benchmark harness can be built while M1 is in flight; it touches only `docs/notes/` and bench code.
- M2's tooling work (items 5–11) is independent of M2's crate creation (items 3–4) and can be written first. Items 1–2
  gate everything.
- Within M5, the call-site migration splits cleanly by consumer (`mcp/`, `ipc*`, `file_system/`, `agent/`, `search/`)
  once the handle exists: different files, no shared edits. But the audit lands first, and one person owns it. It's a
  design act, not parallel labor.

Everything else sequential. M3 and M4 touch the same 31 files, and M6 depends on both.

## Risks

- **The API audit gets skipped under pressure** and 65 `pub(crate)` items become 65 `pub` items. The main way this
  plan fails while appearing to succeed: it delivers goal 2 and quietly abandons 1 and 3. Mitigation: the ~25-item
  target gates M5, and the audit mapping is a committed artifact.
- **Checks go blind at the crate boundary**, so later milestones pass vacuously. Hence M2 pairing the first crate with
  the tooling fix plus a meta-check. Three of these have silent failure modes worth naming individually: `cfg-gate`
  (surfaces as a broken Linux build), `bindings-fresh` (reports "in sync" over stale bindings, and it's `NotInCI`), and
  the format/lint config (surfaces as an 89.5k-line reformat that destroys M6's reviewability).
- **Thread QoS silently stops applying** (M4.1). This is the property that kept indexing in-process; losing it re-opens
  the starvation risk `later/out-of-process-indexing.md` documents. Verify explicitly, don't assume.
- **Perf regression on the enrichment hot path**, the sub-ms path the directory-size feature rests on. Mitigation: M0
  baseline (harness built, not assumed), M7 re-measure, thin LTO landed before any code moves.
- **A mid-migration beta bug is hard to bisect** across the M6 rename. Mitigation: worktree, one logical change per
  commit, full checks green at every commit.

**Escape hatch:** M1 through M5 are each independently valuable and independently revertable, and the code stays in
`src-tauri/src/` the whole time. Stopping after any of them leaves the codebase strictly better than it started, with
no half-moved crate to clean up. M1 is a pure dead-code deletion; M2 leaves the check runner permanently better
regardless of what follows. The point of no return is M6, and by then everything risky is proven.

## Related

- `docs/specs/later/out-of-process-indexing.md` — the deferred daemon escalation. This plan is its prerequisite and
  makes it substantially cheaper; it is not a commitment to it.
- `docs/specs/later/db-first-listings-plan.md` — serving listings from the index. Decision 10's mismatch tally is the
  evidence that would eventually justify it.
- `indexing/CLAUDE.md`, `media_index/CLAUDE.md`, `importance/CLAUDE.md` — the subsystems' current must-knows.
- `docs/architecture.md` — the subsystem map, updated at M7.
