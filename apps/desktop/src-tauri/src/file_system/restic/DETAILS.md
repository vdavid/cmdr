# The app's restic wiring: details

Pull-tier docs for `file_system/restic/`. Must-knows live in `CLAUDE.md`. Everything that reads a repository (the
storage adapter, unlock, snapshots, trees, the hub's memo) is `crates/cmdr-restic/DETAILS.md`; this doesn't restate it.

## Two seams, no hooks

The same split as the git portal (`file_system/git/DETAILS.md`), for the same reason: a repo folder is a real folder
whose real entries must stay editable and deletable, so the view can't be a volume over it.

- **The route.** `VolumeManager::resolve` sends `<repo>/Browse snapshots` and everything below it to a
  `cmdr_restic::ResticVolume` once the hub confirms `<repo>` is a repo (`apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing.rs`). The path
  stays the caller's; the view classifies it. The view is read-only by type, unwatchable, and `routes_over_a_parent`,
  so `mount_id_for_path` names the drive holding the repo.
- **The overlay.** `ResticOverlay` adds the one row to a pane's listing of a folder whose ENTRIES look like a repo
  (`is_restic_repo`: a `config` file plus `data`, `index`, `keys`, `snapshots`), on any volume a route didn't mint.
  Unlike git's local-only rule, a repo on SMB, SFTP, or a phone qualifies, since the crate reads it through `Volume`.
  A listing past `MAX_REPO_ROOT_ENTRIES` (32) answers on its length alone, so a 100k-entry folder costs one compare.
- **The overlay seeds the route.** Building the row confirms the repo in the hub's memo, keyed by the volume id the
  listing was read under (`ListingOverlay::extra_entries` carries it for exactly this), so entering the row costs no
  confirm listing. A typed path or a restored session pays one bounded listing of the repo folder, remembered either
  way.
- An incremental watcher `Added` doesn't re-run overlays, so a repo created by `restic init` under an open pane shows
  its row on the next full read (navigation or refresh). Acceptable; no hook for it.

## A new backup reaches an open pane

The view answers `can_watch_listings` false (its paths are on no disk), so the listing watch never arms on it.
`snapshot_watch.rs` watches the repo's REAL `snapshots/` on the parent instead, and re-lists every open pane on the
view's root when a file there appears or goes (#626). restic writes a backup's snapshot file last, after its packs and
index files, so by the time the folder shows, everything it names is on the parent.

- **Arming follows the listing**, the git portal's shape (`file_system/git/DETAILS.md` § "Who arms the repo watcher"):
  the observer takes a hold on `(volume id, repo folder)` when a listing of `<repo>/Browse snapshots` opens and gives it
  back on close; the last hold drops the watch. Only the root arms (`repo_a_listing_watches`: a routed view's path, the
  repo the hub's memo confirmed, zero I/O): a snapshot's tree never changes. A locked repo's root never lands in the
  listing cache, so it arms only once the unlock re-lists it. The stream arms on the blocking pool and is kept only if
  the same watch (its `epoch`) is still there, so a pane leaving mid-arm leaks nothing.
- **Which parents update live** (verified in code, 2026-10-21):
  - The local disk, and an OS-mounted share, through a `notify` stream of ours on `<repo>/snapshots` (non-recursive,
    300 ms debounce, `Access` events ignored so our own read of the new file can't loop on Linux). On an OS-mounted
    share FSEvents sees only this Mac's writes: a backup another machine makes shows on the next refresh.
  - A direct SMB share, through its own share-wide CHANGE_NOTIFY watcher, which reports every change to the composer
    (`listing_host::directory_changed`); the composer hands it here with the change. Only a change whose folder IS a
    watched repo's `snapshots/` counts, plus a `FullRefresh` (the watcher's overflow) on it or a folder above it. ❌
    Never widen that to "any change above": the share reports restic's own `data/` and `index/` writes and every save
    beside the repo, and each would re-read `snapshots/`, which is polling at network speed. No stream of ours.
  - Everything else (SFTP, WebDAV, S3, ADB, MTP) has no watcher reporting arbitrary folders, so its pane shows a new
    backup on the next manual refresh. ❌ No polling.
- **The re-list is the manual refresh**: `Listings::refresh` on each open root listing, which re-reads `snapshots/` in
  the hub (`crates/cmdr-restic/DETAILS.md` § "The view"). It starts 300 ms after the first change, so a burst (restic's
  temp file and its rename, a `forget` of many) is one re-list, on either source. One re-list at a time per repo; a
  change during one runs one more after it. The task belongs to its watch's `epoch` and stops once that watch goes, so a
  pane closing and another opening mid re-list never shares one task's bookkeeping.
- **`latest` follows the hub's rule, untouched here** (`crates/cmdr-restic/DETAILS.md` § "`latest`"): a re-list moves it
  to the new backup only when nothing has read under `latest/` for 30 s. During a copy out of `latest` the new dated
  folder appears and `latest` stays put. It then moves on the next root re-list after the copy goes quiet: the next
  backup, or a manual refresh. A later reader of the new snapshot reloads the index on its own (§ "Coverage and
  reloads" there).
- Cells: `snapshot_watch_tests.rs` (a scripted event re-lists the root with the new folder, the same event during a read
  under `latest` leaves `latest` alone, two panes share one watch that goes with the last, the composer's report
  re-lists, only a change in `snapshots/` or an overflow above it counts, a burst is one re-list, and which listings
  arm).

## The route's order

The leftmost CONFIRMED boundary wins across git, restic, and archive routes; the rule and its why live in
`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Resolving a path: the three routes". Cells:
`apps/desktop/src-tauri/src/file_system/volume/manager/restic_routing_tests.rs`.

- **A non-repo passes through.** A real folder named `Browse snapshots` outside a repo resolves to the parent, which is
  stricter than git (its route answers `NotFound` for a `.git` that isn't a repo). Restic can afford the confirm
  because it's memoized; git's is free because it's lexical.
- **A view is never a restic parent**: the overlay never applies on a routed volume, so a repo backed up inside a
  snapshot (or zipped in an archive) stays bytes in it, on both seams.

## A replaced parent gets a new view

An SMB share reconnecting (or an OS mount upgraded to a direct session) replaces the parent under the same volume id.
`register_if_absent` would keep a view minted over the retired parent, reading through it forever, so the restic route
remembers which parent minted each view (`ResticRoutes::parents`, a `Weak`, compared by address) and re-mints over the
current one, swapping it in place with `register`. The hub re-opens a repo on a changed parent on its own
(`crates/cmdr-restic/DETAILS.md` § "The hub"), and keeps the password.

## The routing LRU bounds registrations only

Four views stay registered (`RESTIC_LRU_CAP`), separate from the archive and git LRUs. A view is cheap: the hub holds
the passwords, opened repos, and indexes, so eviction never drops an index or forgets a password.

## Write guards

`VolumeManager`'s callers (copy, move, delete, trash, rename, create, bulk rename, rename-as-move) refuse through
`resolve` plus `RoutedKind::takes_writes` / `can_give_up_files`: a restic route takes no writes, and a move out of one
is refused before anything is copied, like git's. When the confirm can't decide (the repo folder's listing failed or
outran its deadline), `resolve` answers an UNDECIDED passthrough and those write gates refuse on it too
(`file_system/volume/DETAILS.md` § "Resolving a path: the three routes"). Two seams bypass `resolve` and ask here, each CONFIRMED (❌ never the
permissive path predicate, which would refuse a copy into a real folder named `Browse snapshots`):

- **The local write fast path** (`commands/file_system/write_ops.rs` `reject_if_routed_over_a_parent`) has paths with
  no volume id, so `local_path_is_in_a_restic_view` asks the memo for the boot volume and otherwise reads the candidate
  folder with `std::fs`, giving up past 32 entries. It's the write path, not the listing hot path, and an unconfirmed
  miss that wrote would put a real `Browse snapshots` folder inside someone's repo.
- **The agent's rename plan** (`tools/propose/rename/plan.rs`) must never probe a mount, so
  `memo_says_in_a_restic_view` answers from the memo alone, under the focused pane's volume id. A pane only stands
  inside a view after the route confirmed its repo.

## What each walker sees

`walker_exposure_tests.rs` pins the view's blast radius; the answer for every non-pane walker is "nothing":

- A volume listing of the repo carries no row; a pane's decorated listing does.
- The delete walker lists through `Volume`, never meets the row, and removes the repo completely; the repo's real files
  stay fully mutable.
- The copy scan of the repo folder counts what's on disk.
- The fresh-listing oracle declines a decorated repo listing, and an unrouted path under a `Browse snapshots`
  component (`crates/cmdr-listing/src/authoritative.rs`).
- The drive index's `Volume`-trait walk of a share holding a repo never records the view. The local walker reads the
  disk, where the row doesn't exist.
- On-demand folder sizes (⌥⇧⏎) plan the pane's folder rows, which include the row: the plan leaves out a row a route
  serves when its listing's folder isn't routed (`listing_index_sizes/count/mod.rs` `is_a_view_row`, git's category
  rows included). Space on the row is the person asking and counts through the view.

Selecting the row and copying it is a legal, enormous restore through the routed volume; its pre-scan stays
cancelable, and nothing blocks it.

## Reading out of a snapshot: copy, F3, and the agent

Each rides the route, with no restic-specific code on the way. Cells on a real repo, backed up from a real folder,
written onto a local drive registered in the app's registry (`test_repo.rs`, whose `ReadGate` can hold a reader inside
an `index/` or `data/` read):

- **Copy-out** (`apps/desktop/src-tauri/src/operations_host/restic_copy_out_tests.rs`): a file and a folder land byte
  for byte with their dates and permission bits; a move out is refused before a byte is written. A symlink lands as
  the link, verbatim, where the destination holds one (nested or selected), and is one skipped item otherwise, as a fifo
  always is (`VolumeError::NoContent`). A Cancel during a whole-snapshot pre-scan lands inside the listing that waits on
  the index load (the view opts into `ScanSource::abandons_reads_on_stop`).
  A repo whose password the hub doesn't hold (remember off, then a restart) answers the copy with
  `WriteOperationError::ArchiveNeedsPassword`, the prompt the unlock dialog's repo variant answers. Hub eviction keeps
  the password, so it never prompts.
- **F3** (`apps/desktop/src-tauri/src/file_viewer/session_test.rs`): materializes the file through the view's stream into the bounded temp, like
  a file in a git snapshot; a restic read failure is a plain I/O error.
- **`inspect_file`** (`apps/desktop/src-tauri/src/tools/read/inspect/restic_snapshot_tests.rs`): the same temp, a text window, and the temp
  gone afterwards. No new kind of content leaves.
