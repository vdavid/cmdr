# Network scanner (SMB/MTP) details

Read this before any non-trivial work in `network_scanner/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

This area owns the three `Volume`-trait BFS walks (fresh, reconcile, and the search-driven scoped cover), their
round-trip disciplines, the terminal-disconnect partial-preserving finish, the consecutive-failure backstop, scan
pacing, and the NAS system-dir skips. Points outward: the registry / phase machine / freshness / gating / manual-rescan
routing in `../lifecycle/DETAILS.md`; the honest-sizes model + `dir_stats` ledger + the shared `Arc<AtomicI64>` id
counter in `../writer/DETAILS.md`; the reconcile mode predicate, the shared per-dir diff (`diff_dir_against_db`), the
`BulkReconcileGuard`, and the completion-handler empty-root policy in `../reconcile/DETAILS.md`; mount-relative path
spaces in `../paths/DETAILS.md`; SMB/MTP transport enable + live watch in `../transports/CLAUDE.md`. The local guarded
walker is a different scanner: `../scanner/DETAILS.md`.

## The `Volume`-trait scan path

`scan_volume_via_trait(volume, root, writer, progress, cancelled)` is a BFS over `Volume::list_directory`, the same API
the live pane uses. BFS (not DFS) so a directory's id is registered in the `ScanContext` before its children are listed
(their parent lookup must hit). It produces `EntryRow`s into the EXACT downstream pipeline the local scan uses — its own
`ScanContext` for ids/parent-ids (scan root → `ROOT_ID`), the shared `Arc<AtomicI64>` counter, `InsertEntriesV2` batches
to the single writer, and `ComputeAllAggregates` + `WalCheckpoint` on clean completion. (The `ScanContext` path→id map
stays here on the serial network BFS; the parallel local walker dropped it in favor of the carried-`parent_id` model.)
Sizes come from `FileEntry.size` (SMB stat); since SMB has no separate physical size or inode, physical mirrors logical
and inode is `None`. Symlinks contribute no size (matching the local scanner's `du`-style omission).

No walk here names a backend, which is what makes the coverage concept work on a future one for free: `lifecycle/cover/`
picks between "read the disk" and "ask the `Volume`", and everything downstream of a discovered entry — ids, writer,
epochs, `dir_stats`, the frontier query, the descent rule — is identical either way.

Three disciplines for network round trips (in `list_one_directory`, and in `stat_one_directory`, which the cover walk's
chain materialization uses for a single path):

- **Cancelable at every round trip**: the walk's `CancellationToken` is checked before each directory listing and
  threaded into `list_directory_for_scan`, so an in-flight MTP listing bails within one round trip. A cancel flushes the
  current batch and returns `Err(VolumeScanError::Cancelled)` carrying the partial totals.
- **Timeout-wrapped, but DETACHING**: each listing runs in its OWN task and `LIST_TIMEOUT` (120 s) races that task's
  JOIN HANDLE, so a wedged mount yields `VolumeScanError::Timeout` instead of parking forever. ❌ Never wrap the listing
  future directly: dropping the handle detaches the task, dropping the future cancels it mid-round-trip, and on MTP that
  abandons a PTP transaction and wedges the phone (`mtp/connection/CLAUDE.md`). A background MTP scan crosses 120 s
  routinely, since it parks at `background_yield_point` while the user is active.
- **`autoreleasepool`-drained per listing on macOS**: the SMB listing path touches NSURL/`NSString`-adjacent ObjC, and
  unpooled autoreleases leak multi-GB over a long walk (same rule as the writer thread). We can't hold an
  `autoreleasepool` guard across an `.await` (it isn't `Send`), so we drain AFTER the await resolves
  (`drain_autorelease_pool` wraps a no-op closure), not around it.

A sub-directory that fails to list (permission, transient) is skipped and the walk continues (like the local walker
skipping errored dirs); failing to list the ROOT is fatal (nothing to index) so the caller discards.

### Terminal disconnect keeps an honest partial; cancel discards

A mid-walk **disconnect** (the typed `DeviceDisconnected`/`Disconnected`, or the consecutive-failure backstop for a
disconnect-shaped untyped error) is TERMINAL: the walk stops immediately rather than churning the still-queued dirs into
silently-empty rows (the reported prod bug). Before returning the typed error, it runs the partial-preserving write
sequence (`finish_partial_scan`: flush + `MarkDirsListed` + `ComputeAllAggregates`) so the kept partial is
self-describing — scanned subtrees roll up to `min_subtree_epoch > 0` (exact, stale once the epoch is bumped), unscanned
ones stay `0` (`—`/`≥`). The completion handler (`lifecycle/manager.rs`) then keeps the instance + DB and marks the
volume Stale.

A **user cancel** still discards: it returns `VolumeScanError::Cancelled` with no marks/aggregate, and the completion
handler resets the volume to gray. `is_terminal_disconnect` returns false for it, which is what keeps the two apart.

This scanner NEVER writes the `scan_completed_at` meta marker (on any path); the caller's completion handler does, only
on a clean finish — the same `scan_completed_at`-absent ⇒ no-Fresh / heal-to-rescan mechanism the local scanner relies
on.

### The consecutive-failure backstop (`CONSECUTIVE_FAILURE_ABORT`)

A single global counter (`usize`, `CONSECUTIVE_FAILURE_ABORT` = 32) over the serial BFS: consecutive failed listings
with no success in between abort the WHOLE walk as a disconnect-shaped terminal error (running `finish_partial_scan`
first). A single success resets it. Under the concurrency pump below, "consecutive" spans up to `FULL_LISTING_BUDGET`
in-flight failures rather than strictly one at a time — the same loose-consecutive caveat the local walker's per-subtree
give-up budget notes; the two are mirrored (not shared) counters: this one aborts the whole serial walk, the local one
prunes one parallel subtree.

### Bounded-concurrency walk (`FULL_LISTING_BUDGET`)

All three walks keep up to `FULL_LISTING_BUDGET` (64) `list_directory` round trips in flight at once via a
`FuturesUnordered` pump, instead of one-at-a-time. Directory listing is latency-bound — each dir is an open+query+close
round trip over an otherwise-idle link — so overlapping them is a near-linear speedup (one real first-scan went from ~28
dirs/s serial to ~137 dirs/s and ~4,700 entries/s, ≈7–8× end to end). **Only the network I/O overlaps**: results are
processed serially on the walk task, so `ScanContext` id allocation (fresh) and the DB read connection + diff
(reconcile) stay single-owner with no locking, and the "a dir's id is registered before its children are listed"
invariant still holds — a child is enqueued only after its parent's result is processed.

**Decision/Why concurrency is safe for the data-integrity guarantees:** cancel drops the in-flight set (the smb2/MTP
backends tolerate a dropped request waiter); a typed terminal disconnect stops topping up and runs the
partial-preserving finish; the consecutive-failure backstop still trips on a real disconnect (failures pile up with no
successes to reset the counter). The reconcile path's new-dir id resolution flushes at a WAVE boundary (queue AND
in-flight both drained) rather than per BFS level. Pinned by `walk_lists_directories_concurrently` (proves
max-in-flight > 1, capped at `FULL_LISTING_BUDGET`) plus the disconnect/backstop tests (bounded stop, no full-queue
churn) and the reconcile-correctness suite (identical index vs a from-scratch scan).

**Decision/Why 64, and where the new ceiling is** (measured on a raidz1-of-4-HDDs QNAP, 64 GB RAM, ZFS, 2026-06-29):
past ~64 there's little to gain because the bottleneck moves off the network and onto the single SQLite **writer**. At
128 in-flight on a fresh scan the writer's queue spiked into the thousands during big-directory bursts (it processed
~24k messages in a 5 s window at ~98% busy, then drained to ~0), backpressuring the walk. The NAS itself was never the
limit: the HDDs sat ~10–18% busy (ZFS ARC served most directory metadata from RAM, so the platters barely moved — a
genuinely _cold_ scan would lean harder on raidz1's ~150 random IOPS), CPU was ~idle, and SMB credits weren't observed
saturating. So `FULL_LISTING_BUDGET` is set where the concurrency win is essentially captured without piling work onto
the writer or a busy NAS.

### Two levers past 64 in-flight: connections, and the writer

`FULL_LISTING_BUDGET` stays 64 — but a later NAS-side probe (2026-07-22) showed the _cold_ single-session plateau is
per-connection serialization in the server's ksmbd, not the disks, and that spreading the SAME 64 in-flight listings
over several TCP connections lifts cold throughput ~3.8×. That's a BACKEND concern, not a scanner one: the SMB backend
opens a small pool of extra sessions per scan and `list_directory_for_scan` fans out across them, invisibly to this walk
(the global budget still caps total concurrency). Canonical: `file_system/.../backends/DETAILS.md` § "SMB
scan-connection pool"; evidence: `~/projects-git/vdavid/smb2/docs/benchmark-findings.md`.

At ~4× listing throughput the single writer's per-second insert rate rises the same, so the FRESH scan
(`scan_volume_via_trait`) now wraps its `InsertEntriesV2` stream in ONE explicit transaction committed on an interval
(`SCAN_COMMIT_INTERVAL`, 2 s) via `begin_scan_tx` / `commit_scan_tx`. `insert_entries_v2_batch` already savepoints each
batch, so in autocommit every batch was an fsync; the outer transaction amortizes fsync to once per interval.
`commit_scan_tx` (idempotent) closes the transaction before EVERY exit — clean finish, cancel, root-fatal, empty-root,
disconnect, consecutive-failure — so the connection never returns mid-transaction and `finish_partial_scan`'s marks +
`ComputeAllAggregates` run in autocommit exactly as before (marks still precede the aggregate). **Crash-safety:** an
uncommitted transaction rolls back on process death → the partial is lost → next launch heals to a rescan (identical to
today's `scan_completed_at`-absent behavior); marks/aggregate are still sent AFTER the inserts commit, so a crash never
leaves ancestors claiming exact sizes over an unstamped descendant. Reconcile is untouched — it already brackets its
bulk writes via `BulkReconcileGuard`. The remaining lever is fewer round trips per huge directory (a larger
`QueryDirectory` buffer in smb2), NOT more in-flight listings.

## Yielding to navigation and transfers (`scan_pace.rs`)

The walk's listing budget isn't a constant: at every top-up it asks `ScanPacer::listing_budget()`, which returns
`FULL_LISTING_BUDGET` (64) while the share is quiet and `YIELDING_LISTING_BUDGET` (1) while a higher-priority claim
holds it — the user browsing it, OR a user-initiated transfer touching it (the host's order: interactive > transfers >
indexing; the transfer signal is `priority::transfers`' per-volume gauge). All three walks read it, the search-driven
cover walk included: it runs over the same session the pane browses through, so a search of one folder must not bury the
navigation the user makes while reading its results. **Why it exists:** a scan and the pane's own listings share ONE SMB
session (every `SmbVolume` clone multiplexes frames over the same connection), so 64 in-flight listings bury a
navigation behind the backlog — a 40-entry folder took **10.7 s** to open mid-scan on a real QNAP (`/Volumes/naspi`, ~2M
entries, 2026-07-19) and was instant the second the scan finished. That's also the first impression the app makes on
someone who connects a NAS and enables indexing because it sounds good.

**The signals** are `priority::foreground`'s per-volume timestamp, stamped by the listing IPC
(`note_foreground_activity_on`) on every navigation, and `priority::transfers`' per-volume gauge, raised for the whole
life of a write operation touching the volume. A browsed share counts as in use for `SCAN_FOREGROUND_IDLE_THRESHOLD` (2
s) after the last navigation — long enough to span the gaps in real browsing so a session of clicking around is ONE
throttled stretch, short enough to be back at full speed a couple of seconds after the user stops. There's no separate
debounce: the window IS the debounce. The transfer signal needs no window at all (an op's start and finish are exact).

**Decision/Why throttle instead of park, and why no anti-starvation floor.** The obvious gate ("only scan while idle")
converts "indexing is in the way" into "indexing never finishes", and then needs a quota, a minimum-progress floor, or a
consecutive-yield cap to climb back out — all state that can be reset wrong, leak, or wedge. A budget that bottoms out
at ONE listing instead of zero makes forward progress **structural**: browse the share non-stop for an hour and the scan
spends that hour at one listing at a time and still completes. Nothing to expire, nothing to re-arm. The cost is that a
throttled scan is roughly an order of magnitude slower, which is the correct trade for background work with no deadline.
❌ Don't "improve" this by letting the yielding budget reach 0.

**What the user feels.** In-flight listings are never cancelled (that would throw away a completed round trip), so the
yield takes effect within one drain of the current backlog: the navigation that TRIGGERS the throttle still waits out up
to 64 in-flight listings, and every one after it queues behind at most one. If that first hop ever measures badly on
real hardware, the lever is a lower `FULL_LISTING_BUDGET`, not cancelling in-flight work.

**Decision/Why the scope is per volume, not app-wide.** The contention is one share's SMB session, so browsing a LOCAL
folder is no reason to slow a NAS scan — the app-wide signal would throttle it for activity that isn't competing at all.
Media enrichment keeps reading the app-wide signal, because it's heavy on-device ML where any foreground work is reason
enough to wait; `priority/foreground.rs` documents the two scopes side by side. A volume nobody has browsed has no entry
and reads as idle, so a first scan starts at full speed. ❌ Don't collapse a missing entry to a `0` timestamp: `0` is a
real point on that clock, so "never browsed" would read as "browsed at startup" and throttle every scan for the app's
first two seconds.

Pinned by `browsing_the_share_throttles_the_scan_to_one_listing_in_flight`,
`a_continuously_browsed_share_still_finishes_its_scan` (the anti-starvation guarantee, end to end),
`browsing_a_different_volume_does_not_throttle_the_scan` (the scope decision), and the pure-decision tests in
`pace_tests.rs` (including `the_budget_is_never_zero_for_any_input`). The transfer side of the same problem lives in
`file_system/volume/backends/smb/foreground_yield.rs`.

## NAS snapshot/system dirs aren't recursed (`system_dirs.rs`)

The BFS does NOT descend into NAS snapshot/system pseudo-directories (`@eaDir`, `@Recently-Snapshot`, `@Recycle`,
`#recycle`, `#snapshot`, `.snapshot`, `.zfs`, `.AppleDouble`, `$RECYCLE.BIN`, `System Volume Information`, …; matched
case-insensitively by `system_dirs::is_recursion_excluded_dir`). Both the fresh scan and the reconcile walk apply it:
the dir's own row is still indexed (so it stays listed and navigable — a user can walk into `@Recycle` to restore a
file), but its subtree is never walked, so it rolls up as honestly-unknown (`—`/`≥`) rather than a misleading total.
**Decision/Why:** these dirs are hardlinked, huge, and re-walking them costs a full filesystem traversal _per snapshot_
over serialized SMB — a real first-scan stalled near 50% grinding `@Recently-Snapshot`, which alone reported 44 TB on a
10 TB volume. Summing them is both ruinous and wrong (the bytes are deduped, not real consumed space). **Guardrail:**
don't remove the exclusion to "fill in" the missing sizes — that re-triggers the stall. Scope is the SMB/MTP side (the
home of these dirs): both walks here, plus the SMB live watcher, which drops a `CHANGE_NOTIFY` landing under such a dir
so a live event can't re-create what the walk won't write (`../transports/DETAILS.md` § "Live SMB watch → index"). The
local walker has its own `should_exclude` (`../scanner/DETAILS.md`). `FileEntry` carries no DOS hidden/system attribute
today; if one is plumbed through, "hidden + system" would generalize this without the hardcoded list.

