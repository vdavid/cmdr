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

**36 public items on `Index`**, against a target of about 25. Over, and worth saying plainly rather than redefining what
counts. What justifies the eleven:

- **Two are the coverage pair** (`coverage`, `coverage_token`), added 2026-08-05. See § "Coverage: the one concept added
  since the audit" below.

- **Four are the direct-database read side** (`read_pool`, `read_path`, `volume_id_for_path`, `search_generation`). They
  exist because `search/` and the operation log's coverage check run their OWN SQL over an index database. They're
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

## Coverage: the one concept added since the audit

Added 2026-08-05 for `docs/specs/unindexed-search-plan.md`, which makes search return the same files on an unindexed
drive as on an indexed one. That needs the index to answer a question it never had a shape for: **what can't I answer
for yet?** David approved the ceiling raise with one instruction — design the whole surface as one concept and raise the
ceilings to match, rather than a bump per method — so the shape below is the whole thing, including the half that hasn't
landed.

Two calls, one question:

- **`coverage(volume_id, scope_path, dimension)`** — the frontier (the shallowest directories nothing has listed), the
  directories a walk has tried and can't read, and the token saying which state of the index the answer describes.
- **`coverage_token(volume_id)`** — that token without doing a descent, so a caller can take one when it loads a
  snapshot of the index and re-ask when the two stop matching. This is what absorbed the plan's "an epoch read is
  needed": `IndexStore::read_current_epoch` never reaches the handle, because a bare epoch isn't the question. A walk
  stamps `listed_epoch` and never bumps `current_epoch`, so the epoch alone can't move when rows appear; the token pairs
  it with the id high-water mark, which does.
- **`cover(volume_id, frontier, dimension, cancel)`** — the walk half: it takes the frontier a coverage answer named and
  fills it in, handing back each batch of entries it finds while it's still running. It landed 2026-08-05 into the slot
  reserved for it, and brought the three types its answer is made of (below). The `cancel` token is a parameter rather
  than a method on the handle because the handle can't leave the thread reading its batches (§ below).

**Why the covered half is not in the answer.** It's tempting to return "these subtrees are covered, those aren't", and
it would be a second, weaker copy of something the index already has. The two halves are complementary over the same
subtree, so a caller runs its own query over the scope unfiltered and gets exactly the covered rows. That's what makes
deduplication unnecessary anywhere in the search path, rather than a hash set nobody can size correctly.

**Why `CoverageDimension` exists with one variant.** Content search will ask the same question in a second dimension (a
`content_epoch` sibling to `listed_epoch`, propagated with the same 0-absorbing min). The walk stages that fall out of
it only work if callers were never written against a single implied dimension, and adding the parameter later means
touching every one of them.

**`CoverageMap::being_walked` is a FIELD, not a seventh promise.** Which frontier roots a walk is covering right now
isn't in any database — it's the in-flight claims — so `Index::coverage` fills it after the read query returns, above
the layer that must not import lifecycle state. A `Vec<String>` on a type the surface already carries costs no ceiling,
the same argument M8's two unreadable lists took. What it buys: a caller can tell "nobody has been here" from "somebody
is here already" and wait rather than answer empty, without a method of its own.

