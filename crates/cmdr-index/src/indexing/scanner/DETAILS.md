# Local guarded scanner details

Read this before any non-trivial work in `scanner/`: editing, planning, reorganizing, or advising. Must-know guardrails
are in `CLAUDE.md`.

This area owns the LOCAL fresh-scan walker and the shared exclusion policy. Points outward: the honest-sizes model
(`listed_epoch` / `min_subtree_epoch`), the `dir_stats` ledger, and the shared `Arc<AtomicI64>` id counter are canonical
in `../writer/DETAILS.md`; the serial LOCAL reconcile walk (which reuses the `GuardedReader` + `LOCAL_LIST_TIMEOUT` from
here) and the cost budget in `../reconcile/DETAILS.md`; `IndexPathSpace` + mount-relative resolution in
`../paths/DETAILS.md`; the registry, phase machine, and `IndexVolumeKind` capability axes in `../lifecycle/DETAILS.md`;
the shared `extract_metadata` primitive at `../metadata.rs` (documented in the [hub](../DETAILS.md)). The network
(SMB/MTP) walker is a different scanner entirely: `../network_scanner/DETAILS.md`.

## Module structure

- **mod.rs** — the scan driver: `scan_volume()` (full scan) / `scan_subtree()` (targeted subtree rescan, used by
  post-replay background verification and the per-navigation verifier) / `cover_subtree()` (the search-driven walk over
  a coverage frontier node), `run_scan`, the `ScanRoot` mode, the `ScanConfig` / `ScanProgress` / `ScanHandle` /
  `ScanSummary` / `ScanError` / `CoveredEntry` types, and `LOCAL_LIST_TIMEOUT` (15 s). No path→id map: the walker
  carries each directory's id to its own read, so the visitor attributes children to their parent via the carried
  `parent_id` (`dir.id`) directly, allocating fresh child ids from the shared `Arc<AtomicI64>` counter owned by
  `IndexWriter`. The scan root resolves via `resolve_scan_root` (`ROOT_ID` = 1 for a volume scan, the existing entry id
  for a subtree scan). Sizes come from a per-child `symlink_metadata` (lstat); physical sizes are `st_blocks * 512`.
  Hardlink dedup: files with `nlink > 1` are tracked in a mutex-guarded `HashSet<u64>` by inode (workers run
  concurrently); only the first link's size counts, later links get `size = None`. `nlink == 1` files skip the set. All
  files store `inode`; directories and symlinks get `inode: None`.
- **insert_visitor.rs** — the fresh-scan `DirVisitor`, and the ordering rules its one `Pending` lock enforces (see
  "Marks ride with their rows" below).
- **heartbeat.rs** — what a cover walk reports about itself between batches. **live_emit.rs** — `EmitPacer`, when a
  partial batch of found entries goes to a live consumer anyway (see "The cadence" below).
- **convergence_tests.rs** — what a walk leaves behind when it doesn't finish. `heartbeat_tests.rs` and
  `live_emit_tests.rs` cover the two live-consumer facts. `test_fixtures.rs` holds the writer / temp-tree / mock-reader
  fixtures every test module builds on, including the `ReadGate` that parks one read so a test can look at a walk that
  is genuinely still running.
