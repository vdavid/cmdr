# `cmdr-restic` details

The spec and the research behind it: GitHub issue #422. This crate holds the storage adapter, unlock, the snapshot list,
detection, and the read-only view (`ResticVolume` over the shared `ResticRepos` hub). The host route, the listing
overlay, the unlock UI, and copy-out live in the app.

Written against `rustic_core` 0.13.0 (default features off). The issue was researched on 0.12.0; what that source says
and what moved is in § "rustic_core 0.13.0 facts". The guardrails the whole feature keeps, app side included: §
"Invariants". An adversarial review against them, and each finding's fix: `docs/notes/restic/conformance-review.md`.

## Decisions

- **The row's name is `Browse snapshots`, in every locale.** **Why:** it's a PATH component (history, favorites, MCP,
  and the agent address paths), so a translated one would move every path when the language changes, like git's
  `branches`. A display name over an English path would need a field `FileEntry` doesn't have, touching sorting, quick
  search, type-to-jump, and rename for one row. Everything else the feature says (the dialog, errors, progress) goes
  through the i18n catalog. It sorts first among a repo's lowercase folders and can't clash with the real `snapshots/`.
  The name and the lexical split live in `cmdr_fs::restic_view`.
- **No settings toggle** (git has `showVirtualGitPortal`). **Why:** having a repo is the opt-in. Add one only if someone
  asks.
- **The hub owns the heavy state, not the volume** (§ "The hub"). **Why:** the routing LRU evicts views freely, and an
  evicted view must never take a 600 MB index or a password with it.
- **`max_concurrent_ops` stays at the trait's default of 1.** Concurrent `read_file_at` on one indexed repo is sound
  (`Send + Sync`, § "Opening a repo"); the limit is the parent's throughput, which the stream's read-ahead already uses
  (§ "Reads"). Raise it only with a measurement showing a parallel copy beats that.
- **The password ask stays `VolumeError::NeedsPassword { wrong_attempt }`, with no container field.** **Why:** the
  frontend tells a repo from an archive by the path it already classifies, so no archive mapping site changes
  (`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "The unlock prompt").
- **A repo's unlock verifies before the dialog closes** (an archive's is stored unchecked): the open is the check, so a
  wrong password re-arms the dialog in place.

## The view (`volume.rs`)

`ResticVolume::new(hub, volume_id, parent, repo_root)` is the view rooted at `<repo>/Browse snapshots`
(`VIRTUAL_ENTRY_NAME`). It's the same shape as `GitPortalVolume` (`crates/cmdr-git/DETAILS.md` § "The portal is a routed
volume"): the host routes a path here, and the volume classifies the whole path itself (`path::classify`), so a resolved
path stays the caller's path.

- **The root** lists one folder per snapshot plus `latest`, sorted by name (dated names start with a digit, so `latest`
  sorts last). A row's Modified is the backup's time; its Size (`size` and `recursive_size`, since a folder's cell reads
  the latter) is the summary's `total_bytes_processed`, or none for a snapshot without a summary (restic before 0.17).
  `restic_meta` carries the host, tags, and paths the snapshot recorded (sorted, as rustic's `StringList` keeps them),
  on `latest` too; the frontend words them. Every root listing re-reads `snapshots/` (rustic's `update_all_snapshots`:
  only new files are read, deleted ones drop), so a re-list shows a new backup: a manual refresh, or the app's watch on
  the parent's `snapshots/` re-listing an open root pane (`apps/desktop/src-tauri/src/file_system/restic/DETAILS.md` §
  "A new backup reaches an open pane"). Refreshes are one at a time per repo. A listing takes the list of a refresh that
  STARTED after it asked (`Snapshots::read_at` is the start), so the ones that asked while a refresh ran share the next
  one; ❌ never the running one's, which may have listed `snapshots/` before the backup they're asking about landed. One
  at a time down to the blocking work: a refresh whose pane cancelled still holds the repo's refresh permit until its
  rustic read ends (`ResticRepo::refresh_snapshots`), so one stuck on a hung parent makes the next wait instead of
  building a second pool beside it.
- **A snapshot folder** holds that snapshot's tree. Restic snapshots of a Mac start at `/`, so the tree under a folder
  is `Users/…`; it's shown as-is.
- **Read-only by construction**: `is_writable` false, every mutation at the trait's `NotSupported` default, and
  `create_directory_all` overridden the way git's is. `routes_over_a_parent` true, `can_watch_listings` false,
  `supports_local_fs_access` false, `paths_are_os_visible` false, `local_path` none. Lane key and space answers are the
  parent's. `metadata_caps` is `MODE_REPORTER` (nodes carry xattrs, but nothing here reports them yet).
- **Cancel**: `list_directory_with_cancel` is overridden, since it's the only way the pane's token reaches a volume. A
  cancelled listing answers `Cancelled` at once; the blocking work it started finishes on its own and is dropped.
- **Scans** go through `cmdr_fs::volume::scan_walk::ScanSource`. `max_concurrent_scan_listings` is 4: each listing below
  a snapshot is a tree blob read from the parent, and a copy's pre-scan of a whole snapshot would otherwise fan out
  without bound.
- **Errors**: a path not in a snapshot is `NotFound` with the caller's path, ❌ never a damaged-repo error. No password
  yet is `NeedsPassword { wrong_attempt: false }`; one that stopped working is `{ wrong_attempt: true }`. A parent
  failure surfaces as the parent's own `VolumeError` (a dropped share stays `DeviceDisconnected`, not a generic I/O
  error). `ResticError::into_volume_error` is the one mapping.

### Trees (`tree.rs`)

- A walk reads one tree blob per folder (`get_tree`) and matches each component against `Node::name()`, which unescapes
  rustic's stored name (❌ never the raw `name` field).
- **Mode**: restic stores Go `os.FileMode` and rustic 0.13.0 hands it through raw (its `map_mode_from_go` is
  `pub(crate)`). A row reports `mode & 0o777`: the type comes from `NodeType`, and Go's setuid, setgid, and sticky bits
  sit at other positions, which this drops on purpose (a restore shouldn't recreate setuid binaries). No mode recorded
  is `0`, the "no permission concept" sentinel.
- **Kinds**: symlinks are rows with `is_symlink`, ❌ never followed; devices, fifos, and sockets are non-directory rows
  with size 0. Hard links are separate nodes, so they list and copy as separate files. Owner and group are the user and
  group names the node recorded. Dates are whole Unix seconds; a date before 1970 is left out.
- **Odd names** pass through as stored (NFD, double spaces, 255 bytes). A name that isn't UTF-8 (a Linux machine's
  backup) lists lossily, the way `LocalPosixVolume` lists one (`to_string_lossy`). The lossy name then finds nothing
  (`NotFound`), and when a real sibling's stored name IS that lossy spelling, the lookup answers `AmbiguousName` rather
  than hand over the wrong file.

### Reads (`read_stream.rs`)

`open_read_stream` opens the node (`open_file`, which needs the full index) and reads 1 MiB windows (`read_file_at`) on
the blocking pool, four ahead of the one handed out (`READ_AHEAD`): one at a time left the link idle between blob reads,
~21 MB/s against ~39 MB/s over direct SMB (`docs/notes/restic/open-cost.md` § "Reading blobs ahead"). A dropped stream
DETACHES its reads in flight, ❌ never aborts them, so a parent mid-transaction (a phone) finishes each round trip
whole; memory stays at four windows. `read_range` is one `read_file_at`. Each window reads only the blobs it spans, and
each blob read is a `read_range` window of the pack on the parent, never the whole pack. A chunk that comes back short
of what it asked for is `SourceChanged`: a short file never lands silently. The stream reports the node's date and
permission bits. Directories answer `IsADirectory`; symlinks and special files `NoContent`, which a copy counts as one
skipped item (`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A child with no bytes is one skipped item"). A
symlink's target comes from `read_link_target`, raw bytes included, so a restore lands it as the link where the
destination holds one (same doc, § "A copy carries a link as the link").

Copy-out's scan opts into `ScanSource::abandons_reads_on_stop`: a listing here is safe to drop mid-flight (its waiter
leaves the index load), so a Cancel lands inside a listing that waits on the load.

