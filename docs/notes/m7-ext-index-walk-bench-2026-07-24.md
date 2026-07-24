# M7 `ext`-column + partial-index walk bench (2026-07-24)

The measurement that retired resource-use-plan M7 (the plan itself is wiped; this note is the durable record, and the
skip verdict is at the bottom). Measures the coverage cold walk (`media_index/scheduler/enrich.rs::walk_image_entries`,
the `coverage::get_or_build` path) against copies of the two real dev drive-index DBs, then prototypes M7's `ext`
column + partial image-extension index on those copies and re-runs the walk in the pruned shape. David rejected the
per-folder image-count-aggregate alternative; only the `ext`-column shape was evaluated.

**Headline**: the pruned walk is **6.4x faster on the root DB** (4.7 s → 0.74 s warm) and **statistically nothing on the
NAS DB** (2.2 s → 2.1 s), because 85% of the NAS's files are images so there is almost nothing to prune. Disk cost after
VACUUM: **+21 MB root (+2.4%), +37 MB NAS (+7.4%)**. And the plan's 42 s cold baseline does not reproduce: today's root
walk is 7.0 s cold-ish / 4.7 s warm on the same query shape.

## Environment

- Apple M3 Max, 64 GB RAM, macOS 26.5.2 (build 25F84), rustc 1.97.1, release build, 2026-07-24.
- Harness (standalone cargo project mirroring the walk 1:1, same `rusqlite` 0.40.1 bundled as the app, same read-path
  pragmas + `platform_case` collation):
  `/private/tmp/claude-501/-Users-veszelovszki-projects-git-vdavid-cmdr/733dfd6f-550d-4e88-80f8-df2cf9cf56ca/scratchpad/m7-walk-bench/harness/`
  (session scratchpad, so treat this note as the durable record; `src/main.rs` has `baseline` / `migrate` / `m7walk` /
  `vacuum` subcommands).
- Source DBs, copied 2026-07-24 08:26 with the SQLite backup API (`.backup`) from the live dev data dir
  (`com.veszelovszki.cmdr-dev`), app possibly running (backup API is WAL-safe):
  - `index-root.db`: 950 MB + 190 MB WAL at copy time → 991 MB copy. 6,127,098 entries (5,532,305 files, 594,793 dirs),
    schema v14.
  - `index-smb-192-168-1-111-445-naspi.db` (the NAS): 497 MB, empty WAL → 522 MB copy. 2,642,641 entries (2,571,675
    files, 70,966 dirs), schema v14.

## Method

- **Baseline** replicates `walk_image_entries` exactly: the 9-column `all_directories` prefetch
  (`WHERE is_directory = 1 ORDER BY id`) into an `id → row` map, then the streaming file query
  (`SELECT parent_id, name, modified_at, logical_size FROM entries WHERE is_directory = 0 ORDER BY parent_id`),
  per-parent grouping, the verbatim sibling-aware `qualify_dir` (copied from `predicate.rs`, including RAW+JPEG and
  Live-Photo rules), and path reconstruction.
- **Migrate** is the M7 prototype: `ALTER TABLE entries ADD COLUMN ext TEXT`, populate via one UPDATE with a registered
  scalar function replicating `predicate.rs::ext_of` (lowercased final extension, files only), then
  `CREATE INDEX idx_ext_image ON entries (parent_id) WHERE ext IN (<IMAGE_EXTS ∪ RAW_EXTS>)` (20 extensions, RAW
  included: a lone RAW enriches, so RAW counts as image-bearing), then `PRAGMA wal_checkpoint(TRUNCATE)`.
- **M7 walk** (the pruned shape): same dirs prefetch, then `SELECT DISTINCT parent_id` off the partial index (verified
  via `EXPLAIN QUERY PLAN`: `SCAN entries USING INDEX idx_ext_image`), then per image-bearing dir a full-sibling file
  fetch through the existing `idx_parent_name_folded` prefix, and the unchanged `qualify_dir` over the complete group.
  Qualification is NOT replaced: every dir that survives the prune is qualified over its full file set, so RAW+JPEG
  pairs and Live-Photo `.mov` siblings are still seen.
- **Equivalence**: an order-independent checksum (XOR of per-path hashes, Live-Photo kind bit folded in) over the
  emitted set. Baseline and M7 walk produce identical checksums on both DBs.
- **Disk delta**: VACUUM a pristine copy vs VACUUM the migrated copy (page-slack-honest); index's own size via `dbstat`.
- **Cold caveat**: `purge` needs sudo, so true cold-I/O was not measurable. "Cold-ish" = first run in a fresh process
  (cold SQLite page cache, warm macOS page cache — the copies were freshly written). Warm = repeat runs. All numbers are
  therefore CPU-shape numbers, not disk-I/O numbers.

## Results

### Baseline walk (current shape)

| DB   | Cold-ish total | Warm total     | dirs prefetch (warm) | file walk (warm) | Files streamed | Images emitted | Image dirs        |
| ---- | -------------- | -------------- | -------------------- | ---------------- | -------------- | -------------- | ----------------- |
| Root | 7,030 ms       | 4,715–4,731 ms | ~455 ms              | ~4,270 ms        | 5,532,305      | 231,145        | 10,439 of 594,793 |
| NAS  | 3,291 ms       | 2,218–2,232 ms | ~152 ms              | ~2,070 ms        | 2,571,675      | 2,176,221      | 17,577 of 70,966  |

### Migration (ALTER + populate + partial index + checkpoint)

