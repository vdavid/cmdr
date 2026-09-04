//! The transfer scenarios both network backends owe, written once.
//!
//! `webdav_transfer_integration_test.rs` and `sftp_transfer_integration_test.rs`
//! connect their own fixture and hand the live volume to a function here. The
//! scenarios themselves are backend-blind: everything they touch is
//! `dyn Volume`, so a claim proved against one server is proved in the same
//! words against the other, and neither copy can drift.
//!
//! ❗ **The cells stay in the two backend files.** The integration lane selects
//! the app crate's Docker cells by the `webdav_integration_` /
//! `sftp_integration_` name prefix (`scripts/check/checks/fixture-lane-coverage.go`),
//! so a scenario promoted into a `#[tokio::test]` here would never run.
//!
//! Every scenario checksums BOTH ends. A copy that lands a file of the right
//! length full of the wrong bytes is a data-loss bug that an `exists()`
//! assertion reports as a pass.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::staging::is_staging_temp_name;
use cmdr_fs::volume::Volume;
use sha2::{Digest, Sha256};

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::network_gated_source_test_support::{CANCEL_PAYLOAD_BYTES, gated_upload};
use super::state::{cancel_write_operation, resolve_write_conflict};
use super::types::{ConflictResolution, ConflictResolutionOutcome, VolumeCopyConfig};
use crate::file_system::volume::LocalPosixVolume;
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;
use crate::test_support::TestDir;

/// How long a scenario waits for one copy to reach its terminal event.
///
/// ❗ Deliberately UNDER the workspace-wide 8 s nextest cap (`.config/nextest.toml`):
/// a budget above it is unreachable, because nextest SIGKILLs the process first
/// and the failure reads as a bare "test timed out" with nothing about which
/// wait starved. Every cell here lands in 0.3-2.5 s against the fixture stack,
/// so this is roughly 3x headroom AND still says what it was waiting for.
const SETTLE_BUDGET: Duration = Duration::from_secs(6);

/// The same, for the wait on a clash the copy has to raise. Shorter, so a cell
/// whose conflict never comes still has room to say so before the cap lands.
const CONFLICT_BUDGET: Duration = Duration::from_secs(5);

// ── Bytes ────────────────────────────────────────────────────────────

/// A payload whose every position says where it belongs, so a hole or a
/// duplicated span shifts content the checksum can't miss (and a mismatch is
/// diagnosable by eye).
///
/// `tag` goes in every line, so two files of the same length differ: a copy that
/// crosses two sources' bytes lands the right length and the wrong content, and
/// without a per-file tag both checksums would still match.
pub(super) fn self_describing_bytes(len: usize, tag: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut line = 0u64;
    while out.len() < len {
        out.extend_from_slice(format!("{tag} {line:015}\n").as_bytes());
        line += 1;
    }
    out.truncate(len);
    out
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Every byte at `path`, read back through the volume's own streaming path
/// (which is what a copy uses), so the check covers the same machinery.
pub(super) async fn read_all(volume: &dyn Volume, path: &Path) -> Vec<u8> {
    let mut stream = volume
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("reading {} back: {e:?}", path.display()));
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.unwrap_or_else(|e| panic!("chunk of {}: {e:?}", path.display())));
    }
    out
}

// ── Trees ────────────────────────────────────────────────────────────

/// What a whole subtree holds, as sorted `"<relative path>\t<sha256 | dir>"`
/// lines.
///
/// The one comparison a tree copy is actually about: it catches a missing file,
/// a file that landed one level too high, a directory that never got made, and
/// wrong bytes, all as a plain `assert_eq!` on two `Vec<String>`s that names the
/// first difference itself.
pub(super) fn tree_fingerprint<'a>(
    volume: &'a dyn Volume,
    root: &'a Path,
) -> Pin<Box<dyn Future<Output = Vec<String>> + Send + 'a>> {
    Box::pin(async move {
        let mut lines = Vec::new();
        collect_fingerprint(volume, root, "", &mut lines).await;
        lines.sort();
        lines
    })
}

