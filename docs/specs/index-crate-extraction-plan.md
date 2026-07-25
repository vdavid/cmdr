# Extract the index into a `cmdr-index` crate

Status: planned, 2026-07-25. Not started. Revised after two review rounds; the counts below are measured, not estimated.

Move `indexing/`, `media_index/`, and `importance/` out of the app crate into a standalone, Tauri-free workspace crate
with a designed public API: typed errors, no user-facing strings, real cancellation, structured progress, and a
first-class ingest side alongside the query side.

## Why (the intent behind everything below)

Three motivations, in priority order. When a decision below is ambiguous, resolve it toward the higher one.

1. **Encapsulate the hardest code in the codebase behind a boundary you can reason about.** These three subsystems are
   89,540 of 318,142 `src-tauri/src` lines (28%), and they hold the gnarliest concurrency and lifecycle logic we have.
   Today their "API" is 65 named re-exports plus a glob in `indexing/mod.rs` (26 `pub` / 39 `pub(crate)`, so 60% of it
   is nominally private), on top of three fully public modules (`aggregator`, `store`, `writer`), backed by ~50
   process-wide mutable statics. There's no line between "what the app may rely on" and "internals", so every app
   change can reach into indexing internals and every indexing change has an unbounded blast radius. A crate makes that
   line real and compiler-enforced.
2. **Build-time separation.** David works on app backend without touching indexing, and on indexing without touching
   the app. Today either edit rebuilds all 318k lines. Splitting gives each side its own incremental unit.
3. **Make the index a thing that could later be a product.** "Cmdr, plus a smart file+image index any agent can tap
   into" needs a documented, stable, self-contained API. This plan builds that API. It does NOT build a daemon: see
   Non-goals.

Two features David wants next, which the API must admit without redesign:

- **Listings auto-enrich the index.** A directory listing already paid for the syscalls; feed the result back so the
  index self-corrects exactly where the user is looking.
- **Space-to-size on any folder, including unindexed drives**, persisted and kept fresh.

Both are app → index **writes**. So the crate's API is bidirectional from day one, not a read-only query surface with
ingest bolted on later. See Decision 10.

## Non-goals

- **No separate process, no daemon, no login item.** The deferred escalation is already captured in
  `docs/specs/later/out-of-process-indexing.md`; this plan is its prerequisite, not its start. Nothing here should be
  shaped by "we might split the process later" beyond what's already good design.
