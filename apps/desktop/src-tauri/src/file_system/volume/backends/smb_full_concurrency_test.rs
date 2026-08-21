//! The shape that wedged in production: many files, full concurrency, real SMB
//! destination.
//!
//! A 764-file, 3.10 GB copy from local disk to a NAS share stopped dead after 12
//! files, three times, and could not be cancelled or rolled back
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Every guardrail
//! that came out of it was built after the fact, by hand, from logs. This suite
//! is the automated net under that shape: a batch big enough to keep the
//! concurrency window full for the whole run, a mix of file sizes that exercises
//! BOTH SMB write paths, and byte-level verification of everything that lands.
//!
//! **These tests can't hang.** The property under test is the absence of an
//! infinite wedge, so a suite that could itself block would carry the very bug it
//! guards against. Both tests bound their wait and, on expiry, print the transfer
//! probe's live in-flight table before abandoning the copy — the record the
//! incident needed and did not have. `a_wedged_copy_is_caught_and_names_its_phase`
//! is the test OF that mechanism: it wedges a copy on purpose and asserts the
//! bound fires with a dump that names the parked tasks.
//!
//! `#[ignore]`d like every `smb_integration_` test; the
//! `desktop-rust-integration-tests` check boots the Docker Samba stack and runs
//! them. Locally: `./apps/desktop/test/smb-servers/start.sh`, then
//! `cargo nextest run smb_integration_ --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb`; shared helpers come from
//! `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;
use std::sync::atomic::AtomicBool;

use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::{
    CollectorEventSink, VolumeCopyConfig, WriteProgressEvent, copy_volumes_with_progress, render_live_transfer_dump,
};
use crate::ignore_poison::IgnorePoison;

/// Sources in the batch. Two things pick this number: it has to be well past the
/// concurrency window so the window stays FULL for essentially the whole run
/// (the incident's window was 8 and it died at source 13), and every file is a
/// separate round trip, so it also has to stay inside a lane that runs in
/// seconds. 400 keeps the window saturated for hundreds of task spawns.
const FILE_COUNT: usize = 400;

/// Most sources are small enough to take SMB's compound CREATE+WRITE+FLUSH+CLOSE
/// fast path, which is what a real folder of documents looks like.
const SMALL_FILE_BYTES: usize = 4096;

/// Every Nth source is instead sized past the server's negotiated `max_write`, so
/// it takes the multi-round-trip streaming writer and gets STAGED on a
/// `.cmdr-tmp-*` name. That is the path both files stuck in the incident were on,
/// and the only path that can leave debris if a transfer dies mid-write. 10 of
/// them carry ~98% of the batch's bytes against Samba's 8 MiB `max_write`, so a
/// staged write is in flight for almost the whole run.
const LARGE_EVERY: usize = 40;

/// Hard bound on the healthy copy.
///
/// A backstop, not a budget. Measured against the Docker guest container on an
/// idle M3 Max, 2026-08-02: the copy itself is **0.43 s** (86 MB, 400 files) and
/// the whole test is 3.5 s, so this is ~100x the work it bounds. It sits below
/// this test's nextest cap on purpose (see `.config/nextest.toml`), so THIS
/// deadline is what fires on a wedge and its dump is what a human reads — a cap
/// kill would leave nothing behind, which is the failure mode the whole suite
/// exists to prevent.
const COPY_DEADLINE: Duration = Duration::from_secs(45);

/// Hard bound on the copy the wedge test deliberately parks. Short, because it
/// ALWAYS expires: the test's subject is what happens at expiry.
const WEDGE_DEADLINE: Duration = Duration::from_secs(5);

/// The floor the concurrency window must actually reach for this suite to be
/// testing anything.
///
/// Deliberately a floor, not the driver's formula. Two reasons, and both matter:
///
/// 1. Pinning the formula here would make any change to it fail for the wrong
///    reason. It has already changed once (a LOCAL volume's cap no longer bounds
///    a REMOTE peer, so this copy's window is `network.smbConcurrency` rather
///    than `LocalPosixVolume`'s core-count clamp), and this suite is about
///    whether the batch ran concurrently at all, not about how wide.
/// 2. The number this reads is systematically ONE under the true window. Progress
///    events fire as a source finishes, so the task doing the reporting has
///    already left the in-flight table by the time `activity()` counts it.
///    Measured 2026-08-02 on an idle M3 Max: a sampled peak of 7 while a deadline
///    dump of the same run showed `in_flight=8/8`.
///
/// So the bar is the smallest window the driver could plausibly produce, minus
/// that one, and it stays low on purpose. Under 3 means the batch went down the
/// sequential path or the window never filled — either way, not the shape that
/// wedges. ❌ Don't raise it to track a formula, and ❌ never lower it to make
/// something pass.
const MIN_PEAK_IN_FLIGHT: u32 = 3;

