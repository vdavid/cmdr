//! Concurrency stress test for the SMB backend (requires Docker SMB containers).
//!
//! `#[ignore]`d by default. Hammers `SmbVolume::write_from_stream` with many
//! concurrent streaming writes (mixed `OverwriteSmaller`-skip + real copies)
//! to guard against the streaming-write deadlock fixed in smb2 0.9.0. Runs
//! against the `smb-consumer-maxreadsize` fixture so every write is forced
//! through the streaming-fallback (FileWriter) path. Carries its own
//! diagnostic machinery (`MutexCaptureLogger`) to dump the last mutex/recv
//! log lines on a hang. Declared as a `#[cfg(test)]` submodule of `smb`;
//! shared helpers come from `super::smb_test_support`. This pairs with
//! `smb_soak_test.rs`.

use super::smb_test_support::*;
use super::*;
use crate::file_system::volume::smb_volume_id;

/// Cross-task content integrity: 100 concurrent SMB → local copies, each file
/// with unique deterministic content. After the batch completes, every
/// destination's blake3 hash must match the hash of the source it claims to
/// come from: catches buffer reuse across tasks, wrong-buffer-to-wrong-path
/// routing, races in the `Arc<Mutex<Option<SmbClient>>>` +
/// `Arc<RwLock<Option<Arc<Tree>>>>` split-session (Fix 2), and
/// cross-MessageId wire demux mistakes on cloned `Connection`s.
///
/// Identical-content tests can't see any of these; every file would hash
/// the same, so a "swapped slice mid-file" or "task B's buffer landed under
/// task A's path" bug would pass trivially. Unique per-file content makes
/// any cross-contamination flip at least one destination's hash.
///
/// Runs the real copy pipeline (`copy_volumes_with_progress`, the same
/// function `copy_between_volumes` calls) so `FuturesUnordered` + Fix 2's
/// split session + Fix 3's compound fast-path + Fix 4's pipelined scan all
/// execute together, the way a user's "copy 100 files" action does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_100_unique_files_no_cross_contamination() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };
    use std::time::{Duration, Instant};

    // Content scheme: `blake3(b"cmdr-fix8-" || index_le) .as_bytes() repeated 320 times`
    // = 10_240 bytes per file, truly unique per index, every byte position varies
    // between files. Any cross-task slice swap (even a 32-byte block in the
    // middle of one file coming from a neighbor's buffer) flips blake3.
    // 10 KB keeps fixture setup cheap and stays inside the SMB compound
    // fast-path (Fix 3) so we're exercising it, not the streaming fallback.
    fn expected_content(index: usize) -> Vec<u8> {
        let mut seed = Vec::with_capacity(10 + 8);
        seed.extend_from_slice(b"cmdr-fix8-");
        seed.extend_from_slice(&(index as u64).to_le_bytes());
        let block = *blake3::hash(&seed).as_bytes(); // 32 bytes
        let mut out = Vec::with_capacity(32 * 320);
        for _ in 0..320 {
            out.extend_from_slice(&block);
        }
        out
    }

    const FILE_COUNT: usize = 100;

    // Hold the concrete `SmbVolume` for `ensure_clean` (which takes
    // `&SmbVolume`) and clone an `Arc<dyn Volume>` view of the same
    // session for the copy pipeline.
    let smb_vol = Arc::new(make_docker_volume().await);
    let src_dir = test_dir_name();
    ensure_clean(&smb_vol, &src_dir).await;
    smb_vol.create_directory(Path::new(&src_dir)).await.unwrap();
    let vol: Arc<dyn Volume> = smb_vol.clone();

    // Fixture: create 100 files on the SMB source, serially. Parallel
    // `create_file` on a single SMB session wouldn't speed this up
    // (creates are 1 RTT each), and keeping setup simple keeps any bug
    // the test catches unambiguously a read/copy-path bug, not a
    // write-path races-with-itself bug.
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
        "smb_integration_copy_100_unique_files: fixture setup took {:?}",
        fixture_start.elapsed()
    );

    // Destination: local TempDir wrapped in a LocalPosixVolume. We feed the
    // copy pipeline the same way production does (SMB volume → Local
    // volume → `copy_volumes_with_progress`). `dest_path` is "/" relative to
    // the local volume root (i.e. the TempDir itself).
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    let dest_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "dest",
        local_dir.path().to_path_buf(),
    ));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig::default();

    let copy_start = Instant::now();
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-100-unique",
        &state,
        Arc::clone(&vol),
        &source_paths,
        Arc::clone(&dest_vol),
        Path::new("/"),
        &config,
    )
    .await;
    log::info!(
        "smb_integration_copy_100_unique_files: copy pipeline took {:?}",
        copy_start.elapsed()
    );
    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // Count landed files: cheap aggregate sanity check before per-index
    // verification. A cross-contamination bug that swapped two destinations
    // would still show 100 files here, so this is not the real check.
    let entries = std::fs::read_dir(local_dir.path())
        .expect("read dest dir")
        .filter_map(|e| e.ok())
        .count();
    assert_eq!(entries, FILE_COUNT, "expected {} files at destination", FILE_COUNT);

    // Per-index integrity: for each source index, read its destination file
    // and compare blake3 against the expected hash derived from the same
    // index. Assert each one individually so a swap of two destinations
    // fails loudly with both offending indices, not a vague aggregate.
    let mut mismatches: Vec<String> = Vec::new();
    for i in 0..FILE_COUNT {
        let name = format!("f_{:03}.bin", i);
        let dest_path = local_dir.path().join(&name);
        let actual_bytes = match std::fs::read(&dest_path) {
            Ok(b) => b,
            Err(e) => {
                mismatches.push(format!("{}: couldn't read destination: {}", name, e));
                continue;
            }
        };
        let expected_bytes = expected_content(i);
        let expected_hash = hash_bytes(&expected_bytes);
        let actual_hash = hash_bytes(&actual_bytes);
        if actual_hash != expected_hash {
            // Find the first diff position and a small slice of context;
            // a 10 KB diff dump would drown the terminal on any failure.
            let first_diff = expected_bytes.iter().zip(actual_bytes.iter()).position(|(a, b)| a != b);
            let diff_detail = match first_diff {
                Some(pos) => {
                    let end_exp = pos.saturating_add(16).min(expected_bytes.len());
                    let end_act = pos.saturating_add(16).min(actual_bytes.len());
                    format!(
                        "first diff at byte {}: expected {:02x?}, got {:02x?}",
                        pos,
                        &expected_bytes[pos..end_exp],
                        &actual_bytes[pos..end_act]
                    )
                }
                None => {
                    // Same bytes but different length (hashes differ so
                    // there must be a difference somewhere).
                    format!(
                        "byte-for-byte equal in overlap but lengths differ: expected {}, got {}",
                        expected_bytes.len(),
                        actual_bytes.len()
                    )
                }
            };
            mismatches.push(format!(
                "{}: expected blake3 {} ({} bytes), got blake3 {} ({} bytes); {}",
                name,
                hex_of(&expected_hash),
                expected_bytes.len(),
                hex_of(&actual_hash),
                actual_bytes.len(),
                diff_detail,
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} destinations failed content check:\n  - {}",
        mismatches.len(),
        FILE_COUNT,
        mismatches.join("\n  - "),
    );

    // Cleanup the SMB source. The TempDir cleans itself on drop.
    ensure_clean(&smb_vol, &src_dir).await;
}

/// Hex formatter for blake3 hashes in failure messages. Avoids a hex-crate
/// dep just for test diagnostics.
fn hex_of(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Captures `client-mutex:` (cmdr) and `recv:` (smb2 receiver loop)
/// debug lines into bounded ring buffers so a hung test's panic message
/// can include the last ~30 lines from each stream. That's invaluable
/// for diagnosing a future regression. Installed via `log::set_logger`
/// once per process; subsequent installs are no-ops.
struct MutexCaptureLogger {
    mutex_lines: std::sync::Mutex<std::collections::VecDeque<String>>,
    recv_lines: std::sync::Mutex<std::collections::VecDeque<String>>,
}

impl log::Log for MutexCaptureLogger {
    fn enabled(&self, _md: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let msg = format!("{}", record.args());
        let target = record.target();
        // `client-mutex:` lines come from smb.rs via `log::debug!` with
        // the module-path target (`cmdr_lib::file_system::volume::smb`).
        // `recv:` lines come from the smb2 receiver loop with an `smb2::*`
        // target.
        // allowed-error-string-match: routes log records into ring buffers by our own `log::debug!` message-prefix convention (`client-mutex:` from this file, `recv:` from the smb2 crate's receiver loop). Not error/state classification; we own both prefixes and `cleanup_test_prefix` would notice drift. Pinned by `mutex_capture_logger_routes_known_prefixes`.
        if msg.starts_with("client-mutex:") {
            let mut q = self.mutex_lines.lock().unwrap();
            if q.len() >= 200 {
                q.pop_front();
            }
            q.push_back(format!("[{}] {}", target, msg));
            // allowed-error-string-match: same convention as the `client-mutex:` branch above — routes smb2 receiver-loop log records by message prefix, not error/state classification. Pinned by `mutex_capture_logger_routes_known_prefixes`.
        } else if msg.starts_with("recv:") || (target.starts_with("smb2") && msg.contains("recv")) {
            let mut q = self.recv_lines.lock().unwrap();
            if q.len() >= 200 {
                q.pop_front();
            }
            q.push_back(format!("[{}] {}", target, msg));
        }
        // The captured ring buffers are the diagnostic. We deliberately
        // skip mirroring to stderr: `eprintln!` is denied crate-wide,
        // and re-emitting through `log::*` would recurse into this same
        // logger (and the mutex above) on every call.
    }
    fn flush(&self) {}
}

static MUTEX_CAPTURE_LOGGER: OnceLock<&'static MutexCaptureLogger> = OnceLock::new();

fn install_mutex_capture_logger() -> &'static MutexCaptureLogger {
    if let Some(l) = MUTEX_CAPTURE_LOGGER.get() {
        return l;
    }
    let leaked: &'static MutexCaptureLogger = Box::leak(Box::new(MutexCaptureLogger {
        mutex_lines: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(200)),
        recv_lines: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(200)),
    }));
    // Best-effort: if another logger is already installed, ignore.
    let _ = log::set_logger(leaked);
    log::set_max_level(log::LevelFilter::Debug);
    let _ = MUTEX_CAPTURE_LOGGER.set(leaked);
    leaked
}