- **No behavior change.** Zero user-visible difference at the end of this plan.
- **`search/` stays in the app.** It consumes the crate like any other module. It's a product surface with ranking
  choices and UI copy, and pulling it in would double the API design work. (One constant moves the other way: see
  Decision 6's back-edge table.)
- **No `pub(crate)` laundering.** Making everything `pub` to compile is failure, not progress. See Decision 3.
- **No de-globalization of internals.** The ~50 statics move; they get hidden, not removed. See Decision 4.

## What "a nice crate" means here (the API contract)

This is the acceptance bar. Every milestone is judged against it.

1. **No `tauri` in the dependency tree.** Enforced by a check, not a convention (Decision 7).
2. **No user-facing strings.** The crate emits typed values; the app renders every word a human reads. A crate that
   returns `"Indexing 3 drives…"` is broken by definition. Diagnostic strings for `log::` are fine and stay English.
3. **Typed errors everywhere.** No `Box<dyn Error>`, no stringly-typed failure. Per-area error enums (the existing
   style) plus one crate-level facade error. Every variant carries the data a caller needs to decide what to do, so the
   app never string-matches (`.claude/rules/no-string-matching.md`).
4. **Everything long-running is cancelable**, through one primitive, with cancellation observable from outside.
5. **Everything long-running reports progress** as structured values through a sink the caller supplies.
6. **A handle, not a global.** The public API is methods on an `Index` value the app constructs and owns.
7. **`#![deny(missing_docs)]`.** If it's `pub`, it's documented. This is how the "separate, clear docs" goal gets teeth.
8. **Ingest and query are equal citizens.** Not a read API with a write hole punched in it.

## Target crate graph

```
crates/cmdr-fs/          NEW. Leaf vocabulary + host primitives. No index deps.
crates/cmdr-index/       NEW. indexing + media_index + importance. Owns VolumeScanner/VolumeWatcher.
crates/index-query/      exists. Drops its `cmdr` dep.
apps/desktop/src-tauri/  the app. Depends on both. Owns all Volume backends.
```

### The cycle, and how it actually breaks

`file_system/` sits on **both** sides of the boundary: the three subsystems reference it (47 refs, 26 `use` lines from
`indexing` alone), while `file_system/*` references them back (46 refs across 18 files, including
`file_system/watcher.rs` and `volume/backends/archive/watch/mod.rs`).

The naive fix, "move `Volume` down into a foundation crate", **does not work**. `file_system/volume/mod.rs:20-22`
imports `ScanConfig`, `ScanError`, `ScanHandle`, `ScanSummary`, `DriveWatcher`, `FsChangeEvent`, `WatcherError`, and
`IndexWriter` from `indexing`, because the `Volume` trait has two methods returning index-flavored types:
`fn scanner(&self) -> Option<Box<dyn VolumeScanner>>` (`:575`) and `fn watcher(&self) -> Option<Box<dyn VolumeWatcher>>`
(`:583`). A `cmdr-fs` holding `Volume` would need the scanner, watcher, and writer core of `cmdr-index`.

**The cut is an inversion, not a move** (Decision 1). `VolumeScanner` and `VolumeWatcher` are the *index's* plugin
interface, so they belong to `cmdr-index`. `Volume` loses both methods and becomes pure filesystem vocabulary.

What lands where, verified against the imports:

- **`cmdr-fs`**: `Volume` (post-inversion), `VolumeError`, `ListingProgress`, `FileEntry`, `TagRef`, `InMemoryVolume`,
  `filesystem_kind`, `ignore_poison`, `pluralize`, `thread_qos`, `process_memory`, `test_support`. The last four are
  host primitives the crate genuinely needs and cannot inject (Decision 6).
- **`cmdr-index`**: the three subsystems, plus the `VolumeScanner` / `VolumeWatcher` traits it now owns, plus
  `SYSTEM_DIR_EXCLUDES` (moved down out of `search/`) and `sqlite_util::run_incremental_vacuum`.
- **stays app-side**: every concrete `Volume` backend (`LocalPosixVolume`, SMB, MTP, archive) with their `smb2` /
  `mtp-rs` / `file_system::git` / mount-detection dependencies. `LocalPosixVolume` alone pulls `file_system::git` at
  10+ sites (a 6,614-line subsystem); it is not movable and does not need to be.

`VolumeId` is `pub(crate) type VolumeId = String` (`indexing/lifecycle/state.rs:50`), a bare alias. It moves to
`cmdr-index` with the rest; making it a newtype is a separate, optional cleanup with no bearing on this plan.

## Design decisions

### 1. Invert `VolumeScanner` / `VolumeWatcher` before anything else

`Volume::scanner()` and `Volume::watcher()` are deleted. `cmdr-index` defines both traits; the app implements them for
its backends and supplies them through `VolumeProvider` (Decision 6):

```rust
fn scanner_for(&self, id: &VolumeId) -> Option<Box<dyn VolumeScanner>>;
fn watcher_for(&self, id: &VolumeId) -> Option<Box<dyn VolumeWatcher>>;
```

**Why this is right independent of the extraction:** a filesystem abstraction should not know that an indexer exists.
Today `InMemoryVolume`, `MtpVolume`, `SmbVolume`, and every archive volume carry two trait methods that only
`LocalPosixVolume` ever implements, and the whole `Volume` trait drags eight index types along for it. The inversion
puts the dependency where the need is.

**Scope, stated honestly:** this touches every `Volume` backend and is the first real risk in the plan. It is M1
precisely so it happens while everything is still one crate and the full test suite is watching.

### 2. One crate, not four

`cmdr-index` holds all three subsystems as internal modules. They're genuinely coupled: `media_index` consumes
`indexing`'s lifecycle bus, `importance` reads the index store and feeds `media_index`'s scope gate.

**Why:** the boundary that delivers goals 1–3 is the one between the app and the index; the internal ones are a bonus.
Land the outer boundary, enforce the internal layering with module privacy, and split later when the seams have proven
themselves under real change. Reversing a premature split is far more expensive than doing it late.

**Guardrail:** the internal layering is documented in the crate's `DETAILS.md` with a dependency direction (`store` ←
`writer`/`aggregator` ← `lifecycle` ← `{scanner, watch, reconcile}` ← `importance` ← `media_index`), and no module may
import upward. Reviewed by eye per milestone; a check is overkill for now.

### 3. `pub` is a design act, not a compile fix

The starting surface is bigger than it looks: 65 named re-exports, a `pub use events::*` glob (16 structs + 3 enums),
and three `pub mod`s. Mechanically all of it has to become `pub`. **That is the thing to resist.** For each item, one
of:

- **Facade method.** A real capability the app needs, named for what the caller wants, not for the internal behind it.
- **Fold into another call.** Several exports exist only because a caller does three calls in sequence.
- **Delete.** Some exist for tests only, or for a caller that's since changed.
- **`#[doc(hidden)] pub` behind a `testing` feature**, for genuine test reach-through, gated so it can't leak.

**Concrete target: no more than ~25 public items on `Index`,** each with a doc comment naming the user-visible behavior
it serves. Expanding the glob is step one of the audit, since the real surface isn't readable from the file today.

Note a wrinkle: 14 of the current re-exports are `#[cfg(any(target_os = "macos", target_os = "linux"))]`-gated
(`indexing/mod.rs:63-80`). The public API needs a deliberate platform-conditional story, and `#![deny(missing_docs)]`
has to hold on every platform, not just the one you're building on.

**Why it matters:** if the API ends up as 65 free functions over hidden globals, we've paid the whole cost of the move
and bought only build times.

### 4. Handle-first API, globals hidden behind it

The crate carries ~50 process-wide statics plus five thread-locals: `INDEX_REGISTRY`, `APP_HANDLE`, `READ_POOL`,
`PENDING_SIZES`, `ENRICH_RESULT_MEMO` (on the enrichment hot path), the three lifecycle buses, `RECOMPUTE_BUS`,
`MASTER_ENABLED`, `WRITER_GENERATION`, `VERIFIER_STATE`, `STOP_HOOKS`, two `SCAN_CHANGE_BUFFER`s, `media_index/gate.rs`
(seven atomics), `media_index/network/config.rs` (`CONFIG` + `PAUSED`), `media_index/coverage.rs`'s four maps, the CLIP
`WORKER` / `MODEL_DIR`, three ANN caches, and more. They *are* the current architecture.

