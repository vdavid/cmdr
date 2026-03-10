# DB-first directory listings plan

Serve directory listings from the SQLite index instead of `readdir` + `stat`, cutting navigation time from 2–50ms to
sub-millisecond. Background verification on each navigation keeps the DB accurate.

See also: [plan.md](../drive-indexing/plan.md) (parent plan, "Future: DB-first directory listings" section), [tasks.md](../drive-indexing/tasks.md)

Date: 2026-03-03.

## Intention

The drive index already stores every file and directory on the volume (`entries` table). Today we only use it for
enriching directory entries with recursive sizes. This plan promotes the index to the **primary listing source** — the
first thing the user sees on navigation comes from a SQLite query, not from `readdir`. The filesystem read still happens,
but in the background as a correctness check.

**Why this matters:** `readdir` + `stat` takes 2–50ms per directory (more on network volumes). A `SELECT` on an indexed
`parent_path` column takes <1ms. For power users navigating rapidly (Tab, Enter, Backspace), this difference is
perceptible — especially with 10k+ file directories.

**Why now:** The index has been running in production across milestones 1–8. FSEvents + `sinceWhen` replay + MustScanSubDirs
keep it fresh. The missing piece is per-navigation verification (`verifier.rs`) to close the last gap.

## Design

### Flow on navigation

```
User navigates to /Users/foo/Documents/
  │
  ├─ Step 1: DB query (synchronous, <1ms)
  │   ├─ SELECT * FROM entries WHERE parent_path = '/Users/foo/Documents/'
  │   ├─ Convert ScannedEntry → FileEntry (icon_id derived, defaults for unused fields)
  │   ├─ Enrich with dir_stats (existing batch query)
  │   └─ Sort, cache, return to frontend → instant first paint
  │
  └─ Step 2: Background verification (async, 2–50ms)
      ├─ readdir + stat the same directory on the real filesystem
      ├─ Diff against DB snapshot (bidirectional)
      ├─ If identical (common case, ~99%+): done, no UI update
      └─ If different:
          ├─ Stale entries (DB has, disk doesn't) → DeleteEntry/DeleteSubtree
          ├─ Missing entries (disk has, DB doesn't) → UpsertEntry + PropagateDelta
          ├─ Changed entries (size/mtime differ) → UpsertEntry
          ├─ Update LISTING_CACHE with corrected entries
          └─ Emit listing-updated event → frontend re-fetches visible range
```

Note: extended metadata (permissions, owner, group, addedAt, openedAt) is not currently displayed in the UI, so there's
no need for a separate metadata loading step. If those columns are added in the future, a step 3 can load them lazily
using the existing `get_extended_metadata_batch()` / `get_macos_metadata()` APIs.

### Fallback to readdir

The DB-first path activates **per-directory**. If the index isn't ready or has no entries for a `parent_path`, fall back
to the current `readdir` + `stat` path transparently.

