//! What the transfer driver's concurrency window is actually worth, measured.
//!
//! The driver sizes its sliding window with `min(src, dst, 32)`. M4.3 exists to
//! replace that guess with something principled, and the only way to tell a
//! better formula from a worse one is a throughput curve: wall-clock against
//! window width, on the two corpus shapes that stress completely different SMB
//! write paths, against a target that behaves like a real network.
//!
//! **This is a measurement harness, not a test.** It asserts only what would
//! invalidate a number (the copy succeeded, every file landed at its full size,
//! the window really filled); it never asserts a duration. SMB throughput is
//! noisy, so it reports a median and the spread around it, never a single run.
//!
//! ## Running it
//!
//! ```bash
//! cd apps/desktop/src-tauri
//! # Docker (reproducible, loopback — see the caveat below)
//! cargo test --release --lib concurrency_bench -- --ignored --nocapture --test-threads=1
//! # Real NAS
//! CMDR_BENCH_TARGET=nas SMB2_TEST_NAS_PASSWORD=… \
//!   cargo test --release --lib concurrency_bench -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Knobs, all optional: `CMDR_BENCH_TARGET` (`docker` | `nas`),
//! `CMDR_BENCH_WINDOWS` (comma-separated, default `1,2,4,6,8,10,12,16,24,32`),
//! `CMDR_BENCH_REPS` (default 5), `CMDR_BENCH_SHAPES` (`small` | `large` |
//! `both`), `CMDR_BENCH_SMALL_COUNT`, `CMDR_BENCH_SMALL_KIB`,
//! `CMDR_BENCH_LARGE_COUNT`, `CMDR_BENCH_LARGE_MIB`, `SMB2_TEST_NAS_HOST`,
//! `SMB2_TEST_NAS_SHARE`, `SMB2_TEST_NAS_USER`.
//!
//! ## Why the numbers are shaped the way they are
//!
//! - **Reps are round-robin, not blocked per window.** A NAS warms up, a laptop
//!   thermally throttles, and the server's cache fills; blocking all reps of
//!   window 1 before any rep of window 32 hands that drift entirely to one end
//!   of the curve. Interleaving spreads it evenly, so a difference between
//!   windows is a difference between windows.
//! - **The first pass is a discarded warm-up.** Session setup, the server's
//!   dentry cache, and the source files' page-cache state all land on whichever
//!   window happens to go first otherwise.
//! - **The window is swept by proxy, on unchanged production code.** The
//!   destination side comes from `set_smb_concurrency` (the real
//!   `network.smbConcurrency` setting) and the source side from a wrapper that
//!   reports a fixed `max_concurrent_ops`, so `min(src, dst, 32)` resolves to
//!   exactly the swept value without the driver knowing it is being measured.
//! - **Docker is necessary but not sufficient.** It is loopback: sub-100 µs
//!   round trips, so the per-file latency a wider window exists to hide barely
//!   exists there. A curve that is flat on Docker says nothing about a real
//!   network. The NAS target is what decides.
//! - **The `window = 1` row is the SERIAL driver, not a one-wide concurrent
//!   one.** `use_concurrent_path` needs `concurrency > 1`, so a window of 1
//!   routes to `drive_transfer_serial_async` instead. That is exactly what a
//!   user who sets `network.smbConcurrency` to 1 gets, so the row is honest —
//!   but it is a different code path, and a step between 1 and 2 is partly that
//!   switch rather than the window alone.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::smb::{SmbConnectionParams, connect_smb_volume};
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, LaneKey, ListingProgress, LocalPosixVolume, SpaceInfo, VolumeReadStream,
    smb_volume_id,
};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};
use crate::file_system::{set_smb_concurrency, smb_concurrency};
use crate::ignore_poison::IgnorePoison;
use std::future::Future;
use std::pin::Pin as StdPin;

// ── Corpus shapes ───────────────────────────────────────────────────

/// Default many-small corpus: every file fits one SMB2 compound
/// CREATE+WRITE+FLUSH+CLOSE frame, so the copy is almost pure per-file round
/// trip and the window is the only thing that can hide that latency. This is a
/// folder of documents, and the shape a wider window should help most.
const DEFAULT_SMALL_COUNT: usize = 500;
const DEFAULT_SMALL_KIB: usize = 16;