⚠️ **It means another WALK, not "the claim table has an entry".** A full scan holds the volume root without covering any
particular root of the frontier it was asked about, so `ground_being_walked` filters to `Additive` holders and a running
scan never appears here. Unfiltered, every frontier root would read as being walked for the whole of a scan, and the
host's `DeferredUntilSearchEnds` would promise a search a walk that isn't coming (`../lifecycle/cover/DETAILS.md` § "The
two modes a claim can hold in").

**Six root promises** came with it. `CoverageMap`, `CoverageToken`, and `CoverageDimension` are the read half's;
`CoverWalk`, `CoveredEntry`, and `CoverOutcome` are the walk's, and each earns its place by being something a host
genuinely can't do without:

- **`CoverWalk`** — the running walk. There is no way to take batches off it or wait for it without a handle to it, and
  it can't be a plain `Receiver` because finishing (join, and the totals that come back) is part of the contract.
  **Stopping it is NOT on this type**: a `Receiver` is `!Sync`, so the handle stays on the one thread that reads it,
  while the decision to stop belongs to a closing dialog or a quitting app somewhere else. So `cover` takes the
  `CancellationToken`, the caller keeps a clone, and there is exactly one way to stop a walk from anywhere (2026-08-05).
- **`CoveredEntry`** — one entry the walk found. This type crossing the boundary IS the design: Decision 3 keeps the
  matcher in `search/` and the scan in `indexing/`, so what crosses is data, not a predicate. It carries the entry's own
  pre-dedup sizes, because a result row showing a hardlinked file as 0 bytes would be wrong.
- **`CoverOutcome`** — what the walk covered, and whether somebody stopped it. The search dialog's terminal states are
  exactly that distinction, and neither of them is a failure.

**Why standing a cold volume's index up is NOT a method of its own** (2026-08-05). A drive nobody ever indexed has no
database, no epoch, no writer, and no entry to resolve a scan root against, so `cover` used to refuse it — the one case
that made the whole concept useless where it's needed most. The obvious move was a fifth call ("prepare this volume",
"ensure an index"), and it would have been a mistake twice over: it makes every caller responsible for a sequencing rule
the index can enforce itself, and it names an internal (an `IndexManager` exists) rather than something the caller
wants. `cover` already writes to disk and already means "make the index able to answer for this"; a volume with nothing
to write into is a case of that, not a different question. So the bootstrap is behind `cover`, the surface stays at 38
methods and 50 root promises, and **neither ceiling moved for the cold bootstrap**.

The cost, stated plainly: `cover` on a cold drive creates a database and registers the volume, which is a bigger side
effect than the name suggests. That is bounded by what it stands up — a writer and nothing else, no scan, no watcher —
and by what it refuses (an unmounted drive, a share, a phone: all `NotIndexed`). `lifecycle/DETAILS.md` § "What has to
exist before a walk can run" is the mechanism.

Nothing further is owed to the concept; a fourth type here needs the same argument these did. `CoverageToken`'s fields
stay private and it's `PartialEq` only — the sole question worth asking of it is whether two answers describe the same
rows, and exposing the epoch would invite callers to reason about a scheme the read side deliberately keeps inside
(`../read/CLAUDE.md`: "never ship raw epochs").

The mechanism, the descent rule, and the tests that hold it: `../read/DETAILS.md` § "The coverage frontier".

## What the index occupies on disk: the second concept added since the audit

Added 2026-08-05, the same effort's last milestone. Once a search walks, a machine with drive indexing OFF accumulates
index databases nobody asked for, so the settings screen has to be able to show and reclaim them. Two calls, both about
files rather than volumes, and `HandleMethods` 38 → 40 with no new root promise:

- **`disk_footprint()`** — the bytes every index database occupies, sidecars included. It reads the data dir, ❌ never
  the registry: the whole point is the database a walk built that nothing re-registered after a restart, which
  `status(volume_id)` reports as absent because it asks the live instance.
- **`forget_all_volumes()`** — the whole-index sibling of `forget_volume`, reaching those same unregistered databases.
  Each volume still goes through `clear_index`, so a live one drains its writer and withdraws its read handles first.

Why not one call per volume from the host: the host would have to enumerate `index-{volume_id}.db` itself, which is this
crate's private file convention (`../resources/retention.rs` owns it), and `volume_ids()` reports the REGISTERED
volumes, which is exactly the set that misses the case.

The concept is closed: measuring and clearing is all of it. A cap on the footprint would be a third call and a policy,
and David's decision is that there is no cap for now (`docs/specs/unindexed-search-plan.md` Decision 17).

## The mapping

### The 14 the glob was hiding

`pub use events::*` covered `ScanRunKind`, `RescanReason`, `MemoryWatchdogAction`, `ActivityPhase`, `PhaseRecord`,
`IndexStatusResponse`, `VolumeIndexStatus`, `IndexDebugStatusResponse` plus the sink's `Diagnostic`, `EventSink`,
`IndexErrorReport`, `IndexEvent`, `IndexEventKind`, `NoopEventSink`.

**All 14 stay, now named.** They're the shapes the handle's own signatures are written in and what the host maps to its
wire format. Expanding the glob was step one of the audit for a reason: the surface wasn't readable from the file that
is supposed to BE the surface.

