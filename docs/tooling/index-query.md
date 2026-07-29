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
write to the live one. What it proves and why: `apps/desktop/src-tauri/src/importance/scheduler/DETAILS.md` § The scoped
walk.
