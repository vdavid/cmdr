//! What the read window is actually worth, measured rather than assumed.
//!
//! ❗ **These cells are deliberately NOT `#[ignore]`d.** The integration lane runs
//! `--run-ignored only` over this whole package, so an ignored cell here runs in
//! CI by construction — and a throughput ratio measured under runner contention
//! is a flake that gets a gate lowered until it means nothing. They gate on
//! `CMDR_SFTP_BENCH=1` instead and skip in every other run.
//!
//! ```sh
//! ./apps/desktop/test/sftp-servers/start.sh bench
//! docker exec sftp-fixture-sftp-fixture-bench-1 tc qdisc add dev eth0 root netem delay 50ms
//! CMDR_SFTP_BENCH=1 cargo nextest run -p cmdr-sftp --no-capture streams_bench
//! ```
//!
//! ⚠️ The 50 ms column is the SHAPE of a curve, not absolute truth: Docker plus
//! `netem` carried ±30% run-to-run spread in the crate evaluation, and the same
//! caution governs everything measured here. Which is why the one assertion in
//! this file compares serial against windowed **in the same run on the same
//! server** and asks only for 4×, well under the ~10× the shape shows.
//!
//! The numbers these produced, and the depth they set: `DETAILS.md` § "The read
//! window".

#![allow(
    clippy::print_stdout,
    reason = "a measurement harness reports its measurements; run it with --no-capture"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::{READ_WINDOW_DEPTH, SftpVolume};
use crate::volume::testing::*;

/// The bench server's export, seeded big enough for a throughput number to mean
/// something (`LARGE_MB` in the compose file).
const BENCH_FILE: &str = "large.bin";

/// The depths the curve walks. 32 is where the single-stream evaluation peaked;
/// the point of the curve is that four streams sharing one channel window don't.
const DEPTHS: [usize; 6] = [1, 2, 4, 8, 16, 32];

/// The widths the curve walks: one stream is a single big copy, four is this
/// backend's `max_concurrent_ops`. ❗ Both matter, and they pull in opposite
/// directions — what saturates one stream is four times as much outstanding data
/// when four run.
const WIDTHS: [usize; 2] = [1, 4];

/// How much of the bench file each stream reads before it stops.
///
/// Long enough that the window reaches steady state and the open's two round
/// trips are noise; short enough that walking six depths four streams wide over a
/// 50 ms link is a coffee-length wait rather than an afternoon.
const BENCH_BYTES: u64 = 32 * 1024 * 1024;

/// Whether this run was asked for a measurement.
fn measuring() -> bool {
    std::env::var("CMDR_SFTP_BENCH").is_ok_and(|value| value == "1")
}

/// A volume on the bench server, which nothing in CI brings up, with its
/// connection already warm.
///
/// ❗ The warm-up is not politeness. TCP's congestion window starts small, and
/// over a 50 ms link it takes a few megabytes to open; measuring a 32 MiB read on
/// a cold connection measures the ramp as much as the window, and that showed up
/// as a two-to-one spread between otherwise identical runs.
async fn bench_volume() -> SftpVolume {
    let params = fixture_params("BENCH", 12491);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    read_prefix(&volume, READ_WINDOW_DEPTH).await;
    volume
}

/// Streams `BENCH_BYTES` of the bench file at `depth`, returning what it read.
///
/// Stops by dropping the stream, which is also the cancellation path — so a
/// measurement run exercises it a few hundred times for free.
async fn read_prefix(volume: &SftpVolume, depth: usize) -> u64 {
    let mut stream = volume
        .open_read_stream_impl(std::path::Path::new(BENCH_FILE), depth)
        .await
        .expect("the bench server exports large.bin");
    assert!(
        cmdr_fs::volume::VolumeReadStream::total_size(&stream) >= BENCH_BYTES,
        "the bench server's large.bin is smaller than the measurement wants; raise LARGE_MB"
    );
    let mut total = 0u64;
    while total < BENCH_BYTES {
        let Some(chunk) = cmdr_fs::volume::VolumeReadStream::next_chunk(&mut stream).await else {
            break;
        };
        total += chunk.expect("a bench read never fails").len() as u64;
    }
    total
}

/// `width` reads of the bench file at once, as one aggregate rate.
async fn read_concurrently(volume: &SftpVolume, depth: usize, width: usize) -> (u64, Duration) {
    let started = Instant::now();
    let mut streams = Vec::new();
    for _ in 0..width {
        streams.push(read_prefix(volume, depth));
    }
    let bytes: u64 = futures_util::future::join_all(streams).await.iter().sum();
    (bytes, started.elapsed())
}

