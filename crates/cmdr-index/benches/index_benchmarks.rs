//! Benchmarks for the index's read and aggregation hot paths.
//!
//! Run with: `cargo bench --bench index_benchmarks`. Results land in
//! `target/criterion/` with HTML reports; `--save-baseline <name>` /
//! `--baseline <name>` compares two runs.
//!
//! What's measured and why:
//! - **`enrich_entries_with_index`** — the sub-millisecond path every directory
//!   listing pays for its recursive sizes. The one number in here that has to
//!   stay within noise.
//! - **`get_dir_stats_batch`** — the IPC dir-stats read, same DB but path-keyed
//!   and one volume-routing hop deeper.
//! - **`compute_all_aggregates_reported`** — the bottom-up dir-stats roll-up: the
//!   per-entry write-side cost, and the canary for anything that lands a trait
//!   call or an allocation inside a per-entry loop.
//!
//! The fixture is a synthetic index DB built through the public `store` API: no
//! files on disk, no scan, no lifecycle, no allocator swap. Both read paths
//! resolve everything out of SQLite and never stat the tree they describe, so a
//! DB whose paths don't exist measures exactly what production measures, and it
//! does so deterministically on any machine. Scan throughput is deliberately
//! NOT here; `docs/notes/index-extraction-baseline.md` says why and where it
//! lives instead.

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_index::store::{EntryRow, IndexStore, ROOT_ID, resolve_scan_root};
use cmdr_index::testing::scan::compute_all_aggregates_reported;
use cmdr_index::{
    Index, ROOT_VOLUME_ID, test_install_root_read_pool, test_read_pool_lock, test_uninstall_root_read_pool,
};
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rusqlite::Connection;

/// The handle every benchmark measures through. Built once for the binary, with
/// no host wired: these benchmarks read a synthetic index database, so nothing
/// needs mounting or reporting.
fn index() -> &'static Index {
    static INDEX: std::sync::OnceLock<Index> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| Index::builder().build().expect("one index per bench binary"))
}

/// Where the synthetic listing sits in the index's path space.
///
/// Under `/Users` deliberately: the read paths run the real boot-disk exclusion
/// gate, which skips `/Volumes`, `/System`, and `/private/var`. A fixture rooted
/// in any of those would be excluded before a single DB query and every bench in
/// here would measure the early return instead of the work.
const LISTING_PARENT: &str = "/Users/cmdr-index-bench/listing";

/// Files per directory in the fixture. Small: these benches are about the
/// directory-keyed reads, and file rows only pad the aggregation input.
const FILES_PER_DIR: usize = 4;

/// A fixed `modified_at`, so two runs of the harness build byte-identical DBs.
const FIXED_MTIME: u64 = 1_700_000_000;

/// A synthetic index DB plus the paths of the listing it describes.
struct Fixture {
    /// Held so the DB outlives the benchmark; dropping it deletes the dir.
    _dir: tempfile::TempDir,
    db_path: PathBuf,
    /// Absolute paths of `LISTING_PARENT`'s child directories, in listing order.
    child_dirs: Vec<String>,
}

fn dir_row(id: i64, parent_id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.to_string(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: Some(FIXED_MTIME),
        inode: None,
    }
}

fn file_row(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.to_string(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(size),
        physical_size: Some(size.next_multiple_of(4096)),
        modified_at: Some(FIXED_MTIME),
        inode: None,
    }
}