### The bar for adding a name (it drops the folder from the index)

A name in this list means "the scanner never walks here", and adding one re-arms the rebuild below, so a false positive
costs a user their indexed folder until they rename it. A candidate needs a vendor/protocol citation, and it needs to be
a name no user would pick. Vendor attribution, verified 2026-07-25 against vendor docs:

- **Synology DSM**: `@eaDir` (media-index thumbnails, in every folder holding indexed media), `#recycle`, `#snapshot`,
  `@sharesnap`, `@tmp`.
- **QNAP QTS**: `@Recently-Snapshot`, `@Recycle`, `.@__thumb`. QNAP's FAQ documents `@Recently-Snapshot` as the
  SMB/AFP/FTP-visible snapshot view; QTS docs document `@Recycle` as created per shared folder.
- **NetApp ONTAP**: `.snapshot` (the NFS-side name). **Linux snapper / Btrfs**: `.snapshots`. **OpenZFS**: `.zfs`
  (dataset root, hidden unless `snapdir=visible`). **Netatalk/AFP**: `.AppleDouble` (every folder), `.AppleDB`,
  `.AppleDesktop`, `Network Trash Folder`, `TheFindByContentFolder`, `TheVolumeSettingsFolder`. **macOS**:
  `.TemporaryItems`. **Windows/NTFS**: `$RECYCLE.BIN`, `System Volume Information`.

