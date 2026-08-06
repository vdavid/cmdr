//! `index-size-probe`: what a real drive index is full of, and what a `VACUUM`
//! would give back.
//!
//! Ad-hoc structural questions about a multi-gigabyte index that no query in the
//! app answers: how rows and pages split across tables and indexes, where in the
//! tree the rows and bytes actually sit, how directories fan out, how file sizes
//! are distributed, and how many bytes the freelist is sitting on. Every
//! subcommand emits JSON (`--json`), so two runs diff.
//!
//! ## Subcommands
//!
//! ```text
//! index-size-probe rows <index.db> [--scope <path>] [--json]
//! index-size-probe distribution <index.db> [--scope <path>] [--json]
//! index-size-probe vacuum-probe <index.db> <scratch-copy.db> [--scope <path>] [--json]
//! ```
//!
//! `rows` is READ-ONLY: on-disk file sizes, whole-index totals, a per-child
//! breakdown of where the rows and bytes sit, plus a `dbstat` page attribution that
//! estimates a per-row on-disk cost without mutating anything.
//!
//! `distribution` is READ-ONLY: directory fan-out and the file-size distribution,
//! both computed from the per-file rows.
//!
//! `vacuum-probe` is safe against the live index as well: it reads the source
//! read-only via `VACUUM INTO` and does all its damage to the scratch copy, whose
//! path it refuses if that sits inside an app data directory. Compacting the copy
//! measures what a `VACUUM` would reclaim from the live file. Given a `--scope` it
//! then deletes exactly that subtree's file rows and vacuums again, which is the
//! defensible per-row number on disk; `dbstat` is the estimate.
//!
//! `--scope <path>` narrows a subcommand to one subtree, written as an absolute
//! path (the index's volume root is stripped off it for a non-`/` volume). Without
//! it, every subcommand covers the whole index.

use std::collections::BTreeMap;
use std::path::Path;

use cmdr_index::store::{ROOT_ID, register_platform_case_collation, resolve_path};
use rusqlite::{Connection, OpenFlags};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let args = match parse_args(&argv[1..]) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("index-size-probe: {e}\n\n{}", usage(&argv[0]));
            std::process::exit(2);
        }
    };
    let scope = args.scope.as_deref();

    let result = match args.positional.as_slice() {
        [cmd, db] if cmd == "rows" => run_rows(Path::new(db), scope, args.json),
        [cmd, db] if cmd == "distribution" => run_distribution(Path::new(db), scope, args.json),
        [cmd, src, scratch] if cmd == "vacuum-probe" => {
            run_vacuum_probe(Path::new(src), Path::new(scratch), scope, args.json)
        }
        _ => {
            eprintln!("{}", usage(&argv[0]));
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("index-size-probe: {e}");
        std::process::exit(1);
    }
}

fn usage(program: &str) -> String {
    format!(
        "Usage:\n  {program} rows <index.db> [--scope <path>] [--json]\n  \
         {program} distribution <index.db> [--scope <path>] [--json]\n  \
         {program} vacuum-probe <index.db> <scratch-copy.db> [--scope <path>] [--json]"
    )
}

struct Args {
    positional: Vec<String>,
    scope: Option<String>,
    json: bool,
}

/// Split flags off the positionals. `--scope` takes a value, so "anything not
/// starting with `--` is positional" would swallow its path as a subcommand
/// argument.
fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        positional: Vec::new(),
        scope: None,
        json: false,
    };
    let mut rest = argv.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--json" => args.json = true,
            "--scope" => {
                let path = rest.next().ok_or_else(|| "--scope needs a path".to_string())?;
                args.scope = Some(path.clone());
            }
            other if other.starts_with("--scope=") => {
                args.scope = other.strip_prefix("--scope=").map(str::to_string);
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => args.positional.push(other.to_string()),
        }
    }
    Ok(args)
}

// ── Subcommand: rows ─────────────────────────────────────────────────