### `latest`

Copies address files by path, so `latest` rebinding mid-copy would mix two snapshots in one restore, silently. The hub
pins which snapshot `latest` shows (`LatestPin`) and moves it only on a root refresh, and only when nothing has touched
`latest/…` for the quiet period (30 s): a listing, a stat, a stream open, or any chunk of a stream opened there. Listing
the root only shows the `latest` row and doesn't count, so refreshing to see a new backup moves it. Nor does it move
while a transfer holds the repo (§ "The index lifecycle", "A transfer's hold"): a copy paused between files, or
mid-file, reads nothing, and the snapshot watch would otherwise move `latest` under it with nobody pressing refresh, so
its remaining files would come from a newer snapshot. A copy out of a dated folder freezes `latest` too, which is
harmless. The pin also moves at once when its snapshot is gone from the repo (a prune), since there's nothing left to
show. A dated folder is the exact pin; `latest` is the convenience. With several hosts in one repo, `latest` is still
the newest snapshot overall.

## The hub (`hub.rs`)

`ResticRepos` is a value the host parks once and shares with every `ResticVolume` (git's `wiring::portal()` pattern, ❌
never a static). A volume is cheap (hub, slot, parent); the hub holds what's costly to get again, so a routing cache
dropping a volume loses nothing.

- **Slots**, one per repo, keyed by `(volume id, repo root)` exactly as spelled (❌ no `canonicalize`: it's a `stat`,
  and on a network parent a slow one). A slot is never dropped, so its password outlives the volumes over it. It holds
  the password (`Zeroizing`), the credential-store miss memo, the opened repo for the snapshot list, the index (serving
  and loading), the snapshot list, the `latest` pin, and when it was last used.
- **Locks**: std mutexes for quick state, never held across an `.await`. Per-repo tokio mutexes (the opened repo, the
  credential-store lookup) are held across an open or a Keychain read, so a second caller for the SAME repo waits
  instead of starting another; no other repo waits on them, and ❌ no hub-wide lock is held across scrypt, a Keychain
  call, or a parent read. The index load runs on its own task and is shared through `index_load::Load`, never a lock.
