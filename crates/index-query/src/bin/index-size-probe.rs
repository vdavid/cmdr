//! `index-size-probe`: where a real drive index's rows and bytes actually go,
//! and how many bytes a given slice of rows would give back.
//!
//! Ad-hoc structural questions about a multi-gigabyte index that no query in the
//! app answers: how rows and pages split across tables and indexes, how a subtree's
//! children fan out, how file sizes are distributed, and what deleting a class of
//! rows reclaims once the freelist is vacuumed away. Every subcommand emits JSON
//! (`--json`), so two runs diff.
//!
//! The row slice it works on is **the file rows under `CACHEDIR.TAG`-marked
//! subtrees**: on a developer machine that's the build-output-and-cache share of
//! the index, which is the interesting slice for "what is this index full of".
//!
//! ## Subcommands
//!
//! ```text
//! index-size-probe rows <index.db> [--json]
//! index-size-probe distribution <index.db> [--json]
//! index-size-probe vacuum-probe <index.db> <scratch-copy.db> [--json]
//! ```
//!
//! `rows` is READ-ONLY and safe against the live index: whole-index totals, the
//! marked subtree set, the rows and bytes under it (in total and per root), plus a
//! `dbstat` page attribution that estimates a per-row on-disk cost without
//! mutating anything.
//!
//! `distribution` reports fan-out and file-size distribution inside the marked set,
//! both computed from the per-file rows.
//!
//! `vacuum-probe` is also safe against the live index: it reads the source
//! read-only via `VACUUM INTO` and does all its damage to the scratch copy, whose
//! path it refuses if that sits inside an app data directory. On the copy it
//! deletes exactly the marked set's file rows, VACUUMs, and reports the difference.
//! That is the defensible per-row number on disk; `dbstat` is the estimate.
//!
//! Marked roots are enumerated FROM THE INDEX (every `CACHEDIR.TAG` row), then the
//! signature line is verified on disk. Enumerating from the index answers "how many
//! INDEXED rows are these", where a filesystem walk would also count tags inside
//! excluded subtrees (`node_modules`, `.git`, ...) that cost no rows. Nested roots
//! are collapsed so a `target/` inside a marked tree isn't counted twice.
//!
//! ⚠️ **The signature check here is first-line equality, which is stricter than the
//! standard.** `CACHEDIR.TAG` specifies the first 43 BYTES; 6 of 31 real tag files
//! on the author's machine repeat the signature with no newline between, so a
//! prefix test is the correct one. Kept as-is because a probe that under-counts
//! visibly is safer than one that guesses, but don't copy this predicate into
//! product code. See `docs/notes/size-only-subtrees-rejected-2026-08-06.md`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use cmdr_index::store::{normalize_for_comparison, register_platform_case_collation};
use rusqlite::{Connection, OpenFlags};

/// The `CACHEDIR.TAG` first line that makes a directory a declared cache.
/// <https://bford.info/cachedir/>
const CACHEDIR_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

/// Root entry sentinel: every top-level entry's `parent_id`. Mirrors
/// `cmdr_index::store::ROOT_ID`, which isn't re-exported for tools.
const ROOT_ID: i64 = 1;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json = args.iter().any(|a| a == "--json");
    let positional: Vec<&String> = args[1..].iter().filter(|a| !a.starts_with("--")).collect();

    let result = match positional.as_slice() {
        [cmd, db] if *cmd == "rows" => run_rows(Path::new(db.as_str()), json),
        [cmd, db] if *cmd == "distribution" => run_distribution(Path::new(db.as_str()), json),
        [cmd, src, scratch] if *cmd == "vacuum-probe" => {
            run_vacuum_probe(Path::new(src.as_str()), Path::new(scratch.as_str()), json)
        }
        _ => {
            eprintln!(
                "Usage:\n  {0} rows <index.db> [--json]\n  \
                 {0} vacuum-probe <index.db> <scratch-copy.db> [--json]",
                args[0]
            );
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("index-size-probe: {e}");
        std::process::exit(1);
    }
}

// ── Subcommand: rows ─────────────────────────────────────────────────

