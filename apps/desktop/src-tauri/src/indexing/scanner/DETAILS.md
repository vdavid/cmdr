# Local guarded scanner details

Read this before any non-trivial work in `scanner/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in [CLAUDE.md](CLAUDE.md).

This area owns the LOCAL fresh-scan walker and the shared exclusion policy. Points outward: the honest-sizes model
(`listed_epoch` / `min_subtree_epoch`), the `dir_stats` ledger, and the shared `Arc<AtomicI64>` id counter are canonical
in [`../writer/DETAILS.md`](../writer/DETAILS.md); the serial LOCAL reconcile walk (which reuses the `GuardedReader` +
`LOCAL_LIST_TIMEOUT` from here) and the cost budget in [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md);
`IndexPathSpace` + mount-relative resolution in [`../paths/DETAILS.md`](../paths/DETAILS.md); the registry, phase
machine, and `IndexVolumeKind` capability axes in [`../lifecycle/DETAILS.md`](../lifecycle/DETAILS.md); the shared
`extract_metadata` primitive at `../metadata.rs` (documented in the [hub](../DETAILS.md)). The network (SMB/MTP) walker
is a different scanner entirely: [`../network_scanner/DETAILS.md`](../network_scanner/DETAILS.md).

## Module structure

- **mod.rs** — the scan driver: `scan_volume()` (full scan) / `scan_subtree()` (targeted subtree rescan, used by
  post-replay background verification), `run_scan`, the `InsertVisitor` fresh-scan visitor, the `ScanConfig` /
  `ScanProgress` / `ScanHandle` / `ScanSummary` / `ScanError` types, the `send_marks` helper, and `LOCAL_LIST_TIMEOUT`
  (15 s). No path→id map: the walker carries each directory's id to its own read, so the visitor attributes children to
  their parent via the carried `parent_id` (`dir.id`) directly, allocating fresh child ids from the shared
  `Arc<AtomicI64>` counter owned by `IndexWriter`. The scan root resolves via `resolve_scan_root` (`ROOT_ID` = 1 for a
  volume scan, the existing entry id for a subtree scan). Sizes come from a per-child `symlink_metadata` (lstat);
  physical sizes are `st_blocks * 512`. Hardlink dedup: files with `nlink > 1` are tracked in a mutex-guarded
  `HashSet<u64>` by inode (workers run concurrently); only the first link's size counts, later links get `size = None`.
  `nlink == 1` files skip the set. All files store `inode`; directories and symlinks get `inode: None`.
- **walker/** — the hang-tolerant engine (`walk`, `std_read_dir`, the `DirVisitor` trait, `DirTask` / `RawDirEntry` /
  `WalkReadError` / `WalkConfig` types, the watchdog, the progress-timeout verdict, the `SubtreeBudget` give-up budget)
  plus `bulk_read` (the `getattrlistbulk`-batched `bulk_read_dir` used in production on macOS). Tests are in
  `walker/tests.rs`, all millisecond-scale with a mock reader.
- **exclusions.rs** — the self-contained two-tier path-exclusion policy: `EXCLUDED_PREFIXES`, the
  `FIRMLINKED_SYSTEM_PREFIXES` allowlist, `JUNK_BASENAMES`, `PSEUDO_FS_BASENAMES`, the `ExclusionScope` /
  `ExclusionTier` types, `should_exclude`, `e2e_allowlist_path`, `is_canonicalization_alias`, and `default_exclusions`
  (`#[cfg(test)]` only). Re-exported at `crate::indexing` level so existing `scanner::should_exclude` callers are
  unchanged. It's the single exclusion gate for every code path (scanner, reconcile, watch verification, verifier).
- **tests.rs** — the scanner-driver test module.

E2E scan restriction: when `CMDR_E2E_START_PATH` is set, `should_exclude` restricts scanning to the fixture path, its
children, and ancestors (critical for Docker E2E performance).

## The guarded local walker (`scanner/walker/`)

The LOCAL scan (both the fresh `scan_volume`/`scan_subtree` and the serial reconcile rescan in `../reconcile/`) must
survive a hung `readdir`: a disconnected macOS File Provider mount (Dropbox / Google Drive / MacDroid under
`~/Library/CloudStorage`, iCloud under `~/Library/Mobile Documents`) blocks a `readdir` indefinitely when the provider
is offline, which froze the whole scan.

