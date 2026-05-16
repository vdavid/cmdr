# Fresh-listing reuse for write-op scans

## Why this exists

Pre-flight scans for copy, move, and delete on MTP (and to a lesser degree SMB, network mounts, big local trees)
duplicate work the backend has already done. Three concrete failures the user hit on a connected Android device with 15k
photos in `/DCIM/Camera`:

1. **Copy/move pre-flight re-reads the current folder.** The user selects 135 photos and presses F5. The "Verifying
   before copy…" dialog shows a `5,080 files` counter climbing — that's the MTP device re-listing the parent directory
   just to look up size of the 135 selected entries by name. The listing is already in the pane's `LISTING_CACHE` and
   the MTP event loop is keeping it fresh.
2. **Delete pre-flight does the same re-list**, then on confirm runs _a second_ re-scan inside
   `delete_volume_files_with_progress` (it ignores `config.preview_id`). The second scan emits no progress for top-level
   files, so the UI looks frozen.
3. **Per-file `is_directory` probes during the operation.** For each top-level selected path, the volume scan calls
   `is_directory(path)` which on MTP lists the parent dir again. Copy/move already work around this via `source_hints`
   seeded from the cached scan result. Delete does not.

Beyond the perf wins, the change tightens a contract we want in writing: **write operations trust a cached listing only
when it's watcher-backed.** No "fresh-enough" age heuristics. No "the index says…" shortcuts. A listing is either being
actively kept in sync by a live watcher, or it gets re-read. This matches the user's risk model: write ops can lose
data, so the freshness contract is bright-line.

## Scope

- Backend-only change. No FE protocol change. (`startScanPreview` keeps the same signature; the cache layer below
  decides whether to do real I/O.)
- Applies to **copy, move, delete** on all volume types. Local FS benefits modestly; MTP and SMB benefit a lot.
- Does NOT trust the drive index for write-op decisions. The index continues to feed UX (the "X% of estimated"
  progress-bar denominator, recursive size in the file list). It just doesn't replace a real walk.
- Does NOT remove the existing volume-level caches (MTP's 5 s TTL, SCAN_PREVIEW_RESULTS). Those still play their roles.
  The new oracle is additive.

## Design

### The oracle

A single backend helper, in `file_system/listing/caching.rs`:

```rust
/// Returns cached entries for `(volume_id, path)` if the volume reports that
/// this listing is being kept in sync by an active watcher. Otherwise `None`.
///
/// **Freshness contract (read carefully)**: a `Some(_)` result means the
/// volume has an active change-notification channel and the cache reflects
/// the volume's most recently observed state. It does NOT mean the cache is
/// byte-perfect with the device right now — every backend has a debounce or
/// settling window between a real change and the cache reflecting it:
///   - MTP: 500 ms event debouncer, plus per-device event-loop polling; some
///     MTP devices (cameras especially) don't emit per-object events at all,
///     in which case "watched" means only "the device is reachable and would
///     report changes if it sent any".
///   - SMB: 200 ms watcher debounce; >50 events per dir triggers a
///     FullRefresh request that arrives via a real re-read.
///   - Local FS: FSEvents coalesce window (~10 ms).
///
/// Callers must treat the result as "fresh as our most recent observation,"
/// which is the same guarantee a `list_directory` call gives — it sees the
/// device's state at the moment the call returned, not at the moment the
/// caller reads its result. The contract intentionally accepts this window:
/// a tighter one would force us to re-validate every walk, which defeats the
/// whole point.
pub fn try_get_watched_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>>
```

Implementation flow:

1. `find_listings_for_path_on_volume(Some(volume_id), path)` — already exists, returns matching listing IDs.
2. Pick the **most-recently-updated** matching listing: highest `sequence`, ties broken by latest `created_at`. Don't
   take the first iteration result — `HashMap` iteration order is non-deterministic and a stale duplicate listing could
   poison the result. Worth noting: for path-level watchers (local FS) two panes on the same path both register
   watchers, so both listings are equally fresh; the tiebreaker is just for determinism.
3. Look up the volume in `VolumeManager`. Call `listing_is_watched(path)`.
4. If true, clone the entries and return.

Why on the `Volume` trait, not a free function: each backend has its own "watcher alive" signal (`WATCHER_MANAGER` for
local, `connection_manager().is_connected(device_id)` for MTP, the `smb_watcher` cancel sender for SMB). Putting the
check on the trait keeps the freshness contract colocated with the backend that owns it. Default `false` means a new
backend without a real watcher won't accidentally claim freshness.

The oracle clones the `Vec<FileEntry>` rather than borrowing. The listing cache's `RwLock` would otherwise be held
across awaits in the scan loop, blocking pane navigation. Entries are flat structs; for 15k entries the clone is < 5 ms.

### How `volume_id` flows

The plan does NOT add a `volume_id()` method to the `Volume` trait. `LocalPosixVolume` doesn't store one (it's
constructed with just name + root; the manager assigns the ID at registration time), and adding it would ripple through
every constructor and call site. Instead, write-op callers already have `volume_id` in
`WriteOperationConfig.source_volume_id` / `dest_volume_id`. The oracle and the new scan walker take `volume_id: &str` as
an explicit parameter, propagated down through the recursion. Cheaper, no trait change.