fn run_rows(db: &Path, json: bool) -> Result<(), String> {
    let conn = open_read_only(db)?;
    let files = file_sizes(db);
    let totals = whole_index_totals(&conn)?;
    let volume_path = meta_value(&conn, "volume_path").unwrap_or_else(|| "/".to_string());
    let roots = marked_roots(&conn, &volume_path)?;
    let under = rows_under(&conn, &roots)?;
    let pages = dbstat_pages(&conn);

    let mut out = Doc::new();
    out.str("subcommand", "rows");
    out.str("db_path", &db.display().to_string());
    out.str("volume_path", &volume_path);
    files.emit(&mut out);
    totals.emit(&mut out);
    roots_summary(&roots).emit(&mut out);
    under.emit(&mut out);
    for (name, (bytes, count)) in &pages {
        out.num(&format!("dbstat_{name}_bytes"), *bytes);
        out.num(&format!("dbstat_{name}_pages"), *count);
    }
    // The estimate: what one entry row costs across the table and both indexes it
    // sits in. `dir_stats` is excluded because it is keyed per DIRECTORY, and the
    // slice measured here is file rows, whose directories stay.
    let per_row_bytes: i64 = ["entries", "idx_parent_name_folded", "idx_inode"]
        .iter()
        .filter_map(|n| pages.get(*n).map(|(b, _)| *b))
        .sum();
    if totals.entries > 0 {
        let est = per_row_bytes as f64 / totals.entries as f64;
        out.real("dbstat_bytes_per_entry_row_estimate", est);
        out.real("dbstat_marked_file_rows_bytes_estimate", est * under.files as f64);
    }
    out.table(
        "per_root",
        per_root_stats(&conn, &roots)
            .into_iter()
            .map(|(path, u)| {
                vec![
                    ("path".to_string(), format!("\"{}\"", escape(&path))),
                    ("file_rows".to_string(), u.files.to_string()),
                    ("dir_rows".to_string(), u.dirs.to_string()),
                    ("logical_bytes".to_string(), u.logical_bytes.to_string()),
                ]
            })
            .collect(),
    );
    out.finish(json);
    Ok(())
}

// ── Subcommand: distribution ─────────────────────────────────────────

/// Fan-out and file-size distribution INSIDE the marked set.
///
/// Fan-out is what sizes any per-directory batching window: a re-list-on-event
/// design is cheap at the median and only bites on the handful of directories in
/// the tail. The size buckets say where the BYTES live against where the ROWS
/// live; on a real index those are completely different places, which is worth
/// knowing before optimizing for either.
fn run_distribution(db: &Path, json: bool) -> Result<(), String> {
    let conn = open_read_only(db)?;
    let volume_path = meta_value(&conn, "volume_path").unwrap_or_else(|| "/".to_string());
    let roots = marked_roots(&conn, &volume_path)?;
    if roots.kept.is_empty() {
        return Err("no marked roots; nothing to distribute".to_string());
    }
    let ids = roots
        .kept
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let descent = format!(
        "WITH RECURSIVE sub(id, is_dir) AS (
             SELECT id, is_directory FROM entries WHERE id IN ({ids})
             UNION ALL
             SELECT c.id, c.is_directory FROM entries c JOIN sub s ON c.parent_id = s.id WHERE s.is_dir = 1
         )"
    );

    let mut out = Doc::new();
    out.str("subcommand", "distribution");
    out.str("db_path", &db.display().to_string());
    roots_summary(&roots).emit(&mut out);
    fanout(&conn, &descent)?.emit(&mut out);
    size_buckets(&conn, &descent, &mut out)?;
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

