//! What copy and move MEAN on a network server, written once for every backend.
//!
//! `network_transfer_test_support.rs` proves the byte path: bytes land, a tree
//! lands, a cancel leaves nothing. This file proves the semantics a person
//! relies on when the destination already holds their files: a folder merges
//! rather than replacing, a policy decides per file, a move sweeps only what it
//! actually delivered, and a same-server move or copy stays on the server.
//!
//! Same contract as its sibling: every scenario takes a live `Arc<dyn Volume>`
//! and a scratch directory on it that nothing else in the run touches, and every
//! scenario leaves that directory gone. ❗ The `#[tokio::test]` cells stay in the
//! backend files, because the integration lane selects them by name prefix
//! (`sftp_integration_`, `webdav_integration_`, `smb_integration_`).
//!
//! Every scenario that ends with both trees in place asserts through the safety
//! oracle (`transfer::volume::assert_operation_was_safe`), so "no byte the user
//! didn't approve is gone" is stated in one place for every backend.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;

use super::super::event_sinks::CollectorEventSink;
use super::super::state::WriteOperationState;
use super::super::transfer::volume::{
    SafetySpec, assert_operation_was_safe, copy_volumes_with_progress, move_volumes_with_progress,
    move_within_same_volume_with_progress,
};
use super::super::types::{ConflictResolution, VolumeCopyConfig};
use super::network_transfer_test_support::{
    assert_no_staging_litter, clean_deep, read_all, self_describing_bytes, sha256, tree_fingerprint,
};
use crate::file_system::volume::LocalPosixVolume;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

// ── Seeding and reading ──────────────────────────────────────────────

/// Writes `files` under `root` on any volume, making each parent first.
///
/// Relative paths with `/` separators, so one table seeds a local tree and a
/// server tree the same way.
pub(super) async fn seed(volume: &dyn Volume, root: &Path, files: &[(&str, &[u8])]) {
    // An empty root is the volume's own root, which is always there.
    if root != Path::new("") {
        volume
            .create_directory_all(root)
            .await
            .unwrap_or_else(|e| panic!("making {}: {e:?}", root.display()));
    }
    for (relative, bytes) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent().filter(|p| *p != Path::new("") && *p != root) {
            volume
                .create_directory_all(parent)
                .await
                .unwrap_or_else(|e| panic!("making {}: {e:?}", parent.display()));
        }
        volume
            .create_file(&path, bytes)
            .await
            .unwrap_or_else(|e| panic!("seeding {}: {e:?}", path.display()));
    }
}

/// The bytes at `path`, or `None` when nothing is there.
pub(super) async fn try_read(volume: &dyn Volume, path: &Path) -> Option<Vec<u8>> {
    if !volume.exists(path).await {
        return None;
    }
    Some(read_all(volume, path).await)
}

/// The names `dir` holds, sorted, byte for byte.
pub(super) async fn names_in(volume: &dyn Volume, dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = volume
        .list_directory(dir, None)
        .await
        .unwrap_or_else(|e| panic!("listing {}: {e:?}", dir.display()))
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    names
}

/// A local volume rooted at a fresh temp dir. The `TestDir` has to outlive it.
pub(super) fn local_volume(what: &str) -> (TestDir, Arc<dyn Volume>) {
    let dir = TestDir::new(what);
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*dir));
    (dir, volume)
}

fn path_str(path: &Path) -> String {
    path.display().to_string()
}

// ── Driving one operation ────────────────────────────────────────────

/// Which of the three transfer drivers a scenario runs.
#[derive(Clone, Copy, Debug)]
pub(super) enum Transfer {
    Copy,
    Move,
    /// Source and destination are the same volume: the rename-merge engine.
    MoveWithinOneVolume,
}

/// What one finished operation reported.
pub(super) struct Finished {
    pub(super) events: Arc<CollectorEventSink>,
    pub(super) state: Arc<WriteOperationState>,
}

impl Finished {
    /// How many dir-vs-dir clashes the operation asked about. Always zero: a
    /// folder onto a folder merges without a question.
    pub(super) fn folder_prompts(&self) -> usize {
        self.events
            .conflicts
            .lock_ignore_poison()
            .iter()
            .filter(|c| c.source_is_directory && c.destination_is_directory)
            .count()
    }
}

