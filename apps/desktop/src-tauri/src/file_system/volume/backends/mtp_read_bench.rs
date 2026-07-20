//! Opt-in hardware benchmark for `MtpVolume::read_range` (Cmdr's own path).
//!
//! Ignored by default; needs a real MTP device plugged in. Run it with the
//! device's USB serial in `CMDR_MTP_BENCH_SERIAL` so it never grabs "the first
//! device" (another phone may be attached and owned by someone else):
//!
//! ```text
//! CMDR_MTP_BENCH_SERIAL=46061FDAS000A4 \
//!   cargo test -p cmdr --lib mtp_read_range_hardware_bench -- --ignored --nocapture
//! ```
//!
//! Methodology (mirrors `docs/notes/`-style round-robin benches): warmup
//! iterations discarded, medians not means, and every iteration reads a fresh
//! disjoint offset so the device can never serve a block one phase already
//! warmed.

#![allow(
    clippy::print_stdout,
    reason = "a benchmark's whole product is the numbers it prints, and `cargo test --nocapture` is where they have to land: the fern logger isn't wired up in a unit-test binary, so `log::info!` would swallow them. Scoped to this hardware-only, `#[ignore]`d module."
)]

use super::MtpVolume;
use super::Volume;
use crate::mtp::connection::connection_manager;
use std::path::Path;
use std::time::{Duration, Instant};

/// rc-zip's `EntryFsm` buffer size: the read size the extraction loop issues.
const READ_LEN: usize = 256 * 1024;
const ITERS: usize = 30;
const WARMUP: usize = 3;
/// Only files at least this big give enough disjoint offsets to walk.
const MIN_TARGET_BYTES: u64 = 50 << 20;

fn median_ms(v: &mut [Duration]) -> f64 {
    v.sort_unstable();
    v[v.len() / 2].as_secs_f64() * 1000.0
}

fn p90_ms(v: &mut [Duration]) -> f64 {
    v.sort_unstable();
    v[(v.len() * 9 / 10).min(v.len() - 1)].as_secs_f64() * 1000.0
}

/// Walks the storage root two levels deep for a file of at least
/// `MIN_TARGET_BYTES`, listing as it goes so `resolve_path_to_handle`
/// (cache-only) can resolve the winner later.
async fn find_target(device_id: &str, storage_id: u32) -> Option<(String, u64)> {
    let roots = connection_manager()
        .list_directory(device_id, storage_id, "/")
        .await
        .ok()?;
    for dir in roots.iter().filter(|e| e.is_directory) {
        let Ok(kids) = connection_manager()
            .list_directory(device_id, storage_id, &dir.name)
            .await
        else {
            continue;
        };
        if let Some(f) = kids
            .iter()
            .find(|e| !e.is_directory && e.size.unwrap_or(0) >= MIN_TARGET_BYTES)
        {
            return Some((format!("{}/{}", dir.name, f.name), f.size.unwrap_or(0)));
        }
    }
    None
}

#[tokio::test]
#[ignore = "needs a real MTP device; set CMDR_MTP_BENCH_SERIAL"]
async fn mtp_read_range_hardware_bench() {
    let Ok(serial) = std::env::var("CMDR_MTP_BENCH_SERIAL") else {
        panic!("set CMDR_MTP_BENCH_SERIAL to the USB serial of the device to benchmark");
    };

    #[cfg(target_os = "macos")]
    let _ = crate::mtp::macos_workaround::suppress_ptpcamerad();

    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.serial_number.as_deref() == Some(serial.as_str()))
        .map(|d| d.id)
        .unwrap_or_else(|| panic!("no MTP device with serial {serial} is attached"));

    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("connect to the benchmark device");
    let storage = info.storages.first().expect("a storage").clone();

    let (rel_path, size) = find_target(&device_id, storage.id)
        .await
        .expect("no file >= 50 MB found two levels deep; put one on the device first");
    println!("target: {rel_path} ({:.1} MB)", size as f64 / 1e6);

    let volume = MtpVolume::new(&device_id, storage.id, &storage.name);
    let path = Path::new(&rel_path);

    // Disjoint, forward-walking offsets: no iteration reads a block a previous
    // one already pulled into the device's cache.
    let span = size - READ_LEN as u64;
    let step = span / (ITERS + WARMUP) as u64;

    let mut samples = Vec::with_capacity(ITERS);
    for i in 0..(ITERS + WARMUP) {
        let offset = step * i as u64;
        let start = Instant::now();
        let bytes = volume
            .read_range(path, offset, READ_LEN)
            .await
            .expect("read_range should succeed");
        let elapsed = start.elapsed();
        assert_eq!(bytes.len(), READ_LEN, "short read at offset {offset}");
        if i >= WARMUP {
            samples.push(elapsed);
        }
    }

    let median = median_ms(&mut samples);
    let p90 = p90_ms(&mut samples);
    println!(
        "MtpVolume::read_range {} KiB: median {median:.2}ms  p90 {p90:.2}ms",
        READ_LEN / 1024
    );
    println!(
        "effective throughput: {:.1} MB/s",
        READ_LEN as f64 / 1e6 / (median / 1000.0)
    );

    connection_manager()
        .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .ok();
    #[cfg(target_os = "macos")]
    let _ = crate::mtp::macos_workaround::restore_ptpcamerad();
}
