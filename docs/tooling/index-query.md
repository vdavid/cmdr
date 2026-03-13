# index_query

Query the drive index SQLite database with the `platform_case` collation registered. The standard `sqlite3` CLI can't query these DBs because the custom collation isn't available.

```sh
cargo run --bin index_query -- <db_path> "<sql>"
```

DB paths:
- **Dev**: `~/Library/Application Support/com.veszelovszki.cmdr-dev/index-root.db`
- **Prod**: `~/Library/Application Support/com.veszelovszki.cmdr/index-root.db`

Examples:

```sh
# List top-level directories
cargo run --bin index_query -- ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/index-root.db \
  "SELECT e.id, e.name, ds.recursive_size FROM entries e LEFT JOIN dir_stats ds ON ds.entry_id = e.id WHERE e.parent_id = 1 AND e.is_directory = 1 ORDER BY e.name"

# Check dir_stats coverage
cargo run --bin index_query -- ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/index-root.db \
  "SELECT (SELECT count(*) FROM dir_stats) as has_stats, (SELECT count(*) FROM entries WHERE is_directory = 1) as total_dirs"

# Walk a specific path (resolve component by component)
cargo run --bin index_query -- ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/index-root.db \
  "SELECT id, name FROM entries WHERE parent_id = 1 AND name = 'Users'"
```

Output is tab-separated with a header row (like `sqlite3` default mode).
