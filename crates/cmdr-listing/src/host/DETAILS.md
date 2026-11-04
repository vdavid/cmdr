# The host seams: details

Read this before any non-trivial work on the seams: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows. Cmdr's answers, the composer, and the observer order: `apps/desktop/src-tauri/src/listing_host/DETAILS.md`.

## The shape

**Decision:** eight small traits, one question each, mirroring `cmdr-index`'s `indexing/host/`, never one fat
`ListingHost` trait. **Why:** each one degrades on its own and carries its own recording fake, so a test or a tool
supplies exactly the answers it cares about.

The host hands them to `Listings::builder(volumes, events)` (the two a real host can't do without, as arguments) and the
builder's optional setters; the engine keeps them as `Inner` fields. A seam is a `dyn` call, so the engine asks per read
or per mutation, and resolves an answer (the collator's locale, the volume for a `ListingPath`) BEFORE it takes the
cache lock. The seam files are `pub(crate)`; the host names a seam as `cmdr_listing::host::X`, and those re-exports are
the 20 items `index-crate-isolation` counts (`crates/cmdr-listing/DETAILS.md` § "The public surface is capped").

Two inputs aren't traits:

- **The runtime**, a `tokio::runtime::Handle` (`ListingsBuilder::runtime`), defaulting to `cmdr-fs`'s fallback runtime,
  as `VolumeHost` does. Every engine spawn goes there: a bare `tokio::spawn` inherits an ambient runtime, and a
  watcher's OS thread has none. The browser build has no `runtime()` to call; it always runs on the page's event loop
  (`../../DETAILS.md` § "The browser build").
- **`ScratchVisibility`**, engine STATE the host sets (`Listings::set_scratch_visibility`) and reads back to flip one of
  its two flags. Each operation reads it once, before the cache lock, and passes it down as a `Copy` value.

## The eight seams

Each entry: the question, what the engine does with the answer, the no-op, and Cmdr's adapter.

- **`ListingVolumes`: which volume serves a path.** `get` (no routing, sync), `resolve` (async, routes archive and
  `.git` portal paths), `resolve_local_only` (sync, for the fresh-listing oracle), and `unregistered(volume_id, path)`,
  which answers `Adopted(volume)`, `NotConnected`, or `Gone`. `unregistered` folds the mount adoption (with its 2 s
  deadline, host-side) and "why is nothing registered" into one question, so the engine can't adopt without asking why
  or the reverse. No-op `NoVolumes`: nothing registered, every id `Gone`. Cmdr: `AppVolumes` over the `VolumeManager`.
  Not the index's `VolumeProvider`: that one answers mount identity for scanning, this one routing for reading, and
  sharing a trait would couple two crates through methods neither needs.
- **`ListingEvents`: what happened to a listing.** `opening`, `stalled(StalledOn)`, `progress`, `read_complete`,
  `finished(ListingEnd)`, `diff`, `respelled`, each taking the `&ListingId` first. `ListingEnd` is `Complete` (total,
  volume root, stored spelling as a `PathBuf`, empty-at-volume-root), `Cancelled`, or `Failed` with the `VolumeError`,
  each carrying the volume id and path. A read reports its own complete or cancel; a refused read is reported by its
  task after the cancel token is gone. No-op `NoEvents`: drops everything. Cmdr: `TauriListingEvents`, which owns every
  wire struct and the policy on them, down to formatting a path as the wire's string.
- **`ListingIndex`: what the drive index knows.** `enrich(volume_id, &mut [FileEntry])` and
  `verify_directory(volume_id, path)`. ❗ `enrich` writes only the seven `recursive_*` fields and only on non-symlink
  folders (verified in `cmdr_index`'s `enrich_entries_with_index_on_volume` / `apply_dir_stats`, 2026-10-11 on
  `7be64c4ff`); the off-lock refresh clones exactly those rows (`crates/cmdr-listing/DETAILS.md` § "Index sizes off the
  lock"). The index's own change intake is NOT here: it's a different subsystem hearing the same change, so the host
  composes it around `Listings::directory_changed`. No-op `NoIndex`. Cmdr: `AppListingIndex` over `index_host::index()`,
  resolved per call.
- **`ListingOverlay`: rows a pane sees that no volume holds.** A list, in registration order. § "Overlays".
- **`ListingObserver`: what keeps something alive while a pane shows a directory.** A list, in registration order. §
  "Observers".
- **`ForegroundLeases`: the user is waiting on this volume.** `hold(volume_id) -> ForegroundHold`, a `#[must_use]` RAII
  box the streaming read keeps inside its `ReadSession` for the whole read, so nothing can bind it to `_`. No-op
  `NoLeases`: a hold that holds nothing. Cmdr: `AppLeases` over `priority::foreground`.
- **`ReadingLocale`: which language names collate in.** `locale()`, pulled once per sort, so an OS language switch
  reaches the next sort; the engine caches one collator per tag. No-op `NoLocale`: `"en"`. Cmdr: `AppLocale` over
  `intl::active_locale`.
- **`MountTable`: what a filesystem path is mounted on**, for a stalled listing's wording. `mount_of(path)`, a lexical
  lookup ❌ never a `statfs` (it blocks on the mount that stalled). The engine asks lexically, and only when the answer
  was a local disk resolves symlinks on the blocking pool under a 500 ms bound and asks again. The network-type
  allowlist and the backend-kind mapping stay in the engine (`stalled_on.rs`). No-op `NoMountTable`:
  `StalledOn::Unknown`. Cmdr: `getfsstat(MNT_NOWAIT)` on macOS, `/proc/mounts` on Linux.

## Overlays

A `ListingOverlay` contributes extra `FileEntry`s to one directory's listing, folded in by the read pipeline
(`pipeline.rs`) between the enrich and the sort, on every read that builds or replaces a listing. Cmdr has two
contributors: the virtual `.git` portal, which puts the six category rows (`branches/`, `tags/`, …) into a repo's
`.git/` listing, and restic, which puts one `Browse snapshots` row into a backup repo's folder. Every real entry beside
them stays the volume's.

**Decision:** ❌ never move this into a `Volume` impl or the volume manager. **Why:** a contributed row must reach a
pane and NOTHING else. A copy scan, a delete walker, and the drive indexer all list through `Volume`, and the moment one
of them sees a row with no inode behind it, a repo delete stops half-way with `.git/` still on disk. That was a real
bug; this seam is the shape that makes it unrepresentable. The same rule is why a listing carrying contributed rows
never answers the fresh-listing oracle (`authoritative.rs`, `CachedListing::has_overlay_rows`).

`applies_to(volume, path, entries)` is cheap and zero-I/O: it runs on every listing in the app, and on a backend's
synchronous `Replaced` hand-over, which can't wait for the blocking gather (`crates/cmdr-listing/DETAILS.md` § "The
overlay step"). `extra_entries` runs only when it answered `true`, on the blocking pool. The first overlay per `id`
wins.

`entries` are the volume's own rows (enriched, before any contributor's), which both callers already hold, so a
contributor can recognize a directory by what it holds without a `stat`. **A contributor that scans them caps itself**
(answer `false` past a small count), so a 100k-entry folder costs one length check. The git portal ignores them (it
decides from the path); restic recognizes a repo by its five names.

`extra_entries` also gets the `volume_id` the listing was read under. A contributor whose route confirms from a memo
(restic) keys what it learns by the same id a later `resolve` of a path below sees, so entering its row costs no
confirmation read.

**Gotcha:** an incremental watcher change (`Added`, `Removed`, `Modified`) doesn't re-run the overlays; only a full
re-read does. So a directory that BECOMES one a contributor claims (someone creates the marker file in a folder a pane
is showing) gets its row on the next navigation or refresh, ❌ not live. **Why:** accepted rather than hooked: re-asking
every overlay on every incremental change costs per-change work for a case that happens once per directory's life.

## Observers

A `ListingObserver` hears `listing_opened(&OpenedListing)` once a listing enters the cache (after the streaming read's
respell kick-off) and `listing_closed(&ListingId)` once it leaves, in registration order, outside every engine lock.
`OpenedListing` carries the facts an observer needs (id, volume id, the volume that served the read, the path as read,
the stored spelling), so no observer reads the cache back, which in a test would reach the wrong engine. Its
`still_open()` asks the engine that opened the listing: a close can race the open and reach every observer first.

❗ The close comes AFTER the cache removal: an observer's detached arm reconciles against cache membership, so a release
while the entry is still there lets a racing arm re-take what the close gave back. Both calls sit on the open and close
paths, so an observer does cheap, non-blocking work and detaches the rest.

**The serving volume comes first.** Before any observer hears an open, the engine asks the volume that served the read
for `Volume::watch_listing(path)` and holds the answer (a `ListingWatch`) in `Inner::volume_watches` until the close,
which drops it before the observers hear the close. It's for a backend whose watch arms per shown directory rather than
per volume (the SMB backend's DFS link watch, `crates/cmdr-smb/DETAILS.md` § "A DFS link inside the share"). Built into
the engine rather than an app observer because every host needs it the same way, and it's keyed by the volume the read
used, which only the engine has in hand. A close racing the open is caught the same way as an observer's (`still_open()`
after the insert). Pinned by `lifecycle_order_test.rs::a_volume_keeps_its_listing_watch_until_the_listing_closes`.

**Decision:** arming a watch belongs to the BACKEND side of the engine, ❌ never to a frontend subscription. **Why:** a
breadcrumb chip's subscription was once the only thing starting a repo's watcher, so a pane on `branches/` with nothing
else open went stale, and turning both git features off stopped every watcher in the app. An observer holds for a pane
the MCP server drove, a window that never rendered a chip, and whatever asks next. The FSEvents change watch is an
observer for the same reason: "arm a watch" and "tell the subsystems" are one ordered list.

## Testing fakes

`crates/cmdr-listing/src/testing/` (the `testing` feature): `FakeVolumes`, `RecordingEvents`, `RecordingLeases`,
`FixedLocale`, `FakeMountTable`, `StaticOverlay`, and `RecordingObserver` (it records the cache membership it saw at
each call). `index_sizes_test.rs` keeps its own scripted index, the one test that needs `enrich` to block on a gate.
