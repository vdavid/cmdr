//! Unit tests for the churn monitor's aggregation. Pure and clock-injected:
//! no filesystem, no app, no real time.

use super::*;

fn monitor(now: Instant) -> ChurnMonitor {
    ChurnMonitor::new(Duration::from_secs(30), DEFAULT_TOP_N, now)
}

/// Feed paths, then close the period and return the report.
fn roll(m: &mut ChurnMonitor, t0: Instant, paths: &[&str]) -> ChurnReport {
    m.record_batch(paths.iter().copied(), paths.len() as u64);
    m.rollup(t0 + Duration::from_secs(30)).expect("period elapsed")
}

fn node<'a>(report: &'a ChurnReport, path: &str) -> &'a ChurnNodeReport {
    report
        .top
        .iter()
        .find(|n| n.path == path)
        .unwrap_or_else(|| panic!("{path} missing from report: {:?}", report.top))
}

#[test]
fn churn_rolls_up_the_whole_ancestor_chain() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    let report = roll(&mut m, t0, &["/a/b/c/f1.tmp", "/a/b/c/f2.tmp"]);

    // Every ancestor sees both events; only the containing dir counts them direct.
    assert_eq!(node(&report, "/a/b/c").events, 2);
    assert_eq!(node(&report, "/a/b/c").direct, 2);
    assert_eq!(node(&report, "/a/b").events, 2);
    assert_eq!(node(&report, "/a/b").direct, 0);
    assert_eq!(node(&report, "/a").events, 2);
    assert_eq!(node(&report, "/").events, 2);
    assert_eq!(report.batch_paths, 2);
}

#[test]
fn direct_credits_only_the_containing_directory() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    // A change to the directory `/a/b` itself is direct on its PARENT `/a`.
    let report = roll(&mut m, t0, &["/a/b", "/a/b/f.txt"]);

    assert_eq!(node(&report, "/a").direct, 1, "the `/a/b` event is direct on /a");
    assert_eq!(node(&report, "/a/b").direct, 1, "the file event is direct on /a/b");
    assert_eq!(node(&report, "/a").events, 2, "both roll up to /a");
}

#[test]
fn distinct_children_separates_one_hot_file_from_many_temp_files() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    // The discriminator the seal decision needs: same event volume, different shape.
    let hot: Vec<String> = (0..50).map(|_| "/log/app.log".to_string()).collect();
    let temps: Vec<String> = (0..50).map(|i| format!("/tmp/fetch/{i}.tmp")).collect();
    let paths: Vec<&str> = hot.iter().chain(temps.iter()).map(String::as_str).collect();
    let report = roll(&mut m, t0, &paths);

    assert_eq!(node(&report, "/log").distinct_children, 1, "one file rewritten 50×");
    assert_eq!(node(&report, "/log").direct, 50);
    assert_eq!(
        node(&report, "/tmp/fetch").distinct_children,
        50,
        "50 distinct temp files"
    );
    assert_eq!(node(&report, "/tmp/fetch").direct, 50);
}

#[test]
fn distinct_children_saturates_at_the_cap_and_flags_it() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    let paths: Vec<String> = (0..CHILD_CAP + 200).map(|i| format!("/flat/{i}.tmp")).collect();
    let report = roll(&mut m, t0, &paths.iter().map(String::as_str).collect::<Vec<_>>());

    let flat = node(&report, "/flat");
    assert_eq!(flat.distinct_children, CHILD_CAP, "the exact set stops at the cap");
    assert!(flat.children_capped, "capped is a floor marker for the analysis");
    assert_eq!(
        flat.direct as usize,
        CHILD_CAP + 200,
        "magnitude still exact via `direct`"
    );
}

#[test]
fn ratio_drop_boundary_is_visible_along_the_chain() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    // The shape a seal-root rule depends on: a uniformly churny subtree hanging
    // off a directory that also holds quiet, keep-worthy siblings.
    let mut paths: Vec<String> = (0..200).map(|i| format!("/home/proj/target/debug/{i}.o")).collect();
    paths.push("/home/proj/src/main.rs".to_string());
    let report = roll(&mut m, t0, &paths.iter().map(String::as_str).collect::<Vec<_>>());

    let target = node(&report, "/home/proj/target");
    let proj = node(&report, "/home/proj");
    assert_eq!(target.events, 200);
    assert_eq!(proj.events, 201);
    // The drop is at `proj`, whose churn splits across two children; `target`'s
    // churn funnels through exactly one.
    assert_eq!(target.distinct_children, 1, "target → debug only");
    assert_eq!(proj.distinct_children, 2, "proj → target plus a real source dir");
}

