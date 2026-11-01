# Listing host: details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows. What each seam asks: `crates/cmdr-listing/src/host/DETAILS.md`; the engine itself:
`crates/cmdr-listing/DETAILS.md`.

## The one engine

`install(app)` runs first thing in `lib.rs`'s `setup()`, before `volume_host::install` and `index_host::install`, and
stores the engine in a `OnceLock`. `listings()` hands it out. What it builds:

- `Listings::builder(AppVolumes, TauriListingEvents)` plus `AppListingIndex`, `AppLeases`, `AppLocale`, and
  `AppMountTable` (the shared `builder` helper, so the test engines carry the same answers).
- The runtime: Tauri's own tokio handle, so listings share one thread pool with the app and a watcher's OS thread (no
  reactor) can still start a refresh.
- The overlay: the git portal's `.git/` category rows (`file_system/git/overlay.rs`).
- The observers, in order (§ "The observers").

`setup()` also calls `listings().start_orphan_reaper()` and seeds the scratch visibility from settings; the two
settings commands (`commands/settings.rs`) read `scratch_visibility()`, flip one flag, and write it back.

**A read before `install` is loud.** `index_host::index()` uses `get_or_init`, so an early read would win the slot
forever; for listings that would be an engine whose events go nowhere and every pane stuck on "loading". So outside
tests an early read logs once (`log_error!`) and answers from a SEPARATE detached engine, leaving the slot free, and
`install` reports that early events were dropped. Only the test binary, where nothing installs, builds its engine on
first read (`test_listings`).

## The adapters

Each is a unit struct that resolves the subsystem it answers from per call (never at construction: `install` runs
before `index_host::install`, and capturing `index()` would win its slot with an unconfigured index).

- `volumes.rs`, `AppVolumes`: the `VolumeManager`'s `get` / `resolve` / `resolve_local_only`. `unregistered` first
  tries to adopt the mount under the path when it derives exactly that id (a cloud drive mounted into the home folder
  posts no `NSWorkspace` notification, and the startup sweep can lose the race to a restored tab), bounded at 2 s, then
  asks `unregistered_volumes::why_unregistered`.
- `events.rs`, `TauriListingEvents`: the only place the engine's facts become wire events, and every policy on them.
  `finished(Failed)` classifies the error into the typed `listing-error` (with the damaged-archive, password, and
  cloud-provider readings), computes the PII-free analytics category, records into the MCP recent-errors ring
  (`ui_state/DETAILS.md`), records a TCC denial, and asks `note_root_failure` whether the mount went stale.
  `finished(Complete)` adds the TCC restricted-empty-root hint and clears the path from the restricted set, as a cancel
  does. Split in two so a test drives the whole policy without an `AppHandle`: payloads go through a `ListingWire`
  (the `AppHandle` in the app, `WireCollector` in tests), bookkeeping through `ListingEndEffects`. The wire structs
  (`listing-progress`, `-complete`, `-error`, `-cancelled`, `-read-complete`, `-opening`, `-stalled`,
  `directory-diff`, `listing-respelled`) keep their names and `tauri_specta` derives, so `bindings.ts` doesn't move.
- `index.rs`, `AppListingIndex`: `index_host::index()`'s `enrich` and `verify_directory`.
- `leases.rs`, `AppLeases`: `priority::foreground::lease_foreground_on`, boxed as a `ForegroundHold`.
- `locale.rs`, `AppLocale`: `intl::active_locale`.
- `mounts.rs`, `AppMountTable`: the `getfsstat(MNT_NOWAIT)` snapshot on macOS (`volumes::mount_type_and_source_for`),
  `/proc/mounts` on Linux (`linux_mounts::mount_entry_for_path`).
- `backends.rs`, `AppListings`: the backends' `cmdr_fs` `ListingHost` (installed by `volume_host::install`). The
  oracle, the open-listing question, and the archive refresh are the engine's answers; `directory_changed` goes through
  the composer, so the index and badges hear a backend's change before the panes do.

## The composer