/// How often the staging probe samples the destination while the copy runs.
///
/// Frequent enough to be certain of a hit (the large staged files hold ~98% of
/// the batch's bytes, so one is in flight for nearly the whole copy) and rare
/// enough that the listings don't become load of their own.
const STAGING_PROBE_INTERVAL: Duration = Duration::from_millis(30);

/// Deterministic, unique-per-index content. Every byte position varies between
/// files, so a chunk that lands under a neighbour's path, a reused buffer, or a
/// truncated tail all flip the file's hash. Identical content would hide all
/// three.
fn expected_content(index: usize, len: usize) -> Vec<u8> {
    let mut seed = Vec::with_capacity(18);
    seed.extend_from_slice(b"cmdr-m4.4-");
    seed.extend_from_slice(&(index as u64).to_le_bytes());
    let block = *blake3::hash(&seed).as_bytes();
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let take = block.len().min(len - out.len());
        out.extend_from_slice(&block[..take]);
    }
    out
}

/// Awaits a spawned copy under a hard deadline.
///
/// `Ok` carries the driver's own result. `Err` carries the transfer probe's LIVE
/// in-flight table, taken at the instant the deadline expired: which phase every
/// task is parked in, what the driver was doing, the operation's intent, and how
/// full the window was.
///
/// Takes the `JoinHandle` rather than the copy future on purpose. Timing out on
/// the future itself would DROP it, which drops the probe guard and empties the
/// registry before there is anything to dump — the copy has to stay parked while
/// we read it.
/// Generic over the driver's result type only because `WriteFailure` is private
/// to `write_operations`; there is one caller shape.
async fn await_copy_or_probe_dump<T>(
    copy: &mut tokio::task::JoinHandle<T>,
    operation_id: &str,
    deadline: Duration,
) -> Result<T, String> {
    match tokio::time::timeout(deadline, &mut *copy).await {
        Ok(joined) => Ok(joined.expect("the copy task itself must not panic")),
        Err(_) => {
            let reason = format!("test deadline of {deadline:?} expired with the copy still running");
            let dump = render_live_transfer_dump(operation_id, &reason).unwrap_or_else(|| {
                format!("transfer probe ({reason}): no live probe — the operation had already deregistered")
            });
            copy.abort();
            Err(dump)
        }
    }
}

/// Highest `in_flight` any progress event reported, i.e. how full the concurrency
/// window actually got. Read from the events the FE would have received, so it
/// measures the same number the user's dialog shows.
fn peak_in_flight(progress: &[WriteProgressEvent]) -> u32 {
    progress
        .iter()
        .filter_map(|event| event.activity.map(|activity| activity.in_flight))
        .max()
        .unwrap_or(0)
}

/// Watches the destination for `.cmdr-tmp-*` names while the copy runs, and
/// reports whether it ever saw one.
///
/// Without this the "no staging temps survived" assertion is the classic no-op
/// fixture: a batch where nothing was ever staged has zero leftovers whether the
/// cleanup works or not. Seeing a temp appear and then be gone at the end is what
/// makes the pair mean something.
fn spawn_staging_probe(vol: Arc<SmbVolume>, dir: String, seen: Arc<AtomicBool>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Ok(entries) = vol.list_directory(Path::new(&dir), None).await
                && entries.iter().any(|entry| entry.name.contains(".cmdr-tmp-"))
            {
                seen.store(true, Ordering::SeqCst);
                return; // one sighting is the whole point; stop adding load
            }
            // allowed-test-sleep: this IS a sampling probe's own interval, not a
            // wait. Nothing waits on it — the test reads its flag after the copy
            // has already finished on its own terms.
            tokio::time::sleep(STAGING_PROBE_INTERVAL).await;
        }
    })
}