fn collect_fingerprint<'a>(
    volume: &'a dyn Volume,
    dir: &'a Path,
    prefix: &'a str,
    out: &'a mut Vec<String>,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        let entries = volume
            .list_directory(dir, None)
            .await
            .unwrap_or_else(|e| panic!("listing {}: {e:?}", dir.display()));
        for entry in entries {
            let relative = format!("{prefix}{}", entry.name);
            let child = dir.join(&entry.name);
            if entry.is_directory {
                out.push(format!("{relative}/\tdir"));
                let deeper = format!("{relative}/");
                collect_fingerprint(volume, &child, &deeper, out).await;
            } else {
                let bytes = read_all(volume, &child).await;
                out.push(format!("{relative}\t{}", sha256(&bytes)));
            }
        }
    })
}

/// Removes `dir` and everything under it, deepest first, so a cell leaves the
/// shared export as it found it.
pub(super) fn clean_deep<'a>(volume: &'a dyn Volume, dir: &'a Path) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if let Ok(entries) = volume.list_directory(dir, None).await {
            for entry in entries {
                clean_deep(volume, &dir.join(&entry.name)).await;
            }
        }
        let _ = volume.delete(dir).await;
    })
}

/// Names the entries a finished operation must never have left behind: its own
/// staging siblings, under either marker.
fn staging_leftovers(entries: &[crate::file_system::listing::FileEntry]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| is_staging_temp_name(&e.name))
        .map(|e| e.name.clone())
        .collect()
}

/// Asserts nothing of Cmdr's own scratch survives at `dir`.
pub(super) async fn assert_no_staging_litter(volume: &dyn Volume, dir: &Path, what: &str) {
    let entries = volume
        .list_directory(dir, None)
        .await
        .unwrap_or_else(|e| panic!("listing {} after {what}: {e:?}", dir.display()));
    let litter = staging_leftovers(&entries);
    assert!(
        litter.is_empty(),
        "{what}: {} still holds Cmdr scratch {litter:?}",
        dir.display()
    );
}

// ── Driving one copy ─────────────────────────────────────────────────

/// One `copy_between_volumes` in flight, with the sink it reports through.
pub(super) struct RunningCopy {
    pub(super) events: Arc<CollectorEventSink>,
    pub(super) operation_id: String,
    label: String,
}

impl RunningCopy {
    /// Waits for the operation's `write-settled`, which fires after its terminal
    /// event and after every cleanup the driver owns. Anything the destination
    /// still holds at that point, it means to hold.
    pub(super) async fn settle(&self) {
        crate::test_support::wait_until_async(SETTLE_BUDGET, "the copy to settle", || {
            !self.events.settled.lock_ignore_poison().is_empty()
        })
        .await;
    }

    /// Fails loudly with whatever the sink collected, since a transfer reports
    /// through events rather than by panicking.
    pub(super) fn assert_no_errors(&self) {
        let errors: Vec<String> = self
            .events
            .errors
            .lock_ignore_poison()
            .iter()
            .map(|e| format!("{:?}", e.error))
            .collect();
        assert!(errors.is_empty(), "{}: the copy reported {errors:?}", self.label);
    }

    /// The clash the operation is parked on, once it has raised one.
    ///
    /// ❗ Waits for the clash OR for the operation to end without one, and then
    /// asserts. A bare wait on the clash would report a destination probe that
    /// never happened as a TIMEOUT, which on a fixture stack four agents share
    /// is indistinguishable from a slow container. Ending the wait the moment
    /// the copy settles turns that case into a sentence about what the copy did.
    async fn await_one_conflict(&self) -> super::types::WriteConflictEvent {
        crate::test_support::wait_until_async(
            CONFLICT_BUDGET,
            "the copy to raise its clash or settle without one",
            || {
                !self.events.conflicts.lock_ignore_poison().is_empty()
                    || !self.events.settled.lock_ignore_poison().is_empty()
            },
        )
        .await;
        let conflicts = self.events.conflicts.lock_ignore_poison();
        let Some(clash) = conflicts.first() else {
            panic!(
                "{}: the copy ran to the end without ever asking about the name that was already taken at the destination",
                self.label
            );
        };
        clash.clone()
    }