`directory_changed(volume_id, parent_path, change)` is where every directory change enters: `AppListings` (every
backend's `notify_mutation` and watcher), git wiring, the local backend's mutation patch (deletes, renames, and Cmdr's
own creates), and the rename command. It tells each subsystem that keeps a copy, in this order:

1. **The drive index** (`index_host::index().apply_directory_change`), FIRST, before any pane work. The SMB watcher runs
   for the whole volume's lifetime, not just while a pane shows the share, so the index must update with no listing
   open; and writing it before the panes' enrich means the enrich (and the `DirsUpdated` the writer emits) see the
   just-written sizes. A no-op for `root` and any non-indexed volume.
2. **The cloud badge** (`sync_status::invalidate_dir`, macOS), which caches per directory. One hash lookup.
3. **The restic snapshot watch** (`file_system::restic::snapshot_watch::directory_changed`): a change in a watched
   repo's `snapshots/` re-lists its `Browse snapshots` panes, which is how a direct SMB share's watcher shows a new
   backup. Returns at once while no such pane is open (`file_system/restic/DETAILS.md` § "A new backup reaches an open
   pane").
4. **The panes** (`Listings::directory_changed`).

The coupling is one-directional: listing → indexer, never the reverse. The archive content watch's
`refresh_archive_listings` deliberately skips the composer: an archive-inner path isn't a real filesystem path
(`crates/cmdr-archive/src/watch/DETAILS.md`).

## The observers

Registered in this order by `install` (the list is `observers()` in `mod.rs`, pinned by
`adapters_test.rs::the_app_installs_its_observers_watch_then_git_portal_then_restic_then_index_sizes`), and called in it on every
open and close (after the cache removal on close):

1. **The FSEvents change watch** (`file_system::watcher::observer()`): arms through `start_watching_detached` only when
   `Volume::can_watch_listings`, and disarms in the close call, so an event landing between the removal and the disarm
   finds no listing to patch. Why the arm is detached: `file_system/DETAILS.md` § "Arming a listing watch is detached".
2. **Git arming** (`file_system::git::arming::observer()`): keeps a repo's `.git/*` watcher armed while a pane shows one
   of its virtual listings, which FSEvents can't watch (`file_system/git/DETAILS.md` § "Who arms the repo watcher").
3. **The restic snapshot watch** (`file_system::restic::snapshot_watch::observer()`): keeps a repo's real `snapshots/`
   watched while a pane shows its `Browse snapshots` root (`file_system/restic/DETAILS.md` § "A new backup reaches an
   open pane").
4. **Index sizes** (`listing_index_sizes::observer()`): routes the index's folder-size updates to the listings they
   touch, keyed on `OpenedListing`'s volume id and stored spelling, checking `still_open()` before keeping a record.

## Tests

`tests/` holds the app's half of the engine's tests: each adapter against the real subsystem it answers from
(`adapters_test.rs`, plus the locale and mount-table cells in `locale.rs` / `mounts.rs`), and the cells whose point is
an app piece: the events policy's reaction matrix (`streaming_reaction_test.rs`), the FSEvents watch going with an ended
listing (`caching_reaper_test.rs`) and its disarm order (`lifecycle_order_test.rs`), archive and SMB integration, the
MTP spelling, the git overlay's refresh, and the SMB-upgrade respell. The engine's own cells build an engine of their
own in `crates/cmdr-listing/`.

The test binary shares ONE engine, `test_listings()`: events dropped, the fallback runtime, every app answer (so tests
list through the real registry, index, lease, language, and mount table), the git overlay, the change watch, git
arming, and restic snapshot watch observers, and a lifecycle probe last (`tests/support.rs`). Index sizes stay out, since its worker needs an
`AppHandle`. Because it's shared, cells keep unique ids (`TestListing` derives one from `cmdr_fs::testing::unique_test_id`)
and `TestListingGuard` ends its listing on drop. `engine_reporting_to(events)` builds an app-wired engine of a cell's
own, with no observers, for a cell that reads what the engine reported.
