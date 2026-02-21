# Drive indexing benchmarks

Benchmarks run on macOS Sequoia (Darwin 25.2.0), Apple Silicon, APFS. 2026-02-19.

Machine has ~5M files on disk (~1.9M indexed by Spotlight).

Benchmark scripts are in this directory:

- `bench-searchfs-count-all-files.c` — counts files via `searchfs()` catalog scan (two passes: with/without dot)
- `bench-searchfs-find-by-name.c` — finds a single file by exact name via `searchfs()`
- `bench-getattrlistbulk-recursive-walk.c` — counts + sums sizes via `getattrlistbulk()` recursive walk
- `bench-enumerator-at-url.m` — counts files via `NSFileManager enumeratorAtURL` (ObjC)

Compile C with `clang -O2 -Wall -o <output> <source>.c`.
Compile ObjC with `clang -O2 -Wall -o <output> <source>.m -framework Foundation`.
Run with `time`.

## 1. Counting all files on disk

| Method                                       | Files  | Dirs | Symlinks | Total entries | Logical size | Physical size | Wall clock | CPU time       |
|----------------------------------------------|--------|------|----------|---------------|--------------|---------------|------------|----------------|
| `mdfind` (Spotlight index)                   | —      | —    | —        | ~1,877K*      | —            | —             | **1m 24s** | 5.3s user+sys  |
| `searchfs()` (catalog scan)                  | 4,799K | —    | —        | 4,799K        | —            | —             | **2m 24s** | 36.2s sys      |
| `getattrlistbulk` (recursive walk)           | 4,957K | 877K | 164K     | 5,999K        | —            | —             | **1m 49s** | 45.1s sys      |
| `getattrlistbulk` + logical sizes            | 4,957K | 877K | 164K     | 5,999K        | 10.6 TB      | —             | **1m 53s** | 47.9s sys      |
| `getattrlistbulk` + logical + physical sizes | 4,957K | 877K | 164K     | 5,999K        | 10.6 TB      | 905 GB        | **1m 53s** | 46.6s sys      |
| `enumeratorAtURL` (empty keys + type)        | 4,644K | 706K | 148K     | 5,499K        | —            | —             | **2m 53s** | 106.5s usr+sys |
| `enumeratorAtURL` (prefetched keys+sizes)    | 4,644K | 706K | 148K     | 5,499K        | 10.5 TB      | —             | **2m 05s** | 57.4s usr+sys  |
| `enumeratorAtURL` + logical + physical sizes | 4,645K | 707K | 148K     | 5,499K        | 10.5 TB      | 844 GB        | **2m 03s** | 57.3s usr+sys  |

*`mdfind` doesn't distinguish types; its total includes files, dirs, and symlinks mixed together.

The ~10.5 TB logical size is real but inflated by APFS sparse files, VM images (`Docker.raw`), swap, and
clones. The physical sizes (905 / 844 GB) are closer to reality but still overcount — the actual volume
usage reported by Disk Utility / `statfs()` is **746 GB**.

### Per-file physical sums don't match volume usage

| Source                            | Reported |
|-----------------------------------|----------|
| Disk Utility / `statfs()`         | **746 GB** (ground truth) |
| `getattrlistbulk` `DATAALLOCSIZE` sum | 905 GB (+21%) |
| `enumeratorAtURL` alloc size sum  | 844 GB (+13%) |
| `DATALENGTH` logical sum          | 10.5 TB (meaningless for disk usage) |

The 746→905 GB overcounting is caused by **APFS clones** (reflinks, copy-on-write). When APFS clones a
file — which happens frequently with Xcode, iOS simulators, Time Machine, and even `cp` since Ventura —
each clone reports its full `DATAALLOCSIZE` individually, but the underlying disk blocks are shared. Two
1 GB clones = 2 GB per-file sum, but only 1 GB of actual disk blocks.

`statfs()` reports true block-level usage and is always correct. Per-file `DATAALLOCSIZE` is correct
*per file* but overcounts when files share blocks.

**Planned solution for Cmdr (Option A: scale to fit):**

- **Volume usage bar** ("746 GB of 995 GB"): always use `statfs()`. One syscall, always correct.
- **Directory sizes and treemaps**: use per-file `DATAALLOCSIZE` for *relative proportions*, then normalize
  to the volume total: `display_size = file_alloc * (volume_used / sum_of_all_alloc)`. Per-file sizes are
  still correct relative to each other (a 10 GB dir is 10x a 1 GB dir), they just get scaled so the
  treemap fills the box exactly. This is what GrandPerspective and WinDirStat do.

### Why the counts differ

- **mdfind (1.9M)**: Only counts Spotlight-indexed items. Excludes `.git/` object stores, `node_modules/`,
  caches, Xcode derived data, Homebrew internals, Docker layers, and anything on the Spotlight privacy list.
- **searchfs (4.8M)**: Walks the raw APFS catalog B-tree. Finds every file but can't distinguish types in
  a single pass (only MATCHFILES flag). Searched `/` and `/System/Volumes/Data` separately.
- **getattrlistbulk (5.0M files + 877K dirs + 164K symlinks)**: Recursive directory walk from `/`. Reports
  per-entry type breakdown. Found ~158K more files than `searchfs` — likely because `searchfs` can't
  enumerate files inside the 341 TCC/SIP-protected directories that `open()` can't access, while
  `getattrlistbulk` counts everything it can reach. The 877K dirs and 164K symlinks are absent from the
  searchfs count because it only used MATCHFILES.