/// Megabytes per second, in the unit the evaluation note reports.
fn rate(bytes: u64, elapsed: Duration) -> f64 {
    bytes as f64 / 1_000_000.0 / elapsed.as_secs_f64()
}

/// This process's resident set, in MiB.
///
/// `ps` rather than a crate: one number, no dependency, and the same number
/// Activity Monitor shows.
fn resident_mib() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps is on every machine this runs on");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .expect("ps prints the resident set in KiB")
        / 1024.0
}

#[tokio::test(flavor = "multi_thread")]
async fn a_windowed_read_beats_a_serial_one_by_at_least_four_times() {
    if !measuring() {
        return;
    }
    let volume = bench_volume().await;

    // Same file, same server, same run: the only thing that differs between the
    // two numbers is the depth, which is what makes a ratio meaningful where an
    // absolute wouldn't be.
    let started = Instant::now();
    let serial_bytes = read_prefix(&volume, 1).await;
    let serial = rate(serial_bytes, started.elapsed());

    let started = Instant::now();
    let windowed_bytes = read_prefix(&volume, READ_WINDOW_DEPTH).await;
    let windowed = rate(windowed_bytes, started.elapsed());

    println!("serial (depth 1):            {serial:.1} MB/s, bytes read: {serial_bytes}");
    println!("windowed (depth {READ_WINDOW_DEPTH}):          {windowed:.1} MB/s, bytes read: {windowed_bytes}");
    println!("ratio:                       {:.1}x", windowed / serial);

    assert_eq!(serial_bytes, windowed_bytes, "both paths must read the same file whole");
    assert!(
        windowed >= serial * 4.0,
        "the window must be worth at least 4x a serial read; got {windowed:.1} MB/s against {serial:.1} MB/s"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_depth_curve_one_stream_wide_and_four() {
    if !measuring() {
        return;
    }
    let volume = bench_volume().await;

    println!("       aggregate MB/s, one SSH channel");
    println!("depth   1 stream   4 streams   requests in flight (1 / 4)");
    for depth in DEPTHS {
        let mut rates = Vec::new();
        for width in WIDTHS {
            let (bytes, elapsed) = read_concurrently(&volume, depth, width).await;
            rates.push(rate(bytes, elapsed));
        }
        println!(
            "{depth:>5}  {:>9.1}  {:>10.1}   {:>3} / {:>3}",
            rates[0],
            rates[1],
            depth,
            depth * 4
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn peak_resident_memory_with_the_channel_window_open() {
    if !measuring() {
        return;
    }
    // Principle 5, as a number: a channel window is per volume and a reassembly
    // buffer is per stream, so eight mounted servers each running four streams is
    // the shape that has to stay affordable.
    const VOLUMES: usize = 8;

    let baseline = resident_mib();
    let mut volumes = Vec::new();
    for _ in 0..VOLUMES {
        volumes.push(bench_volume().await);
    }
    let connected = resident_mib();

    // Every volume streaming at once, which is the worst honest case: eight
    // mounted servers each running its own four-wide window. A sampler watches
    // the resident set while they do, because the peak is somewhere in the middle
    // rather than at either end.
    let watching = Arc::new(AtomicUsize::new(1));
    let peak_kib = Arc::new(AtomicUsize::new(0));
    let sampler = tokio::spawn({
        let watching = Arc::clone(&watching);
        let peak_kib = Arc::clone(&peak_kib);
        async move {
            while watching.load(Ordering::Relaxed) == 1 {
                peak_kib.fetch_max((resident_mib() * 1024.0) as usize, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    });

    let mut streams = Vec::new();
    for volume in &volumes {
        streams.push(read_concurrently(volume, READ_WINDOW_DEPTH, 4));
    }
    futures_util::future::join_all(streams).await;
    watching.store(0, Ordering::Relaxed);
    sampler.await.expect("the sampler only ever returns");
    let peak = peak_kib.load(Ordering::Relaxed) as f64 / 1024.0;

    println!("resident, no volumes:        {baseline:.1} MiB");
    println!("resident, connected volumes ({VOLUMES}): {connected:.1} MiB");
    println!("resident, peak while streaming: {peak:.1} MiB");
    println!(
        "cost per idle volume:        {:.1} MiB",
        (connected - baseline) / VOLUMES as f64
    );
}
