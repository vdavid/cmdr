---
title: Index DB shrunk 7x with one schema change
date: 2026-03-04
description:
  How switching from text-path primary keys to an integer parent-child tree cut the Cmdr drive index from 3.85 GB to 540
  MB.
---

Cmdr is a keyboard-driven file manager I'm building in Rust. (See the rest of this website for details.) One of its
coolest features is a [background drive index]() that tracks every file on your volume so it can show recursive
directory sizes in file listings. The index lives in SQLite, gets populated by a parallel jwalk scan, and stays current
via FSEvents ( macOS) or inotify (Linux).

Last week, the index for my dev machine (5.5M entries, 528K directories) was **3.85 GB**. That's embarrassingly large
for what's essentially a file tree with some metadata.

After a schema migration, it's **540 MB**. Same data, 7x smaller. Here's what happened.

<!-- more -->

## The problem: paths as primary keys

The original schema used the full filesystem path as the primary key:

```sql
CREATE TABLE entries
(
    path         TEXT PRIMARY KEY,
    parent_path  TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    is_directory INTEGER NOT NULL DEFAULT 0, ..
    .
) WITHOUT ROWID;

CREATE INDEX idx_parent ON entries (parent_path);
```

This seems reasonable until you measure it:

| Component                                | Size     | %     |
| ---------------------------------------- | -------- | ----- |
| `entries` table (WITHOUT ROWID, text PK) | 1,938 MB | 52.8% |
| `idx_parent` secondary index             | 1,659 MB | 45.2% |
| `dir_stats` table                        | 75 MB    | 2.0%  |

Two things conspire to make this huge:

1. **Redundancy**: `parent_path` and `name` are always derivable from `path`. That's ~700 MB stored for free
   information.

2. **WITHOUT ROWID + text PK amplifies indexes**: In a WITHOUT ROWID table, secondary indexes store the full primary key
   as the row pointer. So `idx_parent` stores `(parent_path, full_path)` per row — averaging 235 bytes across 5.5M rows.
   The index is almost as large as the table.

## The fix: integer parent-child tree

The new schema models the filesystem as a parent-child tree with integer keys:

```sql
CREATE TABLE entries
(
    id           INTEGER PRIMARY KEY,
    parent_id    INTEGER NOT NULL,
    name         TEXT    NOT NULL COLLATE platform_case,
    is_directory INTEGER NOT NULL DEFAULT 0,
    is_symlink   INTEGER NOT NULL DEFAULT 0,
    size         INTEGER,
    modified_at  INTEGER
);
CREATE UNIQUE INDEX idx_parent_name ON entries (parent_id, name);
```

A root sentinel (id=1, parent_id=0) anchors the tree. `/Users/foo/bar.txt` becomes three entries connected by parent_id
references.

The index `idx_parent_name` does double duty: listing directory children (`WHERE parent_id = ?`) and resolving path
components (`WHERE parent_id = ? AND name = ?`). Both are integer-prefix scans or single seeks.

No path column at all. Full paths get reconstructed by walking up the parent chain when needed, which in practice is
almost never — the frontend already knows which directory it's in.

### The result

| Component       | Before       | After      |
| --------------- | ------------ | ---------- |
| `entries` table | 1,938 MB     | ~400 MB    |
| Index           | 1,659 MB     | ~120 MB    |
| `dir_stats`     | 75 MB        | ~20 MB     |
| **Total**       | **3,672 MB** | **540 MB** |

Per-row, entries went from ~350 bytes (path + parent_path + name + metadata) to ~80 bytes (two integers + basename +
metadata). The index went from ~300 bytes (parent_path + full_path per row) to ~44 bytes (parent_id + name + rowid).

## Things that got better for free

**Renames and moves** went from delete-and-reinsert (the entire subtree, for directory moves) to a single UPDATE:

```sql
-- rename
UPDATE entries
SET name = 'new_name'
WHERE id = 42;
-- move
UPDATE entries
SET parent_id = 99
WHERE id = 42;
```

