# The index handle: the public surface, item by item

## Why this file exists

The index's "API" used to be 65 named re-exports plus a `pub use events::*` glob (14 more) on top of three fully public
modules, over ~50 process-wide statics. There was no line between "what the app may rely on" and "internals", so every
app change could reach into an internal and every internal change had an unbounded blast radius.

Turning that into a crate makes the line compiler-enforced, and the danger is obvious: mechanically, everything the app
touches has to become `pub`, and 65 `pub(crate)` items become 65 `pub` items. That delivers the build-time split and
quietly abandons the encapsulation. So every item got one of four dispositions, and the reasoning is the deliverable:

- **Facade** — a method on `Index`, named for what the caller wants rather than for the internal behind it.
- **Fold** — the export existed only because callers did two or three calls in sequence, or made a decision the index
  should have made.
- **Delete** — no caller outside the index, or no caller at all.
- **`testing`** — real reach-through that only a test needs, behind a feature (see § "The gated surface").

## Where it landed

**34 public items on `Index`**, against a target of about 25. Over, and worth saying plainly rather than redefining
what counts. What justifies the nine:

- **Four are the direct-database read side** (`read_pool`, `read_path`, `volume_id_for_path`, `search_generation`).
  They exist because `search/` and the operation log's coverage check run their OWN SQL over an index database. They're
  co-designed consumers sharing the schema, not API users, and a query API for them would have to grow a case per
  question. Grouping them behind a sub-handle would hit the number without changing the surface, which is the kind of
  bookkeeping this audit exists to avoid.
- **Two are the designed-not-implemented write side** (`observe_listing`, `size_of`). They're deliberately present
  before they work.
- **Three are pairs the target didn't anticipate**: `status` / `debug_status` (the second computes watcher counters and
  page counts the first must not pay for), `volume_status` / `volume_status_for_path` (an id-keyed badge and a
  path-keyed one, both live IPC surfaces), and `dir_stats` / `dir_stats_batch` (single-path resolution versus a
  common-parent batch — different queries, and the single one is on the write-operation durability path).

Everything else on the handle is one of the folds below.

## The mapping

### The 14 the glob was hiding

`pub use events::*` covered `ScanRunKind`, `RescanReason`, `MemoryWatchdogAction`, `ActivityPhase`, `PhaseRecord`,
`IndexStatusResponse`, `VolumeIndexStatus`, `IndexDebugStatusResponse` plus the sink's `Diagnostic`, `EventSink`,
`IndexErrorReport`, `IndexEvent`, `IndexEventKind`, `NoopEventSink`.

**All 14 stay, now named.** They're the shapes the handle's own signatures are written in and what the host maps to its
wire format. Expanding the glob was step one of the audit for a reason: the surface wasn't readable from the file that
is supposed to BE the surface.

### Folded (the interesting ones)

- **`is_active` + `master_enabled` + `is_failed` + `clear_index` + `is_mtp_volume_id` + `start_indexing` +
  `start_indexing_for_mtp` + `start_indexing_for_local_external` + `LocalExternalEnable` + `start_indexing_for_smb`
  ⇒ `Index::start_volume`.** The app's `enable_drive_index` command was doing the index's routing: classify the volume
  id, try the local-external probe, fall through to the share gate, and rebuild a failed index on the way. All of that
  is decided from the volume's own facts, which the index has. What stayed app-side is the mDNS kick, which is the
  app's network layer.
- **`is_active` + `force_scan` ⇒ `Index::rescan_volume`.** "Rescan now" on a drive that isn't indexing yet means "start
  it", and the caller shouldn't have to know that.
- **`registered_mtp_volume_ids_for_device` + `buffer_mtp_handle_if_scanning` + `apply_mtp_added_or_changed` +
  `MtpUpsert` + `apply_mtp_removed` ⇒ `Index::on_device_object_changed` / `on_device_object_removed`.** The app carried
  50 lines implementing the index's gate-before-resolve rule (during a walk the device is contended, so buffer the raw
  PTP handle instead of paying a round trip the walk is about to make anyway — the original livelock fix). It now
  forwards the bare handle; the index resolves through the volume seam it already had for the post-walk replay.
- **`on_smb_watcher_died` + `on_smb_overflow` + `on_mtp_watch_continuity_lost` ⇒ `Index::on_watch_gap(scope, reason)`.**
  One question — "live watching lost continuity, so the index can no longer claim it has seen every change" — asked in
  two scopes (a volume, a whole device).
- **`volume_kind` + `stop_indexing` ⇒ `Index::stop_removable_volume`.** Two call sites (`eject.rs`,
  `volumes/watcher.rs`) each open with the identical `!= LocalExternal` guard, because only that kind holds a watcher
  and open database handles that can wedge an unmount. That's the index's knowledge, not the app's.
- **`is_active` + `get_freshness` ⇒ `Index::is_fresh`.** The operation log's coverage gate wants one predicate: can
  these rows be trusted as a complete answer?
