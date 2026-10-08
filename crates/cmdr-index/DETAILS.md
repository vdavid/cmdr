# `cmdr-index` details

## Why the crate exists

Three reasons, in priority order. When a decision here is ambiguous, resolve it toward the higher one.

1. **Encapsulate the hardest code in the codebase behind a boundary you can reason about.** These three subsystems are
   about 28% of the backend and hold the gnarliest concurrency and lifecycle logic in the product, over ~50 process-wide
   mutable statics. While they lived in the app there was no line between "what the app may rely on" and "internals", so
   every app change could reach into index internals and every index change had an unbounded blast radius. A crate makes
   that line real and compiler-enforced.
2. **Build-time separation.** Backend work that doesn't touch the index no longer rebuilds the index, and vice versa.
3. **The index could one day be a product of its own.** "Cmdr, plus a smart file+image index any agent can tap into"
   needs a documented, stable, self-contained API. This is that API. It is NOT a daemon; there's no separate process
   here, and why is § "Considered and deferred: out-of-process indexing".

## The contract this crate is held to

1. **No `tauri` in the dependency tree.** The load-bearing property, and exactly the kind that erodes one convenient
   import at a time.
2. **No user-facing strings produced here.** The index emits typed values; the host renders every word. One deliberate
   exception, from `cmdr-fs` rather than this crate: `pluralize`, which builds log lines. `PhaseRecord.trigger` carries
   pluralized text that the developer debug panel renders, which is acceptable but means "pluralize is purely log-only"
   isn't a claim to make.
3. **Typed errors everywhere.** No `Box<dyn Error>`, no stringly-typed failure, so a host never string-matches. Two
   named exceptions rather than zero: `IndexError::Internal(Diagnostic)` is the log-only residue for causes no caller
   acts on, and `ReadPool::with_conn` still returns `Result<T, String>`. Both are in `src/indexing/handle/DETAILS.md` §
   "The three exceptions, named". Typing the causes inside `lifecycle/state.rs` and `read/queries.rs` is still open
   work. The house error style is in `apps/desktop/src-tauri/CLAUDE.md`.
4. **Long-running work is cancelable** through one primitive (`tokio_util::sync::CancellationToken`), with cancellation
   observable from outside: a cancelled operation returns a distinct error variant, never a silent early return. The
   topology is in `src/indexing/host/DETAILS.md` § Cancellation.
5. **Everything long-running reports progress** as structured values through a caller-supplied sink.
6. **A handle, not a global.** The public API is methods on an `Index` the host constructs and owns.
7. **The house lint set is replicated, not weakened**, via `lints.workspace = true`, plus `#![deny(missing_docs)]`.
8. **Ingest and query are equal citizens.** `observe_listing` and `size_of` are designed and compiled, returning
   `NotImplemented` until they're built. They exist so the two features that need them can't force a redesign.

## What the host has to answer

`indexing/host/` declares five seams and nothing else may cross the boundary. In Cmdr, `src/index_host.rs` answers all
five in one function, early in `setup()`, right after the instance lock.

- **`EventSink`** — where the index reports. The host maps `IndexEvent` to its own wire format; error reporting rides
  the same channel, because a crate can't invoke the app's `log_error!` macro across the boundary and dropping it
  silently would be a feedback-loop regression.
- **`VolumeProvider`** — which volumes exist, volume identity, and mount classification ("is this a network fs?"). The
  index never touches a volume manager, a platform mount probe, or an MTP session layer directly.
- **`HostPolicy`** — "may I do background work right now?", composed from the host's own priority signals. Consulted
  inside scan loops, so it returns a cheap `Copy` value and callers cache it per batch. ❌ No trait may be introduced on
  a per-entry path; wanting one is a signal to restructure the call.
- **The runtime** — a `tokio::runtime::Handle`. See the `CLAUDE.md` must-know.
- **`IndexConfig`** — plain values passed at build time and updatable through `reconfigure`. The crate never reads a
  settings file, an env var, or the full-disk-access choice for itself: policy belongs to the product. The `CMDR_*`
  debug knobs are the deliberate exception, and stay `std::env::var` reads.

Their rationale, and what each one replaced, is in `src/indexing/host/DETAILS.md`.

## Why `specta` is an unconditional dependency

