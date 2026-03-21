# Incremental file watcher

Make directory changes appear instantly in large directories (50k–200k files) by processing individual FS events
incrementally instead of re-reading the entire directory.

## Problem

The file watcher currently discards individual events from `notify_debouncer_full` and re-reads the entire directory on
every change (`list_directory_core` → `compute_diff` → emit). For 200k files, this re-read takes several seconds,
creating terrible UX: mkdir appears to silently fail, copied files don't show up, external changes are invisible until
the re-read completes.

## Key insight

The debouncer already provides `Vec<DebouncedEvent>` with individual `Event { kind: EventKind, paths: Vec<PathBuf> }`.
We currently discard these (`Ok(_events)` on line 119 of `watcher.rs`). We can use them to apply targeted changes
(stat one file, insert/remove at sorted position) instead of re-reading everything.

## Pre-existing bug: sort mismatch in `compute_diff`

`list_directory_core` always sorts by Name/Asc (hardcoded, `reading.rs` line 144). `compute_diff` then compares
`old_entries` (sorted by user's chosen sort) against `new_entries` (sorted by Name/Asc). The `index` field in
`DiffChange` refers to positions in these differently-sorted lists. `update_listing_entries` then re-sorts by the
user's sort params. So diff indices are wrong for any sort other than Name/Asc.

**Fix**: In `handle_directory_change`, re-sort `new_entries` by the listing's sort params BEFORE calling
`compute_diff`. This way both old and new are in the same order and indices are correct. This fix is included in
milestone 2 since it shares the comparator extraction work.

## Design

### Incremental path (fast, ~1ms per event batch)

1. Receive `Vec<DebouncedEvent>` from debouncer
2. Classify each event: create → `stat` + sorted insert, remove → find by path + splice out, modify → find by path +
   re-stat + update in place (or remove + re-insert if sort-relevant fields changed)
3. For each mutation, record the `DiffChange` with its index (we already have this field)
4. Emit one `directory-diff` event with all changes

### Full re-read fallback (slow but authoritative)

Fall back to full re-read when:
- An event batch is "too complex" to handle incrementally (for example, > 500 events, suggesting a mass change)
- An event type is ambiguous or unsupported (`EventKind::Any` or `EventKind::Other`)
- The incremental path encounters an error (stat fails for unexpected reasons)

When a full re-read fallback runs, it uses `list_directory_core` → re-sort by listing params → `compute_diff` → emit.

Periodic consistency re-reads are deferred until evidence of drift appears. If the incremental path uses the exact same
comparator and enrichment functions, drift should not occur.

### Synthetic diff for mkdir

As a quick win independent of the watcher refactor: make `create_directory` immediately insert the new entry into the
listing cache and emit a synthetic `directory-diff`. This gives instant feedback for the most common "why is nothing
happening?" case.

**Why separate from incremental watcher**: The watcher debounces for 200ms, then processes. Even with incremental
updates, there's a 200ms+ delay. The synthetic diff from mkdir bypasses the watcher entirely — the folder appears in
< 50ms. When the watcher fires later, incremental processing sees the entry already in the cache → no-op (or minor
metadata `modify`).

### Cursor adjustment

Small related fix: apply `adjustSelectionIndices` to `cursorIndex` in the same diff handler block where we already
adjust selection. Fixes cursor drift when files are added/removed around the cursor position.

## Implementation

### Milestone 1: Cursor adjustment (small, independent)

**Why first**: Smallest change, no backend work, immediately useful, no risk. Already discussed and designed.

**Scope**: In the `directory-diff` handler in `FilePane.svelte`, in the fallback block (where `operationSelectedNames
=== null`), add cursor adjustment using `adjustSelectionIndices([backendCursorIndex], removeIndices, addIndices)`.

**Details**:
- Convert `cursorIndex` from frontend to backend space: `backendCursor = cursorIndex - offset` where
  `offset = hasParent ? 1 : 0`
- Call `adjustSelectionIndices([backendCursor], removeIndices, addIndices)`
- If result is non-empty, set `cursorIndex = result[0] + offset`
- If result is empty (cursor file was removed), clamp: `cursorIndex = Math.min(cursorIndex, totalCount - 1)`
- Ensure `cursorIndex >= 0` (edge case: last file removed from empty-ish directory)

**Files**:
- `apps/desktop/src/lib/file-explorer/pane/FilePane.svelte` — add ~8 lines in the diff handler

**Tests**: Add a test case in `adjust-selection-indices.test.ts` for single-element arrays (cursor use case). Already
covered implicitly but an explicit test documents the intent.

### Milestone 2: Sorted insertion infra + mkdir synthetic diff + sort mismatch fix

**Why second**: Independent of the watcher refactor. Gives the biggest UX improvement for the least effort. Also fixes
the pre-existing sort mismatch bug which affects the existing `adjustSelectionIndices` wiring.

**Scope**:

**2a. Extract comparator** (`listing/sorting.rs`):
- Extract the closure from `sort_entries` into a standalone function:
  `pub fn entry_comparator(sort_by, sort_order, dir_sort_mode) -> impl Fn(&FileEntry, &FileEntry) -> Ordering`
- Refactor `sort_entries` to use it: `entries.sort_by(entry_comparator(sort_by, sort_order, dir_sort_mode))`
- Note: the comparator must handle `recursive_size: None` for directories (sorts last regardless of asc/desc — see
  `sorting.rs` lines 107-132)

**2b. Cache helpers** (`listing/caching.rs`):
- `find_listings_for_path(parent_path: &Path) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)>` — scans
  `LISTING_CACHE` for entries whose `CachedListing.path` matches `parent_path` (the directory being listed, NOT the
  new file/folder's path). `PathBuf` comparison is sufficient — paths are already tilde-expanded and normalized by the
  time they reach the cache. Return listing_id + sort params for each match. There may be 0 (no pane showing that
  dir), 1 (typical), or 2 (both panes showing same dir).
- `insert_entry_sorted(listing_id: &str, entry: FileEntry) -> Option<usize>` — write-locks `LISTING_CACHE`, uses
  `partition_point` with `entry_comparator` to find insertion position, splices in, returns the index. Returns `None`
  if listing not found or entry already exists (by path).

**2c. Emit synthetic diff from mkdir** (`commands/file_system.rs`):
- After `create_directory` succeeds (line ~85), construct `FileEntry` via `get_single_entry(&full_path)`
- Enrich via `enrich_entries_with_index(&mut [entry])` (single-element slice works fine)
- Call `find_listings_for_path` to find affected listings
- For each listing, call `insert_entry_sorted` → get insertion index
- Emit `directory-diff` event with a single `add` change
- **AppHandle access**: Add `app: tauri::AppHandle` parameter to the Tauri command (standard pattern, Tauri injects
  it automatically). Or access via `WATCHER_MANAGER.app_handle` — prefer the parameter since it's cleaner.
- **Where to add the code**: After the `spawn_blocking` returns and the `map_err` line (around line 126), before
  `return Ok(...)`. The synthetic diff runs on the async command thread, not inside `spawn_blocking`.
- **Sequence number**: Increment the listing's sequence in `WATCHER_MANAGER` (same as existing watcher path).
  **Lock ordering**: acquire `LISTING_CACHE` write lock first (for `insert_entry_sorted`), release it, then acquire
  `WATCHER_MANAGER` write lock (for sequence increment). This matches the order used in `handle_directory_change`.

**2d. Fix sort mismatch in full re-read** (`watcher.rs`):
- In `handle_directory_change`, after `list_directory_core` returns `new_entries` and before `compute_diff`:
  read the listing's sort params from `LISTING_CACHE`, re-sort `new_entries` with
  `sort_entries(&mut new_entries, listing.sort_by, listing.sort_order, listing.directory_sort_mode)`
- This ensures `compute_diff` compares two lists in the same sort order, making diff indices correct
- Requires reading sort params from cache (read lock) before calling `compute_diff`
- Note: `update_listing_entries` also re-sorts, so entries are sorted twice. This is intentional — the first sort
  makes `compute_diff` produce correct indices, the second sort (in `update_listing_entries`) is the existing
  cache-update path. Both are fast (~15ms for 50k entries). Don't "optimize" away either one.

**2e. Watcher deduplication**:
- When the watcher fires after a synthetic mkdir diff, it re-reads the directory. The new folder is already in the
  cache. `compute_diff` compares old (with folder) vs new (with folder) → sees it as existing → either no change or a
  minor `modify` if metadata differs slightly. No duplicate `add`.

**Files**:
- `listing/sorting.rs` — extract `entry_comparator`
- `listing/caching.rs` — add `find_listings_for_path`, `insert_entry_sorted`
- `commands/file_system.rs` — synthetic diff after mkdir
- `watcher.rs` — fix sort mismatch in `handle_directory_change`

**Tests**:
- Rust unit test for `entry_comparator` (verify it produces same order as `sort_entries`)
- Rust unit test for `insert_entry_sorted` with various sort configurations (Name asc, Size desc, dirs-first)
- Rust unit test for `find_listings_for_path` (0, 1, 2 matches)
- Rust unit test verifying `compute_diff` produces correct indices when both lists are sorted the same way

### Milestone 3: Incremental watcher — core

**Why third**: The biggest change. Builds on the sorted insertion infrastructure from milestone 2.

**Scope**:

1. **Change the debouncer callback** to pass events:
   - `Ok(events) => handle_directory_change_incremental(&listing_for_closure, events)` (note: the closure captures
     `listing_for_closure`, not `listing_id`)
   - Keep `Err(_) => handle_directory_change(&listing_for_closure)` as full-re-read fallback

2. **New function `handle_directory_change_incremental(listing_id: &str, events: Vec<DebouncedEvent>)`**:
   - If events.len() > 500, fall back to `handle_directory_change(listing_id)` and return
   - Group events by path (one path may have multiple events — for example, create + modify)
   - For each path, check if it's a child of the watched directory (ignore subdirectory events — we watch
     non-recursively but the debouncer may report subdirectory changes)
   - For each path, resolve the net state: `stat` the path. If it exists → create or modify. If gone → remove.
   - Compare against current cache: if path in cache and stat succeeded → modify. If path not in cache and stat
     succeeded → create. If path in cache and stat failed → remove. If path not in cache and stat failed → ignore.
   - Apply changes using `insert_entry_sorted`, `remove_entry_by_path`, `modify_entry_in_place` from caching helpers
   - Enrich new/modified entries via `enrich_entries_with_index` (single-entry slices)
   - Build `Vec<DiffChange>` with indices
   - Emit `directory-diff` event
   - If any event has `EventKind::Any` or `EventKind::Other`, fall back to full re-read

3. **Event classification** (simplified — don't trust event kinds, trust the filesystem):
   - The debouncer's event kinds can be unreliable across platforms. Instead of matching on `EventKind`, use the
     "stat and compare" approach: stat every mentioned path, compare against cache, derive the change type.
   - Exception: `EventKind::Access(_)` can be skipped (performance optimization — no visible change)
   - Exception: `EventKind::Any` / `EventKind::Other` → fall back to full re-read (unknown territory)

4. **Additional cache helpers** (`listing/caching.rs`):
   - `remove_entry_by_path(listing_id: &str, path: &Path) -> Option<(usize, FileEntry)>` — finds entry by path,
     splices out, returns old index + entry
   - `modify_entry_in_place(listing_id: &str, entry: FileEntry) -> Option<(Option<usize>, usize)>` — finds by path,
     replaces. If sort-relevant fields changed, removes and re-inserts. Returns `(old_index_if_moved, new_index)`.
   - `has_entry(listing_id: &str, path: &Path) -> bool` — quick check if path exists in cache

5. **Locking considerations**:
   - The watcher callback runs on a `notify` background thread, not a tokio task
   - Keep `LISTING_CACHE` write lock duration short — do stat calls BEFORE acquiring the lock
   - Pattern: stat all paths (no lock needed), then acquire write lock, apply all changes, release, then emit events

6. **Rename handling**:
   - The "stat and compare" approach handles renames naturally: the old path stat fails (→ remove), the new path stat
     succeeds (→ add). No special rename logic needed.
   - External renames will deselect the renamed file — acceptable since we can't know the new name maps to the same
     user intent.

**Files**:
- `watcher.rs` — new `handle_directory_change_incremental`, refactored callback
- `listing/caching.rs` — `remove_entry_by_path`, `modify_entry_in_place`, `has_entry`

**Tests**:
- Rust unit tests for each cache helper
- Rust integration tests for `handle_directory_change_incremental`:
  - Create event → entry appears at correct sorted position
  - Remove event → entry gone
  - Modify event (size change with size sort) → entry moves to new position
  - Create for entry already in cache → treated as modify (idempotent)
  - Remove for entry not in cache → no-op
  - Batch with mixed creates/removes/modifies → correct final state
  - \> 500 events → falls back to full re-read
  - Rename (old path gone, new path appeared) → old removed, new inserted
  - Event for file in subdirectory → ignored (non-recursive watch)

### Milestone 4: Docs update and CLAUDE.md maintenance

- Update `apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md` — document the incremental watcher path, fallback
  strategy, synthetic diff for mkdir, and the sort mismatch fix
- Update `apps/desktop/src-tauri/src/file_system/watcher.rs` module doc comment
- Update `apps/desktop/src/lib/file-explorer/CLAUDE.md` — add cursor adjustment to the selection section

### After each milestone

- Run `./scripts/check.sh` (or at minimum `--check desktop-rust-clippy --check desktop-rust-tests --check
  desktop-svelte-tests`)
- Manual QA: in a directory with many files, create a folder, verify it appears instantly

## Agent distribution

Milestones are sequential. Each milestone is one agent (Opus).

| Milestone | Agent scope | Depends on |
|-----------|------------|------------|
| 1 | Cursor adjustment in FilePane.svelte + test | None |
| 2 | Comparator extraction + cache helpers + mkdir synthetic diff + sort fix | None (but run after 1) |
| 3 | Incremental watcher core | Milestone 2 (uses sorted insertion helpers) |
| 4 | Docs update | All above |
| Review | Full review agent | All above |
| Check | Run `./scripts/check.sh` | All above |

Milestones 1 and 2 could theoretically run in parallel (different files), but sequential is safer since milestone 2
touches `watcher.rs` which milestone 3 heavily modifies.

## Risks and mitigations

**Risk**: Incremental path produces different results than full re-read (sort order, enrichment)
**Mitigation**: Use the exact same comparator (`entry_comparator`) and enrichment functions. Full re-read now also uses
the same comparator (sort mismatch fix). If drift is ever observed, add a periodic full re-read (simple tick counter).

**Risk**: Performance regression for rapid bulk changes (drag 1000 files into directory)
**Mitigation**: Fallback threshold (> 500 events → full re-read). The full re-read is no worse than today.

**Risk**: Race between synthetic mkdir diff and watcher's first tick
**Mitigation**: Watcher's incremental processing checks if entry already exists before inserting. Synthetic diff wins
(instant), watcher deduplicates (no-op or modify).

**Risk**: Hidden files handling inconsistency
**Mitigation**: All entries stored in cache including hidden files (filtering is at query time). Incremental
insert/remove treats all files equally.

**Risk**: `enrich_entries_with_index` is expensive for full listing
**Mitigation**: For incremental changes, pass single-entry slices to `enrich_entries_with_index`. This function accepts
`&mut [FileEntry]` and does per-entry index lookups by path — single-element slices work fine.

**Risk**: Lock contention between synthetic diff (mkdir thread) and watcher (notify thread)
**Mitigation**: Both acquire `LISTING_CACHE` write lock briefly. `RwLock` serializes them. Keep lock duration short by
doing stat calls before acquiring the lock. Consistent lock ordering: always `LISTING_CACHE` before `WATCHER_MANAGER`.

## Known limitations (pre-existing, out of scope)

**Hidden files and diff indices**: When `include_hidden=false`, the frontend's selection indices are in the
visible-only space, but diff `index` fields are in the full (including hidden) space. `adjustSelectionIndices` uses
these directly, which is incorrect when hidden files exist before the changed file. This is a pre-existing bug in the
selection adjustment from the previous commit, not introduced or worsened by this plan. Fix separately if needed.
