//! What one live tick costs, split into the two halves the tick spends its time in.
//!
//! `#[ignore]`d: it prints wall-clock numbers over synthetic stores rather than
//! asserting. ⚠️ Everything here is WALL time against a warm page cache on a temp
//! dir, ❌ never CPU: both arms bottom out in SQLite reads, and
//! `docs/notes/idle-cpu-attribution-2026-08-03.md` records what happens when a
//! syscall leaf gets reported as CPU.
//!
//! Two arms, matching the two halves of `run_live_tick_blocking`:
//!
//! - **The scoped walk** ([`walk_image_entries_in_dirs`]): one `resolve_path` plus one
//!   `list_children_on` per touched dir. What the tick pays per directory FSEvents
//!   named, whether or not that directory could ever be enriched.
//! - **The coverage gate** (`MediaScheduler::folder_scores` versus
//!   `coverage::importance_scores`): the `above_threshold` read the tick runs every 60
//!   seconds, against the subscription-backed cache that answers the same question.
//!
//! ```sh
//! cargo test -p cmdr-index --release --lib -- --ignored --nocapture live_tick_cost
//! ```
//!
//! Results and the call they back: `docs/notes/live-tick-cost-2026-08-21.md`.

use std::collections::HashSet;
use std::io::Write;
use std::time::{Duration, Instant};

use super::enrich::walk_image_entries_in_dirs;
use crate::importance::store::{ImportanceStore, importance_db_path};
use crate::importance::writer::{ImportanceWriter, WeightRow};
use crate::indexing::store::{IndexStore, ROOT_ID};

/// Touched-dir counts to measure the walk at. A tick's set is whatever FSEvents named
/// in the window; on a machine running cargo builds under `.claude/worktrees/*/target`
/// it is thousands, so the ladder brackets that rather than inferring the curve from
/// one point.
const WALK_WIDTHS: &[usize] = &[100, 500, 2_000, 10_000];

/// Scored-folder counts to measure the coverage gate at. The top of the ladder is the
/// 90,308 folders `docs/specs/idle-cost.md` quotes for David's root volume.
const SCORE_WIDTHS: &[usize] = &[1_000, 10_000, 90_308];

/// Files per synthetic touched dir, none of them images: build output is the churn
/// that drives ticks, and a dir with no qualifying image is exactly the case the walk
/// pays for and gets nothing from.
const FILES_PER_DIR: usize = 6;

#[test]
#[ignore = "benchmark over synthetic stores; run manually with --nocapture"]
fn live_tick_cost() {
    let mut out = std::io::stderr();

    let _ = writeln!(&mut out, "\n── the scoped walk, per touched dir ──");
    let _ = writeln!(
        &mut out,
        "{:>8}  {:>12}  {:>10}",
        "dirs", "walk", "µs/dir"
    );
    for &width in WALK_WIDTHS {
        let elapsed = walk_cost(width);
        let _ = writeln!(
            &mut out,
            "{width:>8}  {elapsed:>12.2?}  {:>10.1}",
            elapsed.as_secs_f64() * 1e6 / width as f64,
        );
    }

    let _ = writeln!(&mut out, "\n── the coverage filter that replaces the walk, per touched dir ──");
    let _ = writeln!(&mut out, "{:>8}  {:>12}  {:>10}", "dirs", "filter", "µs/dir");
    for &width in WALK_WIDTHS {
        let elapsed = filter_cost(width);
        let _ = writeln!(
            &mut out,
            "{width:>8}  {elapsed:>12.2?}  {:>10.3}",
            elapsed.as_secs_f64() * 1e6 / width as f64,
        );
    }

    let _ = writeln!(&mut out, "\n── the coverage gate, per tick ──");
    let _ = writeln!(
        &mut out,
        "{:>8}  {:>14}  {:>14}  {:>14}  {:>10}",
        "folders", "uncached", "cache (cold)", "cache (warm)", "speed-up"
    );
    for &width in SCORE_WIDTHS {
        let (uncached, cold, warm) = gate_cost(width);
        let _ = writeln!(
            &mut out,
            "{width:>8}  {uncached:>14.2?}  {cold:>14.2?}  {warm:>14.2?}  {:>10.0}x",
            uncached.as_secs_f64() / warm.as_secs_f64().max(f64::MIN_POSITIVE),
        );
    }
}

/// One `walk_image_entries_in_dirs` over `width` touched dirs of a synthetic index.
fn walk_cost(width: usize) -> Duration {
    let temp = tempfile::tempdir().expect("temp");
    let dirs = synthetic_dirs(width);
    let store = IndexStore::open(&temp.path().join("index-root.db")).expect("open index");
    build_dirs(store.read_conn(), &dirs);

    let set: HashSet<String> = dirs.iter().cloned().collect();
    // One untimed pass first, so the measured one runs against the same warm page
    // cache production's every-60-seconds tick does.
    let _ = walk_image_entries_in_dirs(store.read_conn(), &set).expect("warm walk");
    let start = Instant::now();
    let entries = walk_image_entries_in_dirs(store.read_conn(), &set).expect("walk");
    let elapsed = start.elapsed();
    assert!(entries.is_empty(), "build output holds no qualifying image");
    elapsed
}