fn run_rows(db: &Path, scope: Option<&str>, json: bool) -> Result<(), String> {
    let conn = open_read_only(db)?;
    begin_snapshot(&conn)?;
    let volume_path = meta_value(&conn, "volume_path").unwrap_or_else(|| "/".to_string());
    let scope = Scope::resolve(&conn, &volume_path, scope)?;
    let files = file_sizes(db);
    let totals = whole_index_totals(&conn)?;
    let pages = dbstat_pages(&conn);
    // A whole-index scope would only restate the totals above, so it stays quiet.
    let scoped = if scope.is_whole_index() {
        None
    } else {
        Some(rows_under(&conn, &scope)?)
    };

    let mut out = Doc::new();
    out.str("subcommand", "rows");
    out.str("db_path", &db.display().to_string());
    out.str("volume_path", &volume_path);
    out.str("scope_path", &scope.path);
    files.emit(&mut out);
    totals.emit(&mut out);
    if let Some(under) = &scoped {
        under.emit(&mut out);
    }
    for (name, (bytes, count)) in &pages {
        out.num(&format!("dbstat_{name}_bytes"), *bytes);
        out.num(&format!("dbstat_{name}_pages"), *count);
    }
    // The estimate: what one entry row costs across the table and both indexes it
    // sits in. `dir_stats` is excluded because it is keyed per DIRECTORY, so it
    // doesn't scale with the file rows this attributes bytes to.
    let per_row_bytes: i64 = ["entries", "idx_parent_name_folded", "idx_inode"]
        .iter()
        .filter_map(|n| pages.get(*n).map(|(b, _)| *b))
        .sum();
    if totals.entries > 0 {
        let est = per_row_bytes as f64 / totals.entries as f64;
        out.real("dbstat_bytes_per_entry_row_estimate", est);
        if let Some(under) = &scoped {
            out.real("dbstat_scope_file_rows_bytes_estimate", est * under.files as f64);
        }
    }
    out.table("per_child", child_stats(&conn, &scope)?);
    out.finish(json);
    Ok(())
}

// ── Subcommand: distribution ─────────────────────────────────────────

/// Fan-out and file-size distribution.
///
/// Fan-out is what sizes any per-directory batching window: a re-list-on-event
/// design is cheap at the median and only bites on the handful of directories in
/// the tail. The size buckets say where the BYTES live against where the ROWS
/// live; on a real index those are completely different places, which is worth
/// knowing before optimizing for either.
fn run_distribution(db: &Path, scope: Option<&str>, json: bool) -> Result<(), String> {
    let conn = open_read_only(db)?;
    begin_snapshot(&conn)?;
    let volume_path = meta_value(&conn, "volume_path").unwrap_or_else(|| "/".to_string());
    let scope = Scope::resolve(&conn, &volume_path, scope)?;
    let row_set = scope.row_set();

    let mut out = Doc::new();
    out.str("subcommand", "distribution");
    out.str("db_path", &db.display().to_string());
    out.str("scope_path", &scope.path);
    fanout(&conn, &row_set)?.emit(&mut out);
    size_buckets(&conn, &row_set, &mut out)?;
    out.finish(json);
    Ok(())
}

struct Fanout {
    dirs_with_children: usize,
    mean: f64,
    p50: i64,
    p90: i64,
    p99: i64,
    max: i64,
    over_100: usize,
    over_1000: usize,
    over_5000: usize,
}

impl Fanout {
    fn emit(&self, out: &mut Doc) {
        out.num("fanout_dirs_with_children", self.dirs_with_children as i64);
        out.real("fanout_mean", self.mean);
        out.num("fanout_p50", self.p50);
        out.num("fanout_p90", self.p90);
        out.num("fanout_p99", self.p99);
        out.num("fanout_max", self.max);
        out.num("fanout_dirs_over_100", self.over_100 as i64);
        out.num("fanout_dirs_over_1000", self.over_1000 as i64);
        out.num("fanout_dirs_over_5000", self.over_5000 as i64);
    }
}