/// Runs one transfer to the end under `policy` and insists it succeeded.
///
/// The drivers here are the ones the managed ops run inside; calling them
/// directly returns their `Result`, which is what a policy cell asserts on.
pub(super) async fn transfer(
    label: &str,
    kind: Transfer,
    source: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest: &Arc<dyn Volume>,
    dest_path: &Path,
    policy: ConflictResolution,
) -> Finished {
    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let config = VolumeCopyConfig {
        conflict_resolution: policy,
        ..VolumeCopyConfig::default()
    };
    let result = match kind {
        Transfer::Copy => copy_volumes_with_progress(
            events.clone(),
            label,
            &state,
            Arc::clone(source),
            source_paths,
            Arc::clone(dest),
            dest_path,
            &config,
        )
        .await
        .map_err(|e| format!("{e:?}")),
        Transfer::Move => move_volumes_with_progress(
            events.clone(),
            label,
            &state,
            Arc::clone(source),
            source_paths,
            Arc::clone(dest),
            dest_path,
            &config,
        )
        .await
        .map_err(|e| format!("{e:?}")),
        Transfer::MoveWithinOneVolume => {
            assert!(
                Arc::ptr_eq(source, dest),
                "{label}: a same-volume move takes ONE volume"
            );
            move_within_same_volume_with_progress(
                events.clone(),
                label,
                &state,
                Arc::clone(source),
                source_paths,
                dest_path,
                &config,
            )
            .await
            .map_err(|e| format!("{e:?}"))
        }
    };
    assert!(result.is_ok(), "{label}: the {kind:?} must finish, got {result:?}");
    Finished { events, state }
}

// ── The merge fixture ────────────────────────────────────────────────

/// The folder the user already has on the server: two files nothing may touch,
/// one at each depth, and one the source is about to shadow.
const DEST_ALBUM: [(&str, &[u8]); 3] = [
    ("keep.txt", b"DEST-keep"),
    ("sub/keep2.txt", b"DEST-keep2"),
    ("clash.txt", b"DEST-clash"),
];

/// The folder arriving: a new file at each depth, and the one that clashes.
///
/// ❗ The clash is LARGER than the destination's, which is what makes
/// `OverwriteSmaller` reduce to an overwrite for it.
const SOURCE_ALBUM: [(&str, &[u8]); 3] = [
    ("fresh.txt", b"SRC-fresh"),
    ("sub/fresh2.txt", b"SRC-fresh2"),
    ("clash.txt", b"SRC-clash-larger"),
];

/// The deliveries that hold under every policy: the source files nothing
/// clashes with.
const DELIVERED_EVERYWHERE: [(&str, &[u8]); 2] = [("/fresh.txt", b"SRC-fresh"), ("/sub/fresh2.txt", b"SRC-fresh2")];

/// The dest-only files, which the merge invariant says are never touched.
const DEST_ONLY: [(&str, &[u8]); 2] = [("/keep.txt", b"DEST-keep"), ("/sub/keep2.txt", b"DEST-keep2")];

/// Every file the source starts with, for the oracle's first clause.
const SOURCE_FILES: [(&str, &[u8]); 3] = [
    ("/fresh.txt", b"SRC-fresh"),
    ("/sub/fresh2.txt", b"SRC-fresh2"),
    ("/clash.txt", b"SRC-clash-larger"),
];

/// A local `album` merging into the server's `album` under `policy`, copied or
/// moved. Returns the local side so a move cell can look at what's left there.
async fn merge_album_onto_the_server(
    label: &str,
    kind: Transfer,
    remote: &Arc<dyn Volume>,
    dir: &Path,
    policy: ConflictResolution,
) -> (TestDir, Arc<dyn Volume>, Finished) {
    let album = dir.join("album");
    seed(remote.as_ref(), &album, &DEST_ALBUM).await;
    let (local_dir, local) = local_volume(label);
    seed(local.as_ref(), Path::new("album"), &SOURCE_ALBUM).await;

    let finished = transfer(label, kind, &local, &[PathBuf::from("album")], remote, dir, policy).await;

    assert_eq!(
        finished.folder_prompts(),
        0,
        "{label}: a folder onto a folder merges without asking"
    );
    assert_operation_was_safe(
        &local,
        remote,
        &SafetySpec {
            label,
            source_root: "album",
            dest_root: &path_str(&album),
            source_files: &SOURCE_FILES,
            delivered: &DELIVERED_EVERYWHERE,
            untouched_dest: &DEST_ONLY,
        },
    )
    .await;
    assert_no_staging_litter(remote.as_ref(), &album, label).await;
    assert_no_staging_litter(remote.as_ref(), &album.join("sub"), label).await;
    (local_dir, local, finished)
}