/// What a tick now pays for `width` touched dirs that turn out to be ineligible: the
/// coverage filter alone, with no index touched at all. The number the walk arm's
/// µs/dir is replaced by, on the machine whose churn is build output.
fn filter_cost(width: usize) -> Duration {
    let dirs = synthetic_dirs(width);
    let config = crate::media_index::network::config::NetworkEnrichConfig {
        // One chosen folder, so the override arm of the filter does real work rather
        // than short-circuiting on an empty set.
        always_index_folders: ["/Users/dev/Pictures".to_string()].into_iter().collect(),
        ..Default::default()
    };
    let scores: std::collections::HashMap<String, f64> =
        [("/Users/dev/Pictures/Trips".to_string(), 0.9)].into_iter().collect();

    let start = Instant::now();
    let kept = dirs
        .iter()
        .filter(|dir| super::lifecycle::local_dir_may_be_covered(dir, Some(&scores), &config, "root"))
        .count();
    let elapsed = start.elapsed();
    assert_eq!(kept, 0, "build output is neither chosen nor scored");
    elapsed
}

/// The gate at `width` scored folders: the uncached `above_threshold` read the tick
/// runs today, the cache's first (cold) answer, and its steady-state (warm) one.
fn gate_cost(width: usize) -> (Duration, Duration, Duration) {
    let temp = tempfile::tempdir().expect("temp");
    let volume = format!("bench-{width}");
    seed_importance(temp.path(), &volume, width);

    // Warm the page cache for the uncached arm too, so the two are compared on equal
    // footing rather than the first one paying for the file's first read.
    let _ = read_scores_uncached(temp.path(), &volume);
    let start = Instant::now();
    let scores = read_scores_uncached(temp.path(), &volume);
    let uncached = start.elapsed();
    assert_eq!(scores.len(), width, "every seeded folder is above 0.0");

    let start = Instant::now();
    let cached = crate::media_index::coverage::importance_scores(temp.path(), &volume, Some(0.0)).expect("scored");
    let cold = start.elapsed();
    assert_eq!(cached.len(), width, "the cache agrees with the direct read");

    let start = Instant::now();
    let _ = crate::media_index::coverage::importance_scores(temp.path(), &volume, Some(0.0)).expect("scored");
    let warm = start.elapsed();

    (uncached, cold, warm)
}

/// Read every scored folder straight from the importance store, which is what
/// `MediaScheduler::folder_scores` did before it moved to the cache. Kept here (rather
/// than reached through the scheduler) so the "before" column stays the before column
/// however the scheduler's own read is wired.
fn read_scores_uncached(data_dir: &std::path::Path, volume_id: &str) -> std::collections::HashMap<String, f64> {
    use crate::importance::{ImportanceIndex, SignalSet};
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    index
        .above_threshold(0.0)
        .expect("read weights")
        .into_iter()
        .map(|w| (w.path, w.score.value()))
        .collect()
}

/// `width` synthetic touched-dir paths, ten components deep and spread across
/// worktrees so they share a long prefix the way real build output does.
/// `resolve_path` runs one indexed query per path component, so depth is a direct
/// multiplier on the walk's per-dir cost and a shallow fixture would flatter it.
fn synthetic_dirs(width: usize) -> Vec<String> {
    (0..width)
        .map(|i| {
            format!(
                "/Users/dev/projects-git/vdavid/cmdr/.claude/worktrees/wt{}/target/debug/deps{i}",
                i % 8
            )
        })
        .collect()
}

/// Insert every component of every path in `dirs` as a directory row, plus
/// [`FILES_PER_DIR`] non-image files in each leaf.
fn build_dirs(conn: &rusqlite::Connection, dirs: &[String]) {
    let mut ids: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut next_id = ROOT_ID + 1;
    for dir in dirs {
        let mut parent_id = ROOT_ID;
        let mut prefix = String::new();
        for component in dir.split('/').filter(|c| !c.is_empty()) {
            prefix.push('/');
            prefix.push_str(component);
            parent_id = match ids.get(&prefix) {
                Some(&id) => id,
                None => {
                    let id = next_id;
                    next_id += 1;
                    IndexStore::insert_entry_v2_with_id(conn, id, parent_id, component, true, false, None, None, None, None)
                        .expect("insert dir");
                    ids.insert(prefix.clone(), id);
                    id
                }
            };
        }
        for f in 0..FILES_PER_DIR {
            let id = next_id;
            next_id += 1;
            IndexStore::insert_entry_v2_with_id(
                conn,
                id,
                parent_id,
                &format!("unit-{f}.o"),
                false,
                false,
                Some(1_024),
                Some(1_024),
                Some(1_700_000_000),
                None,
            )
            .expect("insert file");
        }
    }
}

/// Seed an importance store for `volume_id` with `count` scored folders.
fn seed_importance(data_dir: &std::path::Path, volume_id: &str, count: usize) {
    let path = importance_db_path(data_dir, volume_id);
    ImportanceStore::open(&path).expect("open importance store");
    let writer = ImportanceWriter::spawn(&path).expect("importance writer");
    let rows: Vec<WeightRow> = (0..count)
        .map(|i| WeightRow {
            path: format!("/Users/dev/folder{}/sub{}", i % 512, i),
            score: 0.5,
            signals_json: "{}".to_string(),
        })
        .collect();
    writer.write_weights(1, rows).expect("write weights");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
}