- **walker/** — the hang-tolerant engine (`walk`, `std_read_dir`, the `DirVisitor` trait, `DirTask` / `RawDirEntry` /
  `WalkReadError` / `WalkConfig` types, the watchdog, the progress-timeout verdict, the `SubtreeBudget` give-up budget)
  plus `bulk_read` (the `getattrlistbulk`-batched `bulk_read_dir` used in production on macOS). Tests are in
  `walker/tests.rs`, all millisecond-scale with a mock reader.
- **The bulk read is shared with the serial reconcile walk.** `bulk_read::bulk_read_dir_unwatched` is re-exported at
  `scanner` level (along with `RawDirEntry` / `RawFileType`) for `reconcile::reconciler::read_fs_children`, which reads
  on its own `GuardedReader` thread and has no watchdog to publish `ReadProgress` to. The walker engine itself stays
  private to the scanner. It returns a `BulkDirRead` (entries + an `unusable` count) rather than a bare `Vec`, because
  the reconcile can't tolerate a silently-short listing: `diff_dir_against_db` deletes index rows the live listing
  lacks, so it re-reads a directory with `read_dir` when `unusable > 0`. See `../reconcile/DETAILS.md`.
- **Degrade, never drop.** `parse_entry` returns an entry with `stat: None` (the caller pays one `symlink_metadata`)
  when an attribute it needs wasn't returned, or when the type isn't file / dir / symlink and so carries no inline size
  — a fifo, socket, or device node. It returns `None`, counted as `unusable`, only for a record with no recoverable name
  or type. Both branches are unreachable on the filesystems we've measured; the synthetic-record tests in `bulk_read.rs`
  are what keep them correct.
- **exclusions.rs** — the self-contained two-tier path-exclusion policy: `EXCLUDED_PREFIXES`, the
  `FIRMLINKED_SYSTEM_PREFIXES` allowlist, `JUNK_BASENAMES`, `PSEUDO_FS_BASENAMES`, the `ExclusionScope` /
  `ExclusionTier` types, `should_exclude`, `e2e_allowlist_path`, `is_canonicalization_alias`, and `default_exclusions`
  (`#[cfg(test)]` only). Re-exported at `crate::indexing` level so existing `scanner::should_exclude` callers are
  unchanged. It's the single exclusion gate for every code path (scanner, reconcile, watch verification, verifier). Its
  tests live in `exclusions_tests.rs`, pulled in as `exclusions`'s own `tests` child module via `#[path]` so they keep
  reaching the module's private helpers.
- **tests.rs** — the scanner-driver test module.

E2E scan restriction: when `CMDR_E2E_START_PATH` is set, `should_exclude` restricts scanning to the fixture path, its
children, and ancestors (critical for Docker E2E performance).

## Three scan roots, and what each may do to the ground under it

`ScanRoot` replaced an `is_volume_root` boolean, because the third case is exactly the one a boolean couldn't express.

- **`Volume`** — the whole volume. The root maps to `ROOT_ID` whatever its path is, `seed_current_epoch` runs on a WRITE
  connection, and the per-child exclusion gate applies.
- **`Rebuild`** — a subtree the index already holds, rebuilt from scratch. Sends `DeleteDescendantsById(root_id)` first,
  then re-inserts every child under fresh ids. What `scan_subtree` is, and what the verifier and FSEvents verification
  want: they've just decided the index's picture of that subtree is wrong.
- **`Virgin`** — a coverage frontier node, walked by `cover_subtree` for a search. ❌ Deletes NOTHING.

This walker covers a frontier node only when the volume's ground is a local filesystem. Everything the index reaches
through a `Volume` instead — a share, a phone, a future backend — is covered by `../network_scanner/`'s scoped walk,
which its driver (`../lifecycle/cover/`, `Ground`) picks by volume kind. Nothing else about coverage forks: same writer,
same epochs, same frontier query, same descent rule. The one place the two walks genuinely differ is
`ScanError::NotVirgin` — the trait walk compares each directory's names against the index instead of refusing, because
there an indexed query is free next to the network round trip that produced the listing, while here it would sit in the
hot path of eight worker threads reading a `readdir` that costs microseconds.

Each root also picks the walk's `WalkPolicy` — what it refuses to descend into — described below.

## `WalkPolicy`: what a walk refuses to descend into

Two rules, both about staying on the ground this walk owns, resolved once before the walk starts and carried by the
visitor. Neither is a second source of scope.

**The structural exclusion policy, which EVERY walk runs.** WHICH rules apply comes from the volume KIND, via
`ExclusionScope` (below); whether they run isn't a question — the invariant is that a walk writes the rows a full scan
would write, whatever pointed it at its root.

- A `Virgin` walk MUST: its roots come from a coverage answer that looked at nothing under them, so without the gate a
  scoped search of `/` walks `/private/var` and `/proc`, and the walk-written index stops matching what a full scan
  would have produced.
- A `Rebuild` MUST too, and this is what the gate its callers run does NOT cover. `../reconcile/verifier.rs` and
  `../watch/event_loop/verification.rs` each ask `should_exclude` about the new DIRECTORY before handing it over, and
  that stops at the root: a rebuild of a newly discovered `/Library` used to index `/Library/Caches`, which no boot scan
  does. Verification would write rows a scan would never produce, and structurally-excluded content could surface in
  search results.
- The policy never touches the walk's own ROOT (`insert_visitor` gates discovered CHILDREN), so applying it everywhere
  can't cut a caller's chosen directory out from under it.
- ❌ The `SYSTEM_DIR_EXCLUDES` tier is NOT part of this and never should be (Decision 6 of the unindexed-search plan):
  it's large, it sits under folders people search, and skipping it at walk time would stamp coverage on parents whose
  `dir_stats` are badly short. It's a MATCH-time filter, applied by search, importance, and the folder-size tooltip.

**The volume boundary.** A search targets ONE volume (Decision 4), so `Virgin` pins the device its root sits on and cuts
where another filesystem is mounted. Cut means NO ROW, exactly like an exclusion: a row nothing ever lists would sit in
the coverage frontier forever and re-offer itself to every later search, and the bytes under it belong in the other
volume's `dir_stats`.

- **The pin comes from the WALK's root, not the volume's.** A walk whose root is itself inside a mount would otherwise
  cut away every one of its own children, list the root, and read as fully covered while holding nothing — the same
  silent false-complete `ExclusionTier` exists to prevent one rule over.
- **A device that can't be read is not a boundary.** The walk descends, the read fails, and `visit_read_error` reports
  that honestly rather than this rule guessing.
- **⚠️ File Provider domains are NOT a boundary** (Decision 16). Dropbox, iCloud Drive, and Google Drive report the same
  device as `$HOME` and belong to the boot volume's scope; the guarded walker's stall detection is what makes descending
  into a disconnected one safe. ❌ Never repurpose `cmdr_fs::file_provider::domain_id_for_dir` (wired as
  `RootProbes::is_domain_root`) as a cut — it answers where a volume ROOT sits, for the pseudo-filesystem rule.
- **A full scan pins nothing**, deliberately: it bounds itself by path prefix (`/Volumes/` under `BootDisk`) and pinning
  it would silently change what a boot index contains for anyone with a disk image mounted in their home dir.
- Accepted edge, the same one an exclusion carries: a cut directory's parent reads as covered, so if the drive is later
  UNMOUNTED, the (now ordinary, and almost always empty) mount-point directory stays invisible to search until something
  re-lists its parent — which FSEvents does on the next change there. `/Volumes/X` under the boot scan has behaved this
  way all along.
- Cost: one `symlink_metadata` per discovered DIRECTORY, about 2–3 µs and 3–6% of a walk's wall clock
  (`docs/notes/cover-walk-primitive-2026-08-05.md`, which also records why `ATTR_CMN_DEVID` on the batched read and a
  `getmntinfo` snapshot were both rejected). The probe is a `fn` pointer so a test can put a mount anywhere in a temp
  tree without one, the same way `RootProbes` injects its two.

**Why `Virgin` can't just reuse `Rebuild`.** A frontier node is one nothing has listed, which does NOT mean nothing is
known below it: FSEvents verification upserts newly-seen children under a directory without ever marking that directory
listed (`../watch/event_loop/verification.rs` sends `UpsertEntryV2`, never `MarkDirsListed`) and then scans each new
child directory, which does mark it. So a frontier node can sit above genuinely-covered ground, and
`DeleteDescendantsById` there throws away rows the walk did not write —
`convergence_tests::a_frontier_node_can_hold_a_listed_descendant` builds exactly that state.

**Why `Virgin` can't just drop the delete either.** The walk allocates a FRESH id for every name it finds, and
`insert_entries_v2_batch` is `INSERT OR IGNORE` against `UNIQUE (parent_id, name_folded)`. Over a pre-existing sibling
the fresh row is silently skipped, the walk keeps attributing that directory's children to the id it just lost, and the
whole subtree below it is orphaned — quieter and worse than a constraint error. So `run_scan` checks
`count_children_capped(root_id, .., 1)` on the same connection it resolves the root with, and a non-empty root is
`ScanError::NotVirgin` rather than a walk. `lifecycle/cover/` takes that case to the serial reconcile, which compares by
name and writes only differences.

## Marks ride with their rows

A directory's `listed_epoch` is stamped by `MarkDirsListed`, a PK `UPDATE` that silently updates zero rows. The
directory's own row is written by its PARENT's `visit_dir`, so at the moment its own read succeeds that row may still be
sitting in an unflushed batch — and a mark that overtakes it leaves the directory at `listed_epoch = 0` forever, which
in the coverage model means "walk this again on every search".

`InsertVisitor` accumulates rows, listed ids, and discovered entries in ONE `Pending` behind ONE mutex, and sends
rows-then-marks **inside** that critical section. That makes both overtakes unrepresentable: an id can only be appended
after its own row was (same lock), and two workers can't hand the writer batch 2 before batch 1 (the send is under the
lock, so sends happen in take order). ❌ Don't split `Pending` into separate mutexes, and ❌ don't move the send outside
the lock.

The consequence that matters is convergence: `finish()` flushes rows AND marks on the **cancel** path too, so a walk
someone stopped keeps every directory it read. Before this, `run_scan` returned `Vec::new()` for `listed_ids` on a
cancel and the whole subtree re-entered the frontier on the next search.

Two things fall out for free: the mark-before-final-aggregate ordering invariant now holds by construction rather than
by the caller remembering it, and a long walk's coverage becomes queryable as it goes rather than only at the end.

## Ground the walk couldn't read

Every directory whose contents this walk didn't get is recorded with a cause, so the coverage frontier stops offering it
on every later search. The causes themselves and why there are three are `../store/DETAILS.md` § "What coverage needs";
what this module owns is which one each failure earns and when the message goes out.

`InsertVisitor` accumulates ids into `UnreadableIds`, split two ways:

- **`denied`** — `WalkReadError::Io` whose `ErrorKind` is `PermissionDenied`. Also emits `IndexEvent::PathAccessDenied`,
  which is what puts the "limited by macOS" styling on a TCC-restricted folder in the sidebar.
- **`abandoned`** — the other three ways a listing doesn't arrive: `WalkReadError::TimedOut` (the watchdog condemned a
  stalled read), `WalkReadError::Io` with any other errno, and `DirVisitor::visit_pruned` (the give-up budget dropped
  the task unread). A pruned task never reaches `visit_read_error`, so that hook is its only mention in the whole
  system; without it the pruned MAJORITY of a dead mount — the part the budget exists to avoid probing — would stay
  silently in the frontier.

`send_unreadable_marks` sends one `MarkDirsUnreadable` per cause, in `MARK_CHUNK` batches, from `run_scan` **after
`visitor.finish()`**. Two ordering properties ride on that:

1. Every `MarkDirsListed` this walk earned is already ahead of the marks on the writer channel, so a directory that
   failed once and then succeeded on a retry within the same walk ends up listed rather than pinned unreadable.
   (`mark_dirs_listed` clears the cause anyway, so either order is survivable; this makes it not depend on luck.)
2. ⚠️ The condemned ids are **only the ones a read actually failed on**. ❌ Never compute them as "whatever is still
   unlisted under the root", which is the shape a phase driver reaches for: run it before the walk's own marks commit
   and it condemns everything the walk read but hasn't stamped. That reads as a 2× speed-up and its only symptom is an
   entry count 21% low. `convergence_tests::marking_abandoned_ground_costs_no_coverage` pins it.

## What a walk emits, and to whom

`cover_subtree` takes an optional `EntrySender` (`SyncSender<Vec<CoveredEntry>>`). When one is present the visitor
builds a `CoveredEntry` alongside each `EntryRow` and flushes both on the same boundary, so a search sees results while
the walk is still running (Decision 3 of `docs/specs/unindexed-search-plan.md`: the scan stays in `indexing/`, the
matching stays in `search/`, one channel crossing per batch and no matcher in this crate).

A `CoveredEntry` carries the entry's OWN sizes, before hardlink dedup. Dedup exists so the stored recursive sums don't
count a file twice; a search result row showing a hardlinked file as 0 bytes would just be wrong.

A send failure means the consumer went away (the search dialog closed, the query was superseded). The visitor drops the
sender and **keeps walking**: walking is coverage work, and its rows are in the index for the next query either way.

### The cadence: 2 000 entries, or 100 ms

A batch that only ever went out full is the right size for the crossing and the wrong one for the wait. A search over a
sparse tree (one matching file per directory, which is what most searches look like) finds rows the whole time and shows
none until the walk is nearly done: measured on a 1 642-directory disk image, no rows until the end.

So `live_emit.rs`'s `EmitPacer` gives the pending batch a deadline, `EMIT_INTERVAL` (100 ms) from the moment its FIRST
row lands. Two places consult it, and both walkers own one:

- the push path (`InsertVisitor::push_row`, `CoverWrites::push`), which hands the batch over when the next row arrives
  past the deadline;
- the local walker's **watchdog tick** (`DirVisitor::on_watchdog_tick`), because a walk parked on one slow directory
  calls no visitor hook at all and would otherwise sit on everything it found before it parked. A walk with a live
  consumer therefore runs its watchdog at `EMIT_INTERVAL` rather than the usual second. No thread of its own: the
  watchdog was already awake.

The trait walk (`../network_scanner/cover_scan.rs`) has no watchdog, so its worst case is one listing's round trip
rather than 100 ms; a serial walk with nothing in flight has already handed everything over, so nothing else can delay
it.

❌ Don't shrink the batch to the interval's worth of rows instead. The channel is bounded on purpose (Decision 3), and
100 entries per crossing would spend that bound on chatter. ❌ Don't make the tick unconditional either: the deadline is
what keeps a full scan (no consumer, so nothing ever waiting) from paying for a clock read per entry. 100 ms is the rate
search's own `ResultStream` emits at (`apps/desktop/src-tauri/src/search/live.rs`), so the pipe has one cadence end to
end rather than two that beat against each other.

The engine itself, its progress timeout, and the macOS bulk reader: `walker/DETAILS.md`.

## Scan-scope-aware exclusions (`scanner/exclusions.rs`)

`should_exclude(path, &ExclusionScope)` splits the exclusion policy into two tiers so a mount-rooted scan can index its
own subtree while the boot-disk scan stays off mounted volumes:

- **Tier (a) — boot-disk absolute prefixes** (`EXCLUDED_PREFIXES`: `/Volumes/`, `/System/...`, `/private/var/`, `/dev/`,
  ...; plus the `/System/` firmlink allowlist). Applied ONLY under `ExclusionTier::BootDisk`. These keep the `/`-rooted
  boot scan from wandering onto mounted volumes and system trees.
- **Tier (b) — per-volume skips**, applied under BOTH tiers:
  - **Junk basenames** (`JUNK_BASENAMES`: `.Spotlight-V100`, `.fseventsd`, `.Trashes`, `.TemporaryItems`), matched on
    the path's final component so they're caught at the boot root AND under a mount. `.Spotlight-V100`/`.fseventsd` used
    to be tier-(a) prefixes; they moved here so a mount-rooted scan still skips them.
  - **Pseudo-filesystems at a corroborated Unix volume root** (`PSEUDO_FS_BASENAMES`: `proc`, `sys`, `dev`) — below.

### Pseudo-filesystems at a volume root

A directory named `proc`, `sys`, or `dev` is skipped in every tier when BOTH hold (`is_pseudo_fs_at_volume_root`): it
sits DIRECTLY at a volume root, AND that root is corroborated as a Unix-like filesystem. "Volume root" is the boot
disk's `/`, a `/Volumes/X` mount, an SMB or MTP scan root (all of them `ExclusionScope::volume_root()`), or a **File
Provider domain root** (below).

**Why:** MacDroid mounts an Android phone as a File Provider domain under `~/Library/CloudStorage`, and that phone's
Linux `proc/<pid>/task/<tid>/{attr,ns,fd,net,map_files}` tree cost ~454 s of a measured 21m49s reconcile walk (~35% of
it). Tier (a) only ever had `/proc/`, `/dev/`, `/sys/` as ABSOLUTE prefixes under `BootDisk`, so it caught the boot
volume's and missed every other volume's.

**Half one, root POSITION.** A user's `~/projects/myapp/proc` is an ordinary folder and stays indexed; only
`<volume root>/proc` is a candidate. Pinned by `pseudo_fs_below_the_volume_root_stays_indexed`.

**Half two, corroboration: all three of `proc`, `sys`, and `dev` must be present as sibling DIRECTORIES**
(`has_pseudo_fs_trio`). Position alone is not enough, and this is the half that's easy to "simplify" away later, so:
`dev` is an extremely ordinary name for a real user folder. A developer with `~/Library/CloudStorage/Dropbox/dev` has a
File Provider domain root as that folder's parent, so a name-only rule would drop it from the index and from folder
sizes with NO error at all. A wrong size nobody is told about is worse than a slow walk, and this whole effort exists to
stop silent failures. Any one of the three alone is just a folder name; all three co-occurring is diagnostic. Verified
against the real data: the phone's root lists `proc`, `sys`, AND `dev` among `bin`, `etc`, `sdcard`, …, so it still
qualifies; David's Dropbox root has none of them, so it can never qualify. Pinned by
`a_cloud_folder_named_dev_is_not_mistaken_for_a_pseudo_filesystem` and its `/Volumes/X` twin.

Symlinks don't corroborate (`symlink_metadata`, no follow): an Android root carries a symlink `d` next to its real
`proc`/`sys`/`dev`, and a symlink named `proc` is not the real thing.

**This does NOT replace the boot-disk absolute prefixes.** macOS `/` has `/dev` but neither `/proc` nor `/sys`, so the
boot disk does not satisfy the three-sibling test; `/dev/` and `/proc/` staying in `EXCLUDED_PREFIXES` is what keeps the
boot scan out of them. The corroboration rule is for the OTHER volume roots.

**Cost:** the basename test runs BEFORE either probe, so the syscalls fire only for directories actually named
`proc`/`sys`/`dev` — at most three per volume root per walk, each costing one xattr read plus three `symlink_metadata`
calls. That's why there's no memo: a cache would save single-digit syscalls per walk and cost a shared mutable map on
the walk path. The domain probe is additionally **boot-disk-tier only**: it's a syscall, a mount-rooted scope can sit on
a network mount where any syscall blocks indefinitely, and providers register their domains in the home dir anyway, so
there'd be nothing to find.

**Recognizing a File Provider domain root** (`cmdr_fs::file_provider::domain_id_for_dir`): a domain root carries the
`com.apple.file-provider-domain-id` xattr; its children, `~/Library/CloudStorage` itself, and ordinary folders don't. ~5
µs, a plain APFS read with no XPC, works while the provider is offline, needs no entitlement. It resolves Dropbox,
Google Drive, MacDroid, and iCloud Drive — and iCloud's domain root is `~/Library/Mobile Documents`, which is NOT under
`~/Library/CloudStorage`, which is exactly why a path-prefix heuristic was rejected. Full measurements, the
authoritative-but-costly `NSFileProviderManager` alternative, and the dead ends:
`docs/notes/fileprovider-domain-detection.md` (verified on macOS 26.5.2, build 25F84, 2026-07-20). The reader lives in
`crates/cmdr-fs/src/file_provider.rs`, because the app's sync badge asks the sibling question ("is any ANCESTOR a domain
root?") off the same marker and neither side may own the other's copy.

**The xattr is a private Apple detail, so this is an OPTIMIZATION, never a safety guarantee.** It's undocumented and not
contractual; if Apple drops it, unrecognized domain roots simply go back to being walked. Nothing may depend on it for
correctness or for bounding cost. The actual contract against pathological trees is the cost-budget backstop
(`../reconcile/DETAILS.md`) — the two are not redundant, and neither makes the other unnecessary.

**Injectability:** `ExclusionScope` carries both filesystem questions as `fn(&str) -> bool` pointers (`RootProbes`:
`is_domain_root`, `is_unix_like_root`), so `with_probes` lets tests exercise the rule without a real provider domain or
a real Unix root on the machine. Non-macOS builds get a constant `false` for the domain half.

**Why the split (the false-complete bug it prevents):** a `LocalExternal` scan is ROOTED at `/Volumes/X`, so under the
old single-tier gate every child of the scan root started with `/Volumes/` → `should_exclude` returned true for all of
them → the walker emitted zero rows → the completion path wrote `scan_completed_at` and flipped the drive to Fresh. A
silently empty, falsely-complete index (the same shape as the "rescan does nothing to the NAS" bug). Tier (a) must not
apply to a mount-rooted scan.

**Scope is derived from the volume kind (`mount_rooted()` → `MountRooted`, else `BootDisk`), never from
`is_volume_root`** — the boot `/` scan is ALSO a volume root, so that bool can't distinguish it from a mount-rooted
scan. The `CMDR_E2E_START_PATH` allowlist is a `BootDisk`-only concept (it bounds the otherwise-unbounded `/` walk; a
mount-rooted scan is already bounded to its mount). Enrichment derives the scope from `volume_id` via
`exclusion_scope_for_volume` (root ⇒ boot disk, every other registered volume ⇒ mount-rooted at its registered root), so
a mount-rooted volume never excludes its own `/Volumes/X/...` paths, only junk it navigates into.

`ExclusionScope` is a VALUE carrying the mount root (`None` = the `/`-rooted boot disk) plus the domain probe, not a
bare enum: the root-position rule needs to know where the volume starts, and passing a scope is mandatory at every call
site, so no path can be gated without saying which volume it's being gated for. `ExclusionTier` (the `BootDisk` /
`MountRooted` enum) is derived from it. `IndexPathSpace` STORES its space as an `ExclusionScope` and reads its mount
root back through it, so the path space and the exclusion gate can't disagree about where the volume begins (see
`../paths/DETAILS.md`). The scanner (`InsertVisitor` via `ScanConfig::scope`), the reconciler, and the local reconcile
derive the scope from the volume's `IndexPathSpace`, and so does the per-navigation verifier (see
`../reconcile/DETAILS.md`).

**Both scan entry points take the volume's `IndexPathSpace`, not a loose scope + inode flag** (`ScanConfig::space`,
`scan_subtree`'s `space` argument): the two facts always travel together and a scan that had one right and the other
wrong is exactly the class of bug this replaces. `scan_subtree` needs a third thing from it, which is why the type
rather than the pair: its `root` is an ABSOLUTE FS path but `resolve_scan_root` walks from `ROOT_ID`, so a mount-rooted
volume's subtree resolves only after `space.index_relative` strips the mount root. A volume-ROOT scan maps to `ROOT_ID`
whatever its path is, so only the subtree half depends on this.

## Canonicalization aliases

**The scanner skips canonicalization aliases** (`scanner::is_canonicalization_alias`, fired when an entry's
`normalize_path` form differs from its real path). The three `/private` root symlinks (`/tmp`, `/var`, `/etc`)
canonicalize onto the same `(parent_id, name_folded)` key as the real directory under `/private`. Storing the alias
collides on `INSERT OR IGNORE` (the source of "skipped due to UNIQUE conflict" log lines on a normal Mac) and risks an
order-dependent race where the symlink row wins and the real directory's row, hence its recursive size, is dropped.
Skipping the alias is correct because the real directory owns the canonical slot, and the resulting index is identical
to the pre-skip outcome minus the race. **Don't "fix" this by storing the raw `/tmp` path instead**: that would make the
entry invisible to the ~15 lookup sites that all normalize to canonical form. The firmlink/`normalize_path` model itself
is canonical in `../paths/DETAILS.md`.