With path-keyed entries, renaming a directory meant deleting and reinserting every descendant because the path (the
primary key!) changed. With integer keys, the ID stays the same. dir_stats references don't break. Nothing downstream
needs updating.

**Enrichment** (populating directory sizes in file listings) went from N individual path lookups to two indexed queries:
resolve the parent directory once, fetch all child directory IDs and their stats in a batch. For a listing page of 50
directories, that's 2 queries instead of 50.

## Things that got trickier

**Path resolution** is the main new complexity. Every external input (watcher events, IPC commands, user navigation)
arrives as a filesystem path. The old schema could look it up directly. Now, resolving `/Users/foo/bar/baz.txt` means
walking from root: look up "Users" under root → look up "foo" under that → look up "bar" → look up "baz.txt". Four index
seeks instead of one.

In practice, this is fine. Each seek hits a cached B-tree page, and an LRU cache (50K entries, ~10 MB) catches 95%+ of
lookups during normal operation because watcher events cluster by directory and file listings resolve siblings with the
same parent.

**Subtree operations** lost the range-scan trick. With path keys, you could delete an entire directory tree with
`DELETE FROM entries WHERE path >= '/foo/' AND path < '/foo0'` — a single B-tree range scan. With integer keys, you need
a recursive CTE:

```sql
WITH RECURSIVE subtree(id) AS (SELECT id
                               FROM entries
                               WHERE id = ?
                               UNION ALL
                               SELECT e.id
                               FROM entries e
                                        JOIN subtree s ON e.parent_id = s.id)
DELETE
FROM entries
WHERE id IN (SELECT id FROM subtree);
```

I benchmarked this before committing to the migration (5.5M rows, 100K-entry subtree, 5 runs each):

|              | Path range scan | Recursive CTE |
| ------------ | --------------- | ------------- |
| 100K entries | 214 ms          | 180 ms        |
| 1K entries   | 1.6 ms          | 1.5 ms        |

The CTE was actually 16% faster. The bottleneck is WAL write I/O, not traversal. One less thing to worry about.

**Case sensitivity** needed explicit handling. macOS (APFS) is case-insensitive and normalization-insensitive: "Café" in
NFC and "Café" in NFD are the same file. The old path-based approach inherited this from the filesystem — you'd just ask
SQLite to match the path as-is and the OS handled the rest.

With integer keys, the `name` column needs to respect the platform's rules. I register a custom `platform_case` SQLite
collation at connection init: NFD normalization + case folding on macOS, binary comparison on Linux. The same algorithm
goes into the LRU cache's key wrapper. One gotcha: you can't open the DB with the sqlite3 CLI anymore without
registering the collation first, so add a comment for your future self.

## Implementation notes

The migration touched 11 Rust source files (~3,900 lines added, ~1,800 removed) and added a `PathResolver` module with
its own LRU cache. I kept the IPC boundary path-based — the frontend sends filesystem paths, the backend resolves to IDs
internally. No frontend changes at all.

The scan takes about 5 minutes for 5.5M entries on my machine (M1 Mac, APFS). Aggregating recursive directory sizes for
528K directories takes another 10 seconds — topological sort on (id, parent_id) pairs, bottom-up accumulation,
batch-write the results.

The old DB gets automatically deleted on upgrade. It's a cache, not user data, so the disposable-cache pattern works:
detect schema version mismatch, drop everything, rescan. Users see a brief "indexing..." overlay on first launch after
the update, then it's done.

132 tests cover the indexing module. The full Rust check suite (rustfmt, clippy, 836 tests) passes clean.

## Was it worth it?

3.85 GB → 540 MB, simpler renames, faster enrichment, cleaner aggregation code. The only real cost is the PathResolver
complexity, which is about 400 lines of well-tested cache code. I'd do it again.

The lesson, if there is one: measure your SQLite tables before assuming the schema is fine. `WITHOUT ROWID` with text
primary keys can silently blow up your secondary indexes to absurd sizes. Integer keys are boring, but boring is small.