#[test]
fn top_n_never_truncates_an_ancestor_chain_in_the_middle() {
    let t0 = Instant::now();
    let mut m = ChurnMonitor::new(Duration::from_secs(30), 4, t0);
    let paths: Vec<String> = (0..20).map(|i| format!("/a/b/c/d/{i}.tmp")).collect();
    let report = roll(&mut m, t0, &paths.iter().map(String::as_str).collect::<Vec<_>>());

    // All five nodes tie at 20 events; the top-4 cut must keep the shallow end,
    // so every emitted node still has its parent in the report.
    let emitted: Vec<&str> = report.top.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(emitted, vec!["/", "/a", "/a/b", "/a/b/c"]);
    assert_eq!(report.node_count, 5, "the fifth node existed, it just didn't rank");
}

#[test]
fn ranking_is_count_desc_then_path_asc() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    let mut paths: Vec<String> = (0..5).map(|i| format!("/x/hot/{i}.tmp")).collect();
    paths.push("/x/cold/a.txt".to_string());
    let report = roll(&mut m, t0, &paths.iter().map(String::as_str).collect::<Vec<_>>());

    let emitted: Vec<(&str, u64)> = report.top.iter().map(|n| (n.path.as_str(), n.events)).collect();
    assert_eq!(emitted, vec![("/", 6), ("/x", 6), ("/x/hot", 5), ("/x/cold", 1)]);
}

#[test]
fn rollup_returns_none_before_the_period_and_resets_after() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    m.record_batch(["/a/f.txt"].into_iter(), 3);

    assert!(m.rollup(t0 + Duration::from_secs(29)).is_none(), "period not elapsed");

    let first = m.rollup(t0 + Duration::from_secs(30)).expect("period elapsed");
    assert_eq!(first.seq, 0);
    assert_eq!(first.raw_events, 3);
    assert_eq!(first.batch_paths, 1);
    assert_eq!(first.period_ms, 30_000);

    // Nothing survives a period: that's what bounds memory over a long run.
    let second = m.rollup(t0 + Duration::from_secs(60)).expect("second period elapsed");
    assert_eq!(second.seq, 1);
    assert_eq!(second.raw_events, 0);
    assert_eq!(second.batch_paths, 0);
    assert_eq!(second.node_count, 0);
    assert!(second.top.is_empty());
}

#[test]
fn raw_event_count_survives_dedup_so_the_ratio_is_visible() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    // 500 raw events deduplicated down to one path.
    m.record_batch(["/log/app.log"].into_iter(), 500);
    let report = m.rollup(t0 + Duration::from_secs(30)).expect("period elapsed");

    assert_eq!(report.raw_events, 500);
    assert_eq!(report.batch_paths, 1);
}

#[test]
fn node_cap_drops_deep_nodes_and_keeps_shallow_totals_honest() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    // Each path adds up to 3 new nodes (`/cap`, `/cap/<i>`, plus `/` once), so
    // this comfortably overshoots MAX_NODES.
    let paths: Vec<String> = (0..MAX_NODES).map(|i| format!("/cap/{i}/f.tmp")).collect();
    let report = roll(&mut m, t0, &paths.iter().map(String::as_str).collect::<Vec<_>>());

    assert!(report.nodes_dropped > 0, "the cap engaged");
    assert!(report.node_count <= MAX_NODES, "tracked nodes stay bounded");
    // The shallow end is created first, so its rolled-up totals are complete.
    assert_eq!(node(&report, "/").events as usize, MAX_NODES);
    assert_eq!(node(&report, "/cap").events as usize, MAX_NODES);
}

#[test]
fn a_pathological_depth_is_cut_and_counted() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    let deep = format!(
        "/{}/leaf.txt",
        (0..MAX_DEPTH + 10).map(|i| i.to_string()).collect::<Vec<_>>().join("/")
    );
    let report = roll(&mut m, t0, &[&deep]);

    assert_eq!(report.deep_truncated, 1);
    assert_eq!(report.node_count, MAX_DEPTH, "the chain stops at MAX_DEPTH levels");
    assert_eq!(node(&report, "/").events, 1, "the shallow end is still credited");
}

// ── The observer: the seam that made the instrumentation miss production ──

