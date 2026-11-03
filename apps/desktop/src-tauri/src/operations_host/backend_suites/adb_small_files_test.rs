//! What a copy of many small files onto a phone costs, counted in device round
//! trips: each one is a socket to the device (a sync session or one shell
//! command), worth tens of milliseconds over USB, so the count per file is what
//! sets a small-file copy's speed (`crates/cmdr-adb/DETAILS.md` § "Known gaps").
//!
//! The regression cell counts; the bench cell times the same copy against a fake
//! that waits a modeled round trip before every device service. The bench gates
//! on `CMDR_ADB_BENCH=1` rather than `#[ignore]`, like `cmdr-sftp`'s
//! `streams_bench.rs`, so a timing never runs as a CI gate:
//!
//! ```sh
//! CMDR_ADB_BENCH=1 cargo nextest run -p cmdr --no-capture adb_small_files_test::bench
//! ```

#![allow(
    clippy::print_stdout,
    reason = "the bench cell reports its measurement; run it with --no-capture"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cmdr_adb::testing::{FakeTree, split_argv};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::staging::STAGING_TEMP_MARKER;

use super::adb_transfer_test::{dialed_phone_with, registered_local};
use super::network_transfer_test_support::{assert_no_staging_litter, read_all, start_copy_by_id};
use cmdr_operations::VolumeCopyConfig;

/// The issue's shape (200 files of 20 KB), scaled down for the unit lane.
const FILE_COUNT: usize = 20;
const FILE_BYTES: usize = 20_000;

/// Device round trips one small file may cost on its way onto a phone: the
/// engine's probe of the name, the `SEND` session, the two listing patches'
/// stats (the temp appearing, the rename), the landing's stat of the free name,
/// its `mv`, and the engine's read-back of the landed file. Was nine before
/// #339: the writer's own staging `mv` and the rename's stat of its source.
const ROUND_TRIPS_PER_FILE: usize = 7;

/// What the copy costs once, whatever its size: the pre-flight's `df` and
/// `test -w`, and the stats of the destination folder.
const ROUND_TRIPS_PER_COPY: usize = 10;

/// A backstop for the whole copy, far above what even the bench's modeled
/// round trips take.
const COPY_BUDGET: Duration = Duration::from_secs(120);

/// The requests that reach the DEVICE, each its own socket: a sync session or
/// one shell command. Host-only requests (`host:transport:…`, features) stay
/// on the loopback server and cost nothing comparable.
fn device_round_trips(requests: &[String]) -> usize {
    requests
        .iter()
        .filter(|r| *r == "sync:" || r.starts_with("shell,v2,raw:"))
        .count()
}

/// The same requests by kind (`sync`, or the shell verb), for a report.
fn by_kind(requests: &[String]) -> BTreeMap<String, usize> {
    let mut kinds = BTreeMap::new();
    for request in requests {
        let kind = if request == "sync:" {
            "sync".to_string()
        } else if let Some(command) = request.strip_prefix("shell,v2,raw:") {
            split_argv(command).into_iter().next().unwrap_or_default()
        } else {
            continue;
        };
        *kinds.entry(kind).or_insert(0) += 1;
    }
    kinds
}

fn moves(requests: &[String]) -> Vec<Vec<String>> {
    requests
        .iter()
        .filter_map(|r| r.strip_prefix("shell,v2,raw:"))
        .map(split_argv)
        .filter(|argv| argv.first().is_some_and(|verb| verb == "mv"))
        .collect()
}

/// Copies [`FILE_COUNT`] small files onto a fresh fake phone and hands back the
/// requests the copy made and how long it took.
async fn copy_small_files(serial: &'static str, what: &str, round_trip: Duration) -> (Vec<String>, Duration) {
    let phone = dialed_phone_with(serial, what, FakeTree::new()).await;
    let local = registered_local(what);
    let names: Vec<PathBuf> = (0..FILE_COUNT)
        .map(|i| PathBuf::from(format!("small_{i:03}.bin")))
        .collect();
    for name in &names {
        std::fs::write(local.dir.join(name), vec![b'x'; FILE_BYTES]).expect("seeding a small file");
    }
    phone.fake.set_device_round_trip(round_trip);
    let before = phone.fake.requests().len();
    let started = Instant::now();
    let running = start_copy_by_id(
        what,
        local.volume_id.clone(),
        names.clone(),
        phone.volume_id.clone(),
        phone.dir.to_string_lossy().into_owned(),
        VolumeCopyConfig::default(),
    )
    .await;
    // ❗ Not `settle()`: its budget is sized for a loopback fake answering at
    // once, and the bench's modeled round trips take seconds.
    crate::test_support::wait_until_async(COPY_BUDGET, "the small-file copy to settle", || {
        !running.events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    let took = started.elapsed();
    running.assert_no_errors();
    phone.fake.set_device_round_trip(Duration::ZERO);
    let requests = phone.fake.requests()[before..].to_vec();

    for name in &names {
        let landed = read_all(phone.volume.as_ref(), &phone.dir.join(name)).await;
        assert_eq!(landed.len(), FILE_BYTES, "{name:?} must land whole");
    }
    assert_no_staging_litter(phone.volume.as_ref(), &phone.dir, what).await;
    (requests, took)
}

/// ❗ #339: the engine stages every upload on its own `.cmdr-tmp-*`, so the
/// phone's writer must stream straight onto it. Staging it AGAIN cost a second
/// `mv` and a second name per file, about 135 ms each on a Pixel.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_small_files_onto_a_phone_land_with_one_move_each() {
    let (requests, _) = copy_small_files("R58M-Small-Files", "small-files", Duration::ZERO).await;

    let moves = moves(&requests);
    let first_moves = &moves[..moves.len().min(3)];
    assert_eq!(
        moves.len(),
        FILE_COUNT,
        "one `mv` per file, the engine's landing; the first moves: {first_moves:?}"
    );
    for argv in &moves {
        let from = &argv[argv.len() - 2];
        assert_eq!(
            from.matches(STAGING_TEMP_MARKER).count(),
            1,
            "only the engine's temp is ever moved, never a temp of a temp; {argv:?}"
        );
    }
    let budget = ROUND_TRIPS_PER_FILE * FILE_COUNT + ROUND_TRIPS_PER_COPY;
    let spent = device_round_trips(&requests);
    assert!(
        spent <= budget,
        "{FILE_COUNT} small files may cost {budget} device round trips; this copy took {spent}: {:?}",
        by_kind(&requests)
    );
}

/// The wall time of the same copy over a modeled USB round trip: 30 ms per
/// device service unless `CMDR_ADB_BENCH_RTT_MS` says otherwise.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bench_small_files_onto_a_phone_over_a_modeled_round_trip() {
    if std::env::var("CMDR_ADB_BENCH").as_deref() != Ok("1") {
        return;
    }
    let rtt_ms = std::env::var("CMDR_ADB_BENCH_RTT_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .unwrap_or(30);
    let (requests, took) = copy_small_files("R58M-Small-Bench", "small-bench", Duration::from_millis(rtt_ms)).await;
    let per_file_ms = took.as_secs_f64() * 1000.0 / FILE_COUNT as f64;
    println!(
        // allowed-pluralize-noun: FILE_COUNT is a constant well above one
        "{FILE_COUNT} files of {FILE_BYTES} B, {rtt_ms} ms per device service: {:.0} ms total, {per_file_ms:.1} ms per file, {:.2} device round trips per file {:?}",
        took.as_secs_f64() * 1000.0,
        device_round_trips(&requests) as f64 / FILE_COUNT as f64,
        by_kind(&requests),
    );
}