- **`should_auto_start` + `set_master_enabled` ⇒ `IndexBuilder::indexing_enabled`.** The stored setting is
  configuration, so it arrives at build time (Decision 9) rather than as a setter the host has to remember to call
  first.
- **`should_auto_start_indexing` + `start_indexing` ⇒ `Index::start_root_at_launch(fda_pending)`.** The host answers
  the permission question; the index composes it with the master switch and acts.
- **`init` ⇒ absorbed by `IndexBuilder::build`.** An API whose first rule is "call `init()` or else" isn't one.
- **`stop_all_indexing` ⇒ absorbed by `Index::set_indexing_enabled(false)`.** Turning the master switch off and
  stopping every volume were always one action; two exports let a caller do half of it.
- **`WRITER_GENERATION` (a `pub` atomic) ⇒ `Index::search_generation()`.** A cache-validity counter, not a static.
- **`get_read_pool` ⇒ `Index::read_pool(ROOT_VOLUME_ID)`.** The root-only variant of a per-volume call.
- **`expected_totals_for_sources` ⇒ `Index::expected_totals`.** A read of the index, so it belongs on the handle.
- **`enrich_entries_with_index` ⇒ deleted; `Index::enrich` takes the volume.** The wrapper existed for callers with no
  volume id in scope, and after the migration every caller has one.

### Deleted

- **`replay_buffered_changes`, `discard_buffered_changes`, `replay_buffered_mtp_changes`,
  `discard_buffered_mtp_changes`** — re-exported at the root, called only from `lifecycle/network_scan.rs`, which is
  inside the index.
- **`register_subsystem_stop_hook`** — `pub`, and the only caller is `media_index/scheduler/lifecycle.rs`, also inside.
- **`enrich_entries_with_index`** — see above.

### Modules

- **`pub mod aggregator` ⇒ `pub(crate)`.** One item left it: `AggregationPhase`, an event payload, now re-exported at
  the root. The benchmark's `compute_all_aggregates_reported` moved to the gated surface.
- **`pub mod writer` ⇒ `pub(crate)`.** One item left it: `IndexWriter`, and only a test needed it.
- **`pub mod store` stays public.** The one deliberate exception, below.
- **`pub mod host`** is new-to-public: it's the plugin interface (`VolumeProvider`, `HostPolicy`, `IndexConfig`,
  `MountFacts`, …), which a host has to be able to implement.
- **`pub mod handle`, `pub mod testing`** are the API and the gated surface.

## The two exceptions, named

**1. `store` is wholesale public, and `IndexStore` keeps 28 methods.** `search/` is a product surface that stays
app-side and runs its own SQL over an index database: it opens connections, walks `idx_parent`, and reads `meta`. That
is a schema dependency, not an API one, and pretending otherwise would mean a query facade with a case per question
that changes every time search's ranking does. Eleven of the 21 module-level items in `store/` were reachable for no
reason and are now `pub(crate)`; what's left is the vocabulary an outside reader genuinely uses. **Follow-up**: when a
machine-checked public-item ceiling lands, count `store` separately and decide whether the schema surface wants its own
crate-level module doc rather than being counted against the handle.

**2. `IndexError::Internal(Diagnostic)`.** The internals below the facade still report a formatted diagnostic for
causes no caller acts on (a poisoned registry lock, a database open failure). Every cause a caller CAN act on has its
own variant — `NotIndexed`, `NotConfigured`, `UnsupportedVolume` — and nothing matches on the text. Converting the
residue means typing the failures inside `lifecycle/state.rs` and `read/queries.rs`, which is a separate change with
its own risk; this is the honest interim, not the end state.

## The platform story: no `cfg` on the surface

`Index`'s signature is identical on every platform. The `cfg`s that used to sit on
14 root re-exports are gone: `SmbIndexGateReason` needed none in the first place (the SMB transport module was never
platform-gated, only its re-export was), and the MTP and local-external routing that IS gated now lives inside method
bodies, where a platform without those transports simply falls through. `start_volume` reaches the share gate directly
there, and `on_device_object_changed` / `on_watch_gap(Device(..))` no-op.

**Why it matters beyond tidiness**: `#![deny(missing_docs)]` is a per-platform lint. A `cfg`-gated public item can be
documented on macOS and undocumented on Linux, and nothing on a Mac would ever say so. That is not hypothetical here —
`store::normalize_for_comparison` had exactly that shape, documented on its macOS arm and bare on the other, and this
milestone is where it surfaced.

## The gated surface

`indexing::testing` (`#[doc(hidden)] pub mod`, behind the `testing` feature) is the one door for test reach-through:
the fake host seams, the recording sink, the scan and writer entry points a real-backend test drives, the registry-slot
reservation, and the macOS disk-image fixture.