**Decision/Why `~snapshot` is deliberately NOT on the list.** It's ONTAP's SMB rendering of `.snapshot`, so it looks
like the obvious addition. But an SMB 2.x client cannot enumerate it even with `showsnapshot` enabled — it's reachable
only by typing the path — so a `~snapshot` that actually appears in a listing is a user folder, making the entry pure
false-positive risk with zero benefit. Pinned by `does_not_exclude_the_smb_invisible_ontap_snapshot_name`. Windows
"Previous Versions" uses the SMB shadow-copy FSCTL, not a pseudo-directory, so it needs no entry either.

**Weaker entries, kept but worth knowing:** `@sharebin` has no vendor documentation behind it; `.snapshots` is snapper's
default subvolume name, which a Btrfs user could plausibly have created themselves; `@tmp` and `@sharesnap` live at the
Synology volume root and so normally aren't reachable through a share at all.

### Rebuilding an index that predates the current list

The exclusion only stops the walk. Rows an OLDER index (or a pre-exclusion build) already wrote under such a dir are
invisible to every later pass, because a reconcile diffs the dirs it LISTS and this one is never listed. On the author's
QNAP index that was 10 898 710 rows, 80% of a 13 541 603-row, 1.88 GB DB, against a last-scan count of 2 642 902 —
enough to roll a 10 TB NAS up to 89 TB and make every O(entries) walk pay 5×. Measurements:
`docs/notes/excluded-subtree-rows-2026-07-25.md`.