/// Build an index DB describing `dirs` child directories under [`LISTING_PARENT`],
/// each holding [`FILES_PER_DIR`] files plus a `nested/` subdirectory with the same
/// number again, then aggregate it.
///
/// Every directory is stamped `listed_epoch = 1` before aggregation, so the
/// roll-up produces `min_subtree_epoch > 0` and the read paths take the
/// fully-covered "exact size" branch rather than the lower-bound one, which is
/// what a settled index looks like in production.
fn build_fixture(dirs: usize) -> Fixture {
    let dir = tempfile::tempdir().expect("create temp dir for the bench index DB");
    let db_path = dir.path().join("index-bench.db");

    // `open` creates the schema and stamps the version; we then write through a
    // separate write connection, the way the writer thread does.
    drop(IndexStore::open(&db_path).expect("create the bench index DB"));
    let conn = IndexStore::open_write_connection(&db_path).expect("open a write connection");

    resolve_scan_root(&conn, Path::new("/"), true).expect("seed the root sentinel");
    IndexStore::seed_current_epoch(&conn).expect("seed current_epoch");

    let mut rows: Vec<EntryRow> = Vec::new();
    let mut dir_ids: Vec<i64> = vec![ROOT_ID];
    let mut next_id = ROOT_ID + 1;

    // The `/Users/cmdr-index-bench/listing` spine.
    let mut parent_id = ROOT_ID;
    for component in LISTING_PARENT.trim_start_matches('/').split('/') {
        let id = next_id;
        next_id += 1;
        rows.push(dir_row(id, parent_id, component));
        dir_ids.push(id);
        parent_id = id;
    }
    let listing_id = parent_id;

    let mut child_dirs = Vec::with_capacity(dirs);
    for i in 0..dirs {
        let name = format!("dir_{i:05}");
        let dir_id = next_id;
        next_id += 1;
        rows.push(dir_row(dir_id, listing_id, &name));
        dir_ids.push(dir_id);
        child_dirs.push(format!("{LISTING_PARENT}/{name}"));

        // One nesting level, so the aggregator has a genuine bottom-up roll-up
        // to do instead of a single flat layer.
        let nested_id = next_id;
        next_id += 1;
        rows.push(dir_row(nested_id, dir_id, "nested"));
        dir_ids.push(nested_id);

        for f in 0..FILES_PER_DIR {
            let size = 4096 * (f as u64 + 1);
            rows.push(file_row(next_id, dir_id, &format!("file_{f:03}.bin"), size));
            next_id += 1;
            rows.push(file_row(next_id, nested_id, &format!("nested_{f:03}.bin"), size / 2));
            next_id += 1;
        }
    }

    IndexStore::insert_entries_v2_batch(&conn, &rows).expect("insert the fixture entries");
    IndexStore::mark_dirs_listed(&conn, &dir_ids, 1).expect("stamp listed_epoch");
    compute_all_aggregates_reported(&conn, &mut |_| {}).expect("aggregate the fixture");

    // Fold the WAL back into the main DB file: the read paths open read-only
    // connections, and leaving the fixture's writes in the WAL would make the
    // first measured read pay for replaying them.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .expect("checkpoint the fixture WAL");
    drop(conn);

    Fixture {
        _dir: dir,
        db_path,
        child_dirs,
    }
}

/// A directory `FileEntry` as a listing hands it to enrichment: the four
/// identity fields set, every recursive-size field still `None`.
///
/// Spelled out rather than built with `FileEntry::new`, which is `pub(crate)`.
/// A new field breaks this with a compile error, which is the right failure.
fn listing_entry(path: &str) -> FileEntry {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    FileEntry {
        name,
        path: path.to_string(),
        is_directory: true,
        is_symlink: false,
        is_archive: false,
        size: None,
        physical_size: None,
        inode: None,
        modified_at: Some(FIXED_MTIME),
        created_at: None,
        added_at: None,
        opened_at: None,
        permissions: 0o755,
        owner: String::new(),
        group: String::new(),
        icon_id: "dir".to_string(),
        extended_metadata_loaded: false,
        tags: Vec::new(),
        recursive_size: None,
        recursive_physical_size: None,
        recursive_file_count: None,
        recursive_dir_count: None,
        recursive_has_symlinks: None,
        recursive_size_complete: None,
        recursive_size_stale: None,
        redirect_to_path: None,
        git_meta: None,
    }
}

/// Listing sizes to measure: a small pane, an ordinary big folder, and a
/// pathological one.
const LISTING_SIZES: [usize; 3] = [50, 500, 2000];