/// The bug this pins: the churn hook lived at ONE live loop's flush tick, and
/// the cold-start journal-replay path runs a SECOND live loop that never calls
/// it, so a whole boot route measured nothing while every unit test passed.
///
/// Both live loops now funnel through `process_live_batch`, which takes a
/// `ChurnObserver` by `&mut` — so the compiler enforces the hook at every live
/// batch. This test guards the remaining hole the compiler can't see: a NEW
/// live loop appearing in a third file, or an existing one quietly downgrading
/// to `ChurnObserver::disabled()`.
///
/// The scan is RECURSIVE: a live loop added in a subdirectory of `event_loop/`
/// would otherwise slip past the very guard this test exists to be. `tests`
/// directories are skipped, since a test harness driving `process_live_batch`
/// with a disabled observer is legitimate.
#[test]
fn every_live_loop_owns_a_real_churn_observer() {
    /// Collect every non-test `.rs` file under `dir`, recursively, as a path
    /// relative to the `event_loop` root (so a subdirectory driver is named
    /// unambiguously in the failure message).
    fn collect(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
        for entry in std::fs::read_dir(dir).expect("event_loop dir") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().expect("file name").to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if path.is_dir() {
                if name == "tests" {
                    continue;
                }
                collect(&path, &rel, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !name.ends_with("tests.rs") {
                out.push((rel, path));
            }
        }
    }

    let event_loop = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/indexing/watch/event_loop");
    let mut sources: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect(&event_loop, "", &mut sources);

    let mut drivers: Vec<String> = Vec::new();
    for (name, path) in sources {
        let src = std::fs::read_to_string(&path).expect("read source");
        if !src.contains("process_live_batch(") {
            continue;
        }
        assert!(
            src.contains("ChurnObserver::from_env("),
            // allowed-pluralize-noun: `{name}` is a file name and `drives` is the verb, not a plural noun.
            "{name} drives live batches but never builds a real ChurnObserver, \
             so the churn spike would silently measure nothing on that route"
        );
        drivers.push(name);
    }
    drivers.sort();
    assert_eq!(
        drivers,
        vec!["live.rs".to_string(), "replay.rs".to_string()],
        "the set of live-batch drivers changed; wire the new one's ChurnObserver, then update this list"
    );
}

#[test]
fn an_idle_period_still_closes_so_the_time_series_has_no_holes() {
    let t0 = Instant::now();
    let mut obs = ChurnObserver::enabled_for_test("root", Duration::from_secs(30), 10, t0);

    // No paths at all for a whole period: the rollup must still fire, or a
    // subtree going quiet reads as missing data rather than as quiet.
    obs.observe(std::iter::empty(), t0 + Duration::from_secs(30));
    let next = obs.take_report(t0 + Duration::from_secs(60)).expect("second period");
    assert_eq!(next.seq, 1, "the idle period closed and advanced the sequence");
}

#[test]
fn raw_totals_are_diffed_from_the_loops_cumulative_counter() {
    let t0 = Instant::now();
    let mut obs = ChurnObserver::enabled_for_test("root", Duration::from_secs(30), 10, t0);

    obs.with_raw_total(100).observe(["/a/f.txt"].into_iter(), t0);
    obs.with_raw_total(250).observe(["/a/f.txt"].into_iter(), t0);
    let report = obs.take_report(t0 + Duration::from_secs(30)).expect("period elapsed");

    assert_eq!(report.raw_events, 250, "cumulative counter is diffed, not re-added");
    assert_eq!(report.batch_paths, 2);
}

#[test]
fn a_restarted_loop_counter_does_not_underflow() {
    let t0 = Instant::now();
    let mut obs = ChurnObserver::enabled_for_test("root", Duration::from_secs(30), 10, t0);

    obs.with_raw_total(500).observe(["/a/f.txt"].into_iter(), t0);
    // A counter that goes backwards contributes nothing rather than wrapping.
    obs.with_raw_total(7).observe(["/a/f.txt"].into_iter(), t0);
    let report = obs.take_report(t0 + Duration::from_secs(30)).expect("period elapsed");

    assert_eq!(report.raw_events, 500);
}

#[test]
fn a_disabled_observer_records_nothing() {
    let t0 = Instant::now();
    let mut obs = ChurnObserver::disabled();
    obs.with_raw_total(1000).observe(["/a/f.txt"].into_iter(), t0);
    assert!(obs.take_report(t0 + Duration::from_secs(30)).is_none());
}

#[test]
fn root_and_empty_paths_are_ignored_without_panicking() {
    let t0 = Instant::now();
    let mut m = monitor(t0);
    let report = roll(&mut m, t0, &["/", "", "///"]);

    assert_eq!(report.batch_paths, 0, "no path names a changed child");
    assert_eq!(report.node_count, 0);
}