/// Builds the local source tree and returns the relative paths plus the per-index
/// byte length, so the verification pass knows what to expect without rebuilding
/// the fixture.
fn build_source_tree(dir: &Path, large_bytes: usize) -> (Vec<PathBuf>, Vec<usize>) {
    let mut paths = Vec::with_capacity(FILE_COUNT);
    let mut sizes = Vec::with_capacity(FILE_COUNT);
    for index in 0..FILE_COUNT {
        let len = if index % LARGE_EVERY == 0 {
            large_bytes
        } else {
            SMALL_FILE_BYTES
        };
        let name = format!("f-{index:04}.bin");
        std::fs::write(dir.join(&name), expected_content(index, len)).expect("write source file");
        paths.push(PathBuf::from(name));
        sizes.push(len);
    }
    (paths, sizes)
}

/// THE regression, end to end: 400 files at the driver's full concurrency onto a
/// real SMB share, every byte verified, no staging debris left behind.
///
/// What each assertion is for:
///
/// - **The copy finishes inside a bounded wait.** The incident's copy never
///   finished and never failed; it simply stopped. A red here prints the
///   in-flight table rather than the suite hanging.
/// - **The window really filled.** A batch that quietly took the sequential path
///   would pass every content check while testing none of the concurrency this
///   exists for, so the peak in-flight is asserted, not assumed.
/// - **Both write paths ran.** Small files take the compound fast path; the large
///   ones are sized past the server's negotiated `max_write` so they take the
///   streaming writer and get staged. Only the second path can strand a partial.
/// - **Every file is byte-exact.** Unique per-index content means a cross-task
///   buffer mix-up or a truncated tail flips a hash, which `bytes_written == n`
///   and `metadata.size == n` would both wave through.
/// - **A `.cmdr-tmp-*` appeared AND is gone.** A staged write that dies leaves its
///   temp behind, so the destination listing must hold exactly the batch and
///   nothing else. On its own that assertion is worthless — a batch that staged
///   nothing has no leftovers either — so a probe watches the destination during
///   the copy and the test fails if it never saw a temp at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_many_files_at_full_concurrency_land_intact() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();

    // Size the large files off what THIS server negotiated rather than a
    // hardcoded number: `max_write` differs per server (and per dialect), and a
    // "large" file that still fits one compound write would silently test the
    // fast path twice.
    let max_write = negotiated_max_write(&smb_vol)
        .await
        .expect("a live session reports its negotiated params");
    let large_bytes = usize::try_from(max_write + 64 * 1024).expect("max_write fits usize on any host we run on");
    assert!(
        !smb_vol.write_is_single_shot(large_bytes as u64).await,
        "a {large_bytes}-byte write must NOT fit one compound write against max_write={max_write}, \
         or this batch never exercises the staged streaming path"
    );
    assert!(
        smb_vol.write_is_single_shot(SMALL_FILE_BYTES as u64).await,
        "a {SMALL_FILE_BYTES}-byte write must fit one compound write against max_write={max_write}, \
         or the batch is all one path and the size mix proves nothing"
    );

    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    let (source_paths, source_sizes) = build_source_tree(local_dir.path(), large_bytes);
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));
    let total_bytes: usize = source_sizes.iter().sum();
    log::info!(
        // allowed-pluralize-noun: a diagnostic log line; the counts are compile-time constants and never 1.
        "full-concurrency copy: {FILE_COUNT} files, {total_bytes} bytes, large={large_bytes} \
         (max_write={max_write}), src_concurrency={}, dst_concurrency={}",
        source_vol.max_concurrent_ops(),
        dest_vol.max_concurrent_ops(),
    );

    let guard = TestOperationGuard::register("smb-full-concurrency");
    let operation_id = guard.id().to_owned();
    let state = Arc::clone(guard.state());
    let events = Arc::new(CollectorEventSink::new());

    let staging_seen = Arc::new(AtomicBool::new(false));
    let staging_probe = spawn_staging_probe(Arc::clone(&smb_vol), base.clone(), Arc::clone(&staging_seen));

    let started = std::time::Instant::now();
    let mut copy = tokio::spawn({
        let events = Arc::clone(&events) as Arc<dyn crate::file_system::OperationEventSink>;
        let operation_id = operation_id.clone();
        let source_vol = Arc::clone(&source_vol);
        let dest_vol = Arc::clone(&dest_vol);
        let dest = base.clone();
        async move {
            copy_volumes_with_progress(
                events,
                &operation_id,
                &state,
                source_vol,
                &source_paths,
                dest_vol,
                Path::new(&dest),
                &VolumeCopyConfig::default(),
            )
            .await
        }
    });

    let outcome = await_copy_or_probe_dump(&mut copy, &operation_id, COPY_DEADLINE).await;
    let elapsed = started.elapsed();
    staging_probe.abort();

    // The share is shared machine-wide, so clean up before any panic path.
    let cleanup = |vol: Arc<SmbVolume>, base: String| async move { ensure_clean(&vol, &base).await };

    let result = match outcome {
        Ok(result) => result,
        Err(dump) => {
            cleanup(Arc::clone(&smb_vol), base.clone()).await;
            panic!(
                "the copy did not finish within {COPY_DEADLINE:?} — this is the production wedge. \
                 The live in-flight table below says where it stopped:\n{dump}"
            );
        }
    };
    if let Err(failure) = &result {
        cleanup(Arc::clone(&smb_vol), base.clone()).await;
        panic!("a full-concurrency copy onto SMB must succeed, got {failure:?}");
    }

    let errors = events.errors.lock_ignore_poison().clone();
    let progress = events.progress.lock_ignore_poison().clone();
    let peak = peak_in_flight(&progress);
    log::info!("full-concurrency copy: finished in {elapsed:?}, peak in-flight {peak}");

    // Collect every failure before reporting, so one red run tells the whole
    // story instead of one file at a time.
    let mut problems: Vec<String> = Vec::new();
    if !errors.is_empty() {
        problems.push(format!("the copy emitted {} error events: {errors:?}", errors.len()));
    }
    if peak < MIN_PEAK_IN_FLIGHT {
        problems.push(format!(
            "the concurrency window peaked at {peak} in flight, under the floor of {MIN_PEAK_IN_FLIGHT}: \
             this batch never ran at full concurrency, so it isn't the shape that wedges \
             (src={}, dst={}, {} progress events seen)",
            source_vol.max_concurrent_ops(),
            dest_vol.max_concurrent_ops(),
            progress.len(),
        ));
    }

    let landed = smb_vol
        .list_directory(Path::new(&base), None)
        .await
        .expect("list the destination");
    let leftovers: Vec<&str> = landed
        .iter()
        .map(|entry| entry.name.as_str())
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect();
    if !leftovers.is_empty() {
        problems.push(format!(
            "{} staging temps survived the copy: {leftovers:?}",
            leftovers.len()
        ));
    }
    if !staging_seen.load(Ordering::SeqCst) {
        problems.push(format!(
            "no `.cmdr-tmp-*` was ever observed on the destination, so the clean-listing check above \
             proves nothing: the {large_bytes}-byte sources were supposed to stage \
             // allowed-pluralize-noun: a failure diagnostic; `FILE_COUNT` is a compile-time constant well above 1.
            (max_write={max_write}, {} of {FILE_COUNT} files, copy took {elapsed:?})",
            FILE_COUNT.div_ceil(LARGE_EVERY),
        ));
    }
    if landed.len() != FILE_COUNT {
        problems.push(format!(
            "the destination holds {} entries, expected exactly {FILE_COUNT}",
            landed.len()
        ));
    }

    let landed_size = |name: &str| -> String {
        landed
            .iter()
            .find(|entry| entry.name == name)
            .map_or_else(|| "missing".to_owned(), |entry| format!("{:?}", entry.size))
    };
    for (index, len) in source_sizes.iter().copied().enumerate() {
        let name = format!("f-{index:04}.bin");
        let path = format!("{base}/{name}");
        let expected = hash_bytes(&expected_content(index, len));
        let actual = hash_volume_file(&*dest_vol, Path::new(&path)).await;
        if actual != expected {
            // A short hash prefix plus the landed size is what tells the two
            // failure shapes apart at a glance: a truncated tail (size differs)
            // from a chunk that came off another task's buffer (size matches,
            // hash doesn't).
            problems.push(format!(
                // allowed-pluralize-noun: a byte count in a failure diagnostic; the compact form is the point.
                "{name} is not byte-identical: expected {len} bytes hashing {:02x?}…, \
                 got size {} hashing {:02x?}…",
                &expected[..4],
                landed_size(&name),
                &actual[..4],
            ));
        }
    }

    cleanup(Arc::clone(&smb_vol), base.clone()).await;
    assert!(
        problems.is_empty(),
        "a {FILE_COUNT}-file full-concurrency copy onto SMB left {} problems:\n  - {}",
        problems.len(),
        problems.join("\n  - "),
    );
}

