# index_query

Query the drive index SQLite database with the `platform_case` collation registered. The standard `sqlite3` CLI can't
query these DBs because the custom collation isn't available.

```sh
cargo run -p index-query -- <db_path> "<sql>"
```

DB paths:

- **Dev**: `~/Library/Application Support/com.veszelovszki.cmdr-dev/index-root.db`
- **Prod**: `~/Library/Application Support/com.veszelovszki.cmdr/index-root.db`

Examples:

```sh
# List top-level directories
cargo run -p index-query -- ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/index-root.db \
  "SELECT e.id, e.name, ds.recursive_size FROM entries e LEFT JOIN dir_stats ds ON ds.entry_id = e.id WHERE e.parent_id = 1 AND e.is_directory = 1 ORDER BY e.name"

# Check dir_stats coverage
cargo run -p index-query -- ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/index-root.db \
  "SELECT (SELECT count(*) FROM dir_stats) as has_stats, (SELECT count(*) FROM entries WHERE is_directory = 1) as total_dirs"

# Walk a specific path (resolve component by component)
cargo run -p index-query -- ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/index-root.db \
  "SELECT id, name FROM entries WHERE parent_id = 1 AND name = 'Users'"
```

Output is tab-separated with a header row (like `sqlite3` default mode).

## `index-size-probe`: what the index is full of, and what a `VACUUM` would reclaim

Structural questions about a multi-gigabyte index that no app query answers. All three subcommands are safe against a
LIVE index. Add `--json` to any of them so two runs diff, and `--scope <path>` to narrow to one subtree (an absolute
path; without it each subcommand covers the whole index). Build in release: `rows` over a 6M-row index takes ~25 s
released and minutes debug.

```sh
# Rows and bytes per table and index, dbstat page attribution, and a per-child breakdown of where they sit.
cargo run --release -p index-query --bin index-size-probe -- rows <index.db> [--scope <path>]

# Directory fan-out percentiles and the file-size distribution.
cargo run --release -p index-query --bin index-size-probe -- distribution <index.db> [--scope <path>]

# What a VACUUM would give back; with --scope, what that subtree's file rows cost on disk.
cargo run --release -p index-query --bin index-size-probe -- vacuum-probe <index.db> <scratch.db> [--scope <path>]
```

- **`rows` and `distribution` never write.** Each run pins one read snapshot, so the per-child rows still add up to the
  totals printed above them while the app is indexing.
- **`vacuum-probe` never writes to the source either.** It copies via `VACUUM INTO` from a read-only connection (a `cp`
  of a live WAL-mode DB is a torn read, and stopping the app to copy it costs a rescan) and does its deleting on the
  scratch copy, whose path it refuses if that sits inside an app data directory.
- **Descend by re-running.** `rows` sorts children by file rows, so `--scope` on the top row is the next question.
- **Two per-row numbers, and they disagree on purpose.** `dbstat_bytes_per_entry_row_estimate` attributes pages;
  `measured_bytes_per_file_row` (scoped `vacuum-probe`) deletes the rows and weighs the file. Trust the measured one.

## The importance binaries in the same crate

Both point at a REAL index READ-ONLY (WAL gives a consistent snapshot) and report numbers only, never a folder name.

```sh
# What a full pass costs: rows, store size, phase split, memory growth.
cargo run --release -p index-query --bin importance-measure -- <index.db> <home-or-mount-root> <local|listing-only> <out-importance.db>

# Whether the SCOPED incremental walk agrees with the full walk, and how much cheaper it is.
# Third argument: a number samples that many origins across the index, a comma-separated
# list compares exactly those (for a deliberately deep or wide subtree).
cargo run --release -p index-query --bin importance-diff -- <index.db> <home-or-mount-root> [count | paths]
```

`importance-diff` exits non-zero when the two walks disagree. Copy the DB to scratch first if the app is running; never
write to the live one. What it proves and why: `crates/cmdr-index/src/importance/scheduler/DETAILS.md` § The scoped
walk.