fn fanout(conn: &Connection, row_set: &str) -> Result<Fanout, String> {
    let sql = format!(
        "{row_set}
         SELECT count(*) FROM entries c JOIN sub s ON c.parent_id = s.id
          WHERE s.is_dir = 1
          GROUP BY c.parent_id"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("preparing fan-out: {e}"))?;
    let mut counts: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("querying fan-out: {e}"))?
        .flatten()
        .collect();
    counts.sort_unstable();
    let n = counts.len();
    if n == 0 {
        return Err("no directories with children in scope".to_string());
    }
    let pct = |p: f64| counts[((n as f64 * p) as usize).min(n - 1)];
    Ok(Fanout {
        dirs_with_children: n,
        mean: counts.iter().sum::<i64>() as f64 / n as f64,
        p50: pct(0.50),
        p90: pct(0.90),
        p99: pct(0.99),
        max: counts[n - 1],
        over_100: counts.iter().filter(|c| **c > 100).count(),
        over_1000: counts.iter().filter(|c| **c > 1000).count(),
        over_5000: counts.iter().filter(|c| **c > 5000).count(),
    })
}

/// Decade-wide size buckets. `null` is the hardlink-dedup marker and gets its own
/// bucket rather than being folded into zero: a deduped repeat is a real file that
/// deliberately carries no bytes, not a zero-byte file.
fn size_buckets(conn: &Connection, row_set: &str, out: &mut Doc) -> Result<(), String> {
    let sql = format!(
        "{row_set}
         SELECT CASE
                  WHEN e.logical_size IS NULL THEN '0_null_hardlink_deduped'
                  WHEN e.logical_size = 0 THEN '1_zero'
                  WHEN e.logical_size < 1024 THEN '2_under_1k'
                  WHEN e.logical_size < 10240 THEN '3_1k_10k'
                  WHEN e.logical_size < 102400 THEN '4_10k_100k'
                  WHEN e.logical_size < 1048576 THEN '5_100k_1m'
                  WHEN e.logical_size < 10485760 THEN '6_1m_10m'
                  WHEN e.logical_size < 104857600 THEN '7_10m_100m'
                  ELSE '8_over_100m'
                END AS bucket,
                count(*),
                coalesce(sum(e.logical_size), 0)
           FROM entries e JOIN sub s ON e.id = s.id
          WHERE s.is_dir = 0
          GROUP BY bucket ORDER BY bucket"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("preparing size buckets: {e}"))?;
    let rows: Vec<Record> = stmt
        .query_map([], |r| {
            Ok(vec![
                ("bucket".to_string(), format!("\"{}\"", r.get::<_, String>(0)?)),
                ("files".to_string(), r.get::<_, i64>(1)?.to_string()),
                ("logical_bytes".to_string(), r.get::<_, i64>(2)?.to_string()),
            ])
        })
        .map_err(|e| format!("querying size buckets: {e}"))?
        .flatten()
        .collect();
    out.table("size_buckets", rows);
    Ok(())
}

// ── Subcommand: vacuum-probe ─────────────────────────────────────────

fn run_vacuum_probe(source: &Path, scratch: &Path, scope: Option<&str>, json: bool) -> Result<(), String> {
    guard_not_live(scratch)?;
    for suffix in ["", "-wal", "-shm"] {
        let mut p = scratch.as_os_str().to_os_string();
        p.push(suffix);
        let _ = std::fs::remove_file(Path::new(&p));
    }

    // `VACUUM INTO` runs on a READ-ONLY connection and yields an already-compact,
    // transactionally consistent copy. That is what makes this probe safe against
    // the live index while the app is writing: `cp` of a 1 GB WAL-mode DB under a
    // running app is a torn read, and stopping the app to copy it costs a rescan.
    let src = open_read_only(source)?;
    let volume_path = meta_value(&src, "volume_path").unwrap_or_else(|| "/".to_string());
    let on_disk = file_sizes(source);
    src.execute("VACUUM INTO ?1", [scratch.to_string_lossy().as_ref()])
        .map_err(|e| format!("VACUUM INTO {}: {e}", scratch.display()))?;
    drop(src);
    let compacted = file_len(scratch);

    let conn = open_read_write(scratch)?;
    let scope = Scope::resolve(&conn, &volume_path, scope)?;
    let totals = whole_index_totals(&conn)?;

    let mut out = Doc::new();
    out.str("subcommand", "vacuum-probe");
    out.str("source_db", &source.display().to_string());
    out.str("scratch_db", &scratch.display().to_string());
    out.str("volume_path", &volume_path);
    out.str("scope_path", &scope.path);
    on_disk.emit(&mut out);
    totals.emit(&mut out);
    out.num("compacted_bytes", compacted);
    // Against the whole footprint, sidecars included: a real `VACUUM` checkpoints
    // the WAL into the main file, so that is what the live install gets back.
    out.num("vacuum_reclaimable_bytes", on_disk.total() - compacted);
    if totals.entries > 0 {
        out.real("compacted_bytes_per_row", compacted as f64 / totals.entries as f64);
    }

    // The scoped arm: what one class of rows is worth on disk, measured rather than
    // attributed. Unscoped there is nothing to delete, and the reclaim number above
    // is the whole answer.
    if !scope.is_whole_index() {
        let deleted = delete_file_rows(&conn, &scope)?;
        vacuum(&conn)?;
        let after = file_len(scratch);
        out.num("scope_file_rows_deleted", deleted);
        out.num("entries_after_delete", whole_index_totals(&conn)?.entries);
        out.num("bytes_after_delete", after);
        out.num("delete_reclaimed_bytes", compacted - after);
        if deleted > 0 {
            out.real(
                "measured_bytes_per_file_row",
                (compacted - after) as f64 / deleted as f64,
            );
        }
    }
    out.finish(json);
    Ok(())
}

/// Refuse to write the scratch copy anywhere an app instance owns its data. This
/// path gets overwritten, and with a `--scope` it gets rows deleted out of it;
/// aimed at a live index that would destroy a rescan costing tens of minutes.
fn guard_not_live(db: &Path) -> Result<(), String> {
    let s = db.display().to_string();
    if s.contains("com.veszelovszki.cmdr") || s.contains("Application Support") {
        return Err(format!(
            "refusing to write {s}: that path is inside an app data directory. Point the scratch \
             copy somewhere else; the source index is only ever read."
        ));
    }
    Ok(())
}

// ── Index facts ──────────────────────────────────────────────────────

struct Totals {
    entries: i64,
    dirs: i64,
    files: i64,
    dir_stats: i64,
    null_logical_size_files: i64,
}

impl Totals {
    fn emit(&self, out: &mut Doc) {
        out.num("entries_total", self.entries);
        out.num("entries_dirs", self.dirs);
        out.num("entries_files", self.files);
        out.num("dir_stats_rows", self.dir_stats);
        out.num("files_with_null_logical_size", self.null_logical_size_files);
    }
}

fn whole_index_totals(conn: &Connection) -> Result<Totals, String> {
    let row = conn
        .query_row(
            "SELECT count(*),
                    sum(is_directory = 1),
                    sum(is_directory = 0),
                    sum(is_directory = 0 AND logical_size IS NULL)
             FROM entries",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                ))
            },
        )
        .map_err(|e| format!("counting entries: {e}"))?;
    let dir_stats = conn
        .query_row("SELECT count(*) FROM dir_stats", [], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("counting dir_stats: {e}"))?;
    Ok(Totals {
        entries: row.0,
        dirs: row.1,
        files: row.2,
        dir_stats,
        null_logical_size_files: row.3,
    })
}