58 data types derive `specta::Type`, plus `FileEntry` and `TagRef` down in `cmdr-fs`, and `FolderSignals`'s serde shape
is load-bearing. Making that an optional feature reads like the tidier choice and is the worse one: the app is the only
consumer and always enables it, and nothing in the check runner builds `--no-default-features`, so the specta-off
configuration would be compiled zero times and rot on the first edit. Unconditional costs nothing now that
`tauri-specta` is out of the tree, and it means the crate has one shape rather than two, one of which is never tested.

Bindings collection is unaffected: the app's `ipc.rs` collects types transitively through command signatures, and a
cross-crate `specta::Type` impl collects normally. What DOES break is two `specta` versions in one graph, which is why
the pin is exact and identical to the app's.

## Why `indexing` is private and the other two aren't

`lib.rs` re-exports the file index's promises item by item, so the crate root is the one place that answers "what may a
host rely on?". Keeping `mod indexing` private buys two things: `cmdr_index::indexing::…` never appears in a caller (the
handle and its vocabulary sit at the root, where they read cleanly), and `pub(in crate::indexing)` keeps meaning
"internal to the file index" rather than silently widening to `media_index` and `importance`, which are siblings in the
same crate.

`media_index` and `importance` stay public under their own names because each carries a curated surface of its own (see
each `mod.rs`) and because their names don't stutter against the crate's.

## The two gated surfaces

Both are `#[doc(hidden)]`, both are turned on through a **dev-dependency** so they stay out of shipped builds, and
neither is a promise.

- **`testing`** — what a test outside this crate needs to drive the index: fake volumes, a recording sink, a
  controllable priority policy, a temp data dir, a reserved registry slot, the disk-image fixture, and the direct
  scan/writer/store entry points a host-side test uses to prove its own backend works with the index's scanner. Reached
  through `cmdr_index::testing`, grouped by what a test is trying to do. `tempfile` is a normal optional dependency
  rather than a dev-dependency because `reserve_initializing_index_for_test` hands a `TempDir` back to its caller.
- **`tooling`** — the importance evaluation corpus and measurement entry points that `crates/index-query`'s three
  importance binaries drive. Separate from `testing` because they answer different questions: a test needs fakes and
  guards, a tool needs the real scoring pipeline plus a corpus. And because those consumers are BINARIES in another
  crate, which `cfg(test)` can never reach.

## The `cfg(test)` trap

`cfg(test)` is set only while a crate compiles its OWN test target. It is NOT set when a consumer compiles this crate as
a dependency, even from that consumer's test build. So a `#[cfg(test)]` item with a consumer outside the crate simply
isn't there, and the failure is a missing symbol at best.

This fired four separate times across the extraction, and the last batch surfaced only when the code actually became a
dependency: `one_of_every_kind` (the host's event-mapping completeness test), the disk-image fixture,
`ScanPacer::unpaced`, `IndexStore::list_children`, and `handle::test_lock`. Each is now on the `testing` surface.

**The rule, in both directions:** `#[cfg(test)]` while every consumer is inside the crate; a feature the moment one
isn't. And a feature for an item with only in-crate callers isn't a harmless over-approximation, because the app enables
`testing` for every dev target, so the item exists in the non-test lib build with nothing calling it and
`#[deny(unused)]` turns that into a hard error.

## The counting allocator is duplicated, on purpose

`indexing/test_support.rs` installs a `#[global_allocator]` so the memory-shape guards can assert what a hot path
allocates. A binary gets exactly ONE global allocator, so it can't live in `cmdr-fs` (every binary linking that crate,
including the shipped app, would get a second one) and it can't ride a feature (dev-dependency features unify with
normal ones for the same package).

That makes it per test BINARY, and the host has its own. Cmdr keeps a trimmed copy in
`apps/desktop/src-tauri/src/test_support.rs` for `search/ranking/memory_tests.rs`. **This fails by measuring zero, not
by failing to compile**, so that test asserts a non-zero measurement before it asserts a budget. Note for anyone
comparing memory numbers: Rust test runs are measured under the counting allocator, not the release build's.

## One fingerprint helper, two policy stamps (`fingerprint.rs`)

All three databases here are disposable caches, and two of them persist a stamp saying which compile-time policy their
rows were written under: the index's scan exclusions (`indexing/scanner/exclusions.rs::exclusion_policy_fingerprint`,
gating `index_predates_exclusion_policy`) and importance's classification rules
(`importance/classify.rs::scoring_policy_fingerprint`, gating `store::needs_full_pass`). Both hash their constant lists
through `fingerprint::fingerprint_of`, so editing a list re-arms every existing DB with no version number for anyone to
forget to bump.

