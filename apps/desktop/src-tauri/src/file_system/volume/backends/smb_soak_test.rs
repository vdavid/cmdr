//! Soak test for the SMB backend (requires Docker SMB containers).
//!
//! `#[ignore]`d by default. Hammers the SMB->Local copy pipeline thousands
//! of times to catch credit drift, FD leaks, and memory growth. Run with
//! `CMDR_SOAK_ITERATIONS` or `CMDR_SOAK_DURATION_SECS` set. Declared as a
//! `#[cfg(test)]` submodule of `smb`; shared helpers come from
//! `super::smb_test_support`.
//!
//! Run a manual soak alone: it shares the machine-wide `smb-consumer` stack
//! with any other live SMB work (a sibling worktree's integration suite, an
//! E2E run), and a concurrent load that *ramps* mid-soak can inflate the
//! drift ratio below into a false failure. The drift assertion is relative
//! (last-10% vs first-10%), so a *uniform* concurrent slowdown won't trip it,
//! but a load that grows over the soak's lifetime can.

use super::smb_test_support::*;
use super::*;
use crate::file_system::volume::smb_volume_id;

// ── Soak test: repeated SMB→Local copy pipeline ────────────────
//
// Catches accumulating bugs that short tests miss: credit drift,
// file-descriptor leaks, memory growth, per-iteration slowdown. The short
// integration tests above verify single-operation correctness; this one
// hammers the same pipeline thousands of times and watches for drift.
//
// Modes (pick via env):
// - Default (no env):           `CMDR_SOAK_ITERATIONS=100` (≈1–2 min). Sanity-check run for gross
//   leaks.
// - Explicit iteration count:    `CMDR_SOAK_ITERATIONS=3000 ...`
// - Time-bounded:                `CMDR_SOAK_DURATION_SECS=1800 ...` (30 min)
//
// Uses `smb-consumer-auth` (port 10481, share `private`, `testuser` /
// `testpass`) because it permits writes. Never runs by default; gated
// on `#[ignore]`.