/// Default few-large corpus: each file is far past any server's negotiated
/// `max_write`, so it takes the staged streaming writer — which already
/// pipelines up to 32 wire WRITEs *within a single file*. The window buys much
/// less here by construction; measuring it is how we find out whether it costs
/// anything.
const DEFAULT_LARGE_COUNT: usize = 8;
const DEFAULT_LARGE_MIB: usize = 24;

/// Windows swept by default. Stops at 32 because that is the driver's current
/// hard ceiling: on unchanged code `min(src, dst, 32)` cannot produce more, so a
/// baseline run cannot honestly report past it.
const DEFAULT_WINDOWS: &[usize] = &[1, 2, 4, 6, 8, 10, 12, 16, 24, 32];

const DEFAULT_REPS: usize = 5;

/// Everything this harness writes to a real NAS lives under here, and the
/// cleanup helper refuses to delete a path that doesn't start with it. `_test/`
/// is already the share's established scratch area.
const NAS_BENCH_ROOT: &str = "_test/cmdr-m43-bench";

/// Hard bound on any single copy in the sweep. A bench that can hang is a bench
/// that eats an afternoon and reports nothing; on expiry this prints the
/// transfer probe's live in-flight table, exactly like the M4.4 suite, instead
/// of waiting forever.
const COPY_DEADLINE: Duration = Duration::from_secs(300);

// ── The source wrapper that makes the window sweepable ──────────────

/// A `LocalPosixVolume` that reports a fixed `max_concurrent_ops`.
///
/// The driver's window is `min(src, dst, 32)`. The destination side is already
/// runtime-settable (`set_smb_concurrency`), but the source side is a CPU-core
/// heuristic clamped to 4..=16 — so on a normal laptop the LOCAL source, not the
/// network, is what caps a network transfer. Pinning the source high moves the
/// whole sweep onto the destination knob, leaving one variable instead of two.
///
/// Everything except `max_concurrent_ops` delegates, so what gets measured is
/// the production path.
struct FixedConcurrencySource {
    inner: LocalPosixVolume,
    concurrency: usize,
}

impl Volume for FixedConcurrencySource {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lane_key(&self) -> LaneKey {
        self.inner.lane_key()
    }
    fn max_concurrent_ops(&self) -> usize {
        self.concurrency
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> StdPin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> StdPin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy_batch(paths)
    }
    fn get_space_info<'a>(&'a self) -> StdPin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn local_path(&self) -> Option<PathBuf> {
        self.inner.local_path()
    }
    fn supports_export(&self) -> bool {
        self.inner.supports_export()
    }
    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream_with_hint(path, size_hint)
    }
    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        self.inner.read_range(path, offset, len)
    }
}

// ── Environment plumbing ────────────────────────────────────────────

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn windows_to_sweep() -> Vec<usize> {
    match std::env::var("CMDR_BENCH_WINDOWS") {
        Ok(raw) => raw.split(',').filter_map(|p| p.trim().parse().ok()).collect(),
        Err(_) => DEFAULT_WINDOWS.to_vec(),
    }
}

/// A scratch directory name no concurrent run can collide with.
fn scratch_name() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("cmdr-bench-{}-{nanos}-{n}", std::process::id())
}

// ── Corpus ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Many files that each fit one compound frame.
    Small,
    /// Few files that each take the staged streaming writer.
    Large,
}

impl Shape {
    fn label(self) -> &'static str {
        match self {
            Shape::Small => "many-small",
            Shape::Large => "few-large",
        }
    }
}

/// A built source tree plus what the verification pass needs to know about it.
struct Corpus {
    _dir: tempfile::TempDir,
    volume: Arc<dyn Volume>,
    paths: Vec<PathBuf>,
    /// Bytes per file. Uniform within a shape.
    file_len: u64,
    total_bytes: u64,
}