fn fanout(conn: &Connection, descent: &str) -> Result<Fanout, String> {
    let sql = format!(
        "{descent}
         SELECT count(*) FROM entries c
           WHERE c.parent_id IN (SELECT id FROM sub WHERE is_dir = 1)
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
        return Err("no directories with children under the marked set".to_string());
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
fn size_buckets(conn: &Connection, descent: &str, out: &mut Doc) -> Result<(), String> {
    let sql = format!(
        "{descent}
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

fn run_vacuum_probe(source: &Path, scratch: &Path, json: bool) -> Result<(), String> {
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
    src.execute("VACUUM INTO ?1", [scratch.to_string_lossy().as_ref()])
        .map_err(|e| format!("VACUUM INTO {}: {e}", scratch.display()))?;
    drop(src);
    let size_full = file_len(scratch);

    let conn = open_read_write(scratch)?;
    let roots = marked_roots(&conn, &volume_path)?;
    let totals_before = whole_index_totals(&conn)?;
    let deleted = delete_file_rows_under(&conn, &roots)?;
    vacuum(&conn)?;
    let size_reduced = file_len(scratch);
    let totals_after = whole_index_totals(&conn)?;

    let mut out = Doc::new();
    out.str("subcommand", "vacuum-probe");
    out.str("source_db", &source.display().to_string());
    out.str("scratch_db", &scratch.display().to_string());
    out.str("volume_path", &volume_path);
    roots_summary(&roots).emit(&mut out);
    out.num("entries_before", totals_before.entries);
    out.num("entries_after", totals_after.entries);
    out.num("file_rows_deleted", deleted);
    out.num("vacuumed_bytes_before", size_full);
    out.num("vacuumed_bytes_after", size_reduced);
    out.num("vacuumed_bytes_reclaimed", size_full - size_reduced);
    if deleted > 0 {
        out.real(
            "measured_bytes_per_file_row",
            (size_full - size_reduced) as f64 / deleted as f64,
        );
    }
    if totals_before.entries > 0 {
        out.real(
            "vacuumed_bytes_per_row_whole_index",
            size_full as f64 / totals_before.entries as f64,
        );
    }
    out.finish(json);
    Ok(())
}

/// Refuse to write the scratch copy anywhere an app instance owns its data. The
/// probe deletes rows and VACUUMs; aimed at a live index it would destroy a rescan
/// that costs tens of minutes to redo.
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

/// On-disk bytes for the DB and its sidecars. The WAL is reported separately and
/// NOT folded in: an uncheckpointed WAL is real disk use but is slack, not index
/// content, and folding the two would make the before/after comparison depend on
/// when the app last checkpointed.
struct Files {
    db: i64,
    wal: i64,
    shm: i64,
}

impl Files {
    fn emit(&self, out: &mut Doc) {
        out.num("db_bytes", self.db);
        out.num("wal_bytes", self.wal);
        out.num("shm_bytes", self.shm);
        out.num("on_disk_bytes_total", self.db + self.wal + self.shm);
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

// ── Marked roots ─────────────────────────────────────────────────────

struct Root {
    id: i64,
    path: String,
}

/// Why a `CACHEDIR.TAG` row in the index did NOT become a marked root. Split out
/// rather than lumped, because the three causes mean different things: a gone tag
/// is index staleness, a bad signature is a tool writing a non-standard file, and
/// an unresolvable path is real index drift.
#[derive(Default)]
struct Rejected {
    tag_gone_on_disk: usize,
    signature_mismatch: usize,
    path_unresolvable: usize,
}

struct RootsSummary {
    verified: usize,
    collapsed_nested: usize,
    tags_in_index: usize,
    rejected: Rejected,
    paths: Vec<String>,
}

impl RootsSummary {
    fn emit(&self, out: &mut Doc) {
        out.num("cachedir_tag_rows_in_index", self.tags_in_index as i64);
        out.num("marked_roots_verified", self.verified as i64);
        out.num("marked_roots_collapsed_nested", self.collapsed_nested as i64);
        out.num("rejected_tag_gone_on_disk", self.rejected.tag_gone_on_disk as i64);
        out.num("rejected_signature_mismatch", self.rejected.signature_mismatch as i64);
        out.num("rejected_path_unresolvable", self.rejected.path_unresolvable as i64);
        out.list("marked_root_paths", &self.paths);
    }
}

fn roots_summary(roots: &Roots) -> RootsSummary {
    RootsSummary {
        verified: roots.kept.len(),
        collapsed_nested: roots.collapsed,
        tags_in_index: roots.tags_in_index,
        rejected: Rejected {
            tag_gone_on_disk: roots.rejected.tag_gone_on_disk,
            signature_mismatch: roots.rejected.signature_mismatch,
            path_unresolvable: roots.rejected.path_unresolvable,
        },
        paths: roots.kept.iter().map(|r| r.path.clone()).collect(),
    }
}

struct Roots {
    kept: Vec<Root>,
    collapsed: usize,
    tags_in_index: usize,
    rejected: Rejected,
}

fn marked_roots(conn: &Connection, volume_path: &str) -> Result<Roots, String> {
    let folded = normalize_for_comparison("CACHEDIR.TAG");
    let mut stmt = conn
        .prepare("SELECT parent_id FROM entries WHERE name_folded = ?1 AND is_directory = 0")
        .map_err(|e| format!("preparing tag query: {e}"))?;
    let parent_ids: Vec<i64> = stmt
        .query_map([&folded], |r| r.get::<_, i64>(0))
        .map_err(|e| format!("querying tags: {e}"))?
        .flatten()
        .collect();
    let tags_in_index = parent_ids.len();

    let mut candidates: Vec<Root> = Vec::new();
    let mut rejected = Rejected::default();
    for id in parent_ids {
        let Some(path) = resolve_path(conn, id, volume_path) else {
            rejected.path_unresolvable += 1;
            continue;
        };
        match read_first_line(&Path::new(&path).join("CACHEDIR.TAG")) {
            None => rejected.tag_gone_on_disk += 1,
            Some(line) if line == CACHEDIR_SIGNATURE => candidates.push(Root { id, path }),
            Some(_) => rejected.signature_mismatch += 1,
        }
    }

    // Collapse nested roots: a `target/` inside an already-marked tree is not a
    // second slice of rows and must not be double-counted. Nesting is common, not a
    // corner case (10 of the author's 60 tags were under `~/.cache/uv` alone).
    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    let mut kept: Vec<Root> = Vec::new();
    for c in candidates {
        let nested = kept
            .last()
            .is_some_and(|prev| c.path.starts_with(&format!("{}/", prev.path.trim_end_matches('/'))));
        if !nested {
            kept.push(c);
        }
    }
    let rejected_total = rejected.tag_gone_on_disk + rejected.signature_mismatch + rejected.path_unresolvable;
    let collapsed = tags_in_index - rejected_total - kept.len();
    Ok(Roots {
        kept,
        collapsed,
        tags_in_index,
        rejected,
    })
}

/// The file's first line, trimmed of its line ending. `None` when the file can't
/// be opened or read. Only the first line matters: that's what the `CACHEDIR.TAG`
/// standard specifies, and it keeps this one small read per candidate.
fn read_first_line(file: &Path) -> Option<String> {
    let f = std::fs::File::open(file).ok()?;
    let mut line = String::new();
    BufReader::new(f).read_line(&mut line).ok()?;
    Some(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Rebuild an entry's absolute path by walking `parent_id` up to the root
/// sentinel. Returns `None` on a broken chain, which is drift worth counting
/// rather than a reason to stop.
fn resolve_path(conn: &Connection, id: i64, volume_path: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = id;
    // Bounded so a cyclic chain can't hang the tool; no real path is this deep.
    for _ in 0..512 {
        if current == ROOT_ID {
            let mut path = volume_path.trim_end_matches('/').to_string();
            for part in parts.iter().rev() {
                path.push('/');
                path.push_str(part);
            }
            return Some(path);
        }
        let (parent, name) = conn
            .query_row("SELECT parent_id, name FROM entries WHERE id = ?1", [current], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })
            .ok()?;
        parts.push(name);
        current = parent;
    }
    None
}

// ── Rows under the marked set ────────────────────────────────────────

struct Under {
    files: i64,
    dirs: i64,
    logical_bytes: i64,
    physical_bytes: i64,
    null_logical_files: i64,
}

impl Under {
    fn emit(&self, out: &mut Doc) {
        out.num("marked_file_rows", self.files);
        out.num("marked_dir_rows", self.dirs);
        out.num("marked_logical_bytes", self.logical_bytes);
        out.num("marked_physical_bytes", self.physical_bytes);
        out.num("marked_files_with_null_logical_size", self.null_logical_files);
    }
}

/// Per-root rows and bytes, biggest first. The whole-set totals hide whether the
/// win is spread across the marked set or concentrated in three directories, and
/// that difference is what decides how much the `CACHEDIR.TAG` policy is worth.
fn per_root_stats(conn: &Connection, roots: &Roots) -> Vec<(String, Under)> {
    let mut out: Vec<(String, Under)> = roots
        .kept
        .iter()
        .filter_map(|r| {
            let one = Roots {
                kept: vec![Root {
                    id: r.id,
                    path: r.path.clone(),
                }],
                collapsed: 0,
                tags_in_index: 0,
                rejected: Rejected::default(),
            };
            rows_under(conn, &one).ok().map(|u| (r.path.clone(), u))
        })
        .collect();
    out.sort_by_key(|(_, u)| -u.files);
    out
}

/// One recursive descent over every marked subtree at once. The seed rows are the
/// marked ROOTS, which are directories and stay in the index, so they're
/// subtracted from the directory count.
fn rows_under(conn: &Connection, roots: &Roots) -> Result<Under, String> {
    if roots.kept.is_empty() {
        return Ok(Under {
            files: 0,
            dirs: 0,
            logical_bytes: 0,
            physical_bytes: 0,
            null_logical_files: 0,
        });
    }
    let ids = roots
        .kept
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "WITH RECURSIVE sub(id, is_dir, lsize, psize) AS (
             SELECT id, is_directory, logical_size, physical_size FROM entries WHERE id IN ({ids})
             UNION ALL
             SELECT c.id, c.is_directory, c.logical_size, c.physical_size
               FROM entries c JOIN sub s ON c.parent_id = s.id
               WHERE s.is_dir = 1
         )
         SELECT sum(is_dir = 0),
                sum(is_dir = 1),
                sum(CASE WHEN is_dir = 0 THEN coalesce(lsize, 0) ELSE 0 END),
                sum(CASE WHEN is_dir = 0 THEN coalesce(psize, 0) ELSE 0 END),
                sum(is_dir = 0 AND lsize IS NULL)
           FROM sub"
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
        .map_err(|e| format!("walking marked subtrees: {e}"))?;
    Ok(Under {
        files: row.0,
        dirs: row.1 - roots.kept.len() as i64,
        logical_bytes: row.2,
        physical_bytes: row.3,
        null_logical_files: row.4,
    })
}

/// Delete exactly the measured slice: file rows at any depth under a marked root.
/// Directory rows and their `dir_stats` stay, so what the VACUUM reclaims is
/// attributable to file rows alone.
fn delete_file_rows_under(conn: &Connection, roots: &Roots) -> Result<i64, String> {
    if roots.kept.is_empty() {
        return Ok(0);
    }
    let ids = roots
        .kept
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "DELETE FROM entries WHERE id IN (
             WITH RECURSIVE sub(id, is_dir) AS (
                 SELECT id, is_directory FROM entries WHERE id IN ({ids})
                 UNION ALL
                 SELECT c.id, c.is_directory FROM entries c JOIN sub s ON c.parent_id = s.id
                   WHERE s.is_dir = 1
             )
             SELECT id FROM sub WHERE is_dir = 0
         )"
    );
    conn.execute(&sql, [])
        .map(|n| n as i64)
        .map_err(|e| format!("deleting marked file rows: {e}"))
}

// ── SQLite plumbing ──────────────────────────────────────────────────

fn open_read_only(db: &Path) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("opening {} read-only: {e}", db.display()))?;
    register_platform_case_collation(&conn).map_err(|e| format!("registering collation: {e}"))?;
    Ok(conn)
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
    lists: Vec<(String, Vec<String>)>,
    /// Lists of records, each already rendered as `key: value` pairs.
    tables: Vec<(String, Vec<Record>)>,
}

impl Doc {
    fn new() -> Self {
        Doc {
            fields: Vec::new(),
            lists: Vec::new(),
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
    fn list(&mut self, k: &str, v: &[String]) {
        self.lists.push((k.to_string(), v.to_vec()));
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
            for (k, items) in &self.lists {
                if !first {
                    println!(",");
                }
                let body = items
                    .iter()
                    .map(|i| format!("    \"{}\"", escape(i)))
                    .collect::<Vec<_>>()
                    .join(",\n");
                print!("  \"{k}\": [\n{body}\n  ]");
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
        for (k, items) in &self.lists {
            println!("{k} ({}):", items.len());
            for i in items {
                println!("  {i}");
            }
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