- **A replaced parent re-opens.** An SMB share reconnecting replaces the parent `Arc` under the same volume id. Each
  opened value (and each load) remembers the parent it was opened through (compared by `Arc` address), and a call
  arriving with a different parent opens again; the password survives.
- **A repo that changed under the view.** Replaced in place (`restic init` again, another repo copied over it), the
  cached open can't decrypt the new snapshot files (`Unreadable`): the hub drops the opened repo and the index and opens
  once more with the password it holds, so the root lists the new repo. Deleted, a refresh finds no snapshots and no
  `config`: `NotFound`, and the memo forgets it was a repo, ❌ never an empty list that reads as "no backups".

Sizing comes from David's NAS repo (`docs/notes/restic/open-cost.md`: 2.4M blobs, 112–119 MB of index files, ~0.58–0.62
GB of steady RSS per loaded index, unlock ~0.5 s, load 2.5–7 s over direct SMB; verified on `rustic_core` 0.13.0, an M3
Max, and a QNAP TS-464, 2026-10-20 and 2026-10-21). `Tuning` holds every number below; tests shorten them, the host
never does.

### The index lifecycle (`hub.rs`, `index_load.rs`)

- **Full index only** (`to_indexed`): reads need `IndexedFull`, and upgrading from `to_indexed_ids` later is a second
  full read plus an unlock. The index is its own open of the repo, since `to_indexed` consumes the open one.