/// `getrusage(RUSAGE_SELF).ru_maxrss`: peak resident set size. On macOS the
/// value is in bytes; on Linux it's in kilobytes. Returns megabytes.
///
/// Why peak-RSS not current-RSS: macOS/Linux both surface `ru_maxrss` from
/// `getrusage(2)` without needing extra deps (`sysinfo` with `process`
/// feature, `proc_pidinfo` FFI, or `/proc/self/status`). For a leak hunt
/// peak RSS is actually the metric we want; current RSS oscillates with
/// glibc/jemalloc GC, peak is monotonic and only grows when we genuinely
/// retain more bytes.
fn process_peak_rss_mb() -> f64 {
    #[cfg(unix)]
    {
        // SAFETY: (test) `rusage` is a plain C struct of integers, so all-zeroes is a valid
        // initial bit pattern; `getrusage` overwrites the fields we read anyway.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        // SAFETY: (test) `RUSAGE_SELF` is a valid plain-integer who-selector and `&mut usage` is a
        // live, fully-sized `rusage` out-param the kernel fills in. We check the return is 0 before
        // reading `usage.ru_maxrss`.
        let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        if rc != 0 {
            return 0.0;
        }
        let ru_maxrss = usage.ru_maxrss as f64;
        #[cfg(target_os = "macos")]
        {
            // bytes → MB
            ru_maxrss / (1024.0 * 1024.0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            // Linux: kilobytes → MB
            ru_maxrss / 1024.0
        }
    }
    #[cfg(not(unix))]
    {
        0.0
    }
}

/// Counts this process's open file descriptors. Both macOS and Linux
/// expose `/dev/fd/` as a directory listing the current process's open
/// descriptors (on Linux it's actually a symlink to `/proc/self/fd/`).
/// A short-lived extra FD is opened to read the directory; subtract 1
/// so the returned number reflects the steady-state count before the
/// measurement started.
fn open_fd_count() -> usize {
    match std::fs::read_dir("/dev/fd") {
        Ok(iter) => iter.count().saturating_sub(1),
        Err(_) => 0,
    }
}

/// Snapshots SMB credit counters inside the `SmbVolume`'s `SmbClient`. Used
/// between iterations to spot credit drift (a leak bleeds credits over
/// time; exhaustion would stall future reads). Returns `None` if the
/// session isn't available.
async fn smb_credits_snapshot(vol: &SmbVolume) -> Option<u16> {
    let guard = vol.client.lock().await;
    guard.as_ref().map(|c| c.credits())
}

/// Connects to the `smb-consumer-auth` Docker container (share `private`,
/// writable, credentials `testuser` / `testpass`). Default port 10481
/// matches smb2's auth test container; override via
/// `SMB_CONSUMER_AUTH_PORT`.
async fn make_docker_auth_volume() -> SmbVolume {
    let port: u16 = std::env::var("SMB_CONSUMER_AUTH_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10481);
    let volume_id = smb_volume_id("127.0.0.1", port, "private");
    let params = SmbConnectionParams::new("127.0.0.1", "private", port, Some("testuser"), Some("testpass"));
    connect_smb_volume("private", "/tmp/smb-soak-mount", &volume_id, params)
        .await
        .unwrap_or_else(|e| panic!("Failed to connect to Docker SMB auth container at 127.0.0.1:{port} ({e:?})"))
}

#[tokio::test]
#[ignore = "Soak test: requires Docker SMB containers. Run with CMDR_SOAK_ITERATIONS or CMDR_SOAK_DURATION_SECS."]
async fn smb_soak_copy_loop() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };
    use std::time::{Duration, Instant};

    let _ = env_logger::try_init();

    // Deterministic per-index content: 10 KB of a repeated 32-byte
    // blake3-derived block. Same scheme as
    // `smb_integration_copy_100_unique_files_no_cross_contamination`.
    fn expected_content(index: usize) -> Vec<u8> {
        let mut seed = Vec::with_capacity(10 + 8);
        seed.extend_from_slice(b"cmdr-soak-");
        seed.extend_from_slice(&(index as u64).to_le_bytes());
        let block = *blake3::hash(&seed).as_bytes();
        let mut out = Vec::with_capacity(32 * 320);
        for _ in 0..320 {
            out.extend_from_slice(&block);
        }
        out
    }

    const FILE_COUNT: usize = 100;

    // Iteration budget. Duration takes priority if both are set; it's
    // the more useful knob for manual long-soak runs.
    let duration_budget: Option<Duration> = std::env::var("CMDR_SOAK_DURATION_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs);
    let iteration_budget: usize = std::env::var("CMDR_SOAK_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    // Fixture: 100 × 10 KB deterministic files, created once on the SMB
    // source. Re-used across every iteration (the loop only reads).
    let smb_vol = Arc::new(make_docker_auth_volume().await);
    let src_dir = test_dir_name();
    ensure_clean(&smb_vol, &src_dir).await;
    smb_vol.create_directory(Path::new(&src_dir)).await.unwrap();
    let vol: Arc<dyn Volume> = smb_vol.clone();

    let fixture_start = Instant::now();
    let mut source_paths: Vec<PathBuf> = Vec::with_capacity(FILE_COUNT);
    for i in 0..FILE_COUNT {
        let name = format!("f_{:03}.bin", i);
        let smb_path = format!("{}/{}", src_dir, name);
        vol.create_file(Path::new(&smb_path), &expected_content(i))
            .await
            .unwrap();
        source_paths.push(PathBuf::from(smb_path));
    }
    log::info!(
        "smb_soak_copy_loop: fixture setup ({} × 10 KB) took {:?}",
        FILE_COUNT,
        fixture_start.elapsed()
    );

    // Baseline snapshot before the loop so deltas are meaningful. Force
    // an initial RSS read so macOS's ru_maxrss is warmed up.
    let baseline_rss_mb = process_peak_rss_mb();
    let baseline_fds = open_fd_count();
    let baseline_credits = smb_credits_snapshot(&smb_vol).await;
    log::info!(
        "smb_soak_copy_loop: baseline: RSS {:.1} MB, FDs {}, credits {:?}",
        baseline_rss_mb,
        baseline_fds,
        baseline_credits,
    );

    let loop_start = Instant::now();

    // Per-iteration wall-clock for drift analysis. Pre-allocate when the
    // iteration count is bounded; grow dynamically when duration-bound.
    let mut per_iter_ms: Vec<f64> = match duration_budget {
        Some(_) => Vec::with_capacity(1024),
        None => Vec::with_capacity(iteration_budget),
    };
    let mut peak_rss_mb = baseline_rss_mb;
    let mut peak_fds = baseline_fds;
    let mut iter_errors: Vec<String> = Vec::new();
    let mut iter_idx: usize = 0;

    // Summary cadence: every 10% of the bound, or every 100 iterations
    // when duration-bound (whichever is reached first).
    let summary_every: usize = match duration_budget {
        Some(_) => 100,
        None => (iteration_budget / 10).max(1),
    };

    loop {
        // Stop condition: duration takes priority if set.
        match duration_budget {
            Some(d) => {
                if loop_start.elapsed() >= d {
                    break;
                }
            }
            None => {
                if iter_idx >= iteration_budget {
                    break;
                }
            }
        }

        // Fresh per-iteration destination. Tempdir drops at the end of
        // the block, so the only on-disk state between iterations is
        // the 100 source bytes on the SMB side.
        let local_dir = tempfile::TempDir::new().expect("create TempDir");
        let dest_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
            "dest",
            local_dir.path().to_path_buf(),
        ));
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
        let events = Arc::new(CollectorEventSink::new());
        let config = VolumeCopyConfig::default();

        let iter_start = Instant::now();
        let result = copy_volumes_with_progress(
            events.clone(),
            &format!("soak-iter-{iter_idx}"),
            &state,
            Arc::clone(&vol),
            &source_paths,
            Arc::clone(&dest_vol),
            Path::new("/"),
            &config,
        )
        .await;
        let iter_elapsed = iter_start.elapsed();
        per_iter_ms.push(iter_elapsed.as_secs_f64() * 1000.0);

        if let Err(e) = result {
            iter_errors.push(format!("iter {iter_idx}: copy failed: {e:?}"));
            break;
        }

        // Per-index blake3 verification. A byte-swap bug or a buffer
        // reuse between concurrent tasks flips the hash immediately.
        let mut mismatches = 0usize;
        for i in 0..FILE_COUNT {
            let name = format!("f_{:03}.bin", i);
            let dest_path = local_dir.path().join(&name);
            let actual_bytes = match std::fs::read(&dest_path) {
                Ok(b) => b,
                Err(e) => {
                    iter_errors.push(format!("iter {iter_idx}: read {name}: {e}"));
                    mismatches += 1;
                    continue;
                }
            };
            if hash_bytes(&actual_bytes) != hash_bytes(&expected_content(i)) {
                iter_errors.push(format!(
                    "iter {iter_idx}: {name} content mismatch (size={} expected={})",
                    actual_bytes.len(),
                    expected_content(i).len()
                ));
                mismatches += 1;
            }
        }
        if mismatches > 0 {
            break;
        }

        // Refresh peaks. Read the current per-iter resource sample and
        // track the high-water marks so the final assertions see the
        // worst case, not just the end state.
        let rss = process_peak_rss_mb();
        let fds = open_fd_count();
        if rss > peak_rss_mb {
            peak_rss_mb = rss;
        }
        if fds > peak_fds {
            peak_fds = fds;
        }

        // Cadence summary: recent window average + current deltas.
        iter_idx += 1;
        if iter_idx.is_multiple_of(summary_every) {
            let window_start = iter_idx.saturating_sub(summary_every);
            let window: &[f64] = &per_iter_ms[window_start..iter_idx];
            let window_avg = window.iter().sum::<f64>() / window.len() as f64;
            let credits = smb_credits_snapshot(&smb_vol).await;
            log::info!(
                "smb_soak_copy_loop: iter {} (window-avg {:.1} ms, RSS {:.1} MB, Δ {:+.1}, FDs {}, Δ {:+}, credits {:?})",
                iter_idx,
                window_avg,
                rss,
                rss - baseline_rss_mb,
                fds,
                fds as i64 - baseline_fds as i64,
                credits,
            );
        }

        // Dest tempdir drops here, so on-disk FD count on the local
        // side lands back at baseline before the next iteration.
        drop(dest_vol);
    }

    let total_elapsed = loop_start.elapsed();
    let total_iters = per_iter_ms.len();
    let final_rss_mb = process_peak_rss_mb();
    let final_fds = open_fd_count();
    let final_credits = smb_credits_snapshot(&smb_vol).await;

    // Cleanup SMB source before any assertion, so a failed assertion
    // doesn't leave debris in the container.
    ensure_clean(&smb_vol, &src_dir).await;

    // Require at least 20 iterations to compute a meaningful drift
    // ratio (10%-window math needs two non-trivial samples).
    if total_iters < 20 {
        panic!(
            "soak ran only {}; need at least 20 to compute drift (set CMDR_SOAK_ITERATIONS=100 minimum)",
            crate::pluralize::pluralize(total_iters as u64, "iteration")
        );
    }

    // Drift ratio: average of the last 10% of iterations vs. the first
    // 10%. A clean pipeline should sit near 1.0; a slowdown above 1.20
    // fails the test.
    let window = (total_iters / 10).max(1);
    let first_avg = per_iter_ms[..window].iter().sum::<f64>() / window as f64;
    let last_avg = per_iter_ms[total_iters - window..].iter().sum::<f64>() / window as f64;
    let drift = last_avg / first_avg;

    log::info!(
        "smb_soak_copy_loop: DONE: {} iters in {:?} ({:.1} ms/iter avg)",
        total_iters,
        total_elapsed,
        per_iter_ms.iter().sum::<f64>() / total_iters as f64
    );
    log::info!(
        "smb_soak_copy_loop: drift first10%={:.1} ms last10%={:.1} ms ratio={:.3}",
        first_avg,
        last_avg,
        drift
    );
    log::info!(
        "smb_soak_copy_loop: RSS baseline {:.1} MB → peak {:.1} MB → final {:.1} MB (Δ peak {:+.1} MB)",
        baseline_rss_mb,
        peak_rss_mb,
        final_rss_mb,
        peak_rss_mb - baseline_rss_mb,
    );
    log::info!(
        "smb_soak_copy_loop: FDs baseline {} → peak {} → final {} (Δ final {:+})",
        baseline_fds,
        peak_fds,
        final_fds,
        final_fds as i64 - baseline_fds as i64,
    );
    log::info!(
        "smb_soak_copy_loop: credits baseline {:?} → final {:?}",
        baseline_credits,
        final_credits
    );

    // Hard failures: any iteration error, drift, memory peak, FD leak.
    assert!(
        iter_errors.is_empty(),
        "{} iteration error(s):\n  - {}",
        iter_errors.len(),
        iter_errors.join("\n  - ")
    );
    assert!(
        drift < 1.20,
        "iteration-wall-clock drift {:.3}× (first10%={:.1} ms, last10%={:.1} ms) exceeds 1.20×",
        drift,
        first_avg,
        last_avg
    );
    assert!(
        peak_rss_mb - baseline_rss_mb < 100.0,
        "peak RSS grew by {:.1} MB (baseline {:.1} MB, peak {:.1} MB); exceeds 100 MB ceiling",
        peak_rss_mb - baseline_rss_mb,
        baseline_rss_mb,
        peak_rss_mb
    );
    let fd_delta = final_fds as i64 - baseline_fds as i64;
    assert!(
        fd_delta < 5,
        "final FD count grew by {} (baseline {}, final {}); exceeds 5 FD ceiling (suggests leak)",
        fd_delta,
        baseline_fds,
        final_fds
    );
}