Removing them is a rewrite of the lifecycle layer and is not in scope. But shipping a crate whose API is "call `init()`
first or panic" fails the contract.

**The decision: separate the two problems.** The public API is handle-based from day one:

```rust
let index = Index::builder()
    .data_dir(dir)                                    // Decision 9
    .volumes(Arc::new(AppVolumeProvider::new(...)))   // Decision 6
    .events(Arc::new(TauriEventSink::new(app)))       // Decision 6
    .host(Arc::new(AppHostPolicy))                    // Decision 6
    .config(IndexConfig { ..Default::default() })     // Decision 9
    .runtime(handle)                                  // Decision 8
    .build()?;
```

Internally, `Index` is initially a thin token: its methods resolve to the same statics that exist today, now
`pub(crate)`.

**Why:** callers get written against the good API immediately, so de-globalizing later is a pure internal refactor with
no call-site churn. It also means this plan carries no lifecycle-rewrite risk, which is where the bugs would be.

**Cost, stated honestly:** `Index::build()` twice in one process is not independent. It returns
`IndexBuildError::AlreadyBuilt` rather than pretending. That's honest for a currently-single-instance system, and the
variant disappears without breaking anyone once the statics move into the handle. Test isolation is unchanged (the
existing `*_TEST_MUTEX` statics keep serializing what they already serialize).

### 5. De-Tauri **in place**, then move

The risky work (the trait inversion, replacing `AppHandle`, replacing `tauri::async_runtime`) happens while the code
still lives in `src-tauri/src/`, with the full existing test suite running against it unchanged. Only once the
subsystems compile with zero app references does anything move directories.

**Why:** if we move and de-couple in one step, every failure is ambiguous ("did the move break it, or the seam?"), and
`git log --follow` gets a rename boundary right where the semantic changes are. Sequenced this way, the move (M6) is
mechanical enough to review in one pass, and each de-coupling step is independently revertable.

### 6. Three injected traits, three moved primitives, and an explicit disposition for every back-edge

An earlier draft claimed the back-edges "reduce to three traits". That was wrong: there are ~20 distinct app modules
referenced from the three subsystems. Every one gets a disposition, because an undisposed back-edge is what blocks M6.

**Injected (the crate asks; the app answers):**

- **`EventSink`** — `fn emit(&self, event: IndexEvent)`. In-house precedent to copy:
  `file_system/write_operations/event_sinks.rs` (`OperationEventSink` + `TauriEventSink` + `CollectorEventSink`), which
  is a closer analogue in scale than `downloads/watcher.rs:109`. Also absorbs `log_error!` and
  `restricted_paths::record_denial` (below).
- **`VolumeProvider`** — replaces 18 `get_volume_manager()` call sites, and supplies `scanner_for` / `watcher_for`
  (Decision 1) and volume identity (absorbing the `mtp::identity` back-edge).
- **`HostPolicy`** — "may I / should I do work right now?" Covers `priority::foreground::idle_for` and
  `priority::transfers::transfer_active` (four production sites across `network_scanner/scan_pace.rs`,
  `media_index/scheduler/mod.rs:566`, `media_index/scheduler/lifecycle.rs:437`) **and** `fda_gate::is_fda_pending` (a
  runtime query, not the static `FullDiskAccessChoice` config value). One trait because they're one question.