- **The pool.** A persistent pool of 8 MB-stack worker threads (dedicated OS threads, never rayon — File Provider reads
  descend deep XPC override chains that overflow rayon's 2 MB stack) pull directory-read tasks from a shared queue and
  call `readdir` directly. A blocking `readdir` can't time itself out, so a **watchdog** thread caps each read from
  outside: every in-flight read carries an `Arc<AtomicU8>` state (`READING → COMPLETED`, won by the worker; or
  `READING → ABANDONED`, won by the watchdog), and whoever wins the compare-and-swap owns the outcome exactly once. A
  read the watchdog condemns is **abandoned**: reported as a read error (subtree pruned, dir left unmarked), a
  replacement worker is spawned to restore pool capacity, and the stuck worker is left parked in the syscall (it exits
  on its own when the File Provider layer finally errors). Only genuinely-hung *frontier* dirs reach this, each pruning
  its subtree, so the parked-worker cost is bounded and self-clearing. Workers are NOT joined (an abandoned one would
  block forever); the walk returns when the outstanding-task count hits zero. The reader is an injected `ReadDirFn`
  (production `bulk_read_dir` on macOS, `std_read_dir` elsewhere, tests a mock that blocks or trickles), so hang /
  big-but-healthy / honest-skip / parallel-correctness are unit-tested with no real hung mount.
- **Per-subtree give-up budget.** The per-dir watchdog abandons ONE hung dir at a time, so a dead mount that fails on
  every read (a disconnected File Provider returning `ETIMEDOUT`/`os error 60` per descendant, e.g. a MacDroid phone's
  `/proc/*/task/*/fd`) still cost one abandon PER DESCENDANT — hundreds/thousands of probes and a log flood. The give-up
  budget bounds that structurally: every read carries a `SubtreeBudget` (`walker/mod.rs`) shared by the children of ONE
  successfully-listed directory. Each failed read (timeout OR IO error) increments it; any successful sibling read
  resets it; when it reaches `WalkConfig::give_up_after` (`DEFAULT_GIVE_UP_AFTER = 32`, mirroring the network scanner's
  `CONSECUTIVE_FAILURE_ABORT`) the budget is **given up** (sticky) — the trip is logged ONCE (subtree path + count), and
  every still-queued sibling sharing that budget is pruned unread by a pre-read check in `run_worker` (no probe, no
  per-dir log). A successfully-listed dir mints a FRESH budget for its own children, so the bound is ~N probes per level
  of a dead subtree instead of N-per-descendant. It's **throttle, not exclude**: purely structural, so a healthy
  provider (reads succeed → counter resets) is fully indexed, and only a genuinely-dead subtree is abandoned — no
  path/CloudStorage denylist. Under concurrency "consecutive" is loose (up to `num_threads` reads can be in flight
  against one budget), the same caveat the network scanner notes. **Honest-stale, never false-complete:** a pruned dir
  is never marked listed (never added to `listed_ids`), so it stays `listed_epoch = 0` (unknown size) — its `EntryRow`
  still exists (its parent listed it), but its subtree is left unknown, not zeroed and not `scan_completed_at`-marked.
  `WalkStats.subtrees_abandoned` counts the trips; `run_scan` logs a one-line scan-wide summary. This MIRRORS (not
  shares) the network scanner's counter: that one is a single global `usize` over a serial BFS that aborts the WHOLE
  walk; this is a per-subtree parallel `Arc<Atomic>` tree that prunes one subtree — different granularity and
  control-flow, and the shared logic is a trivial threshold compare, so a helper would be an inelegant abstraction.
  Test: `walker/tests.rs::gives_up_on_a_dead_subtree_and_keeps_walking_a_healthy_sibling` (synthetic dead subtree, no
  real mount).
- **Parent attribution needs no path→id map.** Each read task carries the directory's own id; the `InsertVisitor`
  attributes children to their parent via that carried id (`dir.id`) and allocates fresh child ids from the shared
  `Arc<AtomicI64>` counter — so the whole-volume `HashMap<PathBuf,i64>` the old fresh scanner kept is gone for the local
  path. (It survives only in the network scanner's `ScanContext`, whose serial BFS still resolves parents by path.)
  `std_read_dir` classifies each child from the dirent (`d_type`, no extra syscall on APFS); the visitor does its own
  per-child `symlink_metadata` for sizes/mtime.

## The walker's progress timeout

**Elapsed time cannot tell a BIG directory from a BROKEN one, so the walker doesn't measure it.** Every read publishes
what it has delivered through a `ReadProgress` handle (`scanner/walker/mod.rs`), and the watchdog judges that
(`Engine::verdict`). Two rules, either of which abandons the read:

- **Stalled** — nothing delivered for `WalkConfig::stall_timeout` (production `LOCAL_LIST_TIMEOUT`, 15 s). This is the
  hung-mount rule, and it applies at any point in a read: a mount that drops after delivering a million entries is
  abandoned as promptly as one that never answers.
- **Over allowance** — total time past `stall_timeout` plus `WalkConfig::per_entry_allowance`
  (`DEFAULT_PER_ENTRY_ALLOWANCE`, 1 ms) per entry delivered. The floor under the stall rule: without it a read trickling
  one entry every 14 s would never stall and never finish. It's ~500× the measured `getattrlistbulk` per-entry cost and
  10× the reconcile cost budget's per-entry threshold for calling a read *pathological*, so a healthy read clears it by
  orders of magnitude.

**Why it changed.** A total-duration cap of 15 s made the 2026-07-21 fresh scan report "complete" with 6,001,637
entries; the reconcile that followed added **661,411 rows** it had silently dropped. All five abandoned directories
were flat and merely large (200,000 / 179,523 / 102,929 / 100,000 / 74,024 entries), and the serial reconcile read
every one of them in 10.8 s or less. They only exceeded 15 s in the parallel scan, which runs one read per core, so the
constant was being asked a question its own doc comment never claimed to answer ("an online cloud dir lists in well
under a second"). Measurements:
[`docs/notes/indexing-benchmarks-2026-07-21.md`](../../../../../../docs/notes/indexing-benchmarks-2026-07-21.md). Same
class of mistake, same week, as the reconcile cost budget's cumulative-time metric (see
[`../reconcile/DETAILS.md`](../reconcile/DETAILS.md)), and the same fix shape: score the work done, not the clock.

**A reader that can't report progress is still bounded.** With `entries` stuck at 0, both rules collapse to the plain
total-duration cap the walker always had — which is the honest verdict, since a read we can't observe is
indistinguishable from one that has produced nothing. That covers the serial reconcile's `GuardedReader` (it awaits a
whole `Vec` on a helper thread and reuses `LOCAL_LIST_TIMEOUT` as a total cap) and any future reader added without
progress plumbing. Both production readers do publish: `bulk_read_dir` per `getattrlistbulk` batch, `std_read_dir` per
entry.

**What did NOT change.** The abandon/replace protocol, the subtree give-up budget's accounting (a timeout is still one
`record_failure` against the subtree budget, an IO error still resets on a successful sibling), and the honest-stale
contract (an abandoned dir is never marked listed, so it stays `listed_epoch = 0`). Fewer false timeouts simply means
`DEFAULT_GIVE_UP_AFTER` trips less often on healthy volumes.