/// Prove the fixture is actually being read before timing anything against it.
///
/// Every read path in here returns early and cheaply when no index is registered,
/// when the parent path is excluded, or when the parent isn't in the index. All
/// three would leave a benchmark that runs green, reports single-digit
/// nanoseconds, and measures nothing. So each group calls this first and panics
/// on a silent skip.
fn assert_fixture_is_read(fixture: &Fixture) {
    let mut entries: Vec<FileEntry> = fixture.child_dirs.iter().map(|p| listing_entry(p)).collect();
    index().enrich(ROOT_VOLUME_ID, &mut entries);
    let enriched = entries.iter().filter(|e| e.recursive_size.is_some()).count();
    assert_eq!(
        enriched,
        entries.len(),
        "enrichment reached the index for {enriched} of {} fixture dirs; anything short of all of \
         them means a read path skipped and the numbers below measure an early return",
        entries.len()
    );

    let stats = index()
        .dir_stats_batch(&fixture.child_dirs)
        .expect("dir-stats batch over the fixture");
    let found = stats.iter().filter(|s| s.is_some()).count();
    assert_eq!(
        found,
        fixture.child_dirs.len(),
        "dir-stats batch resolved {found} of {} fixture dirs",
        fixture.child_dirs.len()
    );
}

/// Enrichment over a whole listing, which is how it's always called: the
/// per-listing cost is what the directory-size feature spends, not a per-entry
/// figure. `Throughput::Elements` reports the per-directory number alongside.
fn bench_enrich_entries_with_index(c: &mut Criterion) {
    // The root read pool is a process-global. Held for the whole group so no
    // other consumer can swap it mid-measurement.
    let _pool_guard = test_read_pool_lock();
    let mut group = c.benchmark_group("enrich_entries_with_index");

    for dirs in LISTING_SIZES {
        let fixture = build_fixture(dirs);
        test_install_root_read_pool(fixture.db_path.clone()).expect("install the bench read pool");
        assert_fixture_is_read(&fixture);

        let listing: Vec<FileEntry> = fixture.child_dirs.iter().map(|p| listing_entry(p)).collect();
        group.throughput(Throughput::Elements(dirs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(dirs), &listing, |b, listing| {
            // Enrichment mutates its input, so each iteration gets a fresh copy.
            // `iter_batched` keeps the clone out of the measurement.
            b.iter_batched_ref(
                || listing.clone(),
                |entries| index().enrich(ROOT_VOLUME_ID, entries),
                BatchSize::SmallInput,
            );
        });

        test_uninstall_root_read_pool();
    }

    group.finish();
}

/// The IPC dir-stats read over the same listing. Path-keyed rather than
/// integer-keyed, so it's the slower of the two read paths by design.
fn bench_get_dir_stats_batch(c: &mut Criterion) {
    let _pool_guard = test_read_pool_lock();
    let mut group = c.benchmark_group("get_dir_stats_batch");

    for dirs in LISTING_SIZES {
        let fixture = build_fixture(dirs);
        test_install_root_read_pool(fixture.db_path.clone()).expect("install the bench read pool");
        assert_fixture_is_read(&fixture);

        group.throughput(Throughput::Elements(dirs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(dirs), &fixture.child_dirs, |b, paths| {
            b.iter(|| index().dir_stats_batch(paths))
        });

        test_uninstall_root_read_pool();
    }

    group.finish();
}

/// The whole-DB bottom-up roll-up. Sized by total entry count rather than
/// listing width, because that's what it walks.
fn bench_compute_all_aggregates(c: &mut Criterion) {
    let mut group = c.benchmark_group("compute_all_aggregates_reported");
    // Entries per fixture dir: the dir, its `nested/`, and both files sets.
    let entries_per_dir = (2 + 2 * FILES_PER_DIR) as u64;

    for dirs in [500usize, 5000] {
        let fixture = build_fixture(dirs);
        let conn = IndexStore::open_write_connection(&fixture.db_path).expect("open a write connection");

        group.throughput(Throughput::Elements(dirs as u64 * entries_per_dir));
        group.bench_with_input(BenchmarkId::from_parameter(dirs), &conn, |b, conn: &Connection| {
            // Idempotent: it recomputes the same totals and upserts them, so
            // repeated iterations measure identical work.
            b.iter(|| compute_all_aggregates_reported(conn, &mut |_| {}));
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_enrich_entries_with_index,
    bench_get_dir_stats_batch,
    bench_compute_all_aggregates
);
criterion_main!(benches);