- **getattrlistbulk + sizes**: Same walk but requesting `ATTR_FILE_DATALENGTH` too. Adding size collection
  had negligible impact on timing (~4% slower). The `off_t` for each file is packed right after `objtype`
  in the same bulk buffer — no extra syscalls.
- **enumeratorAtURL (4.6M files, 706K dirs)**: Foundation's `NSFileManager` wrapper. Found ~500K fewer
  entries than `getattrlistbulk`. The gap is likely because enumeratorAtURL silently skips more protected
  dirs than raw `open()`. Two variants were tested:
    - *Empty keys + per-item `getResourceValue`* (2m53s): Each URL triggers a separate `stat()` for type
      classification. Slowest overall.
    - *Prefetched keys + sizes* (2m05s): Passing `NSURLIsDirectoryKey`, `NSURLIsSymbolicLinkKey`, and
      `NSURLFileSizeKey` in `includingPropertiesForKeys` lets Foundation batch-fetch metadata. **28% faster**
      than the empty-keys variant despite collecting MORE data — proof that prefetching beats per-item stat().

**Both `getattrlistbulk` and `enumeratorAtURL` agree on ~10.5 TB logical size**, confirming this is real
(not a parsing bug). The inflation vs Finder's ~500 GB is due to APFS sparse files, Docker's `Docker.raw`
disk image, VM swap, and possibly Time Machine local snapshots. `ATTR_FILE_DATALENGTH` / `NSURLFileSizeKey`
report *logical* size; Finder shows *physical allocation*.

All methods are I/O-bound (17–61% CPU utilization).

## 2. What each method actually counts

| Aspect                           | mdfind                 | searchfs                              | getattrlistbulk                         | enumeratorAtURL                              |
|----------------------------------|------------------------|---------------------------------------|-----------------------------------------|----------------------------------------------|
| Directories                      | Included in total      | Excluded (MATCHFILES only)            | Counted separately (877K)               | Counted separately (706K)                    |
| Symlinks                         | Included in total      | Probably included (SKIPLINKS not set) | Counted separately (164K), not followed | Counted separately (148K), not followed      |
| Double-count via symlinks        | Possible               | No (catalog entries are unique)       | No (symlinks not followed)              | No (symlinks not followed)                   |
| Mounted DMGs                     | Included               | Only volumes you target               | Walked into if reachable from /         | Walked into if reachable from /              |
| Network drives                   | Excluded (grep filter) | Not searched                          | Excluded (/Volumes/naspi skipped)       | Excluded (/Volumes/naspi skipped)            |
| `.git/`, `node_modules/`, caches | Excluded by Spotlight  | Included                              | Included                                | Included                                     |
| App bundle internals             | Partial                | Full                                  | Full                                    | Full                                         |
| System/hidden files              | Partial                | Full                                  | Full (minus 341 permission errors)      | Full (fewer dirs accessible)                 |
| Data volume handling             | Transparent            | Searched as separate volume           | Skipped mount; firmlinks cover it       | Skipped mount; firmlinks cover it            |
| File sizes                       | Not available          | Not available                         | Same pass, negligible cost (+4%)        | Via prefetched keys, 28% faster than without |

The ~3M gap between mdfind and the other methods is almost entirely explained by Spotlight's exclusion
rules. Developer machines accumulate millions of files in hidden trees that Spotlight deliberately ignores.

## 3. Single file lookup by exact name

Searching for `"Naspolya OpenVPN config with ddns.ovpn"`:

| Method                   | Wall clock | Relative    |
|--------------------------|------------|-------------|
| `mdfind -name`           | **93ms**   | 680x faster |
| `searchfs()` exact match | **63s**    | baseline    |

`mdfind` does an indexed lookup (O(log n) or better), while `searchfs()` performs a full catalog scan
comparing every filename (O(n) where n ~ 5M). For point lookups, Spotlight wins by ~3 orders of magnitude.

`getattrlistbulk` and `enumeratorAtURL` were not benchmarked for single-file lookup — a recursive walk
would be even slower than `searchfs()` since it must traverse the directory tree rather than scanning
the flat catalog.

## Takeaways

- **`getattrlistbulk` is the best fit for Cmdr's indexing use case.** It's the fastest for full
  enumeration (1m49s–1m53s with sizes), gives a clean per-entry type breakdown, and returns sizes in the
  same pass at negligible extra cost. This is what `jwalk` uses under the hood on macOS.
- **`enumeratorAtURL`** with prefetched keys is 11% slower than `getattrlistbulk` (2m05s vs 1m53s) and
  finds ~500K fewer entries. Without prefetching, it's 58% slower (2m53s) due to per-item `stat()`.
  Confirmed that prefetching property keys is critical — `enumeratorAtURL` with empty keys + manual
  `getResourceValue` is the worst of all approaches. Not recommended for Cmdr's Rust backend regardless.
- **`mdfind`** is unbeatable for single-file lookup (~100ms) but undercounts significantly (only
  Spotlight-indexed files) and doesn't return directory sizes.
- **`searchfs()`** is a middle ground — faster than recursive walk for name-based bulk search, but can't
  return metadata (sizes, dates). Useful for search, not enumeration. Requires careful struct packing
  (leading `u_int32_t size` field in search params).
  Reference: [sveinbjornt/searchfs](https://github.com/sveinbjornt/searchfs).
- **Parallelism** (e.g., `jwalk` with rayon) would improve the `getattrlistbulk` approach further by
  walking multiple directory branches concurrently. All benchmarks above are single-threaded.
- Neither method counts files on network volumes unless explicitly targeted.
