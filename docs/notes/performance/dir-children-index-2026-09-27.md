# A partial index for child directories (2026-09-27)

Lever 3 of `idle-census-2026-09-27.md`: the index writer's `recompute_min_subtree_epoch` found a folder's child dirs by
reading every child. The fix is `idx_child_dirs ON entries (parent_id) WHERE is_directory = 1`, created on open. The
mechanism and the decision live in `crates/cmdr-index/src/indexing/store/DETAILS.md` § "child directories have their own
partial index"; this note holds the evidence.

## Setup

- **Data**: `sqlite3 -readonly … ".backup"` of prod's `index-root.db` (v0.47.0 running, so read-only), 931 MB, 5,898,856
  entries, 609,269 of them directories. Two APFS clones of the backup: `base.db` as-is and `after.db` with the index.
- **Tool**: `sqlite3` 3.54.0 CLI, `.timer on` plus `.mode off` (rows are stepped but not printed), one warm-up and nine
  timed runs per query, median reported. Warm page cache. Apple M3 Max.
- **Folders**: the four heaviest by children plus root. `2998255` holds 92,218 files and one dir (Chrome's cache),
  `1064640` 68,590 files and no dirs, `1048403` 50,000 files and four dirs, `2616022` 2,850 dirs and one file (the
  all-dirs case, where the old plan read nothing extra), `1` the root (17 children).

## Per-folder queries

Median ms before → after (`base.db` → `after.db`):

- **`recompute_min_subtree_epoch`**: 59.4 → 0.009 (`2998255`), 49.5 → 0.009 (`1064640`), 16.5 → 0.009 (`1048403`), 1.78
  → 0.61 (`2616022`), 0.014 → 0.010 (root).
- **Subdir half of `recompute_recursive_has_symlinks`**: 61.4 → 0.008, 49.0 → 0.008, 16.5 → 0.009, 1.65 → 0.57, 0.010 →
  0.008.
- **`read_child_dir_coverage`**: 59.8 → 0.010, 49.2 → 0.008, 16.8 → 0.011, 1.83 → 1.36, 0.015 → 0.012.
- **`list_child_dir_ids_and_names`**: 59.8 → 0.006, 48.5 → 0.006, 16.5 → 0.006, 0.96 → 0.72, 0.009 → 0.007.
- **`for_each_child_directory_of`** over three parents (the named one, a 100,000-file folder, and `1064640`): 164 →
  0.015, 100 → 0.016, 121 → 0.016, 103 → 0.85, 102 → 0.020.

Every plan moves from `SEARCH … USING INDEX idx_parent_name_folded (parent_id=?)` to `idx_child_dirs`, a covering search
for the first two. Spread was under 20% of the median everywhere. The old cost is linear in a folder's children and
spent almost entirely on files it throws away; the new one is linear in its child dirs.

What that means for the writer: a `PropagateMinSubtreeEpoch` or a dir create or delete in Chrome's cache folder paid ~60
ms at that level alone before, and the census saw that query as about half of the writer's running samples during churn.
It now pays ~10 µs there. No A/B of the writer's CPU under synthetic churn was run: the query timing is a three-to-four
orders of magnitude cut on the dominant step, which no churn setup would blur.

## Whole-table directory reads

Median ms, two `base.db` runs → `after.db`:

- **`load_dirs_missing_stats`**: 310 / 318 → 97 (covering scan plus the `dir_stats` join).
- **`load_all_directory_ids`**, **`bulk_get_child_dir_ids`**: ~258 → 18 (covering scan).
- **`COUNT(*) … WHERE is_directory = 1`**: 252 → 4.2. **The importance differential sample**: 9 → 0.8.
- **`bulk_get_listed_epochs`**, **`meta.rs`'s directory paths**: ~275 → 175–188 (index scan plus a row lookup per dir;
  still faster than reading 5.9 M rows).
- **`all_directories`**, **`for_each_directory`** (`ORDER BY id`): unchanged at ~285–297; the planner keeps the table
  scan, which yields rowid order for free.
- **`scoped_get_child_dir_ids`** under `2616022`: 17.4 → 13.8.

No query got slower.

## Cost

- **Size**: 7,540,736 bytes (`dbstat`), 0.8% of the file.
- **Build** (`CREATE INDEX`, once per existing DB, on the first open after the upgrade): 2.5–2.8 s wall on a fresh clone
  (0.65 s of it CPU, the rest reading the file), 0.34–0.35 s with a warm cache. Three runs each.
- **Writes**: only directory rows enter the index, so file inserts, updates, and deletes don't touch it. A full scan
  pays the equivalent of the build, ~0.35 s of CPU, spread over minutes of walking.

## Follow-up: the direct-symlink test got its own partial index (2026-10-01, #320)

The direct-symlink test (`parent_id = ? AND is_symlink = 1`) had the same shape and cost ~62 ms on `2998255`. It runs
only when a symlink appears, goes, or changes, so it was first left alone; it now has
`idx_child_symlinks ON entries (parent_id) WHERE is_symlink = 1`, created on open like `idx_child_dirs`. Measured on a
fresh `.backup` of prod's `index-root.db` (5,611,289 rows, 154,156 of them symlinks), `sqlite3 .timer`, warm, load
average ~40 (other agents building), so the "before" figures run high:

- **Query**: 241–316 ms → ~8 µs on the 200,000-file folder `1179741`, 147–161 ms → ~9 µs on the 162,496-entry folder
  `1600180`. The plan moves from `idx_parent_name_folded (parent_id=?)` to `idx_child_symlinks`.
- **Size**: 1,863,680 bytes (`dbstat`).
- **Build**: 3.1 s wall cold, 0.6 s of it CPU.
- **Writes**: only symlink rows enter it, so ordinary file and directory writes don't touch it.

## Left as it is

- **`recompute_dir_stats_from_children`**'s `SUM` over every child reads every child by definition.