/// Connects to a Docker SMB fixture's `public` share at `127.0.0.1:port`
/// as guest. `mount_label` becomes the synthetic mount path
/// (`/Volumes/<label>`); no real OS mount is needed because the test
/// only drives the smb2 path.
async fn connect_docker_smb_volume(port: u16, mount_label: &str) -> SmbVolume {
    let mount_path = format!("/Volumes/{mount_label}");
    let volume_id = smb_volume_id("127.0.0.1", port, "public");
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    connect_smb_volume("public", &mount_path, &volume_id, params)
        .await
        .unwrap_or_else(|e| panic!("connect to 127.0.0.1:{port} failed: {e:?}"))
}

/// One pass of the concurrent-streaming-write scenario:
/// - generate `n_files` source files of `file_size` bytes in a tempdir,
/// - pre-upload `n_conflicts` of them to the destination at the same size so `OverwriteSmaller`
///   resolves them as Skip,
/// - run `copy_volumes_with_progress` over all `n_files` with a timeout,
/// - on timeout, panic with the last 30 mutex/recv lines as a diagnostic dump,
/// - clean up the unique prefix directory either way.
async fn run_concurrent_write_pass(
    vol: Arc<SmbVolume>,
    mount_path: &Path,
    logger: &'static MutexCaptureLogger,
    n_files: usize,
    n_conflicts: usize,
    file_size: usize,
    timeout_secs: u64,
) -> Duration {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };

    assert!(n_conflicts <= n_files);

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Include the PID so two concurrent runs (different worktrees sharing the
    // same `smb-consumer` container) never target the same dest dir within the
    // same wall-clock second. Mirrors `test_dir_name()`'s uniqueness recipe;
    // `ts`'s 1-second granularity alone is collision-prone across sessions.
    let pid = std::process::id();
    let unique_prefix = format!("{TEST_PREFIX_ROOT}{pid}-{ts}-n{n_files}");

    let dest_dir_abs = mount_path.join(unique_prefix.trim_start_matches('/'));
    let _ = vol.create_directory(&mount_path.join("_test")).await;
    vol.create_directory(&dest_dir_abs)
        .await
        .expect("create unique dest dir");

    let local_dir = tempfile::TempDir::new().expect("tempdir");
    for i in 0..n_files {
        let name = format!("f_{i:04}.bin");
        let path = local_dir.path().join(&name);
        // Distinct content per file (byte = i % 251) + an 8-byte seed
        // prefix, so identical-size pre-uploads still hash-differ from
        // their sources should we ever want to verify content.
        let mut buf = vec![0u8; file_size];
        buf[..8].copy_from_slice(&(i as u64).to_le_bytes());
        for b in buf.iter_mut().skip(8) {
            *b = (i % 251) as u8;
        }
        std::fs::write(&path, &buf).expect("write source");
    }

    log::info!(
        "regression: pre-uploading {} to {unique_prefix}",
        crate::pluralize::pluralize(n_conflicts as u64, "conflicting file")
    );
    for i in 0..n_conflicts {
        let name = format!("f_{i:04}.bin");
        let dest_abs = dest_dir_abs.join(&name);
        let buf = std::fs::read(local_dir.path().join(&name)).unwrap();
        let stream: Box<dyn VolumeReadStream> = Box::new(InlineReadStream::new(buf.clone()));
        let size = buf.len() as u64;
        let progress = |_a: u64, _b: u64| -> std::ops::ControlFlow<()> { std::ops::ControlFlow::Continue(()) };
        let bytes = vol
            .write_from_stream(&dest_abs, size, stream, &progress)
            .await
            .unwrap_or_else(|e| panic!("pre-upload {name} failed: {e:?}"));
        assert_eq!(bytes, size, "pre-upload size mismatch");
    }
    log::info!("regression: pre-upload done");

    let src_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "regression-src",
        local_dir.path().to_path_buf(),
    ));
    let dst_vol: Arc<dyn Volume> = vol.clone() as Arc<dyn Volume>;
    let source_rel_paths: Vec<PathBuf> = (0..n_files).map(|i| PathBuf::from(format!("f_{i:04}.bin"))).collect();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: crate::file_system::write_operations::ConflictResolution::OverwriteSmaller,
        ..VolumeCopyConfig::default()
    };

    let start = std::time::Instant::now();
    log::info!(
        "regression: spawning copy n_files={n_files} n_conflicts={n_conflicts} size={file_size} timeout={timeout_secs}s"
    );

    let res = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        copy_volumes_with_progress(
            events.clone(),
            "regression-op",
            &state,
            Arc::clone(&src_vol),
            &source_rel_paths,
            Arc::clone(&dst_vol),
            &dest_dir_abs,
            &config,
        ),
    )
    .await;

    let elapsed = start.elapsed();

    let panic_msg: Option<String> = match res {
        Ok(Ok(())) => {
            log::info!("regression: copy completed in {elapsed:?}");
            None
        }
        Ok(Err(e)) => Some(format!("regression: copy failed in {elapsed:?}: {e:?}")),
        Err(_) => {
            let tail = |q: &std::sync::Mutex<std::collections::VecDeque<String>>| -> Vec<String> {
                let q = q.lock().unwrap();
                let n = q.len().min(30);
                q.iter().skip(q.len() - n).cloned().collect()
            };
            let mutex_dump = tail(&logger.mutex_lines);
            let recv_dump = tail(&logger.recv_lines);
            let last_ticket = CLIENT_LOCK_TICKET.load(Ordering::Relaxed);
            Some(format!(
                "regression: HANG after {:?} (timeout={}s) n_files={} n_conflicts={} last_ticket={}\n\
                 ── last {} client-mutex lines ──\n{}\n── last {} recv lines ──\n{}\n",
                elapsed,
                timeout_secs,
                n_files,
                n_conflicts,
                last_ticket,
                mutex_dump.len(),
                mutex_dump.join("\n"),
                recv_dump.len(),
                recv_dump.join("\n"),
            ))
        }
    };

    cleanup_test_prefix(&vol, mount_path, &unique_prefix).await;

    if let Some(m) = panic_msg {
        panic!("{m}");
    }
    elapsed
}

