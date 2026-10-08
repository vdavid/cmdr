# Listing index sizes: details

## Flow

1. The drive index commits a batch and emits `IndexEvent::DirsUpdated { paths }` through its event sink.
2. `events/index_mapping.rs` routes it here (`dirs_updated`) instead of to the frontend, and reports
   `Destination::ListingIndexSizes`.
3. The worker matches the batch against every open listing (`touched`) and merges what it touched into that listing's
   pending set.
4. A listing refreshes when its pending set is non-empty, its last refresh is at least `COOLDOWN` (2 s) ago, and the main
   window is visible. A listing inside its cooldown gets a deadline, so the last change always lands (leading plus
   trailing).
   A listing also rereads the rows a reading said would flip their hourglass on their own (`schedule.rs` rechecks), at
   that moment and outside the cooldown.
5. The refresh (on the blocking pool) reads the touched rows' `dir_stats` plus the listing's own, keeps the rows whose
   `RowSizes` moved, writes them into `LISTING_CACHE`, and emits `listing-index-sizes-changed` with their readings and,
   when it moved, the listing's own reading (the `..` row). Nothing moved: nothing is sent.
6. The frontend (`src/lib/file-explorer/pane/index-events.ts` → `FilePane.applyIndexSizes`) writes the readings onto
   its cached rows and the cursor entry in place, re-reads only the status-bar totals, and pushes the MCP pane state.

A whole-volume batch (`Touched::Whole`) skips the per-row comparison: it runs the full `refresh_listing_index_sizes`
re-enrich and sends `full: true`, and the pane re-reads its window's sizes the old way. It happens once per scan.

The open-listing set is kept by a `ListingLifecycle` observer (`crate::listing_lifecycle`), registered from setup via
`start`. It reads the listing's volume id and path off the `CachedListing` record, which the open path inserts before it
notifies.

## The batch shapes

- **A live batch**: each changed directory plus its whole ancestor chain up to `/`
  (`cmdr-index` `paths::path_prefix::with_ancestor_closure`). About once a second on a busy disk.
- **A network or phone watcher's batch**: only the changed directory's parent, no chain (`transports/smb/watch.rs`,
  `transports/mtp/watch.rs`). `touched` finds the child row on the way down all the same.
- **Whole-volume moments**: `["/"]` after a full scan completes or a replay overflows, `[volume_id]` after a network
  scan completes, `[volume_root]` after a phased first index completes. The last one is handled by the frontend's
  `index-aggregation-complete` refresh, which fires in the same breath.

## Decisions

**Decision**: the backend decides which listings and rows an update touched, and whether anything shown moved. **Why**:
the frontend used to get every batch and match paths itself. Its `/` short-circuit fired on every live batch (they all
end in `/`), so both panes refreshed on every write anywhere on the disk, each refresh six IPC calls whether or not a
number moved: about 110 refreshes per three idle minutes on a pane on `~`, which made the WebContent process idle at
~1.4% of a core instead of ~0.2% (measured 2026-09-23, dev build, issue #92). The backend already knows every open
listing, and it has to read the stats anyway, so it's also where "did anything change" is cheapest to answer.

**Decision**: compare raw readings, not formatted text. **Why**: the size text depends on settings and locale the
webview owns (units, separators), and the tooltip shows exact counts. A raw change that formats the same still reaches
the pane, where Svelte leaves the DOM alone because the text is equal, and the Full list holds the Size column's width
(`views/measure-column-widths.ts::holdSizeColumnWidth`).

**Decision**: hold everything while the main window is hidden and catch up on show. **Why**: nobody reads a hidden
pane, and WebKit queues the DOM work anyway (the diagnosis saw transitions piling up in a hidden page). The catch-up is
one refresh per listing with every batch merged. The cost: the listing cache's sizes (and so MCP's) are stale while the
window is hidden, by at most what changed in that time.

**Decision**: the hourglass's two-second delay lives in the index, and this worker only follows it. **Why**: the webview
also reads `DirStats` directly (a window fill, the cursor entry), and so does the agent's `list_dir`. A delay kept here
would leave those readers showing the raw flag, and the `lit` set would disagree with what a row on screen wears. The
index answers every reader the same, and tells this worker when a row flips (`recursive_size_pending_changes_in`, plus a
`DirsUpdated` for the folders whose shown hourglass a drain ended). A recheck runs at its moment, outside the cooldown:
waiting would show the hourglass up to two seconds late, or never for a short one the webview already read. It rereads
only the flipping rows and sends only what moved, so under churn it costs an index read, not an event.

## Calculating sizes on demand (`count/`)