A fifteenth joined them later, with David's say-so and a ceiling raise to match (50 → 51 root promises):
**`CoveragePhase`**, which phase of a drive's first index is running. It has to be the crate's because the crate owns
both the order and the `IndexPathSpace` that classifies a root into it; a host deriving it from the phase root would
need its own idea of firmlinks, right on one machine and wrong on the next. It rides one event variant and
`IndexStatusResponse`, which is what lets a reloaded window name the running phase. What each phase is CALLED stays the
host's.

⚠️ **Which of the check's counters a new item spends is not a choice, so read the right one before assuming headroom.**
`index-crate-isolation` counts `SubsystemItems` in the modules it can REACH by walking `pub mod` declarations from
`lib.rs`, which for this crate is `importance` and `media_index` and nothing else. `indexing` is private, so everything
it promises arrives as a `pub use` here and counts as a ROOT PROMISE — including every value an event carries, whose one
sane home is `indexing/events/payload.rs` beside `ScanRunKind` (anywhere else makes the event envelope import its own
parent). A grant of "one item" for such a type is `RootPromises` moving by one and the other three counters staying put.

### Folded (the interesting ones)

- **`is_active` + `master_enabled` + `is_failed` + `clear_index` + `is_mtp_volume_id` + `start_indexing` +
  `start_indexing_for_mtp` + `start_indexing_for_local_external` + `LocalExternalEnable` + `start_indexing_for_smb` ⇒
  `Index::start_volume`.** The app's `enable_drive_index` command was doing the index's routing: classify the volume id,
  try the local-external probe, fall through to the share gate, and rebuild a failed index on the way. All of that is
  decided from the volume's own facts, which the index has. What stayed app-side is the mDNS kick, which is the app's
  network layer.
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
- **`should_auto_start_indexing` + `start_indexing` ⇒ `Index::start_root_at_launch(fda_pending)`.** The host answers the
  permission question; the index composes it with the master switch and acts.
- **`init` ⇒ absorbed by `IndexBuilder::build`.** An API whose first rule is "call `init()` or else" isn't one.
- **`stop_all_indexing` ⇒ absorbed by `Index::set_indexing_enabled(false)`.** Turning the master switch off and stopping
  every volume were always one action; two exports let a caller do half of it.
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

## The ceiling that keeps this honest

`index-crate-isolation` (error-level) counts the surface on every run and fails when a bucket grows. It caps
`cmdr-archive` the same way, from its own entry in the check. Four buckets here, measured 2026-07-31 and raised once on
2026-08-05 for the coverage concept above:

- **50 root promises** — the names `lib.rs` exports, `pub mod` included. 44 at the audit, plus coverage's six types (the
  read half's three on 2026-08-05, the walk half's three the same day).