    /// Answers that clash the way a person clicking the dialog would, and
    /// insists the answer actually reached the parked operation.
    fn answer(&self, conflict: &super::types::WriteConflictEvent, resolution: ConflictResolution) {
        let outcome = resolve_write_conflict(&self.operation_id, conflict.conflict_id, resolution, false);
        assert_eq!(
            outcome,
            ConflictResolutionOutcome::Resolved,
            "{}: the {resolution:?} answer must reach the parked operation",
            self.label
        );
    }
}

/// Starts one copy through the app's own entry point and hands back its handle.
pub(super) async fn start_copy(
    label: &str,
    source: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> RunningCopy {
    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();

    let started = super::transfer::volume::copy_between_volumes(
        events,
        format!("{label}-source"),
        source,
        source_paths,
        format!("{label}-dest"),
        dest,
        dest_path,
        config,
        Initiator::User,
        None,
    )
    .await
    .unwrap_or_else(|e| panic!("{label}: the copy must START; it was refused with {e:?}"));

    RunningCopy {
        events: collector,
        operation_id: started.operation_id,
        label: label.to_string(),
    }
}

/// Starts one copy, waits it out, and insists it reported nothing.
pub(super) async fn run_copy(
    label: &str,
    source: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest: Arc<dyn Volume>,
    dest_path: PathBuf,
) -> RunningCopy {
    let running = start_copy(
        label,
        source,
        source_paths,
        dest,
        dest_path,
        VolumeCopyConfig::default(),
    )
    .await;
    running.settle().await;
    running.assert_no_errors();
    running
}

// ── The tree a scenario copies ───────────────────────────────────────

/// One file in the fixture tree: where it sits and how many bytes it holds.
///
/// The empty file and the two-level nesting are the parts that matter. A zero
/// byte file is where a "did we write anything?" heuristic silently drops a
/// file, and a grandchild directory is where a merge that flattens one level
/// still looks like a success from the top.
const TREE: [(&str, usize); 4] = [
    ("top.bin", 200_000),
    ("empty.bin", 0),
    ("nested/deep.bin", 64_000),
    ("nested/deeper/leaf.bin", 17),
];

/// Builds the fixture tree under `root` on local disk.
fn seed_local_tree(root: &Path) {
    for (relative, len) in TREE {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("making {}: {e:?}", parent.display()));
        }
        std::fs::write(&path, self_describing_bytes(len, relative))
            .unwrap_or_else(|e| panic!("seeding {}: {e:?}", path.display()));
    }
}

/// Builds the same tree under `root` on a live server.
async fn seed_remote_tree(volume: &dyn Volume, root: &Path) {
    for (relative, len) in TREE {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            volume
                .create_directory_all(parent)
                .await
                .unwrap_or_else(|e| panic!("making {} on the server: {e:?}", parent.display()));
        }
        volume
            .create_file(&path, &self_describing_bytes(len, relative))
            .await
            .unwrap_or_else(|e| panic!("seeding {} on the server: {e:?}", path.display()));
    }
}

/// Every line the fixture tree must fingerprint to, whichever end holds it.
fn expected_tree_lines() -> Vec<String> {
    let mut lines = vec!["nested/\tdir".to_string(), "nested/deeper/\tdir".to_string()];
    for (relative, len) in TREE {
        lines.push(format!("{relative}\t{}", sha256(&self_describing_bytes(len, relative))));
    }
    lines.sort();
    lines
}

// ── The scenarios ────────────────────────────────────────────────────