**Guard: full scan must have completed at least once.** During the initial full scan (first launch, or after "Clear
index"), the scanner writes entries in batches. A directory might have its own entry in the `entries` table but only some
of its children written so far. Without this guard, DB-first would show a partial listing, then verification would
"correct" it to the full list — a visible jump. Check `scan_completed_at` in the meta table: if not set, always fall
back to readdir. On subsequent cold starts, the DB is already complete (sinceWhen replay only modifies individual
entries, not partial directories), so this guard only matters for first launch and "Clear index."

```rust
fn is_db_first_available(store: &IndexStore) -> bool {
    // Only use DB-first after the initial full scan has completed at least once
    store.get_index_status().scan_completed_at.is_some()
}
```

**Per-directory check:** Once DB-first is available, `list_entries_by_parent()` might return an empty `Vec`. This is
ambiguous — "genuinely empty directory" vs "not indexed yet." Disambiguate by checking whether the parent directory
itself exists in the `entries` table. If it does, the directory is indexed and legitimately empty. If not, fall back to
readdir.

```rust
fn is_directory_indexed(store: &IndexStore, dir_path: &str) -> bool {
    // If the directory itself is in the entries table, it's been indexed
    // (even if it has no children)
    store.entry_exists(dir_path)
}
```

### ScannedEntry → FileEntry conversion

`ScannedEntry` has: `path`, `parent_path`, `name`, `is_directory`, `is_symlink`, `physical_size`, `logical_size`,
`modified_at`. `FileEntry` needs additionally: `created_at`, `added_at`, `opened_at`, `permissions`, `owner`, `group`,
`icon_id`, `extended_metadata_loaded`, `recursive_size`, `recursive_file_count`, `recursive_dir_count`.

Mapping:

| FileEntry field              | Source                                                             |
|------------------------------|--------------------------------------------------------------------|
| `name`                       | `ScannedEntry.name`                                                |
| `path`                       | `ScannedEntry.path`                                                |
| `is_directory`               | `ScannedEntry.is_directory`                                        |
| `is_symlink`                 | `ScannedEntry.is_symlink`                                          |
| `size`                       | `ScannedEntry.logical_size` (matches readdir's `metadata.len()`)  |
| `modified_at`                | `ScannedEntry.modified_at`                                         |
| `icon_id`                    | Computed: `get_icon_id(is_directory, is_symlink, &name)` — no stat |
| `created_at`                 | `None` (not displayed; lazy-loadable later if needed)              |
| `added_at`                   | `None` (not displayed; lazy-loadable later if needed)              |
| `opened_at`                  | `None` (not displayed; lazy-loadable later if needed)              |
| `permissions`                | `0` (not displayed; lazy-loadable later if needed)                 |
| `owner`                      | `""` (not displayed; lazy-loadable later if needed)                |
| `group`                      | `""` (not displayed; lazy-loadable later if needed)                |
| `extended_metadata_loaded`   | `false`                                                            |
| `recursive_size`             | From `dir_stats` enrichment (existing)                             |
| `recursive_file_count`       | From `dir_stats` enrichment (existing)                             |
| `recursive_dir_count`        | From `dir_stats` enrichment (existing)                             |

The first paint shows: name, icon, size, modified date, and recursive dir sizes — all without any disk I/O. The currently
unused fields (permissions, owner, group, dates) get defaults and can be lazy-loaded later if those columns are added.

### Integration into the listing pipeline

The change is in the **data source**, not the pipeline structure. Currently:

```
list_directory_start_with_volume()
  └─ volume.list_directory(path)        ← readdir + stat (2–50ms)
  └─ enrich_entries_with_index()        ← batch dir_stats (µs)
  └─ sort, cache, return
```

After:

```
list_directory_start_with_volume()
  ├─ if is_directory_indexed(path):
  │   └─ store.list_entries_by_parent()  ← SQLite query (<1ms)
  │   └─ convert ScannedEntry → FileEntry
  │   └─ enrich_entries_with_index()
  │   └─ sort, cache, return             ← instant first paint
  │   └─ spawn verify_and_enrich(path)   ← background: readdir diff + extended metadata
  │
  └─ else (not indexed):
      └─ volume.list_directory(path)     ← existing readdir path (unchanged)
      └─ enrich, sort, cache, return
```

The streaming path (`list_directory_start_streaming`) follows the same pattern — if indexed, populate the cache from DB
instantly (no progress events needed for sub-ms reads), then emit `listing-complete`; if not indexed, use the existing
streaming pipeline. In both cases, the frontend fetches entries via `getFileRange` with virtual scrolling — the IPC pipe
is too narrow to send all entries at once, so the paginated fetch pattern stays regardless of data source.

### Verification architecture (`verifier.rs`)

The existing `verify_affected_dirs()` in `mod.rs` does exactly what we need: bidirectional readdir diff with
`DeleteEntry`/`DeleteSubtree`/`UpsertEntry`/`PropagateDelta` messages to the writer. The per-navigation verifier reuses
this pattern but is:

1. **Triggered on every navigation** (not just post-replay)
2. **Scoped to a single directory** (not a set of affected paths)
3. **Updates the LISTING_CACHE** in addition to the DB (so the current listing reflects corrections)

The verifier cancels on navigate-away (like `CurrentDir` micro-scans) — if the user navigates elsewhere before
verification completes, there's no point finishing.

**Cancellation safety:** Each writer message (`DeleteEntry`, `UpsertEntry`, `PropagateDelta`) is processed atomically by
the writer thread. If verification is cancelled mid-way, some corrections are committed and some aren't. The uncommitted
ones will be caught by the next navigation to that directory, or by FSEvents. The DB is always in a consistent state —
partial verification is safe, never corrupting.

### Handling the listing cache update

When verification finds diffs, it must update both the DB (via writer messages) and the `LISTING_CACHE` (for the current
listing). Approach:

1. Verification builds a complete `Vec<FileEntry>` from the readdir pass (same as today's `list_directory_core`).
2. Compare against the DB-sourced cache entries by path.
3. If identical (sorted by path): done.
4. If different: replace the cache entries, re-sort, emit `listing-updated` with the listing ID.
5. Frontend handles `listing-updated` like it handles watcher diffs today — re-fetches the visible range.

### What the frontend sees

From the frontend's perspective, nothing changes in the API contract:

- `listDirectoryStart` returns a `listingId` and `totalCount` (instantly, from DB)
- `getFileRange` returns `FileEntry[]` with virtual scrolling (paginated fetch, same as today)
- `listing-updated` events trigger re-fetches when verification finds diffs (existing pattern)

The only visible difference: navigation feels instant. Extended metadata fields (permissions, owner, group, dates) are
not currently displayed, so their default values are invisible to the user.

## Key decisions

1. **DB-first is opt-in per-directory, not a global switch.** If a directory isn't indexed, the existing readdir path
   runs transparently. No user-facing setting needed.

2. **Verification is mandatory, not optional.** Every DB-first navigation spawns a background verification. The index
   is a cache, not a source of truth. Skipping verification would risk showing stale data with no correction path.

3. **Reuse the existing verification pattern.** `verify_affected_dirs` already handles the bidirectional diff. The
   per-navigation verifier follows the same pattern (two-phase: bulk DB read under lock, then filesystem I/O without
   lock).

4. **No extended metadata loading needed (for now).** Permissions, owner, group, and date columns are not currently
   displayed. The DB-first entries use defaults for those fields. If those columns are added later, a lazy-loading step
   can be wired in using the existing `get_extended_metadata_batch()` / `get_macos_metadata()` APIs.

5. **Add logical size to entries table; rename `size` → `physical_size`.** The `entries` table currently stores physical
   size (`st_blocks * 512`) in a column ambiguously named `size`. The listing size column shows logical size (`st_size` /
   `metadata.len()`), which is what Finder shows and what users expect. Without this, DB-first would show physical sizes
   (confusing for small files: a 100-byte file showing as "4 KB", or inline-stored files showing "0 B"). Rename the
   existing column to `physical_size`, add `logical_size`; the scanner already has both values from the same `stat()`
   call. `ScannedEntry` gets the same rename (`physical_size` + `logical_size`). `FileEntry.size` stays as-is (it only
   ever carries logical, no ambiguity). `dir_stats.recursive_size` stays as-is (only carries physical, documented).
   Physical size continues to be used for `dir_stats` aggregation (recursive folder sizes), where it's more meaningful
   for disk usage. Bump schema version → existing indexes auto-rebuild.

6. **Cache update on diff, not cache replacement.** Verification only touches the cache when diffs are found. The
   common case (no diffs) has zero overhead on the cache path.

7. **Verification compares against current cache, not original DB snapshot.** The per-directory file watcher may update
   `LISTING_CACHE` between the DB-first paint and verification completion. Comparing the readdir result against the
   current cache state (not the original DB snapshot) means watcher-handled changes are automatically nooped by
   verification. No double-processing.

### Follow-up: disable per-directory watcher for indexed volumes

Today, each listed directory gets its own `notify`/kqueue watcher (`file_system/watcher.rs`). With the volume-level
FSEvents watcher already running, this is redundant — the same file change is processed twice (per-directory watcher
updates `LISTING_CACHE` directly; volume watcher updates DB → emits `index-dir-updated` → frontend re-fetches). The only
tradeoff is latency: per-directory kqueue fires in <100ms, FSEvents batches at 300ms. But 300ms is imperceptible for
background updates.

When DB-first is active for a volume, skip starting the per-directory watcher for directories on that volume. This
simplifies the system and avoids double-processing. Not blocking for milestone 2 — can be done as a follow-up.

## Performance targets

| Operation                                      | Target    | Notes                                          |
|------------------------------------------------|-----------|-------------------------------------------------|
| DB-first listing query (indexed parent_path)   | <1ms      | SQLite WAL read, indexed column                 |
| ScannedEntry → FileEntry conversion (1K items) | <100µs    | No I/O, just field mapping + icon_id derivation |
| Total time to first paint (DB-first path)      | <2ms      | Query + convert + enrich + sort                 |
| Background verification (readdir diff)         | 2–50ms    | Same cost as today's listing                    |

## Milestones

### Milestone 1: Per-navigation verifier (`verifier.rs`)

Implement the background readdir diff that runs on every navigation. This works **independently of DB-first listings**
— it improves index accuracy even when the listing still comes from readdir. Ship and validate before wiring up DB-first.

**Intention:** Build confidence that the verifier keeps the index in sync. Once we trust it, the DB-first switch is safe.

### Milestone 2: DB-first listing path

Switch the listing data source from `readdir` to the SQLite index for indexed directories. Fall back to readdir for
non-indexed directories.

**Intention:** The user perceives instant navigation. The readdir path is still there as a correctness backstop and for
directories not yet indexed. The DB-first path should be invisible to the user except for speed.

### Milestone 3: Performance validation

Benchmark DB-first listings vs readdir across directory sizes. Validate the targets above. Profile and optimize if needed.

**Intention:** Prove the performance win is real and consistent. Identify edge cases (huge directories, slow SQLite,
concurrent writes during verification).

---

## Tasks

### Milestone 1: Per-navigation verifier

- [ ] `verifier.rs`: implement `verify_directory(parent_path, writer, cancel_token)` — bidirectional readdir diff
      against DB (reuse `verify_affected_dirs` pattern: two-phase lock, DeleteEntry/UpsertEntry/PropagateDelta)
- [ ] `verifier.rs`: accept a `CancellationToken`, cancel on navigate-away (safe: each writer message is atomic,
      partial verification leaves DB consistent)
- [ ] `mod.rs`: wire `verify_directory` into navigation flow — after `list_directory_start`, spawn background
      verification for the listed directory
- [ ] `mod.rs`: cancel previous verification when user navigates away (same pattern as `cancel_nav_priority`)
- [ ] Emit `index-dir-updated` with corrected paths after verification completes (reuses existing frontend handler)
- [ ] Rust tests: verification finds stale entry, missing entry, changed entry, empty-dir-is-not-unindexed
- [ ] Manual test: delete/create/modify files while app is open, navigate to directory, verify corrections appear

### Milestone 2: DB-first listing path

- [ ] `entries` table: rename `size` → `physical_size`, add `logical_size` (`st_size`); bump schema version (triggers
      auto-rebuild)
- [ ] `ScannedEntry`: rename `size` → `physical_size`, add `logical_size: Option<u64>`
- [ ] `scanner.rs`: store both `physical_size` (`st_blocks * 512`) and `logical_size` (`st_size`) from the same
      `stat()` call
- [ ] Update all `ScannedEntry` consumers (writer, reconciler, aggregator, verifier) for the field rename
- [ ] `store.rs`: add `entry_exists(path) -> bool` method (single-row existence check on `entries` table)
- [ ] `indexing/mod.rs`: add `list_from_index(parent_path) -> Option<Vec<FileEntry>>` — queries DB, converts
      ScannedEntry → FileEntry (logical_size → FileEntry.size, icon_id via `get_icon_id()`), enriches with dir_stats;
      returns `None` if directory not indexed or `scan_completed_at` not set
- [ ] `operations.rs`: in `list_directory_start_with_volume()`, try `list_from_index` first; fall back to
      `volume.list_directory()` if `None`
- [ ] `operations.rs`: when DB-first path is used, spawn background `verify_directory`
- [ ] `streaming.rs`: in `list_directory_start_streaming()`, same DB-first-then-fallback logic; if DB-first succeeds,
      populate cache and emit `listing-complete` directly (no progress events needed for sub-ms reads)
- [ ] Handle the cache update: when verification finds diffs, compare against current cache state (not original DB
      snapshot) to noop watcher-handled changes; update `LISTING_CACHE`, re-sort, emit `listing-updated`
- [ ] Verify sorting works correctly with DB-first data (especially sort-by-size with recursive sizes from enrichment)
- [ ] Verify watcher diffs still work correctly when initial listing came from DB (cache structure is identical)
- [ ] Rust tests: DB-first path returns correct FileEntry fields (logical size matches readdir); fallback to readdir
      when not indexed or scan incomplete; cache update on diff

### Milestone 3: Performance validation

- [ ] Benchmark: DB-first listing vs readdir for directories with 100, 1K, 10K, 100K files (use `benchmark::` infra)
- [ ] Benchmark: verification overhead (time from first paint until verification completes)
- [ ] Profile: confirm no lock contention between DB-first reads and writer thread during concurrent scan
- [ ] Run all checks: `./scripts/check.sh`
- [ ] Update `indexing/CLAUDE.md` — remove "verifier.rs is a placeholder" note, document DB-first flow
- [ ] Update `listing/CLAUDE.md` — document DB-first path in data flow section

### Follow-up: watcher dedup

- [ ] Skip starting per-directory `notify` watcher for directories on indexed volumes (volume-level FSEvents watcher
      already covers them; 300ms batching latency is acceptable for background updates)