**Decision/Why we rebuild instead of pruning in place.** A prune is a migration: it has to find the roots, delete
post-order, survive a mid-run quit, un-inflate every ancestor, and stay provably narrower than the scanner's own rule,
all so an index nobody would miss keeps its rows. The drive index is a disposable cache (`../CLAUDE.md` § "Rebuild,
don't migrate"), and a NAS rescan is ~10 minutes, so the index is invalidated and rebuilt instead. That also fixes
whatever ELSE an old build got wrong, which no targeted prune can claim.

The mechanism, all in `system_dirs.rs` plus two call sites:

- `exclusion_list_fingerprint()` digests the name list; `exclusion_stamp_message()` writes it under
  `store::SYSTEM_DIR_EXCLUSIONS_KEY`, and `lifecycle/network_scan.rs::start_volume_scan` sends that message ONLY right
  after a `TruncateData`. That's the one moment the DB provably holds nothing beneath an excluded dir. A reconcile never
  stamps: it can't clear what an older list let in.
- `index_predates_exclusion_list()` compares the stamp to the current list. `resume_or_scan_network` asks at load and,
  on a mismatch, runs `start_volume_scan(NetworkScanMode::Rebuild, …)` — a truncate + full walk, not a reconcile. **Why
  at load:** a completed network index loads Stale and never rescans on its own, so an existing install would otherwise
  keep its rows until the user asked for a rescan by hand.
- **Why a content fingerprint, not a schema bump or a version constant.** `SCHEMA_VERSION` deletes EVERY index on the
  machine, including a 6.9M-entry local one, to fix a network-only problem; a hand-maintained version constant is one
  someone forgets to bump when they add a name. The fingerprint is derived from the list's contents, so growing the list
  re-arms every existing network index automatically and shrinking it doesn't resurrect anything.
- **Why it can't loop.** The stamp lands with the truncate, before the walk, so even a rebuild the user cancels leaves
  an index that's honestly built against the current list. A rebuild that can't START (share unmounted) writes nothing
  and re-arms on the next load.
- **Why locally-scanned volumes are untouched.** The stamp and the rebuild live on the network scan path only, which
  `Local`/`LocalExternal` volumes never take. The local walker indexes a folder called `@eaDir` or `.snapshot` in full,
  and rebuilding a local index against this list would be pure loss.

## Empty root

The two network walkers (`scan_volume_via_trait`, `reconcile_volume_via_trait`) return the typed
`VolumeScanError::EmptyRoot` when the ROOT listing yields ZERO children, so the completion handler takes its `Err` arm
and writes NO `scan_completed_at`. A false "complete" over a transiently-empty root permanently strands the index
(startup loads Stale and never rescans; a manual rescan re-"completes" the same empty root). The full completion-handler
policy — empty (`EmptyRoot`) vs failed (`Volume`/`Io`) root, why both reconcile paths bail BEFORE diffing the root, and
the accepted genuinely-empty-volume false-negative — is canonical in `../reconcile/DETAILS.md` § No completion marker on
an empty root.

## The scoped cover walk (`cover_scan.rs`)

`cover_volume_subtree(volume, root, space, writer, emit, cancel, pacer)` is the search-driven half of the coverage
concept over the `Volume` trait: it covers ONE frontier node that `Index::coverage` named, feeding the entries it finds
to a live consumer while filling the index. Its driver is `lifecycle/cover/`, which picks between it and the local
guarded walker by volume kind and owns the frontier loop, the claims, and the session bracket. It keeps every round-trip
discipline above (cancel per round trip, `LIST_TIMEOUT` racing the JOIN handle, autoreleasepool, typed-disconnect and
consecutive-failure backstops, `ScanPacer`, the NAS system-dir skip) and diverges from the two whole-volume walks in the
places below, all of them consequences of a person having asked:

- **Scoped root.** The root resolves through `space.index_relative` + `resolve_scan_root(.., false)` to its own entry
  id, never `ROOT_ID`, and the BFS carries `(path, id)` pairs the way the reconcile walk does.
  `lifecycle/cover/bootstrap.rs` has already materialized the ancestor chain when the index had no row for it, using
  `stat_one_directory` for the same reason the listings use `list_one_directory`.
- **A cancel KEEPS its coverage.** The finish sequence (flush → `MarkDirsListed` → `ComputeSubtreeAggregates`) runs on
  EVERY exit: clean, cancel, disconnect, unlistable root. **Decision/Why:** convergence. A search that walks eight
  minutes of a NAS and is then cancelled has to leave the frontier genuinely smaller, or repeated searching over the
  same area never gets faster and the walk is pure loss. The whole-volume scan is the opposite case (a half-built index
  of a share is not an index of the share), so the two rules disagree on purpose. The aggregate is the SUBTREE one, and
  the writer repairs the ancestor chain above it from there.
- **Add-only, per directory.** Before writing a listing's rows it reads the names the index already holds under that
  directory (`list_children_on`, folded through `normalize_for_comparison`, the same fold `idx_parent_name_folded`
  uses). A name that's already there keeps its row and its id; a directory among them is descended into with that id.
  **Decision/Why not the local walker's virgin-root refusal:** the parallel walker refuses non-virgin ground because a
  per-directory DB lookup from eight worker threads would cost real time against a `readdir` that costs microseconds.
  Here the lookup is an indexed query against a listing that cost a network round trip, so taking the case is cheaper
  than refusing it — and it removes the need for a repair path, which over the trait would have meant a second walk. The
  cost is that stale rows under covered ground aren't corrected; that's `reconcile/`'s job (Decision 5 trusts a
  covered-but-stale subtree rather than re-walking it).