/// Builds the corpus on local disk once and reuses it for the whole sweep.
///
/// Content is a repeated per-index blake3 block rather than a run of zeros: a
/// filesystem or a server that compresses or dedupes zeros would report a
/// throughput that has nothing to do with the wire.
fn build_corpus(shape: Shape, source_concurrency: usize) -> Corpus {
    let (count, len) = match shape {
        Shape::Small => (
            env_usize("CMDR_BENCH_SMALL_COUNT", DEFAULT_SMALL_COUNT),
            env_usize("CMDR_BENCH_SMALL_KIB", DEFAULT_SMALL_KIB) * 1024,
        ),
        Shape::Large => (
            env_usize("CMDR_BENCH_LARGE_COUNT", DEFAULT_LARGE_COUNT),
            env_usize("CMDR_BENCH_LARGE_MIB", DEFAULT_LARGE_MIB) * 1024 * 1024,
        ),
    };
    let dir = tempfile::TempDir::new().expect("create the corpus tempdir");
    let mut paths = Vec::with_capacity(count);
    for index in 0..count {
        let name = format!("b-{index:04}.bin");
        std::fs::write(dir.path().join(&name), bench_content(index, len)).expect("write a corpus file");
        paths.push(PathBuf::from(&name));
    }
    let volume: Arc<dyn Volume> = Arc::new(FixedConcurrencySource {
        inner: LocalPosixVolume::new("bench-src", dir.path().to_path_buf()),
        concurrency: source_concurrency,
    });
    Corpus {
        _dir: dir,
        volume,
        paths,
        file_len: len as u64,
        total_bytes: (count as u64) * (len as u64),
    }
}

fn bench_content(index: usize, len: usize) -> Vec<u8> {
    let block = *blake3::hash(&(index as u64).to_le_bytes()).as_bytes();
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let take = block.len().min(len - out.len());
        out.extend_from_slice(&block[..take]);
    }
    out
}

// ── One timed copy ──────────────────────────────────────────────────

/// Result of a single timed copy.
struct Run {
    elapsed: Duration,
    /// Highest in-flight count any progress event reported, i.e. how full the
    /// window actually got. Systematically one under the true peak (the
    /// reporting task has already left the in-flight table), which is fine: it
    /// is read to confirm the window opened, not to pin its exact width.
    peak_in_flight: u32,
}

/// Runs one copy of the whole corpus into a fresh destination directory and
/// times it end to end, the way the user's progress dialog does.
///
/// The destination directory is created before the timer starts and emptied
/// after it stops, so neither shows up in the number.
async fn timed_copy(corpus: &Corpus, dest: &Arc<dyn Volume>, dir: &str, window: usize) -> Run {
    set_smb_concurrency(window);
    assert_eq!(
        smb_concurrency(),
        window,
        "the sweep asked for a window of {window} but the SMB setting clamped it; \
         windows outside 1..=32 can't be measured on unchanged code"
    );

    dest.create_directory(Path::new(dir))
        .await
        .expect("create the destination directory");

    let guard = TestOperationGuard::register("m43-concurrency-bench");
    let operation_id = guard.id().to_owned();
    let state = Arc::clone(guard.state());
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..Default::default()
    };

    let started = Instant::now();
    let mut copy = tokio::spawn({
        let events = Arc::clone(&events) as Arc<dyn OperationEventSink>;
        let operation_id = operation_id.clone();
        let source_vol = Arc::clone(&corpus.volume);
        let source_paths = corpus.paths.clone();
        let dest_vol = Arc::clone(dest);
        let dir = dir.to_owned();
        async move {
            copy_volumes_with_progress(
                events,
                &operation_id,
                &state,
                source_vol,
                &source_paths,
                dest_vol,
                Path::new(&dir),
                &config,
            )
            .await
        }
    });

    // Bounded, and it dumps rather than hangs: the harness must never become the
    // wedge it is measuring around.
    let outcome = match tokio::time::timeout(COPY_DEADLINE, &mut copy).await {
        Ok(joined) => joined.expect("the copy task must not panic"),
        Err(_) => {
            let dump = crate::file_system::write_operations::render_live_transfer_dump(
                &operation_id,
                "bench deadline expired with the copy still running",
            )
            .unwrap_or_else(|| "no live probe: the operation had already deregistered".to_owned());
            copy.abort();
            panic!("a bench copy at window={window} did not finish within {COPY_DEADLINE:?}:\n{dump}");
        }
    };
    let elapsed = started.elapsed();
    if let Err(failure) = outcome {
        panic!("a bench copy at window={window} must succeed, got {failure:?}");
    }

    let progress = events.progress.lock_ignore_poison().clone();
    let peak_in_flight = progress
        .iter()
        .filter_map(|event| event.activity.map(|activity| activity.in_flight))
        .max()
        .unwrap_or(0);

    // Verify from ONE listing: every file present, every size exact. Cheap
    // enough to run every rep, and it catches the failure modes that would
    // otherwise make a fast run look good — a truncated tail, a dropped file, or
    // a staging temp left behind.
    let landed = dest
        .list_directory(Path::new(dir), None)
        .await
        .expect("list the destination");
    let leftovers: Vec<&str> = landed
        .iter()
        .map(|e| e.name.as_str())
        .filter(|n| n.contains(".cmdr-tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "window={window}: staging temps survived the copy: {leftovers:?}"
    );
    assert_eq!(
        landed.len(),
        corpus.paths.len(),
        "window={window}: destination holds {} entries, expected {}",
        landed.len(),
        corpus.paths.len(),
    );
    let sizes: HashMap<&str, Option<u64>> = landed.iter().map(|e| (e.name.as_str(), e.size)).collect();
    for path in &corpus.paths {
        let name = path.to_str().expect("a corpus name is UTF-8");
        assert_eq!(
            sizes.get(name).copied().flatten(),
            Some(corpus.file_len),
            "window={window}: {name} landed at the wrong size (or not at all)"
        );
    }

    empty_directory(dest, dir).await;

    Run {
        elapsed,
        peak_in_flight,
    }
}