**Moved down into `cmdr-fs`** (host primitives that can't be injected without absurdity):

- **`thread_qos`** (14 refs). `set_current_thread_qos(QosClass::Utility)` on `std::thread`-spawned workers in
  `scanner/mod.rs:258`, `scanner/walker/mod.rs:384,591`, `writer/mod.rs:559`, `reconcile/local_reconcile.rs:130,235`,
  `reconcile/reconciler/rescan.rs:399`. **This is the property that kept indexing in-process at all** (see
  `later/out-of-process-indexing.md`); injecting a tokio `Handle` does nothing for it. Pure leaf module, moves clean.
- **`process_memory`** (8 refs, mostly `resources/memory_watchdog.rs`). Carries `libmimalloc-sys` (macOS-only) and Mach
  FFI; the target-conditional dep moves with it.
- **`pluralize`** (49 refs) and **`ignore_poison`** (39 refs). Both leaves.
- **`test_support`** (9 refs, including `enrich_memory_tests.rs`'s `count_allocations` / `heap_bytes_held`, the harness
  pinning the index-walk memory invariant). It's `pub(crate) mod` in `lib.rs:158` today, and a dev-dep back to `cmdr`
  would be a cycle, so it must move rather than be borrowed.

**Moved down into `cmdr-index`** (they were misfiled):

- `search::SYSTEM_DIR_EXCLUDES` (1 ref) — exclusion policy is the indexer's; `search/` imports it back.
- `sqlite_util::run_incremental_vacuum` (2 refs) — index-DB maintenance.

**Config (Decision 9):** `config::resolved_app_data_dir` (18 refs, 5 after the command relocation), `settings` (3),
`media_index/gate.rs`, `media_index/network/config.rs`.

**Routed through `EventSink`:** `log_error!` (5 refs) is an app `macro_rules!` (`error_reporter/mod.rs:86`) that feeds
`error_reporter::auto_dispatcher::on_error_logged`, the live Discord error pipeline. The crate can't invoke a
crate-root macro across the boundary, and silently dropping it is a feedback-loop regression, so it becomes a typed
`IndexEvent::Error { … }` the app re-raises. Same for `restricted_paths::record_denial` (1).

**Resolves for free:** `crate::ai` (CLIP model download) and `crate::file_viewer` (3) live entirely inside the three
files M4 relocates to `commands/`.

**Covered by Decision 11:** `commands::network::upgrade_to_smb_volume_inner` (`transports/smb/index.rs:148`).

**Dispatch rule:** `EventSink` and `VolumeProvider` are `Arc<dyn …>`; they're called at human-perceptible cadence.
`HostPolicy` is consulted inside scan loops, so it returns a cheap `Copy` value and callers cache it per batch. **No
trait may be introduced on a per-entry path.** Wanting one is a signal to restructure the call, not to add the trait.

### 7. Isolation is a check, not a convention

Add `index-crate-isolation` to the Go check runner (error-level): `crates/cmdr-index` and `crates/cmdr-fs` must not
depend on `tauri`, `tauri-specta`, or `cmdr`, verified against `cargo metadata` so it catches transitive creep.

**`specta` is deliberately NOT on the denylist.** It is an optional feature of both crates that only the app enables.
58 data types across the three subsystems derive `specta::Type` (`DirStats`, `IndexStatus`, `IndexFailure`, `OcrHit`,
`TagHit`, `SemanticHit`, and the whole `importance/scorer/types.rs` set), plus `FileEntry` and `TagRef` in `cmdr-fs`;
`importance/CLAUDE.md` states outright that `FolderSignals`'s serde shape is load-bearing. Banning `specta` would mean
hand-writing 58 mirror types plus `From` impls before `bindings.ts` regenerates identically. **A schema derive is not
presentation**; it doesn't threaten the "no user-facing strings" property the ban exists to protect. Events are a
different case, and they stay pure (Decision 12).

**Why a check at all:** "no Tauri in the crate" is the load-bearing property of this plan, and exactly the kind that
erodes one convenient import at a time. It's also trivially machine-checkable, unlike "no user-facing strings", which
stays a review-time judgment. Familiar house shape (`error-string-match`, `claude-md-length`, `docs-reachable`).

### 8. One runtime, injected

`tauri::async_runtime::spawn` **is** `tokio::spawn` on Tauri's runtime, and `spawn_blocking` likewise; 61 calls plus 5
`JoinHandle` type references across the three subsystems. Replace with a `tokio::runtime::Handle` supplied at build
time, populated with Tauri's own handle.

**Why injected rather than crate-owned:** a crate-owned runtime means two thread pools competing for the same cores,
and the thread-QoS work silently stops applying to half the work. Same runtime, same QoS, same scheduling. A case where
the "cleaner" design would be a regression.

### 9. Config in, no settings reads

`IndexConfig` is a plain struct passed at build time and updatable through `index.reconfigure(cfg)`. The crate never
reads settings, env vars, or the FDA choice for itself.

Absorbs: `settings::FullDiskAccessChoice` (`lifecycle/state.rs:47`), the `media_index/gate.rs` atomics (enabled,
cancelled, semantic-search, scope, importance threshold, parallelism), `media_index/network/config.rs`'s `CONFIG` and
`PAUSED`, `MASTER_ENABLED`, and the data dir. The atomics stay as internal storage, now written only by `reconfigure`.

**Why:** these are policy, and policy belongs to the product. It kills a class of test setup pain, and it makes
"controlled from Cmdr's UI" one concept instead of a scattering of setters. The `CMDR_*` debug knobs
(`CMDR_RECONCILE_LATENCY_SPIKE` and friends) are the deliberate exception: developer diagnostics, they stay
`std::env::var` reads inside the crate, documented as such.

### 10. Ingest is designed now, implemented later

Signatures written and compiled against, so we prove the API admits the two future features without reshaping:

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
(`indexing/mod.rs:63-80`: `start_indexing_for_smb`, `resume_smb_index_if_enabled`, `on_smb_overflow`,
`on_smb_watcher_died`, `apply_smb_change`, `replay_buffered_changes`, `discard_buffered_changes`,
`start_indexing_for_mtp`, `on_mtp_watch_continuity_lost`, `apply_mtp_added_or_changed`, `apply_mtp_removed`,
`buffer_mtp_handle_if_scanning`, `MtpUpsert`, `LocalExternalEnable`), while `transports/smb/index.rs:148` calls **out**
to `commands::network::upgrade_to_smb_volume_inner`, and `paths/routing.rs:51` reaches into
`transports::smb::index::smb_volume_id_for_path`.

**The cut line:** the buffered-change replay state machines (the two `SCAN_CHANGE_BUFFER` statics and their
replay/discard logic) are index internals and go in the crate, surfaced as `Index::apply_change(volume, change)` and
`Index::on_watch_gap(volume, reason)`. Mount detection, `smb_upgrade`, and connection management stay app-side and
reach the crate through `VolumeProvider`.

**Why it needs its own decision:** it's the largest single API-shape question in the plan, and left implicit it would
land mid-M5 as a rushed judgment call.

### 12. Pure-Rust events; the app owns the wire format

The crate's events are a plain Rust enum with no `serde`, no `specta`, no `#[tauri_specta(event_name = …)]`. The app
maps each variant to the existing Tauri event with its existing name and camelCase payload. In scope: 12 `Event`-deriving
structs in `indexing/events/mod.rs` and 2 in `media_index/events.rs` (`importance` emits none), plus the new error
variants from Decision 6.

**Why, given the mapping costs a few hundred lines:** it makes "no user-facing strings in the crate" enforceable rather
than aspirational, copy gets exactly one home, and the FE wire format can change without touching the crate. Moving the
structs as-is would freeze the FE's current shape into the crate's public API.

This is consistent with Decision 7 letting data types keep `specta` behind a feature: **schema derives on data are
fine; presentation decisions are not.** Events are where presentation lives.

**Watch for:** `IndexRescanNotificationEvent.details: String` is free-text English, verified log-only (the FE handler at
`index-state.svelte.ts:370` reads `payload.reason` and maps it to an i18n key; `details` is never rendered). It stays a
diagnostic, typed as `Diagnostic(String)` so the boundary is explicit. Separately, `pluralize` output reaches
`PhaseRecord.trigger`, which **is** rendered, at `DebugDriveIndexPanel.svelte:234`. That's the developer debug panel,
not product copy, so it's acceptable, but "pluralize is purely log-only" is not strictly true and shouldn't be claimed.

### 13. Errors

Keep the house style (hand-rolled enums, manual `From`; no `thiserror`/`anyhow` as a direct dependency anywhere in the
workspace today). Existing per-area errors stay: `ScanError`, `WalkReadError`, `WatcherError`, `IndexStoreError`,
`VolumeScanError`, `MediaStoreError`, `ImportanceStoreError`, `ClipError`, `InstallError`, `FetchError`, `VisionError`,
`AnnError`. Add `IndexError` (crate-level facade), `IndexBuildError`, `IngestError`, `SizeError`.

Every variant carries structured data (volume id, path, errno, counts), never a pre-formatted sentence.

### 14. Cancellation: one primitive

`tokio_util::sync::CancellationToken` everywhere. Already a dependency (`Cargo.toml:90`) and already used at 53 sites
in the agent subsystem, so it's proven in-tree. Today indexing cancellation is `Arc<AtomicBool>` plus bespoke stop
paths.

Child operations get `token.child_token()` so stopping a volume stops its scan, aggregation, and media enrichment
together. Cancellation is **observable**: a cancelled operation returns a distinct error variant rather than a silent
early return.

**Scope note:** `VolumeScanner::scan_subtree(&self, root, writer, cancelled: &AtomicBool)` is part of the trait
signature, so this changes every implementing backend. Post-Decision-1 that trait is crate-owned, which makes the
change the crate's own business, but the app-side backends (local, SMB, MTP, archive, in-memory) all update. Confirm
archive-volume cancellation semantics don't shift.

## Milestones

Sequential unless stated. Each ends green on the named checks and is its own commit (usually several).

### M0 — Baseline, benchmark harness, LTO

**Intent:** make "did we regress?" answerable. Without this, every later perf claim is opinion, and M7 has no premise.

1. Add `[profile.release] lto = "thin"` at the workspace root. There is currently **no** `[profile.*]` section in any
   of the six workspace manifests and no `.cargo/config.toml`, so we're on Cargo defaults (`lto = false`,
   `codegen-units = 16`). Post-split, only `#[inline]` and generic functions can inline across the crate boundary; thin
   LTO restores it. Landing it first means any later delta is attributable to the extraction.