Tests (`scanner/walker/tests.rs`, all millisecond-scale with a mock reader):
`a_read_that_keeps_delivering_is_never_abandoned`, `a_read_that_stops_delivering_is_abandoned_promptly`,
`a_reader_that_cannot_report_progress_is_still_bounded`, `a_trickling_read_is_abandoned_by_the_per_entry_allowance`.

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

**Recognizing a File Provider domain root** (`file_system::file_provider::domain_id_for_dir`): a domain root carries the
`com.apple.file-provider-domain-id` xattr; its children, `~/Library/CloudStorage` itself, and ordinary folders don't.
~5 µs, a plain APFS read with no XPC, works while the provider is offline, needs no entitlement. It resolves Dropbox,
Google Drive, MacDroid, and iCloud Drive — and iCloud's domain root is `~/Library/Mobile Documents`, which is NOT under
`~/Library/CloudStorage`, which is exactly why a path-prefix heuristic was rejected. Full measurements, the
authoritative-but-costly `NSFileProviderManager` alternative, and the dead ends:
[`docs/notes/fileprovider-domain-detection.md`](../../../../../../docs/notes/fileprovider-domain-detection.md) (verified
on macOS 26.5.2, build 25F84, 2026-07-20).

**The xattr is a private Apple detail, so this is an OPTIMIZATION, never a safety guarantee.** It's undocumented and not
contractual; if Apple drops it, unrecognized domain roots simply go back to being walked. Nothing may depend on it for
correctness or for bounding cost. The actual contract against pathological trees is the cost-budget backstop
([`../reconcile/DETAILS.md`](../reconcile/DETAILS.md)) — the two are not redundant, and neither makes the other
unnecessary.

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
[`../paths/DETAILS.md`](../paths/DETAILS.md)). The scanner (`InsertVisitor` via `ScanConfig::scope`), the reconciler,
and the local reconcile derive the scope from the volume's `IndexPathSpace`. The per-navigation verifier stays
`BootDisk` and root-only by design (see [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md)).

## Canonicalization aliases

**The scanner skips canonicalization aliases** (`scanner::is_canonicalization_alias`, fired when an entry's
`normalize_path` form differs from its real path). The three `/private` root symlinks (`/tmp`, `/var`, `/etc`)
canonicalize onto the same `(parent_id, name_folded)` key as the real directory under `/private`. Storing the alias
collides on `INSERT OR IGNORE` (the source of "skipped due to UNIQUE conflict" log lines on a normal Mac) and risks an
order-dependent race where the symlink row wins and the real directory's row, hence its recursive size, is dropped.
Skipping the alias is correct because the real directory owns the canonical slot, and the resulting index is identical
to the pre-skip outcome minus the race. **Don't "fix" this by storing the raw `/tmp` path instead**: that would make
the entry invisible to the ~15 lookup sites that all normalize to canonical form. The firmlink/`normalize_path` model
itself is canonical in [`../paths/DETAILS.md`](../paths/DETAILS.md).