- **MTP same-name siblings become explicit.** The store holds one row per `(parent_id, name_folded)` and
  `insert_entries_v2_batch` is `INSERT OR IGNORE`, so a second child with the same name would take an id, be queued as a
  directory in its own right, have its children written under that id, and then lose the row the id belonged to —
  orphaning everything below it. The name check makes it "keep the first, log the rest". Pinned by
  `cover::network_tests::a_same_name_sibling_keeps_the_first_row_rather_than_orphaning_a_subtree`.
- **NAS system directories are stamped `unreadable_cause = Declined`, not left unlisted.** Both whole-volume walks index
  such a directory's own row and refuse its subtree, which leaves it at `listed_epoch = 0` — and that is precisely what
  the descent rule calls FRONTIER, so a search over a NAS would be handed the hardlinked per-snapshot tree this area
  exists to keep the walk out of. Marking it says "nothing is coming for this subtree", which is what the column means
  and what a user is owed; the descent rule needs no new case and no per-kind branch. The mark survives because nothing
  ever lists the directory (`mark_dirs_listed` is what clears it), and a change to the name list re-arms the whole index
  anyway. A frontier rooted AT one is marked and refused without a single round trip, which is what heals an index built
  before this rule. ❌ The cause is `Declined`, never `Denied`: nobody refused us here, and a user offered Full Disk
  Access over a snapshot tree would be sent to fix something that isn't broken.