The mixing is FNV-1a rather than `DefaultHasher` because the value goes to disk and must not shift with a toolchain
upgrade. It's crate-internal and shared rather than copied per subsystem, so ONE golden test
(`the_fingerprint_mixes_its_input`) covers both: a hash that collided two policies into one value would pass every
symmetric stamp-and-compare test while silently skipping the work the stamp exists to trigger.

## Where the stores live

`IndexConfig` carries two folders, and `volume_files::StoreDirs` says which store is in which:

- **`drive_index_dir`**: the drive index. Cmdr points it at `~/Library/Caches/com.veszelovszki.cmdr/drive-index/` (app
  side: `config::drive_index_dir`, which also covers dev, worktree, and E2E instances).
- **`data_dir`**: importance and media, in `~/Library/Application Support/com.veszelovszki.cmdr/` beside the app's own
  state.

**Decision: only the drive index moves to Caches.** Why: it's the bulk (1.08 GB on David's Mac against 180 MB of
importance and 31 MB of media, 2026-10-06), it changes constantly, and a rescan rebuilds every byte of it, so backing it
up costs disk and backup time for nothing. `~/Library/Caches` is the platform's own "don't back up, may be purged"
folder: Time Machine skips it (verified on macOS 27.0 with `tmutil isexcluded ~/Library/Caches`, 2026-10-06), and it's
the conventional exclusion for other backup tools too (not verified tool by tool), which a per-file Time Machine
attribute isn't. A cleaner app emptying it costs a rescan, the same path as any missing index. The other two stay in the
data dir because they hold what a scan can't bring back: importance's `visits` table is the user's navigation history,
and the media index is hours of OCR and embedding work. A purge of either would silently lose that, and a backup restore
should bring it back.

**Rejected: `NSURLIsExcludedFromBackupKey` on the data-dir files.** It's per file and lost when SQLite deletes and
recreates one (a schema bump, a corruption rebuild), so it would need re-asserting on every open plus each sidecar; only
Time Machine honors it; and excluding importance or media would drop exactly the data that justifies keeping them out of
Caches. Excluding a dedicated FOLDER would hold up better, but nothing in the data dir is a pure cache except what
moved.

**Gotcha: importance's weights are still backed up.** Its file is 99.8% rebuildable weights (67 MB of rows, plus free
pages, against 104 KB of visits, 2026-10-06), and they're rewritten on every rescore. Splitting `visits` into a store of
its own would let the weights move to Caches too; not done yet.

**Gotcha: the cache dir is shared.** WebKit (`WebKit/`) and Core ML (`com.apple.e5rt.e5bundlecache/`) keep their caches
in the same `~/Library/Caches/<bundle id>/`, which is why the index has a `drive-index/` folder of its own.

### Adopting an older build's drive index (`drive_index_relocation.rs`)

`IndexBuilder::build` moves any `index-{id}.db` it finds in `data_dir` to `drive_index_dir`, every launch, before
anything opens either (a no-op once the data dir has none). The module docs carry the per-volume decision table. The
shape that makes it crash-safe: fold the WAL in with a `TRUNCATE` checkpoint, close (which deletes the empty sidecars),
then ONE `rename` of the database file. A crash at any point leaves either nothing done or nothing left to do.

- **Why move rather than delete and rescan**: a rescan of a big disk is tens of minutes of CPU and I/O, a NAS share's
  far more, and a same-volume rename is free. Across volumes (a `CMDR_CACHE_DIR` elsewhere) a rename can't work, and a
  copy of gigabytes isn't worth it for a cache, so that case deletes and rebuilds.
- **Downgrade**: an older build finds no index in the data dir and rescans into it. Upgrading again finds both copies,
  keeps the cache dir's (what this build maintained), and deletes the data dir's. No old copy outlives one launch.
- **Why in `build` and why the lock first**: renaming and deleting database files is only safe while no other process
  has them open, so `lib.rs` claims the data dir's instance lock BEFORE `index_host::install` builds the index. It's in
  `build` rather than `install` so a test handle never runs it, and it's a no-op when both folders are the same (the
  lazy no-host fallback and every test).
- **A stray `-wal` in the destination is deleted before the move**: SQLite would replay a log that belongs to no
  database onto the one that arrives, which is corruption
  (`a_stray_log_in_the_new_folder_is_never_replayed_onto_the_moved_database`).