**Why a feature and not `#[cfg(test)]`.** `cfg(test)` is set only while a crate compiles its OWN test target. The
moment the index is a dependency, every `cfg(test)` item vanishes from its consumers' test builds — silently, at the
worst possible milestone. This trap has fired three times in this effort. `tempfile` joins the gated surface because
`reserve_initializing_index_for_test` hands a `TempDir` back, so it's an optional normal dependency the feature turns
on, not a dev-dependency.

**Known gap for the move**: `search/ranking/memory_tests.rs` reads `indexing::test_support::heap_bytes_held`, and that
module installs a `#[global_allocator]`, which is per binary and so can't be feature-gated (it would give every binary
linking the index a second one). Today the two live in the same test binary. At the move they won't, and the app needs
its own copy of the small counting harness. That's Decision 18's argument one level further out, not a new problem.

## The other two subsystems

The same four dispositions, applied to `media_index` and `importance`. Neither was re-facaded behind a handle — both
already expose handle-shaped read APIs the app holds (`MediaIndex`, `ImportanceIndex`) plus the two schedulers it
manages — but every public item got a decision.

### Where they landed

| | public modules | public items |
| --- | --- | --- |
| `media_index` | 14 → **11** | 142 → **51** |
| `importance` | 8 → **3** | 65 → **23** |

### `media_index`

- **Five modules stop being public**: `backend`, `thermal`, `writer`, `writer_registry`, and `network::policy`, plus
  `clip::{backend, macos, tokenizer}`, `vector::cache`, and `network::enrich`. None had a consumer outside the
  subsystem. `clip::install` went from 18 public items to one (`is_installed`); the other 17 are how a model gets
  unzipped and checksummed, which is nobody's business but the installer's.
- **One fold, and it was hiding an M6 break.** `commands/media_index/file_status.rs` reached
  `scheduler::enrich::{ImageEntry, parent_dir, walk_image_entries_in_dirs}` — through `pub(crate) mod enrich`, so it
  compiles today only because the app and the index are one crate. It is now
  `media_index::read::qualifying_images_for_paths(volume_id, &paths)`: the caller asks the question it actually has
  ("which images here qualify, with their live mtime and size?"), the pool plumbing and the parent-dir derivation move
  inside, and `ImageEntry` becomes the read API's own type.
- **`media_index::testing`** carries the three items one app-side integration test needs to fetch image bytes off a real
  share through the enrichment path's own fetcher.
- **The errors public methods return are public**: `ClipError` and `MediaStoreError` are root re-exports, like
  `ImportanceStoreError`.

### `importance`

- **`importance::tooling`** (new `tooling` feature) is the one door for the three `index-query` binaries: the evaluation
  corpus, the synthetic scenarios, the constraint harness, and the measurement entry points. A feature rather than
  `#[cfg(test)]` because those are BINARIES in another crate — `cfg(test)` can never reach them. This is the second
  gated bucket the plan named, and it now exists.
- **`importance::testing`** carries the four items two app-side tests need to stage a scored folder.
- **Five modules stop being public**: `store`, `writer`, `writer_registry`, `signals`, and `scorer`. The scoring
  vocabulary was already re-exported at the root, so only `ImportanceStoreError` needed a new home there.

### The rule the gates follow

**`#[cfg(test)]` when every consumer is inside the crate; a feature only when one lives outside.** Using a feature for
an in-crate consumer is not harmless: the app enables `testing` for every dev target, so the item exists in the
non-test lib build with nothing calling it, and `#[deny(unused)]` turns it into an error. That is how the four
`ImportanceStore` accessors and the two `MediaStore` ones landed on plain `#[cfg(test)]`.

### What narrowing the modules exposed

Making a module `pub(crate)` makes the compiler honest about what the shipped build actually uses. Nine items had been
invisible behind a `pub mod`:

- **Deleted, no caller anywhere**: `network::policy::{FetchGate, gate_on_idle}` (superseded by the `HostPolicy` seam —
  its neighbours already ask `volume_clear_for_enrichment`), `network::config::covers_override`,
  `BruteForceVectorStore::{len, is_empty, vector_for}`, `MediaStore::db_path`, `ClipError::Decode`, and the unused
  `scheduler` re-export of three `reclaim` types.
- **Tooling-only, and now saying so**: `MeasureOutcome`, `recompute_index_to_db`, the whole `differential` module,
  `classify::under_floored_paths`, `scorer::extension_count`.
- **Kept with the reason written down**: `MediaWriter::{rename_path, purge_volume}` and their `WriteMessage` variants
  have no production sender — `rename_path`'s own doc says the rename-following hook it exists for isn't wired yet, so
  a rename still manifests as GC(old) + enrich(new). Same shape as `WriteMessage::PropagateDeltaById` on the drive
  index: a supported capability, not an accident.
- **`WriterRegistry::shutdown_all` has no caller, in either subsystem.** Its doc says "called on app teardown so the
  writer threads join", and no teardown path calls it. Not a data-loss gap (every write is flushed as it is applied,
  and each writer stops when its last handle drops), but the documented teardown does not happen. Wiring it is a
  shutdown-ordering decision, not a visibility one.