/// On-disk bytes for the DB and its sidecars, each reported on its own. An
/// uncheckpointed WAL is real disk use but is slack rather than index content, so
/// a reader comparing two indexes wants to see the split and not just the sum.
struct Files {
    db: i64,
    wal: i64,
    shm: i64,
}

impl Files {
    fn total(&self) -> i64 {
        self.db + self.wal + self.shm
    }

    fn emit(&self, out: &mut Doc) {
        out.num("db_bytes", self.db);
        out.num("wal_bytes", self.wal);
        out.num("shm_bytes", self.shm);
        out.num("on_disk_bytes_total", self.total());
    }
}

fn file_sizes(db: &Path) -> Files {
    let side = |suffix: &str| {
        let mut s = db.as_os_str().to_os_string();
        s.push(suffix);
        file_len(Path::new(&s))
    };
    Files {
        db: file_len(db),
        wal: side("-wal"),
        shm: side("-shm"),
    }
}

fn file_len(p: &Path) -> i64 {
    std::fs::metadata(p).map(|m| m.len() as i64).unwrap_or(0)
}

fn meta_value(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

/// Per-object page use. Empty when the bundled SQLite lacks `SQLITE_ENABLE_DBSTAT_VTAB`,
/// which is a missing estimate rather than a failure: `vacuum-probe` is the measurement.
fn dbstat_pages(conn: &Connection) -> BTreeMap<String, (i64, i64)> {
    let mut map = BTreeMap::new();
    let Ok(mut stmt) = conn.prepare("SELECT name, sum(pgsize), count(*) FROM dbstat GROUP BY name") else {
        return map;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
    }) else {
        return map;
    };
    for row in rows.flatten() {
        map.insert(row.0, (row.1, row.2));
    }
    map
}