- **No empty-root refusal.** `VolumeScanError::EmptyRoot` exists because a share that lists empty is a glitch, and a
  false "complete" strands the whole index. An empty FOLDER is an ordinary thing to search, and refusing to mark it
  would hand it back to every later search forever, so the cover walk marks it listed and moves on.
- **A directory it couldn't read is stamped `Abandoned`, but only once the share proves it was still there.** The
  section below is the whole rule; it's the one divergence that exists because a SHARE can fail the way a directory
  does, rather than because a person asked.

Tests live with the driver, over an `InMemoryVolume`, because what they pin is the walk's contract with a backend rather
than anything SMB- or MTP-specific: `lifecycle/cover/network_tests.rs` for the walk, and
`lifecycle/cover/network_give_up_tests.rs` for what it does with a directory it couldn't read.

### A failed listing is held until the share answers again

A directory whose listing FAILS (the `Err(err)` arm of the BFS loop) is eventually stamped
`unreadable_cause = Abandoned`, which takes it off the coverage frontier and puts it on the persisted per-volume retry
backoff. Without a cause it would stay frontier, and every later search over an ancestor scope would hand it to a walk
that re-pays the same failing listing, forever, with nothing converging — over a share that is up to `LIST_TIMEOUT` (120
s) per directory per search, eight times what the local walker pays for the identical mistake (the local half measured
1,497 `ETIMEDOUT` directories at 101 s of a 147 s walk, `docs/notes/phased-vs-bulk-index-2026-08-14.md`). The cause, the
writer message, the arming, and the coverage bucket are all shared with the local walker and needed nothing new for this
side; the canonical description of the three causes is `../store/DETAILS.md` § "What coverage needs".