| DB   | Populate (all file rows) | Index build | Checkpoint | WAL peak during build | DB size after (pre-VACUUM) | Image-ext rows |
| ---- | ------------------------ | ----------- | ---------- | --------------------- | -------------------------- | -------------- |
| Root | 4,515 ms                 | 10,182 ms   | 18 ms      | 626 MB                | 1,026 MB (from 991 MB)     | 231,171        |
| NAS  | 2,472 ms                 | 4,309 ms    | 4 ms       | 368 MB                | 593 MB (from 522 MB)       | 2,176,556      |

The populate rewrites every table page, so the WAL peaks at roughly two-thirds of DB size — the plan's "background-build
post-launch, checkpoint after" guidance is confirmed necessary if M7 ships.

### Disk delta (VACUUMed pristine vs VACUUMed migrated)

| DB   | Pristine (VACUUM) | Migrated (VACUUM) | Delta                 | Bytes/entry | `idx_ext_image` alone     |
| ---- | ----------------- | ----------------- | --------------------- | ----------- | ------------------------- |
| Root | 886,775,808 B     | 907,657,216 B     | +20,881,408 B (+2.4%) | 3.4 B       | 2,777,088 B (12.0 B/row)  |
| NAS  | 496,046,080 B     | 532,934,656 B     | +36,888,576 B (+7.4%) | 14.0 B      | 25,952,256 B (11.9 B/row) |

These ARE the 2M-NAS and 5M-root DBs, so the deltas are the projected absolute costs directly. The cost scales with
image density (the `ext` text plus ~12 B per indexed image row), which inverts the benefit: the DB where the index costs
the most (NAS) is the one it helps least.

### The pruned walk (M7 shape) and the payoff

| DB   | M7 total (fresh process) | M7 warm        | dirs prefetch | prune query | per-dir walk | Files streamed        | Checksum vs baseline |
| ---- | ------------------------ | -------------- | ------------- | ----------- | ------------ | --------------------- | -------------------- |
| Root | 741 ms                   | 693–740 ms     | ~470 ms       | 5–6 ms      | ~250 ms      | 284,436 (was 5.5M)    | identical            |
| NAS  | 2,124 ms                 | 2,046–2,109 ms | ~165 ms       | ~50 ms      | ~1,880 ms    | 2,228,082 (was 2.57M) | identical            |

- **Root speedup: 6.4x total warm (4,715 → 740 ms); the file-walk phase alone is 17x (4,270 → 250 ms).** The dirs
  prefetch (~470 ms) is now 65% of the pruned walk.
- **NAS speedup: 1.05x (2,218 → 2,109 ms).** 85% of NAS files are images, so pruning removes only 13% of streamed rows
  and none of the qualification work.
- Baseline re-run on the migrated+vacuumed root: 5,079 ms — the wider rows don't materially slow the unpruned path
  (+~7%, near noise).
- Root image-dir counts differ by 9 (10,448 parent ids vs 10,439 distinct paths) because distinct firmlink-style ids can
  reconstruct to the same path string; the emitted set is byte-identical (checksums match).

## Honest caveats

1. **The plan's 42 s cold baseline (2026-07-16) does not reproduce**: 7.0 s cold-ish on the same walk shape. Two known
   differences: (a) today's root index is 6.1M entries / 0.95 GB, not the ~11.5M rows / 1.9 GB the plan and code
   comments reference — the index has shrunk since; (b) this bench couldn't evict the macOS page cache, and the in-app
   number likely included true-cold I/O plus in-app contention (importance recompute, indexing writer, read-pool
   traffic). Scaling the measured CPU shape to 11.5M rows only reaches ~10 s, so contention or true-cold I/O must have
   carried most of the 42 s. An in-app measurement should precede any decision that leans on the 42 s figure (same
   spirit as M8's "verify where the 1.4 s goes").
2. **On a true cold cache, M7 cannot beat the dirs-prefetch I/O floor**: `all_directories` full-scans the entries table
   (`WHERE is_directory = 1`, no index), so every table page is read even in the pruned walk. M7's I/O saving on cold
   disk is bounded to the second (file) pass; the CPU saving is what the numbers above show.
3. All timings are page-cache-warm (see Method). Cold-ish vs warm differ mainly by SQLite's own page cache and the dirs
   prefetch.
4. The prototype populated `ext` via a SQL scalar function in one UPDATE; the real M7 writer sites (throttled live
   upsert + reconciler rename pre-pass) are not measured here — they'd add a per-row `ext_of` call, negligible per
   write.

## What a schema bump invalidates

M7 would ship as a plain `SCHEMA_VERSION` "14" → "15" bump (`indexing/store/`: mismatch → `delete_and_recreate`, no
migrations by design). That deletes and rebuilds every volume's drive index: a full filesystem rescan per volume (David:
NAS ~10 min, local ~2 min — declared cheap, 2026-07-24). `dir_stats` rebuilds with the rescan. The importance and media
DBs are separate files and survive; media staleness keys on `(path, mtime, size)`, so no re-enrichment follows.

## Verdict: skip (evidence above; David rejected the folder-aggregate alternative, 2026-07-24)

The walk runs once per session (`coverage::get_or_build`, kept warm by passes), so M7 buys ~4 s once per launch on root
and ~0.1 s on the NAS, for a schema bump, two writer-site changes, and +58 MB across the two DBs. Revisit only if an
in-app measurement shows the cold preview still costs tens of seconds (then the fix is the contention actually burning
the time, not the index), or if a sparse-image corpus at 10M+ files appears.