// ── Scope ────────────────────────────────────────────────────────────

/// The subtree a subcommand works on. [`ROOT_ID`] means the whole index, which is
/// both the default and what `--scope` on the volume root resolves to.
struct Scope {
    id: i64,
    path: String,
}

impl Scope {
    /// Resolve `--scope` to an indexed entry. The argument is an absolute
    /// filesystem path; an index of a mounted volume is rooted at its
    /// `volume_path`, so that prefix comes off before the walk down the tree.
    fn resolve(conn: &Connection, volume_path: &str, scope: Option<&str>) -> Result<Self, String> {
        let Some(scope) = scope else {
            return Ok(Scope {
                id: ROOT_ID,
                path: volume_path.to_string(),
            });
        };
        let trimmed = scope.trim_end_matches('/');
        let relative = trimmed
            .strip_prefix(volume_path.trim_end_matches('/'))
            .unwrap_or(trimmed);
        let path = if trimmed.is_empty() {
            volume_path.to_string()
        } else {
            trimmed.to_string()
        };
        match resolve_path(conn, relative) {
            Ok(Some(id)) => Ok(Scope { id, path }),
            Ok(None) => Err(format!(
                "{path} is not indexed here (this index is rooted at {volume_path})"
            )),
            Err(e) => Err(format!("resolving {path}: {e}")),
        }
    }

    fn is_whole_index(&self) -> bool {
        self.id == ROOT_ID
    }

    /// A `WITH` prefix binding `sub(id, is_dir)` to the rows in scope, for a query
    /// that goes on to join `entries` back on `sub.id`.
    ///
    /// Whole-index is a plain scan rather than a descent from the root sentinel:
    /// cheaper, and it also catches rows whose parent chain is broken, which is
    /// drift a diagnostic wants to see rather than silently walk past.
    fn row_set(&self) -> String {
        if self.is_whole_index() {
            return "WITH sub(id, is_dir) AS (SELECT id, is_directory FROM entries)".to_string();
        }
        format!(
            "WITH RECURSIVE sub(id, is_dir) AS (
                 SELECT id, is_directory FROM entries WHERE id = {}
                 UNION ALL
                 SELECT c.id, c.is_directory FROM entries c JOIN sub s ON c.parent_id = s.id
                   WHERE s.is_dir = 1
             )",
            self.id
        )
    }
}