⚠️ **The mark is NOT written when the failure happens, because at that moment the walk can't tell whose failure it is.**
Two different things wear one shape in that arm and they want opposite answers:

- **One directory that won't list** while the share is otherwise healthy. `Abandoned` is right: stop offering it, retry
  it on the backoff.
- **The share itself going away**, which fails listings one directory at a time through the SAME arm. Marking those
  would condemn every directory the walk had queued — potentially thousands — for a disconnect that heals the moment the
  NAS wakes up, and a hole that big is worse than the re-paid listing the mark exists to save. The abandoned ground
  would then be invisible to search until a retry window that grows to 24 h reopened it.

**Decision/Why the boundary is "the share answered again", not "the walk survived".** A give-up is HELD in
`CoverWrites::unproven` and only joins `abandoned` when a LATER listing succeeds — that success is the share answering
after this directory wouldn't, which is the only evidence available that the failure was the directory's. Two things
fall out of it:

- **Nothing branches on how the walk ended.** `finish` stamps exactly the proven set on every exit — clean, cancel,
  unlistable root, typed disconnect, consecutive-failure abort — and drops whatever is unproven. ❌ Don't add an
  exit-path case here; a new one would silently disagree with the others. Both directions are pinned:
  `a_cancelled_walk_keeps_the_give_ups_it_proved` (a cancel says nothing about evidence a walk already had) and the two
  share-went-away tests.