- **One load per repo**, shared by every caller (two panes, a stat, a stalled listing's second attempt). A caller waits
  as an `index_load::Waiter`; dropping it is leaving, and the last waiter to leave a load that hasn't finished cancels
  it. That's the only signal there is: the listing engine never aborts a read, a cancelled pane DETACHES it, and
  `ResticVolume::list_directory_with_cancel` races the pane's token and drops its future, waiter included. A caller
  arriving after a cancel starts a fresh load. Joining and leaving share one lock, so nobody joins a load the last
  waiter just cancelled.
- **Streaming the index files.** rustic's own load reads every index file in parallel on rayon's GLOBAL pool (16 blocked
  workers on an M3 Max for the whole load; Cmdr's search and icons share that pool), can't be cancelled, and counts
  files. So the load lists `index/` itself and reads the files on the async runtime, checking the cancel between chunks,
  starting while the unlock's scrypt runs. They stream to rustic through an `IndexFeed`, a bounded window (`WINDOW`, 16
  MiB): the adapter answers rustic's `list_with_size(Index)` with the load's listing, and each `read_full(Index)` takes
  that file from the feed, blocking its decoder until it's in, so the bytes go the moment rustic has them. A cancel
  answers at once; reads in flight are DETACHED and stop at their next chunk, ❌ never aborted (a read dropped inside a
  device transaction can wedge a phone), a read spawned before the cancel whose task only runs after it opens nothing
  (`read_one` checks first), and a decoder waiting on the feed hears `Cancelled`. The same path serves every index size:
  ❌ no cutoff where rustic reads the files itself.
- **What the window reads ahead.** rustic's decoders each walk a contiguous run of the listing, starting wherever rayon
  split it, so the feed reads, in order: a file a decoder waits on (always, window or not), the file after each one
  asked for, then the listing in order. Reading ahead stops at four files in flight (`READS_AHEAD`) or when the window
  is full (a file counts at its listed size, the whole window when the listing gave none, and what arrived once it's
  in). So it holds at most the window plus one file per decoder. A file read ahead that runs past its listed size plus
  64 KiB is let go of (its bytes taken back off the progress) and read again only when a decoder asks.
- **rustic decodes on the load's own pool.** In 0.13.0, `to_indexed` → `GlobalIndex::new` → `stream_all` → `stream_list`
  does `rayon::spawn` + `into_par_iter` over the index files (read, decrypt, decompress, parse JSON each), feeding a
  zero-capacity channel the caller drains; `into_index` then `par_sort`s the entries. All of that lands on whatever pool
  the caller runs in, the global one from a plain blocking thread. So the build runs inside `repo::on_own_pool`:
  `pool.install` on five `restic-rustic-*` threads (`RUSTIC_POOL_THREADS`: one parks on the channel, four decode), built
  for the call and gone with it. A snapshot refresh (`update_all_snapshots`, the same `stream_list` over `snapshots/`)
  runs there too; nothing else this crate calls fans out. The build itself (decrypt and parse) isn't cancellable; on a
  warm mount it costs ~0.25 s more than on 16 threads (1.03–1.06 s against 0.79 s for 119 MB), and nothing on the
  network.
- **Honest progress**: `ListingPhase::LoadingIndex { done_bytes, total_bytes, done_files, total_files }` from the index
  files' listing and the bytes actually read, ❌ never rustic's file count. Each waiter forwards it to its listing's
  `on_progress` at most every 100 ms. A moving phase is what keeps the listing engine's stall watch quiet
  (`crates/cmdr-listing/DETAILS.md` § "Stalled listings").
- **Coverage and reloads.** A load records the snapshots listed when it started (restic writes a backup's index files
  before its snapshot file). A snapshot outside that set (a backup made since) loads again, beside the index serving,
  which keeps serving until the new one is in; readers holding the old `Arc` finish on it. Memory doubles for the swap.
- **A prune elsewhere reloads too (#653).** `restic forget --prune` repacks the surviving snapshots' blobs into new
  packs and deletes the old ones, but the snapshot ids stay, so coverage still says yes. A pack read the parent answers
  `NotFound` comes back typed (`backend::PackGone` → `ResticError::PackGone`); the read lets go of that index
  (`ResticRepos::reindexed`, only if it's still the one serving, so readers missing together share one load), waits on
  the reload, and runs again: the tree walks through `ResticVolume::on_index`, a read stream mid-file through its
  `IndexSource`. Once per read: a pack missing from a fresh index is a damaged repo and answers the parent's `NotFound`.
  Decision/Why: a manual refresh (every root listing) doesn't reload the index; that would cost a full load and a
  doubled index per refresh, and the first read that misses heals it anyway.
- **A transfer's hold** (`Volume::hold_for_transfer` → `RepoSlot::hold_for_transfer`): a copy, move, or compress reading
  from the repo takes one when it starts running and drops it when it settles
  (`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A transfer holds its source for its whole run"). While any
  is held the index is in use, so a copy paused between files (no read stream open) keeps it past the idle drop and
  another repo's load (#629). It counts on the slot, not the index, so an index loaded later is kept too; letting go
  counts as a use, so the idle clock starts when the transfer ends.
- **The cap: one indexed repo** (`max_indexed`). Installing an index lets go of the least recently used other ones, ❌
  never one in use: an `Arc` held outside the slot (an open read stream, a listing in flight), a transfer's hold, or
  used within the last 2 s (`lease_grace`, the gap between a scan's listings). With every one in use the cap is
  overshot. Eviction drops the index only: the password, the opened repo, and the snapshot list stay, so coming back
  reloads silently. A cap by count, not bytes: estimate an index at ~1x its `index/` folder in footprint, ~1.7x in live
  heap (§ "Why one index and eight opened repos").
- **Idle drop after five minutes** unused (`index_idle_drop`), unless a read stream or a transfer still holds it.
  Getting it back is an unlock plus a load, which the progress phase covers.
- **An index is freed on the blocking pool**, ❌ never under the slot's lock or on an async worker: the last `Arc` of a
  2.4M-blob index plus its blob cache takes a while to deallocate (`hub::index_slot::free_off_the_runtime`).
- **Opened repos** (unlocked, snapshot list read, a few MB each) are capped at eight (`max_opened`), least recently used
  first, never one in use.
- No log lines: the crate has no logger dependency, and nothing here needs one more than the typed answers it gives.

#### Why one index and eight opened repos

Decision/Why, from steady RSS (the process once the load's transient buffers are gone, which is what an index costs for
as long as it stays loaded; `docs/notes/restic/open-cost.md` § "Streaming the index load"):

- **A loaded index costs ~1x its `index/` folder in footprint, ~1.7x in live heap**: 171 MiB and 277 MiB for 162.5 MB of
  index files, with `MallocLargeCache=0` as the app ships (verified on macOS 27.0.1, `open_cost` `load` and `lifecycle`,
  2026-11-03, `../../docs/notes/performance/malloc-large-cache-2026-11-03.md`). A binary without it read 337–370 MiB
  that night and 614–634 MiB on 2026-10-26: the rest is libmalloc's large-block cache, and the ~5x in
  `../../docs/notes/restic/open-cost.md` is that reading. David's 6 TB NAS repo (likely 250–350 MB of index files) would
  hold ~0.3–0.6 GB, and a coverage reload doubles that for the swap. Two at once is past what a file manager should
  hold, so the cap stays one.
- **An opened repo is a few MB** (the whole process is 43–45 MB after an unlock, runtime and all), so eight cost nothing
  worth capping lower, and they're what make coming back to a repo skip scrypt.

### Passwords and the credential store

- **Lookup before prompting.** A call with no password in memory asks the host's credential store once (through
  `cmdr_fs::volume::secret_store`, ❌ never `host.credentials()` directly: a Keychain prompt on the async runtime
  freezes every volume), under the slot's lookup lock. A miss is remembered for the session, so every listing, stall
  retry, and pane doesn't ask again; an unlock clears it.
- **Filed as** service `restic://` (`KEYCHAIN_SERVICE`, which no SMB server, SFTP host, or WebDAV URL can spell: the app
  files every service under one `smb://…` account scheme, pinned by
  `a_restic_repo_entry_never_shares_a_key_with_an_smb_server` in the app's `network/keychain.rs`), scope and username
  `<volume id>:<repo path>`. The same share through an OS mount and through a direct session is two volume ids, so two
  entries.
- **Unlock** (`PasswordLocked::unlock` on `ResticVolume`, over `ResticRepos::unlock`): opens the repo with the password
  (scrypt on the blocking pool) and keeps the password (and the opened repo, unless another open holds the slot's lock)
  only after the open returns, with no await before the answer, so a dialog's Cancel (the host dropping the future)
  keeps nothing. Answers `Unlocked` or `WrongPassword`, never `Stored`. Filing is a separate step,
  `PasswordLocked::remember_password`, which the host runs only after an `Unlocked` nobody cancelled
  (`keep_signed_in_secret`: on saves, off only refreshes an entry already there and never seeds one). A wrong password
  keeps and files nothing. `forget_password` clears the in-memory copy only (a cancelled prompt).
- **Forgetting the saved password** (`PasswordLocked::forget_saved_password`, over `ResticRepos::forget_saved_password`
  in `passwords.rs`; the person's "Forget backup password") locks the repo again: it bumps the slot's forget count
  (`RepoSlot::forgets`), drops the in-memory password, marks the store missed for the session, drops the opened repo,
  the index, and the snapshot list, cancels an index load in flight, then deletes the store entry. Why all of them: each
  one serves reads without asking (a cached list and index answer every path inside a snapshot), so dropping only the
  password left the repo open. Answers `StoreKeptIt` when the store refused the delete; the repo is locked for the
  session either way.
- **A forget wins over work that started before it.** An open, a snapshot refresh, an index load, a store read, and an
  unlock each capture the forget count before their first await and commit only while it's unchanged (the password and
  the snapshot list check it under their own lock, so a forget can't land between check and commit; an opened repo
  carries its count and is never served across a forget). So the forget waits on none of them, ❌ never an open's lock:
  an open wedged on a dead NAS would hang the forget, with the Keychain entry still there. An unlock the forget overtook
  answers `Cancelled` and keeps nothing; `keep_password` files only while the slot still holds that password, and takes
  the entry back out if a forget lands during the write.
- **A password that stopped working** (a stale stored one, or the repo's keys changed) is forgotten from memory, the
  store isn't asked again this session, and the call answers `NeedsPassword { wrong_attempt: true }`. The stored entry
  stays until the next remembered unlock overwrites it, or the person forgets it.
- Cmdr's own copies of a password live in `Zeroizing` slots. rustic's `Credentials::password` makes a plain `String` for
  the open, `SignIn` carries one to the store, and the app's Keychain adapter caches what it reads in a process static;
  none of those is ours to wipe.

### Is this folder a repo?

The host's route is lexical (`restic_route`: the folder holding the FIRST `Browse snapshots` component), so it confirms
the candidate before routing:

- `confirm_repo(volume_id, parent, root)` answers `Some(true)`, `Some(false)`, or `None` (UNDECIDED: the parent failed
  or outran the 5 s deadline), from the memo or ONE listing of `root` run through `is_restic_repo`. A yes is remembered
  for good; a no for 30 s, so a real non-repo folder named `Browse snapshots` doesn't cost a listing per call; an
  undecided answer for 5 s, so every path below a slow folder doesn't pay the deadline. A read may pass through on
  `None`; ❗ a write must refuse on it, or it could put a real `Browse snapshots` folder inside a repo. The listing runs
  on its own task and the deadline only stops the wait: it's detached, ❌ never dropped (an abandoned MTP transaction
  wedges the phone). The index load's listing of `index/` is detached on a cancel the same way.
- `overlay_row(volume_id, root)` is the row the host's overlay adds to a pane's listing of a repo it recognized from the
  entries, and it confirms the repo in the memo for free.
- `serves_confirmed(volume_id, root)` is the memo alone, zero I/O, for a synchronous seam: `Some(true)`, `Some(false)`,
  or `None` (never asked, undecided, or the no went stale).
- The memo holds 1,024 answers; when full, the "no"s and undecided ones go first.

## The storage adapter (`backend.rs`)

`VolumeBackend` implements rustic's blocking `ReadBackend` + `WriteBackend` over a parent `Arc<dyn Volume>`, the repo's
folder on it, and a runtime `Handle`. It's the bridge `cmdr-archive`'s `VolumeByteSource` uses: each method `block_on`s
the parent's async call. That's only sound off the runtime's workers, so every rustic call runs on `spawn_blocking`;
rustic's own worker threads (index loading uses them) are outside the runtime too.

Layout, per restic's repo design:

- `FileType::Config` is the single file `<root>/config`, listed as one entry with the default id.
- `Key`, `Snapshot`, `Index` are `<root>/{keys,snapshots,index}/<id>`.
- `Pack` is `<root>/data/<first two hex digits>/<id>`. Listing reads `data/` once and then only the subfolders that
  exist, never 256 blind lookups.
- A listed name that isn't a 64-digit hex id (`.DS_Store`, a stray temp) is skipped. A missing folder lists as empty.

Reads:

- `read_full` streams the whole file (`open_read_stream`). It serves the small files: config, keys, snapshots, index.
  During an index load, index files come from the load's `IndexFeed` instead (§ "The index lifecycle"), and
  `list_with_size(Index)` answers the load's own listing; once the index is built the feed steps aside.
- `read_partial` reads exactly the requested window, since pack files are read at random offsets and a whole-pack read
  over SMB would be ruinous. A pack goes through a reader held open for it (`open_random_reader`), at most 8 at once,
  least recently used let go first: on SMB a `read_range` is open + read + close, about 14 ms per tree read on David's
  NAS against one round trip through a held handle (`docs/notes/restic/open-cost.md`). Packs never change once written,
  so a held handle never goes stale. Each `HeldReader` closes itself on drop, on the runtime, so dropping the repo (or
  evicting a reader mid-read) closes every handle once its last read ends. The held readers also close once none was
  used for 5 s (`READER_IDLE`), whatever the index does: a held handle keeps a deleted file delete-pending on Samba and
  Windows, so removing the repo's `data/` right after browsing would otherwise answer "folder not empty". A parent
  without readers (`NotSupported`, asked once) gets one `read_range` per window. A parent without `read_range`
  (`NotSupported`) falls back to streaming from the start and cutting the window out; correct, not cheap. A window that
  comes back short is an error, so a pack truncated under us fails loudly instead of feeding rustic short bytes.

Writes: `create`, `write_bytes`, and `remove` all refuse and count the attempt (`write_attempts`, test-only), so a test
can pin that browsing never even tries.

`warmup_path` returns an empty string and `needs_warm_up` keeps its `false` default: cold-storage warm-up never applies.

### Hostile repos

Anyone can leave a folder that looks like a repo on a shared drive, and rustic 0.13.0 passes a key file's own scrypt
`N`, `r`, `p` (plaintext JSON) straight to scrypt with no ceiling, trying every key it lists. So `list_with_size(Key)`
offers rustic at most 32 key files (the first by id), skips any over 64 KiB or not a key file, and skips any whose cost
`N·r·p` exceeds 2^23: 8 times rustic's own default (`N = 2^17`, `r = 8`, `p = 1`), and a 1 GiB ceiling on scrypt's
`128·N·r` bytes. rustic finds keys only through this listing (it passes no key hint on open), so a key skipped here is
never derived. A crafted repo with 32 tolerable decoys sorting before the real key would refuse the password; that's the
price of not pinning a core. rustic tries the offered keys one after the other, so their SUMMED cost is capped too
(`MAX_TOTAL_SCRYPT_COST`: four at the ceiling, or 32 at rustic's default). A key file is read with a bound of 64 KiB
whatever the listing says its size is, since a backend that reports no size lists every file as 0 bytes.

## Opening a repo (`repo.rs`)

`ResticRepo::open(parent, root, password, cache_dir, host)` builds the adapter, then on one blocking thread runs
`Repository::new(..).open(&Credentials::password(..))`. That lists the config, tries the password against each key
(scrypt, deliberately about a second per key), and reads the config. It does NOT load the index. The password copy the
blocking task carries is a `Zeroizing<String>`; rustic's `Credentials` makes its own plain copy, which we can't avoid.
`refresh_snapshots(current)` is `update_all_snapshots`.

`IndexedRepo::index` is `to_indexed()` on a separate open (the index load's, § "The index lifecycle"), because
`to_indexed` consumes the open repo and `OpenStatus` isn't `Clone`. A compile-time assertion pins
`Repository<IndexedFullStatus>` as `Send + Sync`, which concurrent reads through one `Arc<IndexedRepo>` rely on.

`cache_dir: None` sets `no_cache`; `Some(dir)` lets rustic cache snapshot and index files there (never in the repo). The
hub always passes `None`: on the NAS repo, rustic's cache made the first walk 2.5–8 times slower, added 240 MB of RSS,
and wrote 360 MB to disk (`docs/notes/restic/open-cost.md`).

No restic lock is taken on read: rustic 0.13.0 has no lock file type at all (see below), and the zero-write tests pin an
empty `locks/` too.

### Typed errors

`ResticError` is what a host words: `WrongPassword`, `NotARepository`, `Volume(VolumeError)`, `PackGone(VolumeError)`
(worded as its `NotFound`), and `Unreadable { detail }` (rustic's `display_log()`, for the log and the technical details
only). `classify` sorts a `RusticError` by type:

1. **Wrong password**: `RusticError::is_incorrect_password()`. rustic attaches its own error code `C002` when no key
   accepts the password (`find_key_in_backend`), and this accessor is rustic's public, typed way to ask for it. The
   error's `kind` field isn't readable outside rustic in 0.13.0, so the code is the only typed signal there is. Pinned
   by `a_wrong_password_is_typed_and_the_right_one_then_opens`; if a rustic bump drops the code, that test fails.
2. **Pack gone**: a pack read the parent answers `NotFound` is wrapped as `PackGone` (its source the `NotFound`) and
   found by `pack_gone_in`, before step 3. Pinned by `stale_index_tests`.
3. **Parent failure**: the adapter wraps every `VolumeError` with `RusticError::with_source`, so it rides in the error's
   source chain; `volume_error_in` finds it by `downcast_ref`. Pinned by
   `a_parent_that_cant_answer_surfaces_its_typed_error`.
4. **Not a repo**: anything else, when the config is gone (one more typed `list_with_size(Config)` asks the parent).
5. **Unreadable** otherwise: a damaged repo, an unsupported version, or a crashed blocking task.

A panic inside a blocking task comes back as a `JoinError`, which maps to `Unreadable`, ❌ never a crash.

## Snapshot folders (`snapshots.rs`, `path.rs`)

A snapshot folder name is its local time plus its short id: `2026-10-12 21.00.03 (a1b2c3d4)` (David's decision on #422).
Dots in the time, since `:` isn't a valid name character in Finder.

- **Order** is by time, then id. `SnapshotList::latest()` is the last one, so a same-instant tie always resolves the
  same way.
- **Unique across the repo**: the id part is the shortest prefix, eight digits at least, that no other snapshot's id
  starts with. Only the ids that clash grow. (The neighbors in sort order are the only candidates, so naming stays O(n
  log n).)
- **Resolved by the suffix alone** (`short_id_of`, `SnapshotList::by_short_id`): the time prefix is local time, so a
  time zone change or DST would otherwise orphan every path in history and favorites. A suffix must be 8–64 lowercase
  hex digits. A prefix matching several snapshots (a newer backup now shares a saved prefix) resolves to nothing, ❌
  never the wrong snapshot; a longer saved suffix still resolves.
- **Sortable**: sorting by name sorts by time, to the second. During a DST fall-back the repeated hour's two passes
  interleave by name (the list order stays true; only a name sort differs).
- **`latest`**: `LATEST_FOLDER_NAME`. A dated name starts with a digit, so they can't collide.

`snapshot_list` is a pure function of the raw snapshots and a `TimeZone`, so tests pin a zone (Stockholm) rather than
depend on the machine's.

`restic_route` is lexical: the folder holding the path's first `Browse snapshots` component (a repo backed up inside a
snapshot still routes at the outer boundary). It's a candidate, not a verdict; § "Is this folder a repo?".

## Detection (`detect.rs`)

A folder is a restic repo when its listing holds a regular file named `config` and the folders `data`, `index`, `keys`,
and `snapshots`. `locks` is optional: restic creates it lazily, and a copied repo often drops it. A symlink counts as
neither: restic never writes one, and its target is unknown from the listing alone. Names match exactly. A listing of
more than 32 entries (`MAX_REPO_ROOT_ENTRIES`; a repo root holds six) isn't a repo, on its length alone, so a 100k-entry
folder costs one comparison.

## Fixtures (`test_fixtures.rs`)

Under the `testing` feature (this crate turns it on for its own tests through a self dev-dependency), so the host's
tests build repos the same way. `FixtureRepo::init` (v2) and `init_v1`, then `add_snapshot` (an empty-tree snapshot with
no summary, for list tests) or `backup` (a real folder, for trees and contents). `write_into(volume, root)` lays the
repo out on any `Volume`, `LocalPosixVolume` included, and skips files already there, so writing again after another
`backup` adds just the new ones. `files()` hands back the raw layout. `unlock(volume, password)` is the door to an
unlocked view.

- rustic's `init` refuses `set_version = 1` as a downgrade, so `init_v1` hands `init_with_config` a v1 `ConfigFile`
  directly, with restic's test chunker polynomial and an all-zero repository id (the id type isn't public).
- The fixture holds the repo ids-indexed, since `backup` needs that and `OpenStatus` can't be cloned to get there twice.
- scrypt is slow unoptimized (about 8 s per unlock), so the workspace builds `scrypt`, `salsa20`, and `pbkdf2` at
  `opt-level = 3` even in debug (root `Cargo.toml`). Without that this suite takes minutes; with it, under 2 s.
- A backup's source files are real files: macOS refuses a name that isn't UTF-8, so that case is a `tree.rs` cell over
  hand-built nodes.

## How the host uses it

- **The unlock**: the host reaches it through `Volume::as_password_locked`, like an archive's. The hub is built with the
  app's credential store in its `VolumeHost`, or lookups always miss.
- **Index progress**: a listing into a snapshot reports `ListingPhase::LoadingIndex` ticks with zero entries, which the
  pane words as bytes over a bar (`apps/desktop/src/lib/file-explorer/restic/DETAILS.md` § "Index progress").
- **Copy-out** holds the index for its whole run through its transfer hold, paused or not (§ "The index lifecycle", "A
  transfer's hold"), and through its read streams; a whole-snapshot pre-scan goes through `list_directory` per folder,
  which joins the same load.
- **The host route** calls `restic_route`, then `confirm_repo` (or `serves_confirmed` on a sync seam), and builds a
  `ResticVolume` over the parked hub. The overlay calls `is_restic_repo` on the entries, then `overlay_row`, whose row
  carries the reserved `restic:snapshots` icon id (`OVERLAY_ROW_ICON_ID`) the frontend draws and recognizes it by. Both:
  `apps/desktop/src-tauri/src/file_system/restic/DETAILS.md`.
- `ResticVolume::backend_kind` is `BackendKind::Restic`, so the host's matches (favorites, the error report, the stall
  watch, the index) never read a snapshot as a local disk. `VIRTUAL_ENTRY_NAME` and `restic_route` are defined in
  `cmdr_fs::restic_view` (the listing engine asks them without linking rustic) and re-exported here.

## rustic_core 0.13.0 facts

`rustic_core` is vendored (`vendor/rustic_core/CMDR-PATCH.md`) only to sit on `zstd` 0.14; its code is the release's.

Read from `~/.cargo/registry/src/*/rustic_core-0.13.0/src/` (2026-10-20). Each one shaped a design above; re-check them
on a bump.

What differs from the issue's 0.12.0 research:

- `WriteBackend::write_bytes` takes a `BytesList`, not `Bytes`.
- `FileType` has no `Lock` variant (only `Config`, `Index`, `Key`, `Snapshot`, `Pack`), so there's nothing to map to
  `locks/`. Detection still allows the folder.
- `ReadBackend` adds `warmup_path` (required) next to `needs_warm_up` / `warm_up` (defaulted).
- `Repository<OpenStatus>` isn't `Clone`.
- `SnapshotOptions::to_snapshot` fills an all-zero summary; restic before 0.17 wrote none.
- MSRV 1.91; new to the lockfile against 0.12.0's list: `itertools` 0.15 (a third major next to 0.13 and 0.14).

What the design leans on:

- **State transitions consume the repo**: `to_indexed(self)`, `to_indexed_ids(self)`, and `drop_index(self)` take
  `self`, so a failed or cancelled load drops its open with it. There's no public way to get the decrypted master key
  back out for a cheap re-open (`OpenStatus`'s key is `pub(super)`), so every fresh open is another scrypt.
- **`to_indexed_ids` keeps every tree blob**, so `get_tree` works under it, but `open_file` / `read_file_at` need
  `IndexedFull`, and there's no in-place upgrade (`drop_index` + `to_indexed` reads every index file again).
- **`IndexedFull` carries a 32 MB blob cache** (`quick_cache`, `constants::WEIGHT_CAPACITY`) per indexed repo, on top of
  the index; the measured steady RSS in § "Why one index and eight opened repos" includes it.
- **rustic's progress counts files, not bytes** (`stream_list` calls `set_length(files)` and `inc(1)`), and the
  `ProgressBars` factory is fixed per `Repository` at `new_with_progress`, not per call.
- **No retry inside rustic_core for backend reads**: `backon` only retries a warm-up command's spawn (rustic's `warm_up`
  module). A refused read ends a load at once.
- **`update_all_snapshots(current)`** lists `snapshots/`, keeps the known ones, fetches only new files, and drops the
  deleted ones.
- **Panics**: `GlobalIndex::into_index`, reached by `drop_data_from_index` / `into_indexed_tree`, sleeps 100 ms and then
  `expect`s. ❌ Don't call those transitions. Any panic inside `spawn_blocking` comes back as a `JoinError` (§ "Typed
  errors").

## The public surface is capped

`index-crate-isolation` holds this crate to five root promises, no public module, and no public items inside one,
exactly what it exposes. Each has a caller in the host:

- Detection: `is_restic_repo` (the overlay asks it per listing).
- The route: `restic_route` and `VIRTUAL_ENTRY_NAME` (the host's lexical gate, and the row's name).
- The hub: `ResticRepos`, with `new`, `confirm_repo`, `serves_confirmed`, and `overlay_row`.
- The view: `ResticVolume`, with `new`.

`test_fixtures` is `testing`-gated and counts apart. Opening, the snapshot list, trees, and the adapter stay
crate-private: the volume that needs them lives here.

## Invariants

The feature's guardrails, crate and app together, numbered so reviews can cite them
(`docs/notes/restic/conformance-review.md`). Each line points at where its mechanism is described.

1. Browsing never writes to the repo: zero `WriteBackend` calls, no lock files, pinned by the adapter's write counter (§
   "The storage adapter").
2. No `stat` or listing on the hot path: the route's gate is lexical, the overlay reads the entries in hand, and the
   confirm is memoized, so a miss costs one bounded listing per repo (§ "Is this folder a repo?").
3. The `Browse snapshots` row reaches a PANE and nothing else; no volume listing holds it
   (`crates/cmdr-listing/src/host/DETAILS.md` § "Overlays").
4. No walker but a pane and a selection-driven, cancelable scan enters the view
   (`apps/desktop/src-tauri/src/file_system/restic/DETAILS.md` § "What each walker sees").
5. The repo's real files are ordinary files: listable, copyable, deletable, indexable, with no guard.
6. Every mutation inside the view is refused with a typed read-only refusal, on every seam, the ones that bypass
   `resolve` included; a move out is refused before anything is copied (the app's DETAILS § "Write guards").
7. The leftmost CONFIRMED route boundary wins across git, restic, and archive routes; an unconfirmed candidate hands
   over to the next (`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Resolving a path: the three routes").
8. Every `rustic_core` call runs on the blocking pool; every long one (unlock, index load, file read) is cancelable, and
   a cancelled unlock keeps and files nothing. No hub-wide lock is held across scrypt, a Keychain call, or a read.
9. One index load per repo at a time, shared by its waiters (the engine's stall retries included), with progress in real
   bytes that keeps the stall watch quiet (§ "The index lifecycle").
10. Memory is capped by the hub's cap and idle drop, which never take an index in use; the routing LRU bounds
    registrations only and never drops a password or an index.
11. Cmdr's own copies of a password live only in the hub's `Zeroizing` slots, never in a log, a mirror, an error, or a
    tool result; the store is written only after a verified unlock nobody cancelled (§ "Passwords and the credential
    store").
12. A snapshot folder resolves by its short id; the time prefix is display. `latest` doesn't move while anything reads
    under it (§ "`latest`").
13. The crate names no `tauri` / `cmdr`, words nothing a user reads, and never matches on error text.
14. One dialog, one unlock seam (`PasswordLocked`), and one MCP tool (`unlock`) serve every locked container.
15. A crafted repo can't exhaust memory or CPU through its key files: count, size, and summed scrypt cost are capped
    before rustic sees them (§ "Hostile repos").