// ── Destination plumbing ────────────────────────────────────────────

/// Deletes every entry under `dir` and then `dir` itself.
///
/// Refuses any path outside the bench's own scratch root. The NAS share this
/// runs against also holds the user's paperwork, so a caller-side path bug must
/// not be able to reach it.
async fn empty_directory(vol: &Arc<dyn Volume>, dir: &str) {
    assert!(
        !dir.starts_with('/') && (dir.starts_with(NAS_BENCH_ROOT) || dir.starts_with("cmdr-bench-")),
        "refusing to clean {dir:?}: outside the bench scratch root"
    );
    if let Ok(entries) = vol.list_directory(Path::new(dir), None).await {
        for entry in entries {
            let child = format!("{dir}/{}", entry.name);
            if entry.is_directory {
                Box::pin(empty_directory(vol, &child)).await;
            } else {
                let _ = vol.delete(Path::new(&child)).await;
            }
        }
    }
    let _ = vol.delete(Path::new(dir)).await;
}

/// Which server the sweep runs against, and where on it the scratch lives.
struct Target {
    label: String,
    volume: Arc<dyn Volume>,
    /// Parent directory the per-rep scratch dirs are created under. Empty means
    /// the share root.
    scratch_parent: String,
}

async fn connect_target() -> Target {
    let kind = std::env::var("CMDR_BENCH_TARGET").unwrap_or_else(|_| "docker".to_owned());
    match kind.as_str() {
        "docker" => {
            let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10480);
            let volume_id = smb_volume_id("127.0.0.1", port, "public");
            let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
            let volume = connect_smb_volume("public", "/tmp/smb-bench-mount", &volume_id, params)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "no Docker SMB container at 127.0.0.1:{port} \
                         (./apps/desktop/test/smb-servers/start.sh): {e:?}"
                    )
                });
            Target {
                label: format!("Docker Samba (loopback, 127.0.0.1:{port}/public)"),
                volume: Arc::new(volume),
                scratch_parent: String::new(),
            }
        }
        "nas" => {
            let password = std::env::var("SMB2_TEST_NAS_PASSWORD").expect(
                "CMDR_BENCH_TARGET=nas needs SMB2_TEST_NAS_PASSWORD (this harness deliberately does not \
                 walk up to smb2/.env — inside a worktree that path resolves into .claude/worktrees/)",
            );
            let host = std::env::var("SMB2_TEST_NAS_HOST").unwrap_or_else(|_| "192.168.1.111".to_owned());
            let share = std::env::var("SMB2_TEST_NAS_SHARE").unwrap_or_else(|_| "naspi".to_owned());
            let user = std::env::var("SMB2_TEST_NAS_USER").unwrap_or_else(|_| "david".to_owned());
            let volume_id = smb_volume_id(&host, 445, &share);
            let params = SmbConnectionParams::new(&host, &share, 445, Some(user.as_str()), Some(password.as_str()));
            let volume = connect_smb_volume(&share, "/Volumes/naspi-m43-bench", &volume_id, params)
                .await
                .unwrap_or_else(|e| panic!("cannot reach the NAS at {host}/{share}: {e:?}"));
            let volume: Arc<dyn Volume> = Arc::new(volume);
            // The scratch root is created once and left in place; only the
            // per-rep directories under it come and go.
            let _ = volume.create_directory_all(Path::new(NAS_BENCH_ROOT)).await;
            Target {
                label: format!("QNAP TS-464 (\"Naspolya\", {host}/{share}, direct smb2)"),
                volume,
                scratch_parent: NAS_BENCH_ROOT.to_owned(),
            }
        }
        other => panic!("CMDR_BENCH_TARGET must be `docker` or `nas`, got {other:?}"),
    }
}