- **A share can go away without the walk ever concluding it did.** A small scope's queue runs dry after a handful of
  failures, so the walk ends REPORTING the scope covered over a share that isn't there. A rule keyed on the abort would
  write off every one of those directories, and `a_share_that_goes_away_with_little_left_to_walk_gives_up_on_nothing` is
  the test that rules it out. Cancelling is the same shape: a stalled search is one a person stops long before the
  backstop has seen 32 failures.

A walk that DOES conclude the share went away (a typed `DeviceDisconnected`, or `CONSECUTIVE_FAILURE_ABORT`) calls
`share_went_away`, which drops the PROVEN give-ups too: under the concurrency pump a success can return after a failure
purely because it was already on the wire when the session dropped, so up to a budget's worth of "proof" is suspect the
moment the share is.

**Decision/Why hold rather than mark-and-unwind.** Marking as the walk goes and deleting the marks on an abort costs
everything holding costs (the walk remembers the same ids either way) plus a repair path: a second writer message that a
crash, a dead writer, or a new exit path can skip, leaving the database holding exactly the damage the rule exists to
prevent, at exactly the moment the app is least healthy. It would also make the condemned share readable by a search
running concurrently with the walk, since coverage reads through its own connection. Holding has no window to be wrong
in, and a killed process writes nothing — "we learned nothing" is the honest default, and the failed directories stay
frontier exactly as they do today.

**Decision/Why a share's permission denial is `Abandoned` and not `Denied`.** `VolumeError::PermissionDenied` from a
share is a server-side ACL, and `Denied`'s whole meaning downstream is "the user can fix this by granting Full Disk
Access" — advice that does nothing over SMB. The cost of the softer answer is one round trip per backoff window
(converging on 24 h) against a folder that will keep refusing, which is negligible. This matches `../store/DETAILS.md`'s
"one `Abandoned` for all three producers, ❌ don't split by errno".

**Why the tests run over an `InMemoryVolume` and not the Docker SMB fixtures.** `smb-consumer-flaky` cycles 5 s up / 5 s
down on its own schedule, so a test over it can neither place a disconnect at a chosen point in the walk nor tell a
"nothing was marked" pass from a run that never hit the down window; and the `cmdr-index` crate has no SMB backend at
all (the Docker servers are reached only from the app crate). What these tests pin is the walk's contract with A
backend, which is why `going_away_after` fails with an untyped `IoError` rather than `DeviceDisconnected`: the typed
variant has its own arm and would prove nothing about the one that matters.

## Reconcile

`reconcile_volume_via_trait` is the rescan-in-place BFS: it keeps every `scan_volume_via_trait` round-trip discipline
(cancelable, `LIST_TIMEOUT`-wrapped, `autoreleasepool`-drained, the typed terminal-disconnect branch, the
consecutive-failure backstop) but diffs each dir against the DB via the shared `diff_dir_against_db` instead of
inserting fresh, so the last-good index stays visible-stale throughout. The mode predicate (reconcile vs truncate), the
shared per-dir diff, the `BulkReconcileGuard` delta-propagation suppression, and the finish (`MarkDirsListed` → one
`ComputeAllAggregates`) are all canonical in `../reconcile/DETAILS.md`.
