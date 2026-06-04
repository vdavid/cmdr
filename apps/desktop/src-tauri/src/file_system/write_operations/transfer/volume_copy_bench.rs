//! Network-gated SMB→local benchmark for `volume_copy`, split out of
//! `volume_copy_tests.rs`. The single test here is `#[ignore]`d and needs a
//! reachable QNAP NAS plus the `SMB2_TEST_NAS_PASSWORD` env var, so it never
//! runs in CI or a normal `cargo nextest run` — only on demand with
//! `--ignored`. See the test's own doc comment for the run command.

use super::*;
use crate::file_system::write_operations::types::ConflictResolution;

// ── Phase 4 baseline bench (real QNAP NAS) ────────────────────────
//
// Measures end-to-end wall-clock for copying 100 × 10 KB files from
// the QNAP `naspi` share to a local temp dir, through the real
// `copy_volumes_with_progress` code path. Requires:
//
// - QNAP reachable at 192.168.1.111 with the `naspi` share, user "david", password in
//   `SMB2_TEST_NAS_PASSWORD` env var.
// - 100 × 10 KB files pre-uploaded at `_test/bench_100tiny/f_000.bin` through `f_099.bin` (see
//   `smb2`'s `bench_100_tiny_files_seq_vs_parallel` (running that benchmark uploads them as a side
//   effect).
//
// Run with:
//   cd apps/desktop/src-tauri && cargo test --release \
//     --lib phase4_bench -- --ignored --nocapture --test-threads=1

#[tokio::test]
#[ignore = "Phase 4 baseline: requires QNAP at 192.168.1.111 and SMB2_TEST_NAS_PASSWORD env var"]
#[allow(
    clippy::print_stdout,
    clippy::needless_update,
    reason = "Bench test prints a timing report by design (run with --nocapture); the struct-update is intentional for future-proofing."
)]
async fn phase4_bench_baseline_smb_to_local_100_tiny_files() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::smb::{SmbConnectionParams, connect_smb_volume};
    use crate::file_system::volume::smb_volume_id;
    use crate::file_system::write_operations::types::CollectorEventSink;

    const FILE_COUNT: usize = 100;

    // Load password from env (or fall back to the smb2 crate's .env file).
    let password = nas_password_from_env()
        .expect("SMB2_TEST_NAS_PASSWORD not set. Copy smb2/.env.example to smb2/.env, or set in your shell.");

    // Host is configurable so the bench can run via Tailscale
    // (`SMB2_TEST_NAS_HOST=100.127.48.122`) from a different subnet.
    let host = std::env::var("SMB2_TEST_NAS_HOST").unwrap_or_else(|_| "192.168.1.111".to_string());

    // ── Set up source (SMB) ───────────────────────────────────────
    let smb_setup_start = Instant::now();
    let smb_volume_id = smb_volume_id(&host, 445, "naspi");
    let params = SmbConnectionParams::new(&host, "naspi", 445, Some("david"), Some(password.as_str()));
    let smb_volume = connect_smb_volume("naspi", "/Volumes/naspi-bench-p4", &smb_volume_id, params)
        .await
        .expect("SMB connect failed (is QNAP at 192.168.1.111 reachable?)");
    let smb_setup = smb_setup_start.elapsed();

    // ── Set up destination (local temp dir) ───────────────────────
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let local_volume = Arc::new(LocalPosixVolume::new("bench-local", tmpdir.path().to_path_buf()));

    let source_volume: Arc<dyn Volume> = Arc::new(smb_volume);
    let source_paths: Vec<PathBuf> = (0..FILE_COUNT)
        .map(|i| PathBuf::from(format!("_test/bench_100tiny/f_{:03}.bin", i)))
        .collect();

    // ── Run the copy through the real pipeline ────────────────────
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        progress_interval_ms: 200,
        conflict_resolution: ConflictResolution::Overwrite,
        max_conflicts_to_show: 0,
        preview_id: None,
        ..Default::default()
    };

    let copy_start = Instant::now();
    let result = copy_volumes_with_progress(
        events.clone(),
        "phase4-bench",
        &state,
        Arc::clone(&source_volume),
        &source_paths,
        Arc::clone(&local_volume) as Arc<dyn Volume>,
        Path::new("/"),
        &config,
    )
    .await;
    let copy_elapsed = copy_start.elapsed();

    result.expect("copy pipeline failed");

    // Verify all 100 files landed at the destination.
    for i in 0..FILE_COUNT {
        let p = tmpdir.path().join(format!("f_{:03}.bin", i));
        let md = std::fs::metadata(&p).unwrap_or_else(|e| panic!("missing dest file {p:?}: {e:?}"));
        assert_eq!(md.len(), 10 * 1024, "wrong size for {p:?}");
    }

    let fps = FILE_COUNT as f64 / copy_elapsed.as_secs_f64();
    println!();
    println!("─────────────────────────────────────────────────────────");
    println!("Phase 4 baseline: 100 × 10 KB files, QNAP → local (cmdr pipeline)");
    println!("─────────────────────────────────────────────────────────");
    println!("SMB connect + session setup: {:.2?}", smb_setup);
    println!(
        "Copy wall-clock:             {:.2?}  =  {:.1} files/sec",
        copy_elapsed, fps
    );
    println!("─────────────────────────────────────────────────────────");
}

/// Read the NAS test password from env, falling back to `../../smb2/.env`.
fn nas_password_from_env() -> Option<String> {
    if let Ok(p) = std::env::var("SMB2_TEST_NAS_PASSWORD") {
        return Some(p);
    }
    // Fall back: read from the smb2 crate's .env if present.
    let smb2_env_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // src-tauri -> desktop
        .and_then(|p| p.parent()) // desktop -> apps
        .and_then(|p| p.parent()) // apps -> cmdr
        .and_then(|p| p.parent()) // cmdr -> projects-git/vdavid
        .map(|p| p.join("smb2").join(".env"))?;
    let contents = std::fs::read_to_string(&smb2_env_path).ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("SMB2_TEST_NAS_PASSWORD=") {
            let unquoted = rest.trim_matches('"').to_string();
            return Some(unquoted);
        }
    }
    None
}