// ── Rows in scope ────────────────────────────────────────────────────

struct ScopeRows {
    files: i64,
    dirs: i64,
    logical_bytes: i64,
    physical_bytes: i64,
    null_logical_files: i64,
}

impl ScopeRows {
    fn emit(&self, out: &mut Doc) {
        out.num("scope_file_rows", self.files);
        out.num("scope_dir_rows", self.dirs);
        out.num("scope_logical_bytes", self.logical_bytes);
        out.num("scope_physical_bytes", self.physical_bytes);
        out.num("scope_files_with_null_logical_size", self.null_logical_files);
    }
}

/// One descent over the scope. A scoped descent seeds on the scope root itself,
/// which is a directory the subtree's own count shouldn't claim, so it comes back
/// off the directory total.
fn rows_under(conn: &Connection, scope: &Scope) -> Result<ScopeRows, String> {
    let sql = format!(
        "{}
         SELECT sum(s.is_dir = 0),
                sum(s.is_dir = 1),
                sum(CASE WHEN s.is_dir = 0 THEN coalesce(e.logical_size, 0) ELSE 0 END),
                sum(CASE WHEN s.is_dir = 0 THEN coalesce(e.physical_size, 0) ELSE 0 END),
                sum(s.is_dir = 0 AND e.logical_size IS NULL)
           FROM sub s JOIN entries e ON e.id = s.id",
        scope.row_set()
    );
    let row = conn
        .query_row(&sql, [], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                r.get::<_, Option<i64>>(4)?.unwrap_or(0),
            ))
        })
        .map_err(|e| format!("walking {}: {e}", scope.path))?;
    let seed_dirs = i64::from(!scope.is_whole_index());
    Ok(ScopeRows {
        files: row.0,
        dirs: row.1 - seed_dirs,
        logical_bytes: row.2,
        physical_bytes: row.3,
        null_logical_files: row.4,
    })
}

/// Rows and bytes per immediate child directory of the scope, biggest first. A
/// single total hides whether the weight is spread over the tree or concentrated
/// in three directories, and that difference is what "what is this index full of"
/// is really asking. Run it again with `--scope` on the biggest child to descend.
fn child_stats(conn: &Connection, scope: &Scope) -> Result<Vec<Record>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name FROM entries WHERE parent_id = ?1 AND is_directory = 1")
        .map_err(|e| format!("preparing child query: {e}"))?;
    let children: Vec<(i64, String)> = stmt
        .query_map([scope.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("querying children: {e}"))?
        .flatten()
        .collect();

    // A failed descent propagates rather than dropping the child quietly: it would
    // be a SQL error, and a table whose rows silently stop adding up to the totals
    // above it is worse than no table. Index drift shows up as zero rows, not here.
    let mut stats: Vec<(String, ScopeRows)> = Vec::with_capacity(children.len());
    for (id, name) in children {
        let child = Scope {
            id,
            path: format!("{}/{name}", scope.path.trim_end_matches('/')),
        };
        let rows = rows_under(conn, &child)?;
        stats.push((child.path, rows));
    }
    stats.sort_by_key(|(_, rows)| -rows.files);
    Ok(stats
        .into_iter()
        .map(|(path, rows)| {
            vec![
                ("path".to_string(), format!("\"{}\"", escape(&path))),
                ("file_rows".to_string(), rows.files.to_string()),
                ("dir_rows".to_string(), rows.dirs.to_string()),
                ("logical_bytes".to_string(), rows.logical_bytes.to_string()),
            ]
        })
        .collect())
}

/// Delete exactly the measured slice: file rows at any depth in scope. Directory
/// rows and their `dir_stats` stay, so what the VACUUM reclaims is attributable to
/// file rows alone.
fn delete_file_rows(conn: &Connection, scope: &Scope) -> Result<i64, String> {
    let sql = format!(
        "DELETE FROM entries WHERE id IN ({} SELECT id FROM sub WHERE is_dir = 0)",
        scope.row_set()
    );
    conn.execute(&sql, [])
        .map(|n| n as i64)
        .map_err(|e| format!("deleting file rows under {}: {e}", scope.path))
}

