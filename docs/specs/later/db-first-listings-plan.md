# DB-first directory listings

Serve a directory listing from the volume's SQLite index instead of `readdir` + `stat`, so the first paint is a query
rather than a filesystem walk. Background verification on each navigation keeps the index honest.

**Status, re-derived from the tree 2026-08-27**: the per-navigation verifier is BUILT and has been in production for
months. The DB-first listing path itself is NOT built and never was; nothing in `file_system/listing/` reads the index
for entries. The original 2026-03-03 design below has been rewritten against the current schema, because the index it
was written for no longer exists in that shape.

Read `crates/cmdr-index/src/indexing/CLAUDE.md` and `crates/cmdr-index/src/indexing/reconcile/CLAUDE.md` before planning
any of this.

## Why it might still be worth doing

`readdir` + `stat` costs a walk per navigation; an indexed lookup on the same directory is one tree walk to an entry id
plus one child query. For rapid keyboard navigation through large directories that difference is the whole feel of the
app.

⚠️ **The motivating measurement is gone and has to be re-taken.** The original "2–50 ms per directory" number was
measured in March 2026 and is unanchored: no build mode recorded, no directory sizes, and the listing path has been
rewritten since. `docs/notes/listing-wedge-impact-2026-08-22.md` is the reason to be careful with any older listing
number: it measured release builds against debug builds on the same probe and found release roughly two orders of
magnitude faster. ❌ Don't schedule this work off the March figure. Measure a release build first, at several directory
sizes, and decide whether there is a user-visible win left to win.

## What already shipped

- **The per-navigation verifier**, and it went far past the original sketch. `crates/cmdr-index/src/indexing/reconcile/`
  now holds three resync mechanisms (the event-triggered reconciler, the full rescan-in-place, and `verifier.rs`'s
  per-navigation `read_dir` diff), each with hard-won guardrails its `CLAUDE.md` states as must-knows. Milestone 1 of
  the original plan is done, and the "build confidence in the verifier before wiring DB-first" gate it existed to
  satisfy has been satisfied.
- **The logical/physical size split.** `EntryRow` carries both `logical_size` and `physical_size`, and `DirStatsById`
  carries `recursive_logical_size` and `recursive_physical_size` separately. The ambiguity that would have made a
  DB-first listing show allocated blocks where the user expects file bytes is gone.
- **Honest recursive sizes.** `listed_epoch` per directory and `min_subtree_epoch` rolled up through `dir_stats` answer
  "is this total exact or a lower bound", which the frontend already renders. A DB-first path inherits this rather than
  re-deriving it.

## What the original design got wrong about today's index

Every one of these invalidates a specific piece of the 2026-03 plan, so they are the first thing to re-plan around.

- **The `entries` table is id-keyed, not path-keyed.** There is no `parent_path` column and no `path` column, so
  `SELECT * FROM entries WHERE parent_path = ?` cannot be written. The real shape is
  `store::resolve_path(conn, path) -> Option<i64>` (one tree walk, then) `IndexStore::list_children(parent_id)`.
  `ScannedEntry` is gone; `EntryRow` (`indexing/store/mod.rs`) replaced it and carries `id`, `parent_id`, `name`,
  `is_directory`, `is_symlink`, `logical_size`, `physical_size`, `modified_at`, and `inode`.
- **The index is per-volume, not one boot-disk database.** Reads go through `get_read_pool_for(volume_id)`, paths are
  mapped into the volume's own index path space by `routing::index_read_path`, and a volume with no registered index
  answers `None`. Any DB-first check is a per-volume question before it is a per-directory one.
- **`enrich_entries_with_index_on_volume` already does most of the plumbing.** It resolves the parent path to an id
  once, lists the child directory `(id, name)` pairs, and batch-fetches `dir_stats` by integer id: two indexed queries
  for a whole listing. A DB-first read is that same resolve-then-list, taken one step further to build entries instead
  of enriching them. ❌ Don't build a second path-resolution route beside it.
- **`FileEntry` grew fields the index can't answer.** Beyond the March list it now carries `is_archive`, `inode`,
  `physical_size`, `tags`, `recursive_has_symlinks`, `recursive_size_complete`, and `recursive_size_stale`. Most are
  derivable or already enriched: `icon_id` and `is_archive` are pure functions of the name and the directory flag,
  `inode` and both sizes are columns, `tags` are deferred on every path already (`file_system/listing/DETAILS.md` §
  "Finder tags"), and the recursive fields come from the same `dir_stats` batch enrichment does today.
- **❗ `created` is now a sort column, and the index does not store it.** `SortColumn` is
  `'name' | 'extension' | 'size' | 'modified' | 'created'`. The original plan's decision that extended metadata "is not
  currently displayed, so defaults are invisible" is no longer true: a DB-first listing with `created_at: None` on every
  entry would silently produce a wrong sort for anyone who chose that column. This is now a blocker for the DB-first
  path, not a later refinement, and it has to be solved before the switch and not after: either backfill `created_at`
  into the index, or fall back to `readdir` for a listing sorted by `created`. `permissions`, `owner`, `group`,
  `added_at`, and `opened_at` remain undisplayed and can still take defaults.
- **`verify_affected_dirs` moved.** It lives in `indexing/watch/event_loop/verification.rs`, not in an
  `indexing/mod.rs`.

## What is still genuinely open

**The DB-first read path.** For an indexed directory on a volume with a registered index, build the listing from
`list_children(parent_id)` instead of the backend's `list_directory`, then enrich, sort, cache, and return exactly as
today. Fall back to `readdir` per-directory whenever the answer isn't available.

**The readiness predicate**, which the original plan sketched as `entry_exists(path)` and which needs re-deriving. Two
questions have to be separated, and the index now has a better answer for both than it did in March:

- _Has this volume finished a first full scan?_ `IndexStatus::scan_completed_at`, as before. Before it is set, a
  directory can legitimately hold only some of its children, and a DB-first paint would jump when verification corrected
  it.
- _Has this specific directory ever been listed?_ The `listed_epoch` column on `entries` answers exactly this and
  distinguishes "genuinely empty" from "never walked" without the entry-exists proxy. `0` means never listed. ⚠️ The
  verifier already keys off this pairing and its `CLAUDE.md` states the rule in both halves; keep the predicates in
  lock-step rather than inventing a third one.

**The cache-update path when verification finds a diff.** Compare against the CURRENT cache rather than the original DB
snapshot, so a change the per-directory watcher already applied noops instead of being processed twice. That reasoning
still holds and is the one design decision from March worth carrying forward unchanged.

**Streaming.** `list_directory_start_streaming` needs the same fork: if the DB answers, populate the cache and emit
`listing-complete` without progress events, since a sub-millisecond read has nothing to report.

**Then measure.** Benchmark against `readdir` at 100, 1 000, 10 000, and 100 000 entries on RELEASE builds, and check
for lock contention between DB-first reads and the writer during a concurrent scan.

## The watcher-dedup follow-up, and why it is now less obvious

The original follow-up was: once DB-first is active for a volume, stop starting a per-directory `notify` watcher for
directories on it, since the volume-level watcher covers them and 300 ms of FSEvents batching is imperceptible.

That is no longer a clean trade. The per-directory watcher is what carries Finder tags forward across a re-stat
(`caching::carry_forward_tags`) and what drives the in-place listing diffs several features now read. Anyone picking
this up has to work out what else moved onto that watcher since March before removing it.