// ── The serial pre-check floor ──────────────────────────────────────

/// Times the per-file destination probe the DRIVER runs serially, so the sweep
/// can tell two very different stories apart.
///
/// The concurrent spawn loop awaits `dest_volume.get_metadata(dest_item_path)`
/// once per top-level source, **on the driver, before the task is spawned**
/// (`volume_copy.rs`, the `PreparingNext` phase — the call that was the last
/// driver log line in the 2026-07-31 wedge). On SMB that is one round trip per
/// file that no window width can overlap, so a batch of N files carries a hard
/// floor of `N × RTT` however wide the window gets.
///
/// That floor and "the window stopped helping" produce the SAME shape: a curve
/// that flattens. This function measures the floor directly instead of inferring
/// it — same call, same connection, same count, serialized the same way, on
/// paths that don't exist (which is what a copy into a fresh directory probes).
/// If the flattened part of the curve sits near this number, the pre-check is
/// the bottleneck and widening the window is not the fix.
///
/// Deliberately measured OUTSIDE the driver: no production code is instrumented,
/// so the sweep it explains stays a measurement of unmodified behavior.
async fn serial_precheck_floor(dest: &Arc<dyn Volume>, dir: &str, files: usize) -> Duration {
    let started = Instant::now();
    for index in 0..files {
        // Missing on purpose: a copy into a fresh destination probes names that
        // aren't there, and a miss is the cheap answer. If anything, this
        // UNDERSTATES the floor.
        let probe = format!("{dir}/precheck-probe-{index:05}.missing");
        let _ = dest.get_metadata(Path::new(&probe)).await;
    }
    started.elapsed()
}

// ── Reporting ───────────────────────────────────────────────────────

fn median(sorted: &[Duration]) -> Duration {
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2
    }
}

// ── The sweep ───────────────────────────────────────────────────────