Total Commander's ⌥⇧⏎ ("count the space subfolders occupy") and Space on a folder. The index answers the volumes it
runs on; `count/` walks what it can't (SFTP, WebDAV, S3, archives, an excluded folder, any volume while indexing is off).

- **What the user sees, and when** (`count/mod.rs`): a folder being walked shows its running total with the hourglass
  from 200 ms in (`HOURGLASS_DELAY`) and every 250 ms after; a folder waiting its turn shows the hourglass too, from 200
  ms after it was queued (a size it already had stays, an unknown one reads `<dir>`); a finished folder its exact size,
  or a lower bound (`≥`) when parts couldn't be read. A count done within 200 ms never blinks an hourglass. A ticker
  (50 ms) beside the walk does all the sending, so a walk stuck in one `readdir` still lights its row and still stops
  waiting on Esc. The UI finishing does not imply that backend I/O has terminated.
- **Measuring** (`count/measure.rs`): on a volume with a local path (the Mac's disks, shares mounted under `/Volumes`)
  our own `walkdir` walk on a blocking thread, bumping the running totals per entry and asking the stop per entry. An
  unreadable subfolder is SKIPPED and counted, and the folder's size becomes a lower bound: `~/Library` gets a number
  instead of nothing. ❌ Not the copy scan there: it reports only when the folder is done, and a copy rightly refuses a
  folder it can't fully read, where a size wants all it can see. Elsewhere `Volume::scan_for_copy_batch_with_boundary`
  through `ScanBoundary`, which bumps the totals per file and folder; the volume is `manager.resolve`d like a copy source,
  so an archive or `.git` route walks its own tree.
- **Requests queue, one job per listing** (`count/jobs.rs`). The first request owns the job and walks the queue until it
  drains; a request arriving meanwhile appends its folders (skipping ones queued or being walked) and resolves when the
  job ends, with its outcome. So three Spaces in a row count three folders, in order. The job is registered before
  volume resolution starts, so Esc also finds a request still resolving an archive or `.git` route. Esc raises its
  stop: the folder being walked keeps its running total as a lower bound (or goes back to what it showed if it
  never got to show one), and the waiting rows whose hourglass went up go back to what they showed. Closing the listing
  cancels its job (`listing_closed`), and a job that finds its listing gone stops itself. The owner retains the registry
  slot through its final publications. A retry arriving during finalization waits for it, then rereads the requested
  rows (including the exact-size skip) before starting a new job.
- **Backend lifetime is separate from publication ownership** (`count/measure.rs`): a remote scan owns its volume,
  stop signal, and private running totals in a spawned task. Cancellation detaches its join handle, not its backend
  future: an in-flight protocol operation must finish before the scan consults `ScanStop` cooperatively. Only the UI
  owner publishes; a detached worker cannot overwrite a replacement's readings. Local walks use `spawn_blocking`
  directly, retaining no async waiter after cancellation. A hung syscall cannot be interrupted; its blocking worker
  holds only the path, stop signal, and private totals until it returns. Volume resolution likewise owns its in-flight
  future independently of the UI wait and never starts a late walk after cancellation.
- **Readings travel the index's road** (`count/publish.rs`): written into `LISTING_CACHE` (`update_index_sizes_by_path`),
  then sent as `listing-index-sizes-changed`; restoring a row sends a `full` event, since "no size" isn't a reading an
  event can carry. Restoration restores manual ownership as well as sizes, so an unreadable or quickly cancelled retry
  keeps an existing manual lower bound retryable on indexed volumes. The frontend has no new row path.
- **Decision: on a volume the index RUNS on, the index keeps its rows.** "Runs" is `volume_status(..).enabled`: ❌ never
  `volume_ids()`, which still lists an index that is shutting down or failed, and then nothing completed its stale lower
  bounds (the review's "sometimes nothing got counted" with indexing just turned off). Only a folder the index says
  nothing about is walked, plus one whose size a count left (a stopped count's lower bound is ours, tracked per listing
  in `jobs::MANUAL`, so it stays retryable). Why: the worker re-sends its own readings for rows it covers, and two
  writers on one row overwrite each other's size and hourglass. Elsewhere every folder without an exact size is walked.
  Space counts its folder only when its exact size isn't known, the same test the frontend makes (`shouldCountOnSpace`).
- **Honest endings**: `FolderSizeCountOutcome` says how many were counted, how many couldn't be read (wholly: the row
  goes back to what it showed; in part: a lower bound), and whether it was stopped; the frontend toasts the unreadable
  count. `folder_sizes` log lines record each start (folders, volume, local walk or volume scan), each join, each folder
  it couldn't read, and each end (counted, unreadable, stopped, elapsed).
- **Not persisted**: a refresh that re-reads the folder drops the readings, as Total Commander's does.