/// The test OF the safety net above: a copy that will never finish is caught by
/// the bound, and the dump names WHERE it stopped.
///
/// Without this, `COPY_DEADLINE` and the probe dump are untested scaffolding —
/// nothing would notice if the deadline stopped firing, if `abort()` stopped
/// releasing the suite, or if the dump came back empty because the probe had
/// already deregistered. All three turn the sibling test back into something that
/// hangs a suite for hours, which is what this whole milestone exists to stop.
///
/// The wedge is staged through the production pause gate rather than a stalled
/// server: what is under test is the harness's behaviour when a copy does not
/// finish, and a pause reaches that state deterministically, on the real code
/// path, without holding the shared Docker stack hostage. The dump it produces is
/// the same dump a genuine stall produces, from the same probe.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_wedged_copy_is_caught_and_names_its_phase() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let dest_vol: Arc<dyn Volume> = smb_vol.clone();

    // Enough sources to take the concurrent path and fill a window; they never
    // finish, so their size is irrelevant.
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    let mut source_paths = Vec::new();
    for index in 0..32 {
        let name = format!("w-{index:03}.bin");
        std::fs::write(local_dir.path().join(&name), expected_content(index, SMALL_FILE_BYTES)).unwrap();
        source_paths.push(PathBuf::from(name));
    }
    let source_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "src",
        local_dir.path().to_path_buf(),
    ));

    let guard = TestOperationGuard::register("smb-wedged-copy");
    let operation_id = guard.id().to_owned();
    let state = Arc::clone(guard.state());
    let events = Arc::new(CollectorEventSink::new());

    // Park the operation before it starts, so it can never reach completion.
    state.pause_gate.pause();

    let mut copy = tokio::spawn({
        let events = Arc::clone(&events) as Arc<dyn crate::file_system::OperationEventSink>;
        let operation_id = operation_id.clone();
        let state = Arc::clone(&state);
        let source_vol = Arc::clone(&source_vol);
        let dest_vol = Arc::clone(&dest_vol);
        let dest = base.clone();
        async move {
            copy_volumes_with_progress(
                events,
                &operation_id,
                &state,
                source_vol,
                &source_paths,
                dest_vol,
                Path::new(&dest),
                &VolumeCopyConfig::default(),
            )
            .await
        }
    });

    let outcome = await_copy_or_probe_dump(&mut copy, &operation_id, WEDGE_DEADLINE).await;

    // Let the abandoned tasks wind down before the share is cleaned up.
    state.pause_gate.resume();
    ensure_clean(&smb_vol, &base).await;

    let dump = match outcome {
        Ok(result) => panic!("a paused copy must not complete inside {WEDGE_DEADLINE:?}, got {result:?}"),
        Err(dump) => dump,
    };

    // What a human needs off a red run: which operation, how full the window was,
    // and what every in-flight task was doing. `parked(pause)` is this wedge's
    // answer; a silent server's would be `streaming` or `opening-source`.
    assert!(
        dump.contains(&operation_id),
        "the dump must name the operation:\n{dump}"
    );
    assert!(
        dump.contains("paused=true"),
        "the dump must say the operation was parked:\n{dump}"
    );
    assert!(
        dump.contains("driver="),
        "the dump must say what the driver was doing:\n{dump}"
    );
    assert!(
        dump.contains("in_flight="),
        "the dump must say how full the concurrency window was:\n{dump}"
    );
    assert!(
        dump.contains("parked(pause)"),
        "the dump must name the phase every task is stuck in:\n{dump}"
    );
}