- **40 methods on `Index`** — the 36 above plus `Index::builder`, which the headline number treats as the constructor
  rather than a call, plus `cover`, which took the slot reserved for it by name, plus the disk-footprint pair below. The
  cold-volume bootstrap took none of it: it went behind `cover` rather than becoming a method (above, "Why standing a
  cold volume's index up is NOT a method of its own"). No reserved slot is left, so the next method has to be argued the
  way these were.
- **17 public modules** and **156 public items inside them** — the surface the root re-exports don't capture, which is
  where `media_index` and `importance` live. Unchanged: the coverage module is `pub(crate)`, reaching a host only
  through the handle and the root re-exports.
- **10 gated items**, counted apart: the `testing` / `tooling` doors aren't the API.

It counts source, not rustdoc JSON, because that output is nightly-only and a check needing a second toolchain is a
check CI skips. So the count is coarse by design: it has to be stable and it has to MOVE when the surface does, which is
all a ceiling needs.

**Raising a ceiling is a design decision, not a build fix.** It needs David's explicit say-so, like a `file-length`
allowlist entry. Shrinking never fails. The same check asserts the other half of the boundary: neither `cmdr-index` nor
`cmdr-fs` may reach `tauri`, `tauri-specta`, or `cmdr`, verified against the `cargo metadata` graph so the check catches
a dependency that arrives through a helper crate rather than through the manifest.

## The three exceptions, named

**1. `store` is wholesale public, and `IndexStore` keeps 28 methods.** `search/` is a product surface that stays
app-side and runs its own SQL over an index database: it opens connections, walks `idx_parent`, and reads `meta`. That
is a schema dependency, not an API one, and pretending otherwise would mean a query facade with a case per question that
changes every time search's ranking does. Eleven of the 21 module-level items in `store/` were reachable for no reason
and are now `pub(crate)`; what's left is the vocabulary an outside reader genuinely uses. `store`'s items land in the
public-item bucket above rather than against the handle, which is what keeps the schema surface from making the handle's
number look worse than it is.

**2. `IndexError::Internal(Diagnostic)`.** The internals below the facade still report a formatted diagnostic for causes
no caller acts on (a poisoned registry lock, a database open failure). Every cause a caller CAN act on has its own
variant — `NotIndexed`, `NotConfigured`, `UnsupportedVolume` — and nothing matches on the text. Converting the residue
means typing the failures inside `lifecycle/state.rs` and `read/queries.rs`, which is a separate change with its own
risk; this is the honest interim, not the end state.

**3. `ReadPool::with_conn` returns `Result<T, String>`.** The one public signature that isn't typed. Its error is a
connection-open failure and every caller `.ok()`s it, so nothing branches on the text — but "typed errors everywhere"
holds at the boundary with this and `Internal(Diagnostic)` named, not with zero exceptions. (The schedulers'
`run_pass_blocking` had the same shape and turned out to have no consumer outside the crate at all; both are
`pub(crate)` now, which is the better answer when it's available.)

## The platform story: no `cfg` on the surface

`Index`'s signature is identical on every platform. The `cfg`s that used to sit on 14 root re-exports are gone:
`SmbIndexGateReason` needed none in the first place (the SMB transport module was never platform-gated, only its
re-export was), and the MTP and local-external routing that IS gated now lives inside method bodies, where a platform
without those transports simply falls through. `start_volume` reaches the share gate directly there, and
`on_device_object_changed` / `on_watch_gap(Device(..))` no-op.

**Why it matters beyond tidiness**: `#![deny(missing_docs)]` is a per-platform lint. A `cfg`-gated public item can be
documented on macOS and undocumented on Linux, and nothing on a Mac would ever say so. That is not hypothetical here —
`store::normalize_for_comparison` had exactly that shape, documented on its macOS arm and bare on the other, and this
milestone is where it surfaced.

## The gated surface

`indexing::testing` (`#[doc(hidden)] pub mod`, behind the `testing` feature) is the one door for test reach-through: the
fake host seams, the recording sink, the scan and writer entry points a real-backend test drives, the registry-slot
reservation, and the macOS disk-image fixture.

**Why a feature and not `#[cfg(test)]`.** `cfg(test)` is set only while a crate compiles its OWN test target. The moment
the index is a dependency, every `cfg(test)` item vanishes from its consumers' test builds — silently, at the worst
possible milestone. This trap has fired three times in this effort. `tempfile` joins the gated surface because
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

|               | public modules | public items |
| ------------- | -------------- | ------------ |
| `media_index` | 14 → **11**    | 142 → **51** |
| `importance`  | 8 → **3**      | 65 → **23**  |

### `media_index`

- **Five modules stop being public**: `backend`, `thermal`, `writer`, `writer_registry`, and `network::policy`, plus
  `clip::{backend, macos, tokenizer}`, `vector::cache`, and `network::enrich`. None had a consumer outside the
  subsystem. `clip::install` went from 18 public items to four (§ "The sixteen the compiler widened" below); the rest
  are how a model gets unzipped and checksummed, which is nobody's business but the installer's.
- **One fold, and it was hiding a break the move would have caused.** `commands/media_index/file_status.rs` reached
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
  gated bucket, alongside `testing`.
- **`importance::testing`** carries the four items two app-side tests need to stage a scored folder.
- **Five modules stop being public**: `store`, `writer`, `writer_registry`, `signals`, and `scorer`. The scoring
  vocabulary was already re-exported at the root, so only `ImportanceStoreError` needed a new home there.

### The sixteen the compiler widened

Making the index a crate turned every host-side reach into a compile error, and sixteen items that had been `pub(crate)`
or `#[cfg(test)]` still had one. Widening them was the mechanical answer; it is also exactly the failure this audit
exists to prevent, so each got the same four dispositions afterwards. Recorded here because a machine-checked ceiling
frozen around a mechanically-widened surface locks in the thing the audit was for.

**Folded onto the type the host already holds** (a method wearing a module path):

- **`media_index::scheduler::{start, kick_all_ready_passes_with, kick_network_pass}` ⇒
  `MediaScheduler::{start, kick_all_ready_passes, kick_network_pass}`**, and **`importance::scheduler::start` ⇒
  `ImportanceScheduler::start`.** The host already holds both schedulers; passing one back into a free function in the
  module it came from is the module path standing in for a receiver. `kick_all_ready_passes_with`'s `_with` suffix
  existed only to distinguish it from a global-lookup variant that no longer exists.
- **`media_index::store::{open_read_connection, read_status}` ⇒ `MediaIndex::status_for_paths`.** The host was opening
  the database, deriving its path, checking the file exists, and looping point lookups — four steps to ask "what does
  the index have stored for these paths?". `MediaIndex` is the subsystem's read handle and already knew all four.
- **`coverage::ensure_accounted_seeded` + `coverage::folder_coverage` ⇒ `coverage::folder_coverage(data_dir, …)`.**
  `folder_coverage`'s own doc used to say "the caller must have seeded the accounted aggregate first", which is a rule
  the index knows and the host has to remember. The seed is now inside, keyed off the `data_dir` the call already needs.
- **`coverage::importance_scored` ⇒ `ImportanceIndex::is_scored`.** A fact about the importance index that lived in
  media's coverage module because that's where the first caller was. Both callers now ask the index itself, and the
  "generation 0 does not mean unscored" rule sits next to the two probes it composes.
- **`clip::install`'s six ⇒ `state`, `downloads`, `ClipDownload::install`, `remove`.** The host was implementing the
  install policy: iterate the tower table, compare each pinned hash against a placeholder sentinel, build the zip path,
  verify, unpack, delete the archive. All of that is the index's. What genuinely belongs to a host is the HTTP transfer,
  so `downloads()` hands back exactly what to fetch and where to put it, and `ClipDownload::install()` takes it from
  there. The tower table, the checksums, the sentinel, and the model directory are `pub(crate)` again.

**Kept, and why:**

- **`ReadPool::{new, with_conn}`.** The direct-database read side, the exception § "The two exceptions, named" already
  covers. `with_conn` is how you use a `ReadPool` at all, and `new` is `search/` opening an OFFLINE volume's database
  that the lifecycle registry isn't holding. A pool you can hold but not read from would be worse than no pool.
- **`coverage::cached`.** The "never build" counterpart to the already-public `get_or_build`, and the distinction is
  load-bearing: a cold build is a whole-index walk, and running one from a poll is what once ballooned a launch to 50
  GB. Every poll and startup reader must have a call that can't do that.
- **`host::{config, events, policy, volumes}`.** The plugin interface. A host has to be able to install a sink, apply a
  config, and answer for volumes; that's what these modules are for.
- **`importance::{signal_availability, is_background_scored}`.** Pure policy over a volume kind, with no handle they'd
  belong to — putting importance policy on `IndexVolumeKind` would invert the dependency. Both now re-export at the
  `importance` root, so a host reads one place instead of two.
- **The five `#[cfg(test)]` reaches** (`one_of_every_kind`, the disk-image fixture, `ScanPacer::unpaced`,
  `IndexStore::list_children`, `handle::test_lock`) are on the `testing` surface. A gated test surface is a legitimate
  bucket, and the rule below is exactly why they can't stay `cfg(test)`.

### The rule the gates follow

**`#[cfg(test)]` when every consumer is inside the crate; a feature only when one lives outside.** Using a feature for
an in-crate consumer is not harmless: the app enables `testing` for every dev target, so the item exists in the non-test
lib build with nothing calling it, and `#[deny(unused)]` turns it into an error. That is how the four `ImportanceStore`
accessors and the two `MediaStore` ones landed on plain `#[cfg(test)]`.

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
  have no production sender — `rename_path`'s own doc says the rename-following hook it exists for isn't wired yet, so a
  rename still manifests as GC(old) + enrich(new). Same shape as `WriteMessage::PropagateDeltaById` on the drive index:
  a supported capability, not an accident.
- **`WriterRegistry::shutdown_all` has no caller, in either subsystem.** Its doc says "called on app teardown so the
  writer threads join", and no teardown path calls it. Not a data-loss gap (every write is flushed as it is applied, and
  each writer stops when its last handle drops), but the documented teardown does not happen. Wiring it is a
  shutdown-ordering decision, not a visibility one.