### Recursive scan rule

One rule, applied at every level of the scan walker (copy, move, delete):

```
fn scan(volume, volume_id, path):
    entries = oracle.try_get_watched_listing(volume_id, path)
                .unwrap_or_else(|| volume.list_directory(path))
    for entry in entries:
        if entry.is_directory and not entry.is_symlink:
            scan(volume, volume_id, entry.path)        // recurse, same rule re-applies
        else:
            count_as_file(entry)
```

What this gives you, working through the user's `[a]`, `[b]`, `d` scenario:

1. Top level: oracle hits the current folder's cached listing. `d`'s size, `[a]`'s `is_directory=true`, `[b]`'s
   `is_directory=true` all come from cache. No I/O.
2. `d`: leaf, counted from cache.
3. `[a]`: recurse. Oracle misses (`[a]` isn't open in any pane). Real `list_directory(/current/[a])`. Walk continues.
4. `[b]/subfolder/`: recurse. Oracle hits (the other pane has it open). Use cached entries, skip the I/O for that
   subtree's top level. Walk continues below normally for any _non-cached_ sub-subfolders.
5. Anywhere a watcher is dead or the listing isn't cached: real walk.

Symlinks: cached `FileEntry.is_symlink == true` means the symlink is treated as a single entry (no recursion), matching
the existing `walk_dir_recursive` policy (`symlink_metadata`, no dereference). The recurse condition already excludes
symlinks.

### Volume trait addition

```rust
trait Volume {
    /// Returns true when the listing at `path` is currently being kept in sync by
    /// a live watcher on this volume. Used by `try_get_watched_listing` to decide
    /// whether a cached listing can replace a real read in write-op pre-flight.
    ///
    /// "Live watcher" is intentionally coarse for non-local backends — see the
    /// freshness contract on `try_get_watched_listing` for the per-backend
    /// debounce / settling windows callers must tolerate.
    ///
    /// Default `false`: new backends without an active watcher opt in explicitly.
    fn listing_is_watched(&self, _path: &Path) -> bool { false }
}
```

Per-backend implementations:

| Backend            | `listing_is_watched` returns true when                                                                                                                                                                                                              |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LocalPosixVolume` | `WATCHER_MANAGER` has a `WatchedDirectory` entry whose path matches `path` for some active listing                                                                                                                                                  |
| `MtpVolume`        | the device is connected (`connection_manager().is_connected(device_id)`). The MTP event loop is per-device, not per-folder; connected = "the device will report any changes it sends." Documented caveat: many MTP devices don't send events at all |
| `SmbVolume`        | `watcher_cancel.lock().is_some()` AND `connection_state() == Direct`. Volume-level, not path-level: the SMB watcher monitors the whole share via `CHANGE_NOTIFY`                                                                                    |
| `InMemoryVolume`   | never — no real watcher; default `false` is correct                                                                                                                                                                                                 |

The MTP and SMB checks are volume-level, not path-level. This is intentional and a known limit: when the gate flips
true, every path on that volume becomes oracle-eligible, including paths whose cache entries are old enough that the
debounce window may not have fully settled around them. The trade-off: tightening this to "path-level freshness with
last-event-timestamp" requires per-path observation state we don't currently track, and the user already accepted the
volume-level grain ("if the watcher is alive we trust the cache").

### Scan-preview cache, current folder

The pre-flight UI flow (TransferDialog, DeleteDialog) calls `start_scan_preview` which today always invokes
`scan_for_copy_batch_with_progress`. With the oracle in place, `run_volume_scan_preview` checks each input path's parent
against the oracle first; on hit, it builds the `BatchScanResult` from cached entries (no volume call) for any path
whose parent is watcher-backed.

The result is still written into `SCAN_PREVIEW_RESULTS` keyed by `previewId`, so the downstream copy/delete consumes it
the same way. The cached entries' `size` and `is_directory` flow into `per_path` exactly as if a real scan had produced
them.

For paths whose top-level item is a directory, the scan walker recurses into the directory with the same oracle-first
rule. So `[a]` either gets walked from real I/O (no other pane has it open) or from cached sub-listings (it or its
subfolders are open elsewhere).

### Delete: stop ignoring `preview_id`

`delete_volume_files_with_progress_inner` runs its own `scan_volume_recursive` regardless of whether the preview already
cached the same data. Fix:

1. At the top of the function, `take_cached_scan_result(preview_id)` like `copy_volumes_with_progress` does.
2. If present, derive the delete entry list from the cached per-path results:
   - For top-level files: one `VolumeDeleteEntry { path, size, is_dir: false }` per entry, size from cache.
   - For top-level dirs: still need the recursive walk (delete needs per-file paths). But we know it's a dir without an
     `is_directory` probe, and the walker uses the oracle-aware rule from above so the first level under each dir is
     cache-fed when possible.
3. If absent (no preview, MCP path, etc.), fall through to today's `scan_volume_recursive` — but with the walker now
   oracle-aware. On the no-preview path, the top-level `is_directory(source)` probe at `delete.rs:425` STAYS for paths
   whose parent isn't open in any pane (the oracle misses, and we genuinely don't know what type the path is). It only
   goes away when the parent oracle hits. This is fine because the no-preview path is rare (MCP-triggered delete,
   programmatic invocation) and one `get_metadata` per top-level path on a cold cache is acceptable.

### MTP-specific cleanup

`MtpVolume::scan_for_copy_batch_with_progress` (mtp.rs:469-569) currently groups paths by parent dir to amortize
listings — that's a real cold-cache optimization (one parent listing for 135 selected siblings, not 135 separate
`get_metadata` calls). We **keep** this override and layer the oracle on top of it: before the parent-grouping logic
runs, check the oracle for each parent. On hit, skip the `list_directory` call entirely for that parent and look up
children from the cached entries. On miss, fall through to the existing parent-grouping path.

The generic `Volume` trait default for `scan_for_copy_batch_with_progress` (which loops `scan_for_copy` per path, fine
for local FS where stat is cheap) also gets oracle support via the shared walker from M2. Local FS doesn't benefit much
in absolute terms, but the uniform code path is worth it.

### What stays the same

- `SCAN_PREVIEW_RESULTS` cache: unchanged. Still keyed by `previewId`. Still TTL-free (lives until consumed).
- MTP's 5 s `ListingCache` in `mtp/connection/cache.rs`: unchanged. It's a different layer (mtp-rs ↔ MTP volume), hit by
  `list_directory_with_progress` when the oracle misses.
- `recursive_size`, `recursive_file_count` on cached entries: still populated by the indexer, still displayed in the
  file list and used by `expected_totals_for_sources` for the scanning progress denominator. Not consumed by the
  scan-decision path.
- ETA, throughput, conflict detection, rollback: untouched.

## Risks and edge cases

- **Hardlink dedup straddling the cache/walk boundary.** `walk_dir_recursive` dedupes hardlinks by inode for byte
  counts; `FileEntry` doesn't carry inode. When the cache supplies one path and a real walk supplies another path to the
  same inode, dedup misses. Direction is safe (overcount → pessimistic ETA, conservative disk-space reject). Existing
  test `walk_dir_recursive_hardlinks_dedup_inodes` tests the walker, not the oracle path, so it stays valid. If we later
  want true dedup across the boundary, add `inode: Option<u64>` to `FileEntry`. Not in this plan.
- **Watcher races.** The oracle returns entries; between that read and consumption, a watcher event could fire. This is
  exactly the race a real walk has too (something changes between `list_directory` returning and the copy starting). No
  new exposure.
- **Listing closed mid-scan.** User closes the source tab between scan-preview and copy/delete. The cached listing is
  gone. Today `take_cached_scan_result` handles this for copy. The oracle inherits it: if the listing is gone, oracle
  returns `None`, walker falls through to real I/O.
- **Volume disconnected mid-pre-flight (cable yanked).** Oracle returned `Some(entries)` a moment ago; now
  `listing_is_watched` would flip `false`. The walker is already iterating those entries and recursing. Recursive calls
  into the now-disconnected volume's `list_directory` fail fast, but the top-level synthesized totals (file count, byte
  total) may reflect a now-gone state. The actual copy/delete then fails per-file with a confusing-ish error rather than
  "device disconnected." This isn't new — `scan_for_copy_batch` had the same race — but documenting as a Gotcha in
  `write_operations/CLAUDE.md`.
- **Listing exists but watcher not yet attached.** Per `listing/CLAUDE.md`: "File watcher starts AFTER listing
  complete." Tiny window between `list_directory_start_streaming` finishing the read and `WATCHER_MANAGER.insert`
  landing. During this window the cache has entries but `listing_is_watched` returns `false` for local. Oracle correctly
  returns `None`, falls through. Tested explicitly in M1.
- **MTP devices that never send events.** Many cameras claim PTP/MTP but don't emit `ObjectAdded`/`Changed`. The cache
  doesn't get updated reactively, so it ages out via the 5 s mtp-rs TTL (which is a separate layer the oracle doesn't
  see). `is_connected = true` and the oracle will happily serve entries from the open listing in `LISTING_CACHE`
  forever. This is acceptable per the documented freshness contract (we promise "device's most recent observed state",
  which on an event-silent device is "the last time we listed it"). Workaround for users who need strict freshness:
  navigate away and back in the pane to force a re-list.
- **Snapshot semantics.** The cached entries are a snapshot. They're correct as of the watcher's last update. Worst case
  for a 15k-photo folder: one entry being stale-by-debounce-window — same exposure as reading the listing 200 ms ago.
  Documented as a Gotcha.

## Milestones

### M1 — Plumbing: oracle + trait method

Sequential within the milestone (each step depends on the previous compiling cleanly).

1. Add `fn listing_is_watched(&self, _path: &Path) -> bool { false }` to `Volume` in `file_system/volume/mod.rs`.
   Default false. No `volume_id()` method — callers pass the ID explicitly (see "How `volume_id` flows" above).
2. Wire up each backend:
   - `LocalPosixVolume::listing_is_watched`: query `WATCHER_MANAGER` for any `WatchedDirectory` whose path matches.
   - `MtpVolume::listing_is_watched`: `connection_manager().is_connected(&self.device_id)`.
   - `SmbVolume::listing_is_watched`: `self.watcher_cancel.lock().is_some() && self.connection_state() == Direct`. Note:
     `watcher_cancel` is a `Mutex` not `Mutex<async>` — use the std `try_lock` or sync lock variant; avoid holding it
     across awaits. If `try_lock` returns `WouldBlock` (some other task holds the mutex), treat that as "not watched"
     (return `false`); the oracle falls through to a real read, which is the safe direction. Don't retry — the lock is
     brief and another sweep can pick it up on the next pre-flight.
   - `InMemoryVolume::listing_is_watched`: leave default `false`.
3. Add `try_get_watched_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>>` in
   `file_system/listing/caching.rs`. Picks the most-recently-updated matching listing (highest sequence, ties broken by
   `created_at`), looks up the volume in `VolumeManager`, asks `listing_is_watched`, returns cloned entries on hit.
4. Unit tests in `caching_test.rs`:
   - Hit when listing exists and watcher reports true.
   - Miss when listing exists but watcher reports false.
   - Miss when no listing exists.
   - Miss when volume isn't registered.
   - **Determinism**: with two listings on the same path differing in `sequence`, the higher-sequence one is picked.
   - **Race window**: listing exists in cache but `WATCHER_MANAGER` has no entry yet (simulates the start-streaming →
     register-watcher gap) — assert miss.
5. Per-backend tests for `listing_is_watched`:
   - Local: opens a listing, asserts true; closes it, asserts false.
   - MTP: connect/disconnect a virtual device, assert flip.
   - SMB: needs a Docker SMB container test; `#[ignore]`-gated like existing soak tests.

**Docs to touch**:

- `file_system/volume/CLAUDE.md`: add `listing_is_watched` to the capability matrix and a paragraph on the freshness
  contract, explicitly stating the per-backend debounce windows.
- `file_system/listing/CLAUDE.md`: document `try_get_watched_listing` next to `find_listings_for_path_on_volume`,
  including the sequence/created_at tiebreaker rule.

**Checks**: `./scripts/check.sh --rust` after step 2. Full `./scripts/check.sh` end of milestone.

### M2a — Oracle-aware walker (local + scan-preview hookup, MTP/SMB cold-cache override unchanged)

Goal: ship the shared walker abstraction and wire it into the scan-preview path. The MTP and SMB
`scan_for_copy_batch_with_progress` overrides stay in place in this milestone, so the cold-cache parent-grouping
optimization still runs when the oracle misses. M2a only adds an oracle short-circuit _before_ the volume call for
watched parents — the override itself is unchanged, just sometimes skipped.

1. Extract `scan_subtree_with_oracle(volume, volume_id, path) -> SubtreeTotals` into `scan.rs`. Returns file count, byte
   total, per-path facts in the shape `BatchScanResult` expects. At every recursion level, checks the oracle first;
   falls through to `volume.list_directory` on miss. Skips recursion into symlinks.
2. In `run_volume_scan_preview` (`scan_preview.rs`), group input paths by parent. For each parent: oracle check first;
   if hit, build the per-path slice of `BatchScanResult` from cached entries (top-level files done; top-level dirs go
   through `scan_subtree_with_oracle`). If miss, call the existing
   `volume.scan_for_copy_batch_with_progress(paths_for_this_parent, on_progress)` — same path as today, just over a
   subset.
3. Local FS `walk_dir_recursive` in `scan.rs`: add an oracle check at the top of each recursive call. Same rule.
4. `SCAN_PREVIEW_RESULTS` wiring unchanged. The synthesized `BatchScanResult` is written to the cache the same way a
   real scan's result is.
5. Integration tests in `volume_copy_tests.rs`:
   - `scan_preview_uses_watched_listing_for_top_level_files`: InMemoryVolume with test-only override of
     `listing_is_watched`. Counter-wrapping volume asserts `list_directory` call count is 0.
   - `scan_preview_falls_through_when_watcher_dead`: same setup, override returns false, assert `list_directory` is
     called.
   - `scan_preview_uses_cached_subfolder_listing_when_other_pane_has_it`: open `[a]/sub` listing in a second listing ID,
     scan a copy of `[a]`, assert no `list_directory` call for `[a]/sub`.
   - `scan_preview_preserves_symlink_semantics`: cached entry with `is_symlink=true, is_directory=true` — walker counts
     it as one entry, no recursion.
   - `scan_preview_handles_listing_closed_mid_walk`: open `[a]/sub` in pane B, start scan of `[a]`, close pane B before
     `[a]/sub` is reached during recursion. Assert correctness — recursion falls through to real walk for `[a]/sub`
     mid-way.

**Docs**:

- `write_operations/CLAUDE.md`: Decision entry "Scan preview reuses watched listings", Gotcha for hardlink dedup
  straddling the cache boundary, Gotcha for volume-disconnect race.
- `volume/CLAUDE.md`: in "Building a new volume" tier 3, mention `listing_is_watched` and what it gates.

**Checks**: `./scripts/check.sh` end-of-milestone.

### M2b — Port MTP and SMB scan overrides to the shared walker

Goal: layer the oracle on top of the existing parent-grouping optimizations without losing cold-cache perf.

1. In `MtpVolume::scan_for_copy_batch_with_progress`: before the existing parent-grouping logic, add an oracle check per
   parent dir. On hit, populate that parent's child results from cached entries (skip the `list_directory` call). On
   miss, fall through to today's logic unchanged.
2. Same shape for `SmbVolume::scan_for_copy_batch` (if it has an analogous parent-grouping override; otherwise just the
   oracle check before the pipelined stats).
3. Integration tests:
   - `mtp_scan_uses_oracle_on_hit_skips_list_directory`: virtual MTP device, listing in cache, oracle hit, assert no
     `list_directory` call.
   - `mtp_scan_cold_cache_still_uses_parent_grouping`: virtual MTP device, no cached listing, assert one
     `list_directory` per unique parent (preserves today's optimization).
   - `smb_scan_uses_oracle_on_hit_skips_stat_pipeline`: ditto for SMB, behind a Docker-gated test.

**Docs**: update `volume/mtp.rs` doc comment on `scan_for_copy_batch_with_progress` to note the oracle layering.

**Checks**: `./scripts/check.sh` end-of-milestone.

### M3 — Delete reuses the scan preview and the oracle

1. In `delete_volume_files_with_progress_inner` (`delete.rs:410`), call `take_cached_scan_result(preview_id)` at the
   top. On hit, derive the entry list from `per_path`:
   - Top-level files → `VolumeDeleteEntry { path, size, is_dir: false }`.
   - Top-level dirs → recursive walk via `scan_subtree_with_oracle` from M2a.
2. On miss (no preview_id, MCP path, etc.), fall through to today's `scan_volume_recursive` with the walker now
   oracle-aware. Important: on this no-preview path, the top-level `volume.is_directory(source)` probe stays for sources
   whose parent the oracle can't answer. It only goes away when the parent oracle hits (or when we have a cached scan
   result via path 1). This keeps the change scoped — replacing `is_directory` everywhere is its own investigation.
3. Integration tests in a new `delete_volume_reuse_tests.rs`:
   - `delete_consumes_preview_id_skips_rescan`: counter-wrapping InMemoryVolume, run scan_preview then
     delete_files_start with the same preview_id, assert `list_directory` called once total.
   - `delete_without_preview_id_still_walks`: assert correctness when MCP triggers delete with no scan.
   - `delete_top_level_files_no_is_directory_probes`: assert `is_directory` call count is zero when the parent listing
     is in `LISTING_CACHE` and watched.
   - `delete_mid_scan_listing_close`: open the source dir in pane A and a subfolder in pane B, start delete, close pane
     B mid-recursion. Assert correctness (mid-walk fallback).

**Docs**: `write_operations/CLAUDE.md`: update the data-flow diagram for the delete path to note the preview-reuse step.

**Checks**: `./scripts/check.sh` end-of-milestone.

### M4 — End-to-end verification

E2E tests catch the user-visible regression (frozen-looking dialogs, wrong counts) that unit tests can miss.

1. Playwright spec `mtp-copy-preflight-uses-cache.spec.ts`: connect virtual MTP device, list `/DCIM`, select N files,
   press F5, assert the "Verifying" dialog completes within a tight bound (e.g. 300 ms) with the right `filesFound`
   count.
2. Playwright spec `mtp-delete-no-double-scan.spec.ts`: same setup, press F8, confirm, assert the operation dialog
   transitions Scanning → Deleting once (not twice), and the count never goes backwards.
3. Manual QA pass on a real Android device: copy + delete workflows on `/DCIM/Camera` with 5k+ photos. Verify the
   climbing-counter behavior is gone.
4. Run `./scripts/check.sh --include-slow` to catch anything CI's main lane doesn't.

**Docs**: refresh `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`'s top-of-file purpose paragraph to
mention the watcher-backed reuse contract.

## What can be parallelized

- **Within M1**: backend implementations of `listing_is_watched` (step 2's four sub-tasks) are independent once the
  trait method exists. Could go in any order. Not worth a worktree.
- **Within M2a and M2b**: tests can be written alongside implementation.

Milestone-to-milestone dependency is strict: M2a depends on M1's oracle, M2b depends on M2a's walker, M3 depends on
M2a's walker (and benefits from M2b but doesn't require it), M4 depends on all three. Run sequentially.

## Out of scope

- Adding inode tracking to `FileEntry` for true hardlink dedup on the oracle path. Filed as a follow-up.
- Pre-walking subtrees to populate the listing cache before a copy. The oracle is opportunistic; it doesn't warm the
  cache.
- Changing the FE `startScanPreview` signature or the SCAN_PREVIEW_RESULTS schema. Stays the same on purpose; this whole
  change is invisible to the FE.
- Trusting `recursive_size` from the index in any write-op decision. Index data stays UX-only.
- Path-level freshness tracking (last-event-timestamp per listing). The volume-level grain is good enough for now.
- Replacing top-level `is_directory(source)` probes on the no-preview delete path. Scoped to a follow-up.

## Design principle alignment

- **Elegance over hacks**: one oracle, one rule, applied uniformly at every level. No backend-specific short-circuits
  scattered across the scan paths.
- **The app should feel rock solid**: the freshness contract is bright-line at the watcher boundary. No "5 seconds is
  fresh enough" TTL judgment. The per-backend debounce windows are documented honestly.
- **Protect the user's data**: doesn't change any write semantics. Same atomic ops, same rollback, same conflict
  resolution. Just makes the pre-flight honest about what it already knows.
- **Be respectful to the user's resources**: the win. F5 on 135 photos in a watched MTP folder goes from re-listing 15k
  entries (~17 s USB) to ~5 ms (cache clone + walker over 135 entries).
- **Smart backend, thin frontend**: FE is untouched. All the new logic lives in Rust where it belongs.
- **Invest in testability**: every milestone has named test cases targeting specific races and edge cases the reviewer's
  analysis surfaced. Counter-wrapping `InMemoryVolume` is the right test shape for asserting call counts.