/// Sweeps the driver's concurrency window and prints wall-clock per window.
///
/// It asserts no timing of any kind: the printed table IS the deliverable. It is
/// `#[ignore]`d and needs a live SMB server, so it never runs in CI.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "Measurement harness: needs Docker SMB (./apps/desktop/test/smb-servers/start.sh) or a reachable NAS"]
#[allow(
    clippy::print_stdout,
    reason = "a bench reports its table on stdout by design; run with --nocapture"
)]
async fn concurrency_bench_sweep_window_against_wall_clock() {
    let windows = windows_to_sweep();
    let reps = env_usize("CMDR_BENCH_REPS", DEFAULT_REPS);
    let shapes: Vec<Shape> = match std::env::var("CMDR_BENCH_SHAPES").as_deref() {
        Ok("small") => vec![Shape::Small],
        Ok("large") => vec![Shape::Large],
        _ => vec![Shape::Small, Shape::Large],
    };
    let restore = smb_concurrency();

    let target = connect_target().await;

    println!();
    println!("═══════════════════════════════════════════════════════════════════");
    println!("Transfer concurrency sweep (M4.3)");
    println!("  target     {}", target.label);
    println!("  windows    {windows:?}");
    println!("  reps       {reps} (plus one discarded warm-up), round-robin");
    println!("═══════════════════════════════════════════════════════════════════");

    for shape in shapes {
        // A source window well past anything swept, so `min(src, dst, 32)`
        // always resolves to the destination setting.
        let corpus = build_corpus(shape, 64);

        // Prove the shape really is the write path it claims to be, before
        // spending minutes measuring it. A "large" file that still fits one
        // compound frame would silently benchmark the fast path twice.
        let single_shot = target.volume.write_is_single_shot(corpus.file_len).await;
        assert_eq!(
            single_shot,
            shape == Shape::Small,
            "{}: a {}-byte write reports single_shot={single_shot}, which is not the write path \
             this shape exists to measure",
            shape.label(),
            corpus.file_len,
        );

        let mut samples: HashMap<usize, Vec<Duration>> = HashMap::new();
        let mut peaks: HashMap<usize, u32> = HashMap::new();

        // Round-robin over reps so drift lands on every window equally. Pass 0
        // is the warm-up and is thrown away.
        for pass in 0..=reps {
            for &window in &windows {
                let dir = if target.scratch_parent.is_empty() {
                    scratch_name()
                } else {
                    format!("{}/{}", target.scratch_parent, scratch_name())
                };
                let run = timed_copy(&corpus, &target.volume, &dir, window).await;
                if pass == 0 {
                    continue;
                }
                samples.entry(window).or_default().push(run.elapsed);
                let peak = peaks.entry(window).or_default();
                *peak = (*peak).max(run.peak_in_flight);
            }
        }

        println!();
        println!(
            "── {} — {} files × {} B = {:.1} MiB ─────────────────────",
            shape.label(),
            corpus.paths.len(),
            corpus.file_len,
            corpus.total_bytes as f64 / (1024.0 * 1024.0),
        );
        println!("  window     median        min        max      MB/s   files/s   peak");
        let mut best: Option<(usize, Duration)> = None;
        for &window in &windows {
            let mut runs = samples.remove(&window).unwrap_or_default();
            runs.sort();
            let med = median(&runs);
            let secs = med.as_secs_f64();
            println!(
                "  {:>6}   {:>9.3?}  {:>9.3?}  {:>9.3?}   {:>7.1}   {:>7.1}   {:>4}",
                window,
                med,
                runs.first().copied().unwrap_or_default(),
                runs.last().copied().unwrap_or_default(),
                corpus.total_bytes as f64 / secs / 1_000_000.0,
                corpus.paths.len() as f64 / secs,
                peaks.get(&window).copied().unwrap_or(0),
            );
            if best.is_none_or(|(_, b)| med < b) {
                best = Some((window, med));
            }
        }
        // The discriminator. Run AFTER the sweep so it never warms anything the
        // sweep is timing, into a scratch dir of its own.
        let probe_dir = if target.scratch_parent.is_empty() {
            scratch_name()
        } else {
            format!("{}/{}", target.scratch_parent, scratch_name())
        };
        target
            .volume
            .create_directory(Path::new(&probe_dir))
            .await
            .expect("create the pre-check probe directory");
        let floor = serial_precheck_floor(&target.volume, &probe_dir, corpus.paths.len()).await;
        empty_directory(&target.volume, &probe_dir).await;

        if let Some((window, med)) = best {
            println!("  fastest window: {window} at {med:.3?} median");
            // What share of the best achievable time is spent in a serial probe
            // loop that no window can overlap. Near 100% means the window is not
            // the bottleneck and M4.3's premise needs rethinking.
            let share = floor.as_secs_f64() / med.as_secs_f64() * 100.0;
            println!(
                "  serial pre-check floor: {floor:.3?} for {} files ({:.2?}/file) = {share:.0}% of the fastest run",
                corpus.paths.len(),
                floor / u32::try_from(corpus.paths.len()).unwrap_or(1),
            );
        }
    }

    set_smb_concurrency(restore);
    println!();
}