2. **Build the benchmark harness. It does not exist today** (the only bench in the tree is
   `benches/icon_benchmarks.rs`). Needs criterion benches with index fixtures for: `enrich_entries_with_index`
   (`read/enrichment.rs:229`, the sub-ms hot path the directory-size feature rests on), `get_dir_stats_batch`, and scan
   throughput on the disk-image fixture (`indexing/tests/external_drive_fixture.rs`). Decide now where these live after
   M6; a `src-tauri/benches` bench against `cmdr_index::` works and keeps one home.
3. Record baselines in `docs/notes/index-extraction-baseline.md`: the three above, plus clean build time, incremental
   rebuild after a one-line change in `indexing/`, and the same in `commands/`. Those last two are goal 2's scoreboard.
4. Record **release build time** alongside binary size and startup. Thin LTO makes release builds meaningfully slower,
   including the notarized pipeline; the tradeoff belongs on the record.

**Tests:** the benchmark harness is the deliverable; it needs to run green and produce stable numbers across two runs
before its baseline means anything. **Docs:** the baseline note, linked from `docs/notes/README.md`.
**Checks:** `pnpm check rust`, plus a release build.

### M1 — Invert `VolumeScanner` / `VolumeWatcher`

**Intent:** break the real cycle. Nothing else in the plan compiles until this lands (Decision 1).

1. Move the `VolumeScanner` and `VolumeWatcher` trait definitions into `indexing/` (still in `src/`).
2. Delete `Volume::scanner()` and `Volume::watcher()` (`file_system/volume/mod.rs:575,583`). Remove the three
   `crate::indexing::*` imports at `:20-22`.
3. Add `scanner_for` / `watcher_for` to the (not yet extracted) volume-provider seam; for now a plain function in
   `indexing/` that the lifecycle layer calls, so M1 stays a refactor rather than a trait-design exercise.
4. Update every implementor: `LocalPosixVolume` (`backends/local_posix.rs`) is the only one returning `Some` today, but
   the trait methods exist on all backends.