/// Guards the invariant that concurrent streaming writes through
/// `SmbVolume::write_from_stream` complete without deadlocking.
///
/// Uses the consumer-class `smb-consumer-maxreadsize` fixture
/// (`smb2 max read = smb2 max write = 65536`) so every 1 MB write exceeds
/// the server's max_write and is forced through the streaming-fallback
/// (FileWriter) path. That's the path that historically nested a
/// per-write lock under the client mutex and could starve the receiver
/// task to a halt.
///
/// Shape (200 files, 140 OverwriteSmaller conflicts + 60 actual copies,
/// concurrency=8) mirrors the production workload that originally
/// surfaced the bug, where mixed conflict-skip / write iterations on a
/// shared SmbClient stressed the lock-ordering pattern hardest.
///
/// Run with `./apps/desktop/test/smb-servers/start.sh core` (CI does
/// this) or `start.sh all`, then either `./scripts/check.sh --rust` or
/// `cargo nextest run -p cmdr smb_integration_concurrent_streaming_writes_no_deadlock
/// --run-ignored all`.
///
/// Originally hung at a QNAP NAS for >5 minutes before the fix in smb2
/// 0.9.0 (`FileWriter` owns its `Connection`) and the matching
/// `write_from_stream` rewrite. On post-fix code each pass completes in
/// roughly 5–15 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires docker-compose smb-consumer-maxreadsize on port 10494 (started by start.sh core)"]
async fn smb_integration_concurrent_streaming_writes_no_deadlock() {
    use futures_util::FutureExt;

    // 10494 matches smb2's smb-consumer-maxreadsize container; override
    // with `SMB_CONSUMER_MAXREADSIZE_PORT` to match
    // `smb2::testing::maxreadsize_port()` (requires the `smb-e2e`
    // feature; bare integration tests hardcode the default).
    let port: u16 = std::env::var("SMB_CONSUMER_MAXREADSIZE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10494);
    let logger = install_mutex_capture_logger();
    let prior_concurrency = crate::file_system::smb_concurrency();
    crate::file_system::set_smb_concurrency(8);

    let vol = Arc::new(connect_docker_smb_volume(port, "cmdr-regression-maxreadsize").await);
    let mount_path = vol.mount_path.clone();

    let result = std::panic::AssertUnwindSafe(run_concurrent_write_pass(
        Arc::clone(&vol),
        &mount_path,
        logger,
        /* n_files = */ 200,
        /* n_conflicts = */ 140,
        /* file_size = */ 1024 * 1024,
        /* timeout_secs = */ 120,
    ))
    .catch_unwind()
    .await;

    // Always restore concurrency, even on panic, before resuming the unwind.
    crate::file_system::set_smb_concurrency(prior_concurrency);
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
}