// ── SQLite plumbing ──────────────────────────────────────────────────

fn open_read_only(db: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("opening {} read-only: {e}", db.display()))?;
    register_platform_case_collation(&conn).map_err(|e| format!("registering collation: {e}"))?;
    Ok(conn)
}

/// Pin one snapshot for a whole read-only run. The app keeps writing to the live
/// index while the probe reads it, and a run fires one query per child directory,
/// so without this the per-child rows wouldn't add up to the totals printed above
/// them. Costs the app nothing but a deferred WAL checkpoint: readers don't block
/// the writer. Not for the `vacuum-probe` source, since `VACUUM INTO` refuses to
/// run inside a transaction.
fn begin_snapshot(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("BEGIN")
        .map_err(|e| format!("starting read snapshot: {e}"))
}

fn open_read_write(db: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db).map_err(|e| format!("opening {}: {e}", db.display()))?;
    register_platform_case_collation(&conn).map_err(|e| format!("registering collation: {e}"))?;
    Ok(conn)
}

fn vacuum(conn: &Connection) -> Result<(), String> {
    // WAL mode keeps a sidecar the VACUUM wouldn't reclaim, so the file size we
    // compare would include it. DELETE mode leaves one file to measure.
    conn.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))
        .map_err(|e| format!("switching to rollback journal: {e}"))?;
    conn.execute_batch("VACUUM").map_err(|e| format!("vacuuming: {e}"))
}

// ── Output ───────────────────────────────────────────────────────────

/// One record of a `Doc` table: already-rendered `key: value` pairs, where the
/// value carries its own JSON quoting.
type Record = Vec<(String, String)>;

/// A flat ordered document, printed as JSON or as aligned `key: value` text.
/// Flat on purpose: the before and after runs get diffed key by key.
struct Doc {
    fields: Vec<(String, String)>,
    /// Lists of records, each already rendered as `key: value` pairs.
    tables: Vec<(String, Vec<Record>)>,
}

impl Doc {
    fn new() -> Self {
        Doc {
            fields: Vec::new(),
            tables: Vec::new(),
        }
    }
    fn str(&mut self, k: &str, v: &str) {
        self.fields.push((k.to_string(), format!("\"{}\"", escape(v))));
    }
    fn num(&mut self, k: &str, v: i64) {
        self.fields.push((k.to_string(), v.to_string()));
    }
    fn real(&mut self, k: &str, v: f64) {
        self.fields.push((k.to_string(), format!("{v:.2}")));
    }
    fn table(&mut self, k: &str, rows: Vec<Record>) {
        self.tables.push((k.to_string(), rows));
    }

    fn finish(self, json: bool) {
        if json {
            println!("{{");
            let mut first = true;
            for (k, v) in &self.fields {
                if !first {
                    println!(",");
                }
                print!("  \"{k}\": {v}");
                first = false;
            }
            for (k, rows) in &self.tables {
                if !first {
                    println!(",");
                }
                let body = rows
                    .iter()
                    .map(|r| {
                        let pairs = r
                            .iter()
                            .map(|(rk, rv)| format!("\"{rk}\": {rv}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("    {{ {pairs} }}")
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                print!("  \"{k}\": [\n{body}\n  ]");
                first = false;
            }
            println!("\n}}");
            return;
        }
        let width = self.fields.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (k, v) in &self.fields {
            println!("{k:width$}  {}", v.trim_matches('"'));
        }
        for (k, rows) in &self.tables {
            println!("{k} ({}):", rows.len());
            for r in rows {
                let cells = r
                    .iter()
                    .map(|(rk, rv)| format!("{rk}={}", rv.trim_matches('"')))
                    .collect::<Vec<_>>()
                    .join("  ");
                println!("  {cells}");
            }
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
