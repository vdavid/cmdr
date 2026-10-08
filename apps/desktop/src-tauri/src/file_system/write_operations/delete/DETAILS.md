# Delete + trash details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## Volume-delete internals

`delete_volume_files_with_progress_inner` consumes the scan preview via `take_cached_scan_result(preview_id, sources)`. On hit, top-level
files come straight from `CopyScanResult` with no `is_directory` probe; top-level dirs recurse via the oracle-aware
walker. On no-preview paths (MCP, programmatic), the parent oracle answers the top-level type when the source's parent
is watcher-fresh in `LISTING_CACHE`, and otherwise the walker resolves it. Both emit paths use
`with_scan_meta(current_dir, dirs_done, None)` so the scanning UI shows the dir count and the directory the walker is
currently in. The per-entry callback is throttled so the FE tally climbs mid-listing on slow MTP roundtrips.

**Files go in the backend's batches where it has them.** A backend that answers `Volume::delete_batch_size` (S3:
`DeleteObjects`, 1,000 keys a request) gets the scanned files in chunks of that size through `delete_files`, with the
pause and Cancel point between chunks; everyone else gets one `delete_with_cancel` per file, as before. Decision/Why:
`delete` on an object store re-proves each path is a file (a capped listing, then a HEAD) before deleting it, so a
1,005-object folder cost 3,021 requests and minutes on R2 and GCS; the scan already listed these as files, which is the
`delete_files` contract. Every file a batch removed is journaled and counted even when another in it failed; the first
failure ends the delete, reported against its own path (S3 reads each key's failure out of the answer body). A throttled
or cut-off batch is the backend's to send again (`cmdr-s3`'s `batch.rs`). Pinned by
`backend_suites/s3_engine_integration_test.rs::a_folder_of_1005_objects_deletes` (two `DeleteObjects`, no per-object
request).

## The volume delete's own lifecycle

`volume_start.rs::start_volume_delete` registers the op with the manager and hands it a deferred async start; `drive_volume_delete` is that start. A local delete rides `start_write_operation`'s generic spawn, but the `Volume` trait's I/O is `async`, so a volume delete owns its own: the settle guard, `await_claimed_preview`, `open_volume_op` under the REAL volume id (never the `"root"` the local helpers bake in), the source binding, the walk, the terminal event, and `on_settled`.

The deferred is a NAMED future taking one moved-in struct, not an inline `async move` block. Inline, it was ~140 lines nested inside a closure inside a starter inside the module facade, and the shape it forms — descriptor literal, three `Arc::clone` rebinds, `ManagedTaskGuard`, settle guard — is the same boilerplate `transfer/volume/move_same.rs` and `transfer/volume/copy.rs` already carry, so a second literal copy of it is duplication a checker can see. Keep it named.

Per-source outcomes reach the sink from here too: the binding's pre-flight (`../source_binding.rs`) announces every source it drops. `../DETAILS.md` § "Per-source outcomes".

## What each branch does with a missing or wrong fact

Delete is the operation with no rollback, so every branch here has to have an answer for "what if this fact is absent or
wrong". The audit, top to bottom:

- **Which selection the cached preview describes.** Bound at the cache, not here: `take_cached_scan_result` compares the
  entry's `sources` against the operation's and treats a mismatch as a miss (`../DETAILS.md` § "The cache is bound to
  its request"). Both walkers then fall through to a fresh scan. Before that binding, the LOCAL walker was the sharpest
  edge in the app: it takes the cached result wholesale and iterates `scan_result.files` without ever re-reading its own
  `sources`, so a `preview_id` pointing at another tree deleted that tree. Pinned by `preview_binding_tests.rs`.
- **A source the cached `per_path` doesn't cover** (the volume walker's `None` arm). Forwards `is_dir_hint: None`, and
  `scan_volume_recursive` PROPAGATES a failed probe rather than defaulting. It looks like the bug class and is its
  opposite; a `Some(false)` there would be the guess. The whole-map-empty case (`file_count > 0`, `per_path` empty) is
  the exact production shape the original copy bug rode in on, so it's pinned end to end in
  `delete_volume_reuse_tests.rs`.