- **`IndexStore::open` creates its parent folder**, so a cache folder emptied while Cmdr runs costs a rescan of what's
  opened next, never a failed volume until the next launch.

## A volume's files, and the one door they leave by (`volume_files.rs`)

Three stores each keep a file set per volume, all named after the volume id, each in its own folder (§ "Where the stores
live"):

- The drive index: `index-{id}.db`, plus SQLite's `-wal` and `-shm`.
- Folder importance: `importance-{id}.db`, plus the same two.
- The media index: `media-{id}.db`, the same two, and one vector index per ANN space beside it
  (`media-{id}.clip.usearch`, `.usearch.meta`, `.usearch.dirty`).

Each store opens and migrates its own database. `volume_files.rs` owns what they share: the NAMES (every store's path
function delegates to `VolumeStore::db_path`, and `media_index/ann` names its files through `ann_file`) and REMOVAL. A
leaf over `std` and `cmdr_fs::sqlite_util`, importing no subsystem.

**Why removal is one door.** It used to live in each caller, and each one unlinked `index-{id}.db` and stopped.
Forgetting a share left its importance database behind for good (tens of MB each; GitHub issue #327), the retention cap
did the same, and the retired-ID sweep took the media database and left its vector index. So a caller now names a REASON
and never a list of stores, and `Removal::takes` answers per store in one exhaustive match:

- **`Forgotten`** (the user forgot the drive, cleared every index, or the retention cap evicted it): the index and
  importance. Importance scores the folders of an index that is going.
- **`IndexRebuild`** (a `Failed` index a start clears, a walk-built index that predates the exclusion policy): the index
  alone. The volume is staying, the next completed scan rescores importance in full, and the visit history in that
  database is not something a scan brings back.
- **`Unreachable`** (a volume ID from a retired scheme): every store. Nothing can open any of it again.

A fourth store is a new `VolumeStore` variant, and the compiler then asks what each reason does with it.
`each_removal_takes_the_stores_it_names_and_no_others` pins the table, because a change to it is a change to what a user
loses. ❌ Never unlink a per-volume file anywhere else.

**Decision: forgetting a volume keeps its media index.** Its rows are hours of OCR and embedding work, `media_index`
deletes rows on exactly four named paths and never on a volume's absence (`src/media_index/DETAILS.md` § GC safety
argument), and turning the drive's indexing back on picks them straight up. The cost is that a drive forgotten for good
keeps a `media-{id}.db` nothing collects. Whether "forget" should take it is a product call that is David's to make; it
is one line in `Removal::takes` either way.

**What `remove` does per store, in order:** ask the store's holders to let go, retire the read connections threads cache
to the database, unlink it with its sidecars, retire again, then unlink what the store derives from it. The retirement
and why it brackets the unlink: `crates/cmdr-fs/DETAILS.md` § "Retiring cached read connections".

**Holders.** A subsystem that keeps a store open outside the lifecycle registry registers a release through
`register_holder`, so `indexing` never calls `importance` by name. The importance scheduler is the one holder today: its
per-volume writer thread is shut down and joined (`WriterRegistry::retire`) before the unlink. Skipping that costs
twice: the writer's connection keeps the unlinked file's blocks allocated, and the writer stays in the registry, so a
share indexed again writes every weight into the unlinked file while the new database on disk stays empty
(`a_forgotten_volumes_writer_lets_go_of_its_database`). A release is handed the data dir as well as the volume id and
ignores one that isn't its own, since two hosts in one process (tests) can share an id. The index database's own holders
are the lifecycle's, drained by the teardown in its own load-bearing order, and are not registered here.

**A stop retires too.** A volume that stops and keeps its files (disconnected, turned off, the watchdog) has the
connections cached to its databases retired through `volume_files::retire_read_connections`, called where the teardown
withdraws its read handles. Without it a stopped share kept a connection in every blocking thread that had ever read it.

**What already sits on users' disks.** A share forgotten before the stores were removed together left its importance
database behind, and nothing collects it automatically:

- `Index::forget_all_volumes` ("Clear index" in settings) does reach it. It reads volume ids from the files of every
  store `Forgotten` takes, so an importance database whose index is already gone is listed, counted in
  `Index::disk_footprint`, and removed.
- The retired-ID sweep reads ids the same way, so it takes a leftover keyed by a retired ID.
- ❌ There is no startup sweep for a current-ID leftover, deliberately. "An importance database with no index beside it"
  is not proof of abandonment: a production data dir held `importance-cloud-google-drive.db` with no index database for
  that volume and no record of one ever being forgotten (verified on macOS, `ls` of the data dir, 2026-09-30). A sweep
  keyed on that would delete visit history for a volume that may still be in use. The missing piece is a positive signal
  that a volume was forgotten, not a better guess.

## What deliberately stayed with the host

- **Every real-storage `Volume` backend** (local POSIX, SMB, MTP, archive) with its `smb2` / `mtp-rs` / git /
  mount-detection dependencies, plus the volume manager. The index reaches them only through `VolumeProvider`.
- **The 15 event payload structs.** The crate's events are a plain Rust enum with no `serde` and no `tauri_specta`; the
  host owns the wire format and every word in it. Schema derives on DATA are fine (58 types derive `specta::Type`);
  presentation decisions are not, and events are where presentation lives.
- **Every `#[tauri::command]`**, in the app's `commands/`.
- **`search/`.** A product surface with ranking choices and UI copy. It reads the index database directly, which is why
  `store` and `ReadPool` are public: that reach is deliberate and documented, not an accident.
- **`operation_log/`** and the agent's store, which share `cmdr-fs`'s one process-wide SQLite page-cache slab with the
  index's three databases. That's why the connection factories live in `cmdr-fs` rather than either end.

## Considered and deferred: out-of-process indexing

Moving drive and media indexing into its own OS process is the only design that makes "a runaway indexer can never
starve the UI" structural rather than defended: the kernel would arbitrate CPU, memory, and I/O between two processes,
and the OS would account for the indexer's resources separately. It was weighed after an incident where a dead index DB
spun a failing-retry loop at ~190% CPU and froze the webview through CPU plus synchronous log-write contention, and
deferred, because three cheaper in-process fixes closed the actual levers:

- **Thread QoS**: the heavy indexing threads (writer, scanner, walker workers and watchdog, local reconcile) run at
  `QOS_CLASS_UTILITY` (`crates/cmdr-fs/src/thread_qos.rs`), so macOS lets the UI's threads preempt them.
- **Bounded logging**: the file-log writer coalesces identical-line floods
  (`apps/desktop/src-tauri/src/logging/coalesce.rs`), so a runaway loop can't peg a core on `write` or stall other
  threads on the log mutex.
- **The source is stopped**: a fatal storage error fails the index instead of retrying forever
  (`src/indexing/writer/DETAILS.md` § "Fatal storage failure — the writer is the detector").

What a split would cost, so the decision can be re-opened on evidence rather than re-derived:

- **The data plane is easy.** One WAL DB per volume with one writer and read-only readers is exactly what WAL supports
  across processes. The writer moves into the indexer; search can keep reading the same files from the app.
- **The control plane is the cost.** `INDEX_REGISTRY` and the other process-wide statics are reached from dozens of call
  sites, and every app-side read or mutation becomes an RPC or a cache. The shared `Arc`s (`ReadPool`, `PendingSizes`,
  per-volume `Freshness`) each need one owner. This crate already did the enumerating: the five host seams are the RPC
  surface, a pipe-backed `EventSink` is a second implementation of an existing trait, and the `Index` handle is the
  bounded list of status reads to convert (`src/indexing/handle/DETAILS.md`).
- **New failure modes**: a sidecar can crash or hang on its own, so the app needs supervision (detect, restart, back
  off), and the UI needs an "indexer unavailable" state distinct from "indexing off". Hot synchronous status reads
  (volume switching, `cmdr://state`) would need caching to stay sub-millisecond.
- **Prior art**: the AI feature already spawns and supervises `llama-server` as a child process
  (`apps/desktop/src-tauri/src/ai/process.rs`). An indexer sidecar is harder (stateful and bidirectional), but the
  process-management scaffolding exists.

Magnitude: multi-week, with regression surface across the whole indexing lifecycle. **Revisit only if** (1) a new
starvation incident traces to a path QoS and log coalescing can't contain (memory pressure, a syscall storm QoS doesn't
throttle), (2) indexing grows a component that wants its own address space (a heavy native library, a crash-prone
codec), or (3) OS-visible resource accounting and throttling for indexing becomes a product feature.

## Related

- `crates/cmdr-fs/DETAILS.md` — the layer below: the vocabulary this crate indexes, and why each piece is down there.
- `src/indexing/handle/DETAILS.md` — the public-surface audit, item by item.
- `src/indexing/host/DETAILS.md` — the seams and their rationale.