**Tests, TDD:** a test asserting the local scanner is still selected for a local volume and still `None` for SMB/MTP,
written before the inversion, red against a deliberately broken lookup. This is the behavior the two deleted methods
encoded, and it's the thing an inversion can silently get wrong.
**Tests, after:** the full suite, `--include-slow` (this touches the scan path).
**Docs:** `file_system/volume/CLAUDE.md` and `indexing/scanner/CLAUDE.md` for the new ownership.
**Checks:** `pnpm check --include-slow`.

### M2 — `cmdr-fs`, and the tooling that must exist before any crate does

**Intent:** create the foundation crate **and** fix the check runner in the same milestone, because a second crate that
the checks don't cover is worse than no second crate. `cmdr-fs` is deliberately the canary: small enough that a gap is
obvious, real enough to prove the fix.

The tooling problem, measured: `desktop-rust-tests.go:13,29` runs `cargo nextest run --locked --features virtual-mtp`
with `cmd.Dir` set to the package dir and **no `--workspace`**. The three subsystems hold **1,194 of the app crate's
4,918 tests (24%)**. Moved to a crate without this fix, they stop running and every later milestone goes green
vacuously. Same shape in `desktop-rust-tests-linux.go`, `desktop-rust-integration-tests.go:44`, and
`desktop-rust-rustfmt.go` (`cargo fmt` is package-scoped). Nine custom Go scanners hardcode
`apps/desktop/src-tauri/src`: `lock-poison.go:48`, `desktop-rust-error-string-match.go:62`,
`desktop-rust-log-error-macro.go:33`, `desktop-rust-ipc-enum-camelcase.go:16`, `desktop-rust-test-sleep.go:48`,
`desktop-rust-cfg-gate.go:15`, `desktop-rust-jscpd.go:21`, `pluralize-noun.go:106`, plus `claude-md-length.go`.

1. Create `crates/cmdr-fs`: `version = "0.0.0"`, `publish = false`, `#![deny(missing_docs)]`, optional `specta` feature
   (Decision 7).
2. Move the `cmdr-fs` set from the crate graph above. The app re-exports every moved item from its original path so no
   other app file changes.
3. Carry the `lock-poison` clippy config and the macOS-only `libmimalloc-sys` dep (with `process_memory`) into the new
   manifest.