- **A source whose stat can't be answered** (the no-preview path). Also forwards the oracle's `Option` straight to
  `scan_volume_recursive`, so an unanswerable stat fails that item. The walker used to resolve the top level itself with
  `.unwrap_or(false)`, which made an unanswered stat a confident "file": the entry went in as `is_dir: false` with zero
  bytes, so progress described one file and no bytes for what might be a whole tree, and the `delete` that followed
  either took the tree (on a backend that recursed) or died on a confusing `ENOTEMPTY`. **What the user sees now**:
  deleting a folder whose stat failed reports a per-item failure with the folder still standing, instead of appearing to
  delete "one file". That's the honest outcome, it fires only on a probe error, and a retry after a transient MTP stat
  failure is cheap.
- **A cached listing that's stale by one.** Covered by the data-safety contract below: exact observed paths only.

What the `Volume::delete` non-recursion contract does and doesn't buy this walker: it bounds the BLAST RADIUS of a wrong
`is_dir` on a top-level source (a `delete` that would have taken a tree refuses instead), but it never bought a truthful
progress count, and it's a property of the conformance assertion every backend runs, not of a doc comment. The contract
itself is single-sourced at `crates/cmdr-fs/src/volume/mod.rs`.

## Key decisions

**Decision**: Volume delete reuses the scan preview and is oracle-aware on the no-preview path.
**Why**: Before this, `delete_volume_files_with_progress_inner` ignored `config.preview_id` and re-ran
`scan_volume_recursive`. On MTP that meant a second 17 s parent listing for a 135-photo `/DCIM/Camera` delete after the
user already paid that cost in the pre-flight dialog, and the second scan emitted no per-top-level-file progress so the
UI looked frozen. The fix has three parts. (1) `take_cached_scan_result(preview_id, sources)` at the top: on hit, top-level files
are recorded from `CopyScanResult::total_bytes` with no `is_directory` probe and no `list_directory` round-trip, and
top-level dirs recurse via the oracle-aware `scan_volume_recursive` (passing `is_dir_hint = Some(true)` so the recursion
never re-probes). (2) The walker's internal `volume.list_directory(path, ...)` is preceded by
`try_get_authoritative_listing(volume_id, path)`; on hit, the cached entries replace the volume call at every recursion level.
(3) On the no-preview path, the parent oracle supplies the top-level type when a pane has the source's parent open and
watcher-fresh, skipping the probe; on a miss the hint stays `None` and the walker resolves it (see the branch audit
above). The cache-hit path emits a throttled scan-progress event per `progress_interval` while building the entry list,
so the FE dialog shows movement. Pinned by `delete_volume_reuse_tests.rs`.

