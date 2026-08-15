//! What covering a REAL home folder costs. `#[ignore]`d, and a measurement rather
//! than a test.

use super::*;

/// What covering a REAL home folder costs, and when the early signal fires inside
/// that. `#[ignore]`d: it walks the machine's actual `$HOME` (into a temp index),
/// takes minutes, and prints numbers rather than asserting any.
///
/// It exists to answer one question the design rests on: whether `~/Library` is
/// enough of home's wall clock that the early media kick has to skip it. Run it
/// with `CMDR_PHASE_HOME` to point it somewhere smaller.
///
/// ```sh
/// cargo test -p cmdr-index --release --lib -- --ignored --nocapture \
///   indexing::lifecycle::phases::tests::home_bench::how_long_home_takes
/// ```
#[test]
#[ignore = "walks a real home folder; run it explicitly"]
fn how_long_home_takes() {
    let home = std::env::var("CMDR_PHASE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().expect("a home directory"));

    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
    volumes.register(
        "phased-measure",
        std::sync::Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("Measured")
                .with_root(&home)
                .with_local_fs_access(),
        ),
    );
    let (index, _installed) = crate::indexing::handle::Index::builder()
        .data_dir(data.path())
        .volumes(std::sync::Arc::clone(&volumes) as std::sync::Arc<_>)
        .host(crate::indexing::host::policy::FakeHostPolicy::shared() as std::sync::Arc<_>)
        .indexing_enabled(Some(true))
        .install_for_test();
    let _home_override = set_home_override(home.clone());

    let db_path = data.path().join("index-phased-measure.db");
    // Written the way `phased_bench` writes its numbers: `writeln!` to a stderr
    // handle, so a measurement harness can report without the `print_stdout` lint
    // that keeps production code on the logger.
    use std::io::Write;
    let mut out = std::io::stderr();
    let started = std::time::Instant::now();
    crate::indexing::host::runtime::block_on(index.start_volume("phased-measure")).expect("indexing starts");

    let mut home_covered = None;
    loop {
        let meta = |key: &str| {
            IndexStore::open_read_connection(&db_path)
                .ok()
                .and_then(|conn| IndexStore::get_meta(&conn, key).ok().flatten())
        };
        if home_covered.is_none() && meta(HOME_COVERED_AT_KEY).is_some() {
            home_covered = Some(started.elapsed());
            let _ = writeln!(
                out,
                "home covered (minus the deferred folder) after {:?}",
                started.elapsed()
            );
        }
        if meta("scan_completed_at").is_some() {
            let _ = writeln!(out, "all of {} covered after {:?}", home.display(), started.elapsed());
            break;
        }
        // ⚠️ Watch the MACHINE, not only the marker. The stock-take that stamps runs
        // before the machine reports itself idle, so a machine that has stopped
        // without the marker will never write one — and this loop would otherwise
        // sit out its whole patience and record a run that ended in 90 s as one that
        // took ten minutes. That is exactly how one run got filed with no
        // explanation (`docs/notes/churn-against-completion-2026-08-15.md`).
        if !index.status("phased-measure").is_ok_and(|status| status.scanning) {
            let _ = writeln!(
                out,
                "the machine stopped after {:?} with nothing marking {} done; \
                 {} still needs walking, so something was written under it while it ran",
                started.elapsed(),
                home.display(),
                cmdr_fs::pluralize::pluralize(frontier_len(&db_path, &home), "frontier root"),
            );
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(600) {
            let _ = writeln!(out, "gave up after 10 minutes");
            break;
        }
        // allowed-test-sleep: the sampler IS the measurement. It watches two markers
        // land at different moments over minutes, which is the number this prints;
        // a wait-on-one-condition helper can't see the first one go by.
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let entries = IndexStore::open_read_connection(&db_path)
        .ok()
        .and_then(|conn| IndexStore::get_entry_count(&conn).ok())
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "{}; the early signal arrived {}",
        cmdr_fs::pluralize::pluralize_with(entries, "entry", "entries"),
        match home_covered {
            Some(at) => format!("{at:?} in"),
            None => "never".to_string(),
        }
    );
    let _ = index.forget_volume("phased-measure");
}

/// What the index still says it hasn't walked, straight off the database file. The
/// number that says whether a machine that stopped unmarked ran out of ground or
/// ran out of passes.
fn frontier_len(db_path: &Path, home: &Path) -> u64 {
    let Ok(conn) = IndexStore::open_read_connection(db_path) else {
        return 0;
    };
    let home = home.to_string_lossy();
    coverage_for_scope(&conn, "/", &home, CoverageDimension::Listing)
        .map(|map| map.frontier.len() as u64)
        .unwrap_or(0)
}