4. Switch the cargo-driven checks to `--workspace` (`nextest`, `fmt --all`, integration, linux).
5. Generalize the nine scanners from a hardcoded path to a list of Rust source roots.
6. **Fix `desktop-rust-cfg-gate` first-class.** It derives the macOS-only crate list from
   `[target.'cfg(target_os = "macos")'.dependencies]` in the app manifest and scans only `src-tauri/src`
   (`cfg-gate.go:15-16`). It must take (manifest, srcRoot) pairs. Without this it goes blind over 104
   `#[cfg(target_os = "macos")]` and 15 `#[cfg(target_os = "linux")]` sites once M6 moves the CLIP stack, and the
   failure surfaces as a broken Linux build, not a check failure. Commit `aabc4cb11` ("declare `libmimalloc-sys` as
   macOS-only so Linux builds again") is the evidence this breaks in practice.
7. Add a meta-check: every workspace member is covered by the test and lint lanes. This is what stops the next crate
   from re-opening the same hole.
8. `pnpm check` scoping: give `crates/` its own `index` scope, and add the matching `ci.yml` step (or a `NotInCI`
   reason), or `ci-coverage` fails. `rustInputs` already globs `crates/**` (`inputs.go:20`), as do `desktopAppInputs()`
   and ci.yml's `rust` / `desktop` filters, so no input wiring is needed.

**Tests, TDD:** the meta-check and the cfg-gate change each get a Go unit test with a fixture workspace that omits a
member / hides a macOS-only import, asserting failure. Red first: they're checks, so their own red→green is cheap and
they're worthless untested. **Verify the fix empirically**: `cargo nextest run --workspace` must report a test count
that includes `cmdr-fs`'s moved tests.
**Docs:** `crates/cmdr-fs/CLAUDE.md` + `DETAILS.md`, wired into `docs/architecture.md`; `scripts/check/checks/DETAILS.md`
for the generalized scanners.
**Checks:** `pnpm check` (full), `pnpm check go`.

### M3 — `EventSink`: cut the `AppHandle` cord (in place)

**Intent:** the biggest coupling, done where the existing tests can see it.

1. Define `IndexEvent` covering the 14 emitted events plus `Diagnostic(String)` and the `Error { … }` variant that
   replaces `log_error!` (Decision 6).
2. Define `trait EventSink`, modeled on `file_system/write_operations/event_sinks.rs`.
3. Two impls: `TauriEventSink` (app-side, owns the Decision 12 mapping) and `RecordingSink` (test-side).
4. Thread it through the **31** `AppHandle`-carrying files (130 lines). `IndexManager.app: AppHandle` becomes
   `events: Arc<dyn EventSink>`; the `APP_HANDLE` `OnceLock` at `indexing/lifecycle/state.rs:133` is deleted. (Note
   there is a *second*, separate `APP_HANDLE` at `commands/indexing.rs:340`, and 11 across `src/` in total. Only the
   `lifecycle/state.rs` one is in scope; don't delete the others.)
5. `events/progress_reporter.rs` keeps its 500 ms tick, emitting `IndexEvent::ScanProgress`.

**Tests, TDD (red→green):**
- Exhaustiveness: every `IndexEvent` variant maps to a non-empty Tauri event name through the app-side mapper.
- `RecordingSink` ordering: a fixture scan emits `ScanStarted` → progress → `ScanComplete` for the right volume id.
- Per-volume isolation: two concurrent fixture volumes produce two independent streams. This is the crate's one
  cross-area invariant (`indexing/CLAUDE.md`) and deserves an explicit test at the new boundary.
- The `Error` variant reaches `error_reporter::auto_dispatcher::on_error_logged` through the app sink, so the Discord
  pipeline is provably intact.

**Tests, after:** the existing suite. **Docs:** `indexing/events/CLAUDE.md` + `DETAILS.md`.
**Checks:** `pnpm check --include-slow`.

### M4 — Runtime, host seams, config, cancellation (in place)

**Intent:** dispose of every remaining back-edge so the subsystems reference nothing app-side.

1. Replace the 61 `tauri::async_runtime::{spawn, spawn_blocking, block_on}` calls and 5 `JoinHandle` references with an
   injected `tokio::runtime::Handle` (Decision 8). **Verify thread QoS still applies to the spawned work**; that's the
   property that kept indexing in-process at all, and `thread_qos` moving to `cmdr-fs` (M2) is necessary but not
   sufficient.
2. `VolumeProvider` (18 `get_volume_manager()` sites + `scanner_for`/`watcher_for` from M1 + volume identity, absorbing
   `mtp::identity` / `mtp::connection`, including the two core sites at `paths/routing.rs:147` and
   `lifecycle/state.rs:885`).
3. `HostPolicy` (four `priority` sites across three files in two subsystems, plus `fda_gate::is_fda_pending`).
4. `IndexConfig` (Decision 9), including `media_index/network/config.rs`'s `CONFIG` and `PAUSED`.
5. Cancellation on `CancellationToken` (Decision 14), including the `VolumeScanner` signature change and its five
   backend implementors.
6. Relocate the 27 `#[tauri::command]` functions to `commands/` (`media_index/commands.rs` 17,
   `media_index/commands/policy.rs` 9, `importance/commands.rs` 1). This also resolves the `ai` and `file_viewer`
   back-edges for free, since they live entirely in these files.
7. Apply the Decision 11 transport cut.
8. **Gate:** no reference to *any* app module from the three subsystems, not merely no `tauri::`. Verify with a
   scripted check over `crate::` paths, since the earlier `grep tauri::` gate would have passed with ~20 back-edges
   still open.

**Tests, TDD:** `IndexConfig` round-trip (a value set via `reconfigure` is the value the scan reads, proving nothing
still reads a global); a cancelled scan returns the distinct cancellation variant rather than `Ok`; `HostPolicy` is
consulted per batch not per entry (counting fake on a fixture scan, which pins Decision 6's dispatch rule).
**Tests, after:** full suite with `--include-slow`; this milestone touches concurrency.
**Docs:** `indexing/CLAUDE.md` + `DETAILS.md`, `media_index/CLAUDE.md`.
**Checks:** `pnpm check --include-slow`.

### M5 — The `Index` handle and the public-API audit

**Intent:** the design milestone. Where the crate stops being a directory and becomes an API.

1. Expand the `pub use events::*` glob so the real surface is visible, then audit all 65 named re-exports, the three
   `pub mod`s, and `media_index`/`importance`'s surfaces against Decision 3's four buckets. Record the mapping as a
   committed artifact; the reasoning is the deliverable, not just the outcome.
2. Build `Index` + `Index::builder()` (Decision 4), resolving to the existing statics.
3. Move every app call site onto the handle: `ipc.rs` (29), `ipc_collectors.rs` (26), `mcp/resources` (23),
   `mcp/executor` (16), `file_system/*` (46 refs across 18 files), `agent/tools` (11), `search/*` (15),
   `mtp/connection` (8).
4. Write the `observe_listing` / `size_of` signatures and types (Decision 10) with `todo!()` bodies. **Compile them. Do
   not implement them.** If either doesn't fit the handle's shape, the shape is wrong and this is the cheapest moment
   to learn it.
5. Settle the platform-conditional API story for the 14 cfg-gated exports, and hold `#![deny(missing_docs)]` on both
   platforms.

**Tests, TDD:** `Index::build()` twice returns `AlreadyBuilt`; **a full scan driven by a handle built with a
`RecordingSink` and an `InMemoryVolume`, with no app types present.** That second one is the real acceptance test for
the whole extraction, written before the handle exists, and it's what smokes out a hidden global that only works by
accident.
**Tests, after:** the whole suite. **Docs:** the public API doc page; the audit mapping in the crate's `DETAILS.md`.
**Checks:** `pnpm check --include-slow`.

### M6 — The move

**Intent:** by now, mechanical.

1. `git mv` the three module trees into `crates/cmdr-index/src/`, so rename detection survives.
2. `Cargo.toml`: `version = "0.0.0"`, `publish = false`, path dep on `cmdr-fs`, optional `specta` feature. Carry over
   `rusqlite`, `tokio`, `tokio-util`, `notify`, `notify-debouncer-full`, `walkdir`, `rayon`, `image`, `serde`, and the
   macOS `objc2-vision` / `objc2-core-ml` CLIP stack **under `[target.'cfg(target_os = "macos")'.dependencies]`**, with
   the M2 cfg-gate now watching the new manifest.
3. Fix imports; app side switches to `use cmdr_index::…`.
4. Check the `.taurignore` dev-watcher shield still covers the moved docs. `apps/desktop/src-tauri/.taurignore` is what
   stops a colocated-doc edit from restarting `pnpm dev`, and `apps/desktop/CLAUDE.md` calls it a must-not-delete
   shield. This move relocates ~20 `CLAUDE.md` / `DETAILS.md` files into `crates/`, and Tauri's dev watcher also
   watches local path dependencies. Verify empirically; if the shield doesn't reach, extend it in the watch config or
   `scripts/tauri-wrapper.ts` as part of this milestone.

**Tests:** the full suite including `--include-slow` and `desktop-e2e-playwright`. Confirm the workspace test count
matches pre-move (M2 made this meaningful). **If a test needs an assertion changed here, something in M1–M5 was wrong**;
fix it there.
**Docs:** colocated `CLAUDE.md` / `DETAILS.md` move with their code; path references across the repo update
(`docs-dead-links` catches stragglers).
**Checks:** `pnpm check --include-slow`, `pnpm check desktop-e2e-playwright`.

### M7 — Isolation check, measurement, and cleanup

**Intent:** convert the claims in "Why" into measurements, and make the boundary self-defending.

1. Land `index-crate-isolation` (Decision 7), with a Go unit test using a fixture manifest that depends on `tauri`.
2. Regenerate the `file-length` allowlist by running the check and committing its rewrite (never hand-edit, per
   `.claude/rules/file-length-allowlist.md`).
3. Re-run every M0 baseline. The enrichment hot path must be within noise. If it isn't, suspect a missing `#[inline]`
   on a small cross-boundary function or a trait that slipped onto a per-entry path (Decision 6).
4. Record incremental rebuild times for the two M0.3 scenarios. That's goal 2's actual answer.
5. `index-query` drops its `cmdr` dependency. Worth noting the dep is **one call**
   (`crates/index-query/src/main.rs:22`, `cmdr_lib::indexing::store::register_platform_case_collation`), so this is a
   tidy-up that proves the boundary is usable, not the acceptance test. The acceptance test was M5's no-app-types scan.
6. Update `docs/architecture.md`, `AGENTS.md` (file structure gains `crates/`), and
   `docs/specs/later/out-of-process-indexing.md` (note which seams this plan already built, and correct its
   `APP_HANDLE` location: it says `commands/indexing.rs`, and that one is real, but the in-scope one is
   `indexing/lifecycle/state.rs:133`).
7. Wipe this plan from `docs/specs/` per the folder's convention, once its durable intent lives in the crate's
   `CLAUDE.md` / `DETAILS.md`.

**Checks:** full `pnpm check --include-slow` plus E2E.

## Parallelization

Mostly sequential by design; the milestones are a dependency chain and we're not in a hurry. The genuinely safe
overlaps:

- M0's benchmark harness can be built while M1 is in flight; it touches only `docs/notes/` and test-support code.
- M2's tooling work (items 4–8) is independent of M2's crate creation (items 1–3) and can be written first, but must
  land before M6 regardless.
- Within M5, the call-site migration splits cleanly by consumer (`mcp/`, `ipc*`, `file_system/`, `agent/`, `search/`)
  once the handle exists: different files, no shared edits. But the audit must land first, and one person owns it. It's
  a design act, not parallel labor.

Everything else sequential. M3 and M4 touch the same 31 files, and M6 depends on both.

## Risks

- **The API audit gets skipped under pressure** and 65 `pub(crate)` items become 65 `pub` items. This is the main way
  the plan fails while appearing to succeed: it delivers goal 2 and quietly abandons 1 and 3. Mitigation: the ~25-item
  target gates M5, and the audit mapping is a committed artifact.
- **Checks go blind at the crate boundary**, so later milestones pass vacuously. This is why M2 pairs the first crate
  with the tooling fix and adds a meta-check, and why the cfg-gate fix is called out separately: its failure mode is a
  broken Linux build, not a red check.
- **Thread QoS silently stops applying** (M4.1). This is the property that kept indexing in-process; losing it re-opens
  the starvation risk `later/out-of-process-indexing.md` documents. Verify explicitly, don't assume.
- **The M1 trait inversion regresses scanner selection.** It's the first real behavior risk, hence TDD and
  `--include-slow` at M1 rather than at the end.
- **Perf regression on the enrichment hot path**, the sub-ms path the directory-size feature rests on. Mitigation: M0
  baseline (harness built, not assumed), M7 re-measure, thin LTO landed before any code moves.
- **A mid-migration beta bug is hard to bisect** across the M6 rename. Mitigation: worktree, one logical change per
  commit, full checks green at every commit.

**Escape hatch:** M1 through M5 are each independently valuable and independently revertable, and the code stays in
`src-tauri/src/` the whole time. Stopping after any of them leaves the codebase strictly better than it started, with
no half-moved crate to clean up. M2 additionally leaves the check runner permanently better regardless of what follows.
The point of no return is M6, and by then everything risky is proven.

## Related

- `docs/specs/later/out-of-process-indexing.md` — the deferred daemon escalation. This plan is its prerequisite and
  makes it substantially cheaper; it is not a commitment to it.
- `docs/specs/later/db-first-listings-plan.md` — serving listings from the index. Decision 10's mismatch tally is the
  evidence that would eventually justify it.
- `indexing/CLAUDE.md`, `media_index/CLAUDE.md`, `importance/CLAUDE.md` — the subsystems' current must-knows.
- `docs/architecture.md` — the subsystem map, updated at M7.