Data-safety contract: stale-by-one cached entries can either silently skip a now-gone file (acceptable: the user already
moved it) or attempt to delete a missing one (the volume's `delete` errors cleanly). Neither can delete the wrong file
because we feed `volume.delete(&entry.path)` exact paths the cache observed; a cached entry that races with a concurrent
rename addresses the old path the next call won't find.

**Decision**: Delete and trash don't `fsync` (or fire any global `sync(2)`) after removing files.
**Why**: A non-durable delete fails annoyance-class, never data-loss-class: if the machine crashes before the deletion is
flushed, the deleted file reappears and the user re-deletes it. Paying for a targeted `fdatasync` over every removed path
(and its parent dirs) isn't worth the cost. The old code fired a detached whole-machine `sync(2)`; that flushed every
filesystem on the box, stalling unrelated apps (against AGENTS.md principle #5, "be respectful to the user's
resources"), and as fire-and-forget it didn't even make "complete" mean "durable." Copy and move are the data-loss-class
operations (a move can leave bytes nowhere durable), so they get the real targeted flush; see `../transfer/DETAILS.md`
§ "Durability" and `../DETAILS.md` § "Key decisions (shared)". Pinned by
`tests.rs::no_global_sync_or_spawn_async_sync_in_write_operations`, which fails the suite if `spawn_async_sync` or a raw
`libc::sync()` reappears in `write_operations/`.

## Where each of the three loops parks

All three destructive loops in this directory ask `state.stop_or_park_sync()` / `_async()` where they already observed
cancel (`../DETAILS.md` § "Pause / resume" owns the primitive): the local walker's delete-phase file and dir loops, the
volume walker's two, and trash's per-item loop. **The SCAN recursion is deliberately NOT gated** — pausing mid-scan
would freeze a half-counted "Scanning…" for no benefit, since nothing has been destroyed yet.

Trash's boundary is the ITEM, and that's all it can ever be: `trashItemAtURL` hands a whole top-level tree to the OS in
one call, so there is no seam inside it the way the walkers have one per file. Pinned by `trash_pause_tests.rs`, which
drives the loop over paths that don't exist so it exercises the boundary without moving anything into the real trash.

## Who emits `write-cancelled` when a scan bails

`scan_volume_recursive` checks cancel at every recursion level, so a helper that emitted the terminal event where it
bails would fire it once per stacked frame. It returns `Err(Cancelled)` silently instead, and the top-level caller emits
through `emit_cancelled_if_aborted`. **Any new per-level-cancel scan owes the same split**, whichever operation adds it.
Pinned by `delete_cancel_during_scan_emits_write_cancelled`.

## What a batch trash reports (`trash_files_with_progress`)

`trashItemAtURL` is atomic per top-level item, so a batch has three endings, not two, and each one has to be readable
from the terminal event alone. Progress counts items rather than bytes for the same reason; the byte figure, when there
is one, comes from sizes computed before the batch started, never from the trash calls themselves.

- **Every item taken** → `write-complete` with `refused: None`. A plain success.
- **Every item refused** → `write-error` carrying `WriteOperationError::TrashRefused { item_count, reason, message }`,
  where `reason` is `strongest_refusal` over the per-item reasons: the one that opens the most doors, ❌ not the most
  common one, since a batch with a single permission refusal among vanished files is still a batch a permission grant
  might unblock. The error dialog renders it.
- **Some taken, some refused** → `write-complete` with `refused: Some(TrashRefusedItems { item_count, reason })`, the
  same `strongest_refusal`, so the mixed ending explains itself exactly as the total one does.

**Why `refused` exists as its own field.** The mixed ending used to emit a bare completion with `files_skipped: 0` and
nothing else: two items in, one trashed, and the terminal event said "1 file, 0 skipped" while the refused one sat
untouched in the pane (a Dropbox online-only file beside an ordinary one, 2026-09-17). ❌ It can't ride `files_skipped`:
a skip is something the user or a policy CHOSE, and the frontend words the two differently. ❌ And it can't become a
whole-batch error: the items that DID go are in the trash, the journal holds exactly those, and the undo has to reverse
what happened rather than what was asked for.

**The journal is per item, which is what keeps the accounting honest.** `record_local_leaf` and the buffered
`search_only` leaves are persisted only on a successful `move_to_trash_sync`, so a refused item leaves no row: a
rollback restores what really moved, and a search snapshot never reports a trash that never happened. `items_done`
likewise counts successes alone, so `files_processed` and `bytes_processed` describe what landed. Beyond the summary,
every refused item also emits its own `Failed` `write-source-item-done` event, since the terminal event speaks for the
batch and not for any one source. Pinned by `a_partly_refused_batch_reports_what_it_left_behind` and
`a_batch_the_os_took_in_full_reports_nothing_left_behind` in `trash_tests.rs`.

Frontend counterpart (which toast, at which level):
`apps/desktop/src/lib/file-operations/delete/DETAILS.md` § Edge cases.

## Where a trash is (`trash_dir_for_path`)

macOS keeps ONE trash per volume: `~/.Trash` for the boot volume, `<mount point>/.Trashes/<uid>` for everything else.
`trash_dir_for_path` asks Cocoa the same question the trash move itself asks
(`URLForDirectory:inDomain:appropriateForURL:create:`) rather than reconstructing the layout from `statfs` + `getuid`,
so a disk image, a synthetic firmlink, or a volume with no trash answers for itself instead of against our guess.
`create: false` keeps it a pure lookup: asking never brings a trash directory into existence.

**Gotcha: it answers for a VOLUME, not an item.** `appropriateForURL:` resolves the URL's volume, and a path that
doesn't exist has none — so it errors. Every caller asks about paths that are routinely gone by then (the item the user
just trashed is no longer where it was), which is why the resolver walks up to the nearest existing ancestor: same
volume, same trash. ❌ Don't remove the walk; without it the function returns `None` in exactly the case it exists for,
and "Go to trash" silently stops working. `trash_dir_for_path_answers_for_a_path_that_is_already_gone` and
`trash_dir_for_path_climbs_past_several_missing_levels` pin it, and
`trash_dir_for_path_matches_where_the_item_actually_landed` pins the resolved directory against where a real trash move
puts a file.

`None` stays the ordinary answer for a volume with no trash (FAT32, SMB) and on non-macOS, so callers say "nowhere to
go" rather than reporting something went wrong.

## A trash of online-only cloud content becomes a delete (`cloud_trash.rs`)

A third-party File Provider under `~/Library/CloudStorage/<domain>/` evicts files it has uploaded, leaving a placeholder
macOS marks `SF_DATALESS` (Finder calls these "online-only").

**The reason for the whole feature is the DOWNLOAD, ❌ not the refusal.** The Trash is a folder on the boot volume, so
putting an evicted file there means pulling its bytes back from the provider first, and macOS does exactly that: an
online-only Dropbox file trashed in ~500 ms and landed in `~/.Trash` at its full 111 kB (verified on macOS 27.0, prod
build, 2026-09-17). Three small PNGs is half a second; a folder holding 50 GB of online-only content is 50 GB over
someone's connection, to fill a folder they are about to empty, for files they just said they don't want. That is why
the routing fires even in the cases where the trash WOULD have succeeded, and why ❌ nobody should "simplify" this into
reacting to an error.

Reacting to the error wouldn't work anyway, on two counts:

- **It isn't deterministic.** The same file trashed at 08:45, was refused at 09:06:00, refused again at 09:06:36, and a
  sibling went through at 09:06:20 (same log). "Try it and see" is not a design.
- **One condition, two `NSError` codes.** 513 (`NSFileWriteNoPermissionError`, «you don't have permission to access
  it») and 3328 (`NSFeatureUnsupportedError`, «the volume "Macintosh HD" doesn't have one») both come back for it, and
  they land in two different `TrashRefusalKind` buckets. The item's own flag is the reliable signal, and it answers
  before a byte moves.

**`SF_DATALESS` is `0x40000000`**, from `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/sys/stat.h:359`
("file is dataless object"), verified on macOS 27.0 / Command Line Tools, 2026-09-17, against a real evicted Dropbox
file whose `stat -f "%Sf"` reads `compressed,dataless`. ❗ The `libc` crate does NOT export it (its `SF_*` set stops at
`SF_SETTABLE` / `SF_ARCHIVED` / `SF_IMMUTABLE` / `SF_APPEND`), which is why the constant is declared in `cloud_trash.rs`
with that citation.

**Decision**: F8 opens the permanent-delete flow when a selected item is inside such a drive AND carries `SF_DATALESS`.
The confirmation dialog looks different from the trash one, so the swap is visible, and the provider's own server-side
retention (Dropbox keeps 30 days) means the file is still recoverable. The frontend asks `trash_routing_for_paths`
before it opens the dialog; the answer is a typed `TrashRoutingAnswer`, and the routed case lands in the same permanent
delete Shift+F8 runs, with the same `WriteOperationType::Delete`.

**Reading the flag never materializes anything.** `symlink_metadata` + `st_flags` is metadata only; ❌ never open or
read a file to find out, which would be the very download this exists to avoid. `symlink_metadata` also keeps the LEAF
symlink unfollowed, matching the rule below.

**What the rule matches**, and why each edge is where it is:

- The item carries `SF_DATALESS`. ❌ NOT "it's in a cloud folder": an ordinary, materialized Dropbox file trashes
  perfectly well, and routing it to a permanent delete turns a working trash into data loss. Being in the drive only
  makes the flag meaningful.
- A path strictly inside `~/Library/CloudStorage/<domain>/`, Apple's documented File Provider location since
  macOS 12.3. Provider identity itself is `cloud_provider.rs`'s single source; this module only asks it.
- ❌ NOT iCloud Drive (`~/Library/Mobile Documents/`). Finder trashes from there fine, so it keeps today's behavior. A
  provider that refuses anyway is already covered by the typed `TrashRefusalKind` path.
- ❌ NOT a provider Cmdr doesn't know by name (`CloudProvider::keeps_deleted_items_recoverable`). A `CloudStorage`
  directory is not proof of a cloud service: MacDroid publishes an Android phone as a File Provider
  (`CloudStorage/MacDroid-<device>/`), where a delete is final and nothing keeps a copy. The dialog this routing opens
  promises one, so an unknown provider keeps the OS trash and its refusal, which loses nothing.
- ❌ NOT the `CloudStorage` container, and ❌ NOT a drive's own root. Deleting a whole cloud root permanently would take
  the account's entire local copy; that one keeps the OS trash and its refusal.
- ❌ NOT "any location with no trash". A freshly formatted USB stick answers "no trash" from a volume probe only because
  nobody has trashed anything on it yet, and routing that to a permanent delete would be a data-loss bug.
- ❌ NOT a volume question at all. A File Provider folder is not its own volume: `~/Library/CloudStorage/Dropbox` and
  `~` report the same device on the same `/dev/disk3s5` (verified on macOS 27.0, `stat -f '%d'`, 2026-09-17), so
  `trash_dir_for_path` answers `~/.Trash` for these paths and can see nothing.

**A selected FOLDER is answered by the walk the dialog already runs.** `SF_DATALESS` lives on files, so a folder never
carries it. `routing_for` reports `folder_may_hold_online_only` instead, and the confirmation's scan preview tallies
evicted entries as it descends (`scan_walker.rs`'s `OnlineOnlyWatch`, armed by
`selection_touches_known_cloud_drive`), publishing `online_only_found` on `scan-preview-progress` and
`-complete`. ❌ Never a second walk, and ❌ never a walk for a plain-file selection: one `symlink_metadata` per selected
item is the whole cost there. The arming gate is what keeps every ordinary scan byte-for-byte what it was, including the
extra `lstat` the oracle-served (cached-listing) path needs, since a cached `FileEntry` carries no `st_flags`. One hit
ends the probing.

**All or nothing across the selection.** One online-only item routes the whole gesture. ❌ Not per item: splitting one
keystroke into "these go to the Trash, those get deleted permanently" is two mental models in one confirmation, and no
readable copy comes out of it. An empty selection, an item we can't stat, an unreadable home directory, and a timeout
all answer `Trash`: an unanswerable question means the OS attempt, never a delete.

**The routing also says HOW MUCH of the selection is evicted**, because that's the one thing the confirmation's two
wordings differ on: the mixed banner offers "deselect all online-only files" as a way out, and the all-online-only one
can't, since that would leave nothing selected. So `TrashRouting` has two delete variants,
`PermanentDeleteAllOnlineOnly` and `PermanentDeleteMixedOnlineOnly`, rather than one variant plus a field nothing forces
you to set. Both run the same permanent delete.

- **`All` is a claim, so it's only made about items positively read as evicted.** Anything that didn't resolve or
  couldn't be stat'ed counts toward the ordinary side and lands on `Mixed`. That copy says less and its remedies still
  work, so guessing wrong costs nothing; the other direction would tell someone every file they picked is online-only
  when one of them was merely unreadable.
- **A FOLDER's walk can only ever produce `Mixed`.** It reports one boolean for the whole subtree
  (`online_only_found`), so a hit says "something in here is evicted" and nothing about the rest, and a folder holding
  one evicted file beside a hundred ordinary ones is the ordinary case. The frontend maps that hit to the mixed
  wording (`DeleteDialog.svelte`'s `onlineOnlyFoundByWalk`).

**A refusal that IS online-only suppresses the Full Disk Access offer.** `trash_files_with_progress` stats each refused
item (`trashItemAtURL` is atomic, so a refusal left it exactly where it was) and sets `online_only` on
`WriteOperationError::TrashRefused`. Per the two-codes finding above, a 513 refusal of an evicted file is
indistinguishable by reason from a real permission problem, and telling someone to go grant Full Disk Access when their
file simply lives on Dropbox's servers is a wrong answer they would act on. The frontend gates the paragraph on it
(`transfer-error-messages.ts`).

**Symlinks resolve first, but only above the leaf.** Dropbox links `~/Dropbox` at its `CloudStorage` drive, so a literal
prefix test would miss half the ways a person reaches the same file. `resolve_through_symlinks` canonicalizes the
nearest existing ANCESTOR and re-attaches the names below it, which also survives a leaf that's already gone. The leaf
itself is deliberately left unresolved: trashing a symlink acts on the LINK, so a link in an ordinary folder pointing
into a cloud drive keeps the ordinary behavior.

**What the tests can't prove.** Only a File Provider can set `SF_DATALESS`, so no test creates a really-evicted file.
`routing_with` takes an injected `ItemFacts` reader and `OnlineOnlyWatch` an injected probe; the suites pin the RULE and
the WIRING, while the flag read itself (`st_flags & SF_DATALESS`) rests on the header citation and David's
`stat -f "%Sf"` check above.