/// Copying a whole DIRECTORY onto a server, rather than the single file the
/// byte-path cells copy.
///
/// This is the shape every real F5 takes, and the one that spends
/// `create_directory_all`'s answer: the driver makes each level as it walks, and
/// a level that never got made silently strands everything under it. The
/// fingerprint compares structure and bytes in one, so a file that landed a
/// level too high fails here rather than passing an `exists()` check somewhere
/// else.
pub(super) async fn a_directory_tree_lands_intact_on_the_server(remote: Arc<dyn Volume>, dir: PathBuf) {
    let local_dir = TestDir::new("network_tree_onto_server");
    seed_local_tree(&local_dir.join("tree"));
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    // The fingerprint at the SOURCE end, taken through the volume the copy reads
    // from rather than from the buffers we wrote, so a bad seed can't make a bad
    // copy look good.
    let expected = expected_tree_lines();
    assert_eq!(
        tree_fingerprint(local.as_ref(), Path::new("tree")).await,
        expected,
        "the local seed must round-trip before the copy means anything"
    );

    run_copy(
        "tree-onto-server",
        Arc::clone(&local),
        vec![PathBuf::from("tree")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    assert_eq!(
        tree_fingerprint(remote.as_ref(), &dir.join("tree")).await,
        expected,
        "the tree on the server must match the one on disk, name for name and byte for byte"
    );
    assert_no_staging_litter(remote.as_ref(), &dir.join("tree"), "a tree copy onto the server").await;

    clean_deep(remote.as_ref(), &dir).await;
}

/// The same tree, coming the other way.
///
/// The walk is the server's here (`list_directory` per level, `open_read_stream`
/// per file), so a backend that reports a directory as a file, or loses a level
/// in its path mapping, fails here and nowhere in its own crate's suite.
pub(super) async fn a_directory_tree_lands_intact_off_the_server(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed_remote_tree(remote.as_ref(), &dir.join("tree")).await;

    let expected = expected_tree_lines();
    assert_eq!(
        tree_fingerprint(remote.as_ref(), &dir.join("tree")).await,
        expected,
        "the fixture seed must round-trip before the copy means anything"
    );

    let local_dir = TestDir::new("network_tree_off_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    run_copy(
        "tree-off-server",
        Arc::clone(&remote),
        vec![dir.join("tree")],
        Arc::clone(&local),
        PathBuf::from(""),
    )
    .await;

    assert_eq!(
        tree_fingerprint(local.as_ref(), Path::new("tree")).await,
        expected,
        "the tree on local disk must match the one on the server, name for name and byte for byte"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A copy stopped mid-upload leaves the destination exactly as it found it.
///
/// Both backends stage on a `.cmdr-tmp-*` sibling and clean up best-effort, and
/// the whole point of staging is that an interrupted transfer can't leave
/// something wearing the user's filename. Two claims, and the second is the one
/// the 2026-07-31 wedge broke: the real name was never created, AND no scratch
/// survived.
///
/// The source hands its bytes out one chunk per permit, so "the cancel landed
/// mid-file" is settled by the gate rather than by a race against the server.
pub(super) async fn a_cancelled_upload_leaves_nothing_behind(remote: Arc<dyn Volume>, dir: PathBuf) {
    let source = gated_upload(self_describing_bytes(CANCEL_PAYLOAD_BYTES, "cancelled")).await;

    let running = start_copy(
        "cancelled-upload",
        Arc::clone(&source.volume),
        vec![PathBuf::from("/big.bin")],
        Arc::clone(&remote),
        dir.clone(),
        VolumeCopyConfig::default(),
    )
    .await;

    // Two chunks move, and the third can't: from here the destination holds an
    // open, incomplete staging sibling and the copy cannot finish on its own.
    //
    // ❗ TWO, not one. The stream is only polled again once the destination has
    // taken the previous chunk, so a second hand-out is what proves the first
    // one actually reached the server. Waiting on one chunk would let a starved
    // run — bytes offered, nothing ingested — satisfy every "nothing was left
    // behind" claim below without a byte ever landing.
    source.gate.add_permits(2);
    crate::test_support::wait_until_async(SETTLE_BUDGET, "the upload to get two chunks into the server", || {
        source.handed_out.load(std::sync::atomic::Ordering::SeqCst) >= 2
    })
    .await;
    cancel_write_operation(&running.operation_id, false);
    // Let the source answer again, so a backend that only notices the cancel on
    // its next chunk isn't left parked on a permit that never comes.
    source.gate.add_permits(1_000);
    running.settle().await;

    // ❗ The premise, asserted rather than assumed: a copy that had already
    // finished would satisfy every claim below for the wrong reason.
    assert!(
        running.events.complete.lock_ignore_poison().is_empty(),
        "the cancel must land while the upload is still running; it completed instead"
    );
    assert!(
        !running.events.cancelled.lock_ignore_poison().is_empty(),
        "a cancelled copy says so"
    );

    let entries = remote
        .list_directory(&dir, None)
        .await
        .unwrap_or_else(|e| panic!("listing {} after the cancel: {e:?}", dir.display()));
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        !names.contains(&"big.bin"),
        "the user's filename must never appear for a copy that didn't finish; the server holds {names:?}"
    );
    let litter = staging_leftovers(&entries);
    assert!(
        litter.is_empty(),
        "a cancelled upload must take its staging sibling away; the server still holds {litter:?}"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// An Overwrite answer, taken the whole way through the real pipeline.
///
/// The conflict is found at the destination by the driver, the question travels
/// out as a `write-conflict` event, the answer comes back through
/// `resolve_write_conflict` naming that clash by id, and the new bytes replace
/// the old ones. The safe-replace path underneath keeps the original in place
/// until the replacement is fully written, so the destination must end up
/// holding the NEW bytes and nothing else.
pub(super) async fn an_overwrite_answer_replaces_the_destination_bytes(remote: Arc<dyn Volume>, dir: PathBuf) {
    let old = self_describing_bytes(40_000, "the-old-bytes");
    let new = self_describing_bytes(180_000, "the-new-bytes");
    remote
        .create_file(&dir.join("target.bin"), &old)
        .await
        .expect("seeding the destination file on the server");

    let local_dir = TestDir::new("network_overwrite_answer");
    std::fs::write(local_dir.join("target.bin"), &new).expect("seeding the local file");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    // `Stop` is what makes the driver ask rather than decide, which is the whole
    // point: this cell is about the answer travelling, not about a config flag.
    let running = start_copy(
        "overwrite-answer",
        Arc::clone(&local),
        vec![PathBuf::from("target.bin")],
        Arc::clone(&remote),
        dir.clone(),
        VolumeCopyConfig::default(),
    )
    .await;

    let clash = running.await_one_conflict().await;
    assert!(
        clash.destination_path.ends_with("target.bin"),
        "the clash must name the file that is actually in the way, got {}",
        clash.destination_path
    );
    running.answer(&clash, ConflictResolution::Overwrite);
    running.settle().await;
    running.assert_no_errors();

    let landed = read_all(remote.as_ref(), &dir.join("target.bin")).await;
    assert_eq!(
        sha256(&landed),
        sha256(&new),
        "an Overwrite answer must leave the SOURCE bytes at the destination"
    );
    assert_no_staging_litter(remote.as_ref(), &dir, "an overwrite through the pipeline").await;

    clean_deep(remote.as_ref(), &dir).await;
}

/// A destination folder the user already had is probed name by name, even when
/// several sources ride at once.
///
/// ❗ This is what `create_directory_all`'s `DirectoryCreation` answer buys, and
/// what a wrong one costs. The concurrent driver skips its per-file destination
/// probe for a destination directory THIS operation created (nothing the user
/// had can be inside a folder that didn't exist a moment ago). A backend that
/// answered `Created` for a folder that was already there would spend that skip
/// on the user's own files: every name would look free and every clash would
/// become a silent overwrite, with no prompt anywhere.
///
/// Three sources is the threshold the concurrent path runs at, so the cell
/// copies three, one of which clashes.
pub(super) async fn a_pre_existing_destination_still_probes_each_name(remote: Arc<dyn Volume>, dir: PathBuf) {
    let landing = dir.join("landing");
    remote
        .create_directory_all(&landing)
        .await
        .expect("making the destination folder on the server");
    let kept = self_describing_bytes(30_000, "the-file-the-user-already-had");
    remote
        .create_file(&landing.join("keepme.bin"), &kept)
        .await
        .expect("seeding the pre-existing destination file");

    let local_dir = TestDir::new("network_pre_existing_dest");
    let arriving: [(&str, usize); 3] = [("a.bin", 21_000), ("b.bin", 22_000), ("keepme.bin", 23_000)];
    for (name, len) in arriving {
        std::fs::write(local_dir.join(name), self_describing_bytes(len, name))
            .unwrap_or_else(|e| panic!("seeding {name}: {e:?}"));
    }
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    let running = start_copy(
        "pre-existing-destination",
        Arc::clone(&local),
        arriving.iter().map(|(name, _)| PathBuf::from(name)).collect(),
        Arc::clone(&remote),
        landing.clone(),
        VolumeCopyConfig::default(),
    )
    .await;

    let clash = running.await_one_conflict().await;
    assert!(
        clash.destination_path.ends_with("keepme.bin"),
        "the only clash here is the name the user already had, got {}",
        clash.destination_path
    );
    running.answer(&clash, ConflictResolution::Skip);
    running.settle().await;
    running.assert_no_errors();

    assert_eq!(
        sha256(&read_all(remote.as_ref(), &landing.join("keepme.bin")).await),
        sha256(&kept),
        "a Skip answer must leave the user's own file exactly as it was"
    );
    for (name, len) in arriving.iter().take(2) {
        assert_eq!(
            sha256(&read_all(remote.as_ref(), &landing.join(name)).await),
            sha256(&self_describing_bytes(*len, name)),
            "{name} had no clash, so it must have landed"
        );
    }
    assert_eq!(
        running.events.conflicts.lock_ignore_poison().len(),
        1,
        "only the one name that was taken may raise a question"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// Names a copy has to carry unchanged, in both directions.
///
/// Every one of these is a character that means something else to some layer in
/// between: `&`, `+`, `%`, and `#` are all URL syntax, and the non-ASCII name
/// crosses percent-encoding both ways. A name that survives the backend's own
/// verbs can still be mangled by the transfer pipeline, which derives a staging
/// sibling from it and matches it against a destination listing.
///
/// ❌ `?` is deliberately absent: it splits a URL from its query, and a fixture
/// server refusing it would say nothing about Cmdr.
const AWKWARD_NAMES: [&str; 6] = [
    "plain space.bin",
    "tom & jerry.bin",
    "100% sure.bin",
    "a+b.bin",
    "hash#tag.bin",
    "naïve résumé ü.bin",
];

/// An awkwardly-named file, and an empty one, survive a full round trip.
///
/// Copies a folder of them onto the server and straight back off, comparing
/// fingerprints at all three stops. A name mangled on the way out and mangled
/// back on the way in would still pass a one-way check; a round trip that
/// matches the ORIGINAL at both stops can't.
pub(super) async fn awkward_names_survive_a_round_trip(remote: Arc<dyn Volume>, dir: PathBuf) {
    let out_dir = TestDir::new("network_awkward_names_out");
    let source_root = out_dir.join("names");
    std::fs::create_dir_all(&source_root).expect("making the local source folder");
    for (index, name) in AWKWARD_NAMES.iter().enumerate() {
        std::fs::write(source_root.join(name), self_describing_bytes(4_000 + index * 97, name))
            .unwrap_or_else(|e| panic!("seeding {name}: {e:?}"));
    }
    // A zero-byte file rides along: it is where a "did anything arrive?" check
    // silently drops a file the user very much still owns.
    std::fs::write(source_root.join("nothing at all.bin"), b"").expect("seeding the empty file");

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*out_dir));
    let expected = tree_fingerprint(source.as_ref(), Path::new("names")).await;
    assert_eq!(
        expected.len(),
        AWKWARD_NAMES.len() + 1,
        "the local seed must hold every name once, got {expected:?}"
    );

    run_copy(
        "awkward-names-onto-server",
        Arc::clone(&source),
        vec![PathBuf::from("names")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;
    assert_eq!(
        tree_fingerprint(remote.as_ref(), &dir.join("names")).await,
        expected,
        "every name must reach the server spelled the way the user spelled it"
    );

    let back_dir = TestDir::new("network_awkward_names_back");
    let back: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*back_dir));
    run_copy(
        "awkward-names-off-server",
        Arc::clone(&remote),
        vec![dir.join("names")],
        Arc::clone(&back),
        PathBuf::from(""),
    )
    .await;
    assert_eq!(
        tree_fingerprint(back.as_ref(), Path::new("names")).await,
        expected,
        "and come back the same way, rather than decoded twice or encoded twice"
    );

    clean_deep(remote.as_ref(), &dir).await;
}