// ── The scenarios ────────────────────────────────────────────────────

/// FOLDER MERGE under Skip: the folder merges (it isn't skipped wholesale), the
/// clashing FILE is skipped, and the user's own files survive at every depth.
pub(super) async fn a_deep_clash_merge_under_skip_keeps_every_dest_only_file(remote: Arc<dyn Volume>, dir: PathBuf) {
    let (_local_dir, _local, finished) =
        merge_album_onto_the_server("merge-skip", Transfer::Copy, &remote, &dir, ConflictResolution::Skip).await;

    assert_eq!(
        try_read(remote.as_ref(), &dir.join("album/clash.txt")).await.as_deref(),
        Some(&b"DEST-clash"[..]),
        "a skipped clash keeps the user's bytes"
    );
    assert_eq!(
        finished.state.skipped_totals().0,
        1,
        "the one clash is reported skipped"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// FOLDER MERGE under Overwrite: only the clashing file is replaced, with the
/// source's bytes, and every dest-only file is untouched. Overwrite means MERGE
/// for a folder and REPLACE for a file.
pub(super) async fn a_deep_clash_merge_under_overwrite_replaces_only_the_clash(remote: Arc<dyn Volume>, dir: PathBuf) {
    merge_album_onto_the_server(
        "merge-overwrite",
        Transfer::Copy,
        &remote,
        &dir,
        ConflictResolution::Overwrite,
    )
    .await;

    assert_eq!(
        try_read(remote.as_ref(), &dir.join("album/clash.txt")).await.as_deref(),
        Some(&b"SRC-clash-larger"[..]),
        "an Overwrite puts the source's bytes on the clashing name"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// FOLDER MERGE under Rename: the clash lands beside the user's file under a
/// fresh name, so NOTHING is replaced. The oracle's first clause finds the
/// source's bytes wherever they landed.
pub(super) async fn a_rename_policy_lands_the_clash_beside_the_users_file(remote: Arc<dyn Volume>, dir: PathBuf) {
    merge_album_onto_the_server(
        "merge-rename",
        Transfer::Copy,
        &remote,
        &dir,
        ConflictResolution::Rename,
    )
    .await;

    let album = dir.join("album");
    assert_eq!(
        try_read(remote.as_ref(), &album.join("clash.txt")).await.as_deref(),
        Some(&b"DEST-clash"[..]),
        "a Rename never touches the file already on the name"
    );
    let names = names_in(remote.as_ref(), &album).await;
    let renamed: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("clash") && n.as_str() != "clash.txt")
        .collect();
    assert_eq!(
        renamed.len(),
        1,
        "the clash lands once, under a new name, beside the user's file; the folder holds {names:?}"
    );
    assert_eq!(
        try_read(remote.as_ref(), &album.join(renamed[0])).await.as_deref(),
        Some(&b"SRC-clash-larger"[..]),
        "and the renamed file holds the source's bytes"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// FOLDER MERGE under OverwriteSmaller: the clash replaces the destination only
/// because the destination is smaller, and a second clash where the
/// destination is LARGER is skipped. A conditional policy decides per file.
pub(super) async fn overwrite_smaller_replaces_only_the_smaller_destination(remote: Arc<dyn Volume>, dir: PathBuf) {
    // A second clash where the user's file is the BIGGER one.
    remote
        .create_directory_all(&dir.join("album"))
        .await
        .expect("making the album on the server");
    remote
        .create_file(&dir.join("album/big.txt"), b"DEST-big-and-longer-than-the-source")
        .await
        .expect("seeding the larger destination file");

    let (_local_dir, local) = local_volume("overwrite-smaller-extra");
    seed(local.as_ref(), Path::new("album"), &[("big.txt", b"SRC-big")]).await;
    transfer(
        "overwrite-smaller-extra",
        Transfer::Copy,
        &local,
        &[PathBuf::from("album")],
        &remote,
        &dir,
        ConflictResolution::OverwriteSmaller,
    )
    .await;
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("album/big.txt")).await.as_deref(),
        Some(&b"DEST-big-and-longer-than-the-source"[..]),
        "a destination larger than the source is skipped under OverwriteSmaller"
    );

    merge_album_onto_the_server(
        "merge-overwrite-smaller",
        Transfer::Copy,
        &remote,
        &dir,
        ConflictResolution::OverwriteSmaller,
    )
    .await;
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("album/clash.txt")).await.as_deref(),
        Some(&b"SRC-clash-larger"[..]),
        "a destination smaller than the source is replaced under OverwriteSmaller"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A MOVE merging onto the server under Skip spares the skipped source: it's
/// the ONLY copy of those bytes, so the sweep that follows the copy must leave
/// it, and the folder holding it, where they were.
pub(super) async fn a_move_merge_onto_the_server_spares_what_it_skipped(remote: Arc<dyn Volume>, dir: PathBuf) {
    let (_local_dir, local, _finished) = merge_album_onto_the_server(
        "move-merge-skip",
        Transfer::Move,
        &remote,
        &dir,
        ConflictResolution::Skip,
    )
    .await;

    assert_eq!(
        try_read(local.as_ref(), Path::new("album/clash.txt")).await.as_deref(),
        Some(&b"SRC-clash-larger"[..]),
        "❗ a skipped source file survives the move's sweep: it's the only copy"
    );
    assert!(
        !local.exists(Path::new("album/fresh.txt")).await,
        "a delivered source file is swept"
    );
    assert!(!local.exists(Path::new("album/sub/fresh2.txt")).await, "at every depth");
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("album/clash.txt")).await.as_deref(),
        Some(&b"DEST-clash"[..]),
        "and the user's file on the name keeps its bytes"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// The tree every whole-folder move carries: an empty file and two levels of
/// nesting, the parts a flattening or a "did anything arrive?" check drops.
const MOVED_TREE: [(&str, &[u8]); 4] = [
    ("top.bin", b"moved-top"),
    ("empty.bin", b""),
    ("nested/deep.bin", b"moved-deep"),
    ("nested/deeper/leaf.bin", b"moved-leaf"),
];

/// A whole folder MOVED onto the server lands intact and leaves nothing behind
/// on local disk.
pub(super) async fn a_folder_moved_onto_the_server_leaves_no_source(remote: Arc<dyn Volume>, dir: PathBuf) {
    let (_local_dir, local) = local_volume("move-onto-server");
    seed(local.as_ref(), Path::new("tree"), &MOVED_TREE).await;
    let expected = tree_fingerprint(local.as_ref(), Path::new("tree")).await;

    transfer(
        "move-onto-server",
        Transfer::Move,
        &local,
        &[PathBuf::from("tree")],
        &remote,
        &dir,
        ConflictResolution::Stop,
    )
    .await;

    assert_eq!(
        tree_fingerprint(remote.as_ref(), &dir.join("tree")).await,
        expected,
        "the moved tree must match the original, name for name and byte for byte"
    );
    assert!(
        !local.exists(Path::new("tree")).await,
        "a completed move leaves no source folder behind"
    );
    assert_no_staging_litter(remote.as_ref(), &dir.join("tree"), "a move onto the server").await;

    clean_deep(remote.as_ref(), &dir).await;
}

/// The same folder MOVED off the server: it lands on local disk intact, and the
/// server no longer holds it.
pub(super) async fn a_folder_moved_off_the_server_leaves_no_source(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed(remote.as_ref(), &dir.join("tree"), &MOVED_TREE).await;
    let expected = tree_fingerprint(remote.as_ref(), &dir.join("tree")).await;
    let (_local_dir, local) = local_volume("move-off-server");

    transfer(
        "move-off-server",
        Transfer::Move,
        &remote,
        &[dir.join("tree")],
        &local,
        Path::new(""),
        ConflictResolution::Stop,
    )
    .await;

    assert_eq!(
        tree_fingerprint(local.as_ref(), Path::new("tree")).await,
        expected,
        "the moved tree must match the original, name for name and byte for byte"
    );
    assert!(
        !remote.exists(&dir.join("tree")).await,
        "a completed move takes the folder off the server"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A SAME-SERVER move onto a folder that exists merges through server-side
/// renames: no folder prompt, the dest-only file survives, the clash follows
/// the policy, and the source folder survives because it still holds the
/// skipped file.
pub(super) async fn a_same_server_move_merges_without_a_folder_prompt(remote: Arc<dyn Volume>, dir: PathBuf) {
    let src_album = dir.join("src/album");
    let dst = dir.join("dst");
    seed(remote.as_ref(), &src_album, &SOURCE_ALBUM).await;
    seed(remote.as_ref(), &dst.join("album"), &DEST_ALBUM).await;

    let finished = transfer(
        "same-server-move-merge",
        Transfer::MoveWithinOneVolume,
        &remote,
        std::slice::from_ref(&src_album),
        &remote,
        &dst,
        ConflictResolution::Skip,
    )
    .await;

    assert_eq!(finished.folder_prompts(), 0, "a same-server folder merge never prompts");
    assert_operation_was_safe(
        &remote,
        &remote,
        &SafetySpec {
            label: "same-server-move-merge",
            source_root: &path_str(&src_album),
            dest_root: &path_str(&dst.join("album")),
            source_files: &SOURCE_FILES,
            delivered: &DELIVERED_EVERYWHERE,
            untouched_dest: &DEST_ONLY,
        },
    )
    .await;
    assert_eq!(
        try_read(remote.as_ref(), &dst.join("album/clash.txt")).await.as_deref(),
        Some(&b"DEST-clash"[..]),
        "the skipped clash keeps the user's bytes"
    );
    assert_eq!(
        try_read(remote.as_ref(), &src_album.join("clash.txt")).await.as_deref(),
        Some(&b"SRC-clash-larger"[..]),
        "and the skipped source stays where it was"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A SAME-SERVER move with nothing in the way moves the folder whole, deep
/// contents included, and the source is gone afterwards.
pub(super) async fn a_same_server_move_without_a_clash_moves_the_folder_whole(remote: Arc<dyn Volume>, dir: PathBuf) {
    let src_album = dir.join("src/album");
    let files: Vec<(String, Vec<u8>)> = (0..12)
        .flat_map(|i| {
            [
                (format!("f{i:02}.txt"), format!("top-{i}").into_bytes()),
                (format!("deep/g{i:02}.txt"), format!("deep-{i}").into_bytes()),
            ]
        })
        .collect();
    let table: Vec<(&str, &[u8])> = files.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
    seed(remote.as_ref(), &src_album, &table).await;
    let expected = tree_fingerprint(remote.as_ref(), &src_album).await;
    let dst = dir.join("dst");
    remote.create_directory(&dst).await.expect("making the destination");

    transfer(
        "same-server-move",
        Transfer::MoveWithinOneVolume,
        &remote,
        std::slice::from_ref(&src_album),
        &remote,
        &dst,
        ConflictResolution::Stop,
    )
    .await;

    assert_eq!(
        tree_fingerprint(remote.as_ref(), &dst.join("album")).await,
        expected,
        "the folder arrives whole"
    );
    assert!(!remote.exists(&src_album).await, "a fully moved source folder is gone");

    clean_deep(remote.as_ref(), &dir).await;
}

/// A SAME-SERVER copy (one `Arc` at both ends) duplicates a tree byte for byte
/// and leaves the source untouched. Where the server can copy for itself the
/// engine asks it to; where it can't, the bytes stream. Both must land the same
/// tree, so a backend cell can point this at both kinds of server.
pub(super) async fn a_same_server_copy_duplicates_a_tree(remote: Arc<dyn Volume>, dir: PathBuf) {
    let src = dir.join("src");
    let big = self_describing_bytes(300_000, "same-server-copy");
    seed(remote.as_ref(), &src.join("tree"), &MOVED_TREE).await;
    remote
        .create_file(&src.join("tree/big.bin"), &big)
        .await
        .expect("seeding the big file");
    let expected = tree_fingerprint(remote.as_ref(), &src.join("tree")).await;
    let dst = dir.join("dst");
    remote.create_directory(&dst).await.expect("making the destination");

    transfer(
        "same-server-copy",
        Transfer::Copy,
        &remote,
        &[src.join("tree")],
        &remote,
        &dst,
        ConflictResolution::Stop,
    )
    .await;

    assert_eq!(
        tree_fingerprint(remote.as_ref(), &dst.join("tree")).await,
        expected,
        "the copy matches the original, name for name and byte for byte"
    );
    assert_eq!(
        tree_fingerprint(remote.as_ref(), &src.join("tree")).await,
        expected,
        "and a copy never touches its source"
    );
    assert_no_staging_litter(remote.as_ref(), &dst.join("tree"), "a same-server copy").await;

    clean_deep(remote.as_ref(), &dir).await;
}

/// A copy into a destination folder that doesn't exist yet makes every missing
/// level, then lands the files there.
pub(super) async fn a_copy_into_a_missing_nested_destination_makes_every_level(remote: Arc<dyn Volume>, dir: PathBuf) {
    let dest = dir.join("incoming/2026/trip");
    let (_local_dir, local) = local_volume("missing-nested-dest");
    seed(
        local.as_ref(),
        Path::new(""),
        &[("a.txt", b"alpha"), ("b.txt", b"bravo")],
    )
    .await;

    transfer(
        "missing-nested-dest",
        Transfer::Copy,
        &local,
        &[PathBuf::from("a.txt"), PathBuf::from("b.txt")],
        &remote,
        &dest,
        ConflictResolution::Stop,
    )
    .await;

    for level in [dir.join("incoming"), dir.join("incoming/2026"), dest.clone()] {
        assert!(
            remote.is_directory(&level).await.unwrap_or(false),
            "{} must be a folder on the server",
            level.display()
        );
    }
    assert_eq!(
        try_read(remote.as_ref(), &dest.join("a.txt")).await.as_deref(),
        Some(&b"alpha"[..])
    );
    assert_eq!(
        try_read(remote.as_ref(), &dest.join("b.txt")).await.as_deref(),
        Some(&b"bravo"[..])
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A multi-megabyte file with an odd length makes the full round trip byte
/// exact: many read and write windows, and a last one that isn't full.
pub(super) async fn a_multi_megabyte_file_round_trips_byte_exact(remote: Arc<dyn Volume>, dir: PathBuf) {
    const LEN: usize = 6 * 1024 * 1024 + 13;
    let content = self_describing_bytes(LEN, "multi-megabyte");
    let (_out_dir, out) = local_volume("multi-megabyte-out");
    out.create_file(Path::new("big.bin"), &content)
        .await
        .expect("seeding the local file");

    transfer(
        "multi-megabyte-up",
        Transfer::Copy,
        &out,
        &[PathBuf::from("big.bin")],
        &remote,
        &dir,
        ConflictResolution::Stop,
    )
    .await;
    let (_back_dir, back) = local_volume("multi-megabyte-back");
    transfer(
        "multi-megabyte-down",
        Transfer::Copy,
        &remote,
        &[dir.join("big.bin")],
        &back,
        Path::new(""),
        ConflictResolution::Stop,
    )
    .await;

    let landed = read_all(back.as_ref(), Path::new("big.bin")).await;
    assert_eq!(landed.len(), LEN, "the round trip landed the wrong number of bytes");
    assert_eq!(
        sha256(&landed),
        sha256(&content),
        "the bytes back on disk must be the ones that left"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// Many files at once, at whatever concurrency the backend grants, each landing
/// with its OWN bytes: a mixed-up offset or a crossed pair of streams lands the
/// right length full of another file's content.
pub(super) async fn many_files_at_full_concurrency_land_intact(remote: Arc<dyn Volume>, dir: PathBuf) {
    const FILES: usize = 40;
    let (_local_dir, local) = local_volume("many-files");
    let mut names = Vec::new();
    for i in 0..FILES {
        let name = format!("file-{i:03}.bin");
        // Sizes from empty to a few read windows, so every path the writer has
        // runs at once.
        let len = (i * 7_919) % 180_000;
        local
            .create_file(Path::new(&name), &self_describing_bytes(len, &name))
            .await
            .expect("seeding a local file");
        names.push(PathBuf::from(name));
    }

    transfer(
        "many-files",
        Transfer::Copy,
        &local,
        &names,
        &remote,
        &dir,
        ConflictResolution::Stop,
    )
    .await;

    for name in &names {
        let name = name.to_string_lossy();
        let len = (name[5..8].parse::<usize>().expect("index in the name") * 7_919) % 180_000;
        assert_eq!(
            sha256(&read_all(remote.as_ref(), &dir.join(name.as_ref())).await),
            sha256(&self_describing_bytes(len, &name)),
            "{name} must hold its own bytes"
        );
    }
    assert_no_staging_litter(remote.as_ref(), &dir, "many files at once").await;

    clean_deep(remote.as_ref(), &dir).await;
}
