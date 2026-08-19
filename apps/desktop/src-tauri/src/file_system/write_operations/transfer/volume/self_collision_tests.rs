//! Duplicating in place on a volume: copying an item into the folder it already
//! lives in (MTP→MTP, SMB→SMB, USB→USB), and moving one there.
//!
//! The rule is the one `../DETAILS.md` § "Self-collision (duplicating in place)"
//! states; what's volume-specific is how identity is answered (no `dev+ino` out
//! here, so it's one volume, one parent directory, and a folded leaf) and that a renamed
//! directory needs no remap, because `merge_level` threads the destination down
//! through its own recursion.
//!
//! The copy tests drive the operation through a [`ConflictResponderSink`]
//! scripted to `Rename`: a self-collision must raise NO prompt at all, and the
//! sink lets that be an assertion instead of a Stop-mode deadlock when the rule
//! regresses.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::conflict_responder_test_support::ConflictResponderSink;
use super::conflict::resolve_volume_conflict;
use super::copy::copy_volumes_with_progress;
use super::move_same::move_within_same_volume_with_progress;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::file_system::write_operations::conflict::ApplyToAll;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{
    CollectorEventSink, ConflictResolution, SourceItemOutcome, VolumeCopyConfig,
};
use crate::ignore_poison::IgnorePoison;

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// One volume standing in for an MTP device or an SMB share: the source and the
/// destination of a duplicate are the same `Arc`, which is what every volume
/// path already means by "the same volume".
fn one_volume() -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::new("Device").with_space_info(10_000_000, 10_000_000))
}

fn duplicate_config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        // The default policy, and the one that shows the nonsensical
        // "this file conflicts with itself" prompt when the rule is missing.
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

async fn read_all(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.unwrap();
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    out
}

/// Child names of a directory on a volume, sorted.
async fn children(vol: &Arc<dyn Volume>, path: &str) -> Vec<String> {
    let mut names: Vec<String> = vol
        .list_directory(Path::new(path), None)
        .await
        .expect("directory should be listable")
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    names
}

/// Every path under `root`, relative and sorted, so a subtree can be compared
/// whole and a stray ` (N)` inside it can't hide.
async fn subtree(vol: &Arc<dyn Volume>, root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![root.to_string()];
    while let Some(dir) = queue.pop() {
        for entry in vol.list_directory(Path::new(&dir), None).await.expect("listable") {
            let full = format!("{}/{}", dir.trim_end_matches('/'), entry.name);
            out.push(
                full.strip_prefix(root)
                    .unwrap_or(&full)
                    .trim_start_matches('/')
                    .to_string(),
            );
            if entry.is_directory {
                queue.push(full);
            }
        }
    }
    out.sort();
    out
}

// ============================================================================
// Copy
// ============================================================================

/// A file copied into the folder it already lives in duplicates, silently.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicating_a_file_in_place_on_one_volume_renames_it_and_never_prompts() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume
        .create_file(Path::new("/photos/photo.jpg"), b"pixels")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    copy_volumes_with_progress(
        events.clone(),
        "op-volume-duplicate-file",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/photos/photo.jpg")],
        Arc::clone(&volume),
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("duplicating in place must succeed");

    assert_eq!(
        children(&volume, "/photos").await,
        vec!["photo (1).jpg", "photo.jpg"],
        "the duplicate lands beside the original"
    );
    assert_eq!(read_all(&volume, "/photos/photo.jpg").await, b"pixels");
    assert_eq!(read_all(&volume, "/photos/photo (1).jpg").await, b"pixels");
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "a self-collision is not a conflict, so nothing may be asked"
    );
}

/// A whole ` (N)` family duplicated in one operation: three sources, three new
/// names, none of them shared. Three top-level sources is also what puts this on
/// the CONCURRENT driver, where the picks happen at the same time, so the only
/// thing keeping two of them apart is the operation's claimed-name ledger: the
/// namer's `exists()` probe can't see a name whose bytes haven't landed yet.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicating_a_whole_series_at_once_gives_every_source_its_own_name() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    for (name, bytes) in [("photo.jpg", b"a"), ("photo (1).jpg", b"b"), ("photo (2).jpg", b"c")] {
        volume
            .create_file(&Path::new("/photos").join(name), bytes)
            .await
            .unwrap();
    }

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    copy_volumes_with_progress(
        events.clone(),
        "op-volume-duplicate-series",
        &state,
        Arc::clone(&volume),
        &[
            PathBuf::from("/photos/photo.jpg"),
            PathBuf::from("/photos/photo (1).jpg"),
            PathBuf::from("/photos/photo (2).jpg"),
        ],
        Arc::clone(&volume),
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("duplicating a whole series must succeed");

    assert_eq!(
        children(&volume, "/photos").await,
        vec![
            "photo (1).jpg",
            "photo (2).jpg",
            "photo (3).jpg",
            "photo (4).jpg",
            "photo (5).jpg",
            "photo.jpg",
        ],
        "three requested copies stay three, each under a name of its own"
    );
    assert_eq!(
        read_all(&volume, "/photos/photo.jpg").await,
        b"a",
        "originals untouched"
    );
    assert_eq!(read_all(&volume, "/photos/photo (1).jpg").await, b"b");
    assert_eq!(read_all(&volume, "/photos/photo (2).jpg").await, b"c");
    // Which copy lands under which new name is up to the concurrent driver's
    // ordering; that all three arrived, exactly once each, is the rule.
    let mut copies = vec![
        read_all(&volume, "/photos/photo (3).jpg").await,
        read_all(&volume, "/photos/photo (4).jpg").await,
        read_all(&volume, "/photos/photo (5).jpg").await,
    ];
    copies.sort();
    assert_eq!(
        copies,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
        "every source is copied once, and no copy overwrites another"
    );
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "a self-collision is not a conflict, so nothing may be asked"
    );
}

/// The propagation claim: the volume engine threads the destination down through
/// `merge_level`, so renaming the top-level folder carries the whole subtree with
/// it. Nothing inside the original may be touched, and no ` (N)` may appear there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicating_a_folder_in_place_lands_the_whole_subtree_under_the_renamed_root() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_directory(Path::new("/photos/docs")).await.unwrap();
    volume.create_directory(Path::new("/photos/docs/sub")).await.unwrap();
    volume
        .create_file(Path::new("/photos/docs/a.txt"), b"alpha")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/photos/docs/sub/b.txt"), b"beta")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    copy_volumes_with_progress(
        events.clone(),
        "op-volume-duplicate-folder",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/photos/docs")],
        Arc::clone(&volume),
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("duplicating a folder in place must succeed");

    assert_eq!(
        children(&volume, "/photos").await,
        vec!["docs", "docs (1)"],
        "the copy is a sibling of the original"
    );
    assert_eq!(
        subtree(&volume, "/photos/docs (1)").await,
        vec!["a.txt", "sub", "sub/b.txt"],
        "the whole subtree rode along with the renamed root"
    );
    assert_eq!(
        subtree(&volume, "/photos/docs").await,
        vec!["a.txt", "sub", "sub/b.txt"],
        "and nothing was scattered through the original"
    );
    assert_eq!(read_all(&volume, "/photos/docs (1)/sub/b.txt").await, b"beta");
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
}

/// An empty folder duplicates too: `merge_level` creates the renamed destination
/// before it lists anything, so there's no leaf to hang the redirect on and none
/// is needed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicating_an_empty_folder_in_place_lands_an_empty_sibling() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_directory(Path::new("/photos/docs")).await.unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    copy_volumes_with_progress(
        events.clone(),
        "op-volume-duplicate-empty-folder",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/photos/docs")],
        Arc::clone(&volume),
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("duplicating an empty folder in place must succeed");

    assert_eq!(children(&volume, "/photos").await, vec!["docs", "docs (1)"]);
    assert!(children(&volume, "/photos/docs (1)").await.is_empty());
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
}

/// A same-folder copy under `Skip all` still duplicates. The pre-flight matches
/// by NAME, so it lists every one of these sources as conflicting; the bulk-skip
/// prelude would drop them all and the duplicate would silently do nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pre_known_conflict_naming_the_source_itself_does_not_skip_the_duplicate() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume
        .create_file(Path::new("/photos/photo.jpg"), b"pixels")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        pre_known_conflicts: vec!["photo.jpg".to_string()],
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    copy_volumes_with_progress(
        events.clone(),
        "op-volume-duplicate-preknown",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/photos/photo.jpg")],
        Arc::clone(&volume),
        Path::new("/photos"),
        &config,
    )
    .await
    .expect("duplicating in place must succeed");

    assert_eq!(
        children(&volume, "/photos").await,
        vec!["photo (1).jpg", "photo.jpg"],
        "the duplicate must survive a `Skip all` whose conflict list names its own source"
    );
}

/// Two different files sharing a name on one volume is still an ordinary
/// conflict, and still asks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_genuine_conflict_on_one_volume_still_raises_the_normal_flow() {
    let volume = one_volume();
    volume.create_directory(Path::new("/from")).await.unwrap();
    volume.create_directory(Path::new("/to")).await.unwrap();
    volume
        .create_file(Path::new("/from/photo.jpg"), b"a different photo")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/to/photo.jpg"), b"the one already there")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    copy_volumes_with_progress(
        events.clone(),
        "op-volume-real-conflict",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/from/photo.jpg")],
        Arc::clone(&volume),
        Path::new("/to"),
        &duplicate_config(),
    )
    .await
    .expect("a real conflict resolves normally");

    assert_eq!(
        events.inner.conflicts.lock_ignore_poison().len(),
        1,
        "two different files sharing a name is still a conflict"
    );
    assert_eq!(children(&volume, "/to").await, vec!["photo (1).jpg", "photo.jpg"]);
    assert_eq!(read_all(&volume, "/to/photo.jpg").await, b"the one already there");
    assert_eq!(read_all(&volume, "/to/photo (1).jpg").await, b"a different photo");
}

// ============================================================================
// Identity at the resolver seam
// ============================================================================
//
// A volume has no `dev+ino`, so identity is one volume, the same parent, and a
// folded leaf (`dest_name_index::fold`, NFC + lowercase — the project's answer
// to "would this backend treat these two names as the same"). `InMemoryVolume`
// is case- and normalization-sensitive, so the folding cases are pinned here at
// the resolver rather than through a whole copy.

/// Resolve one clash under `policy` and hand back the write path the resolver
/// picked. The self-collision cases pass `Skip` on purpose: the whole point is
/// that identity is settled BEFORE any policy is consulted, so the strictest
/// pin is the policy that would answer `None`. A regression shows up as a skip,
/// not as a differently-named file.
async fn resolve(
    source_volume: &Arc<dyn Volume>,
    source_path: &str,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &str,
    policy: ConflictResolution,
    events: &CollectorEventSink,
) -> Option<PathBuf> {
    let state = make_state();
    let mut apply_to_all = ApplyToAll::default();
    let config = VolumeCopyConfig {
        conflict_resolution: policy,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    resolve_volume_conflict(
        source_volume,
        Path::new(source_path),
        dest_volume,
        Path::new(dest_path),
        &config,
        events,
        "op-resolve",
        &state,
        &mut apply_to_all,
        None,
        None,
        Some(false),
    )
    .await
    .expect("resolution must succeed")
    .map(|resolved| resolved.write_path)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_case_differing_path_on_one_volume_is_the_same_item() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume
        .create_file(Path::new("/photos/photo.jpg"), b"pixels")
        .await
        .unwrap();

    let events = CollectorEventSink::new();
    let write_path = resolve(
        &volume,
        "/photos/Photo.JPG",
        &volume,
        "/photos/photo.jpg",
        ConflictResolution::Skip,
        &events,
    )
    .await;

    assert_eq!(write_path, Some(PathBuf::from("/photos/photo (1).jpg")));
    assert!(
        events.conflicts.lock_ignore_poison().is_empty(),
        "a case-differing route to the same item asks nothing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_nfd_path_on_one_volume_is_the_same_item_as_its_nfc_twin() {
    // "café.jpg" composed (NFC) vs decomposed (NFD): macOS and SMB move paths
    // between the two routinely, and the clipboard can introduce either.
    let composed = "/photos/caf\u{e9}.jpg";
    let decomposed = "/photos/cafe\u{301}.jpg";

    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_file(Path::new(composed), b"pixels").await.unwrap();

    let events = CollectorEventSink::new();
    let write_path = resolve(
        &volume,
        decomposed,
        &volume,
        composed,
        ConflictResolution::Skip,
        &events,
    )
    .await;

    assert_eq!(write_path, Some(PathBuf::from("/photos/caf\u{e9} (1).jpg")));
    assert!(events.conflicts.lock_ignore_poison().is_empty());
}

/// A case-differing PARENT is a different folder as far as we're allowed to say.
/// The leaf is the question `fold` answers (one destination listing, one
/// backend's name resolution); whether `/DCIM` and `/dcim` are one directory is
/// the backend's call, and a case-sensitive one (MTP is, and an SMB share can
/// be) says no. Claiming otherwise turns a real cross-folder transfer into a
/// self-collision.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_case_differing_parent_is_not_the_same_item() {
    let volume = one_volume();
    volume.create_directory(Path::new("/DCIM")).await.unwrap();
    volume.create_directory(Path::new("/dcim")).await.unwrap();
    volume
        .create_file(Path::new("/DCIM/photo.jpg"), b"upper")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/dcim/photo.jpg"), b"lower")
        .await
        .unwrap();

    let events = CollectorEventSink::new();
    let write_path = resolve(
        &volume,
        "/DCIM/photo.jpg",
        &volume,
        "/dcim/photo.jpg",
        ConflictResolution::Skip,
        &events,
    )
    .await;

    // `Skip` is the sharp pin: a self-collision would have handed back
    // `/dcim/photo (1).jpg` without consulting any policy.
    assert_eq!(
        write_path, None,
        "two same-named files in differently-cased folders are an ordinary clash, and `Skip` skips it"
    );
}

/// The volume half of the rule. The same path on two DIFFERENT volumes names two
/// different files, so it stays an ordinary conflict.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_path_on_two_volumes_is_not_a_self_collision() {
    let source = one_volume();
    let dest = one_volume();
    source.create_directory(Path::new("/photos")).await.unwrap();
    source
        .create_file(Path::new("/photos/photo.jpg"), b"source")
        .await
        .unwrap();
    dest.create_directory(Path::new("/photos")).await.unwrap();
    dest.create_file(Path::new("/photos/photo.jpg"), b"dest").await.unwrap();

    let events = CollectorEventSink::new();
    let write_path = resolve(
        &source,
        "/photos/photo.jpg",
        &dest,
        "/photos/photo.jpg",
        ConflictResolution::Rename,
        &events,
    )
    .await;

    // Rename rather than Stop, because nothing here would answer a prompt. What
    // the case pins is that the ORDINARY policy machinery decided this clash;
    // the identity rule must not claim it just because the paths match.
    assert_eq!(write_path, Some(PathBuf::from("/photos/photo (1).jpg")));
}

// ============================================================================
// Move
// ============================================================================

/// Moving an item into the folder it already lives in is already done: nothing
/// is renamed, nothing is shuffled aside, and the item reports itself finished.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_a_file_into_its_own_folder_on_one_volume_leaves_it_alone() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume
        .create_file(Path::new("/photos/photo.jpg"), b"pixels")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    move_within_same_volume_with_progress(
        events.clone(),
        "op-volume-move-in-place",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/photos/photo.jpg")],
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("moving in place must succeed");

    assert_eq!(
        children(&volume, "/photos").await,
        vec!["photo.jpg"],
        "no `photo (1).jpg` appeared"
    );
    assert_eq!(read_all(&volume, "/photos/photo.jpg").await, b"pixels");
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "an item already where it was asked to go raises no conflict"
    );
    let items = events.inner.source_items_done.lock_ignore_poison();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_path, "/photos/photo.jpg");
    assert_eq!(items[0].outcome, SourceItemOutcome::Done, "and it reports itself done");
    assert!(!items[0].source_removed, "the source is still exactly where it was");
    drop(items);
    let complete = events.inner.complete.lock_ignore_poison();
    assert_eq!(complete[0].files_processed, 1, "the item counts toward the total");
    assert_eq!(complete[0].files_skipped, 0, "it wasn't skipped, it was already there");
}

/// The user-visible half of the same question: on a case-sensitive backend,
/// moving `/DCIM/photo.jpg` into `/dcim/` is a genuine cross-folder move. Calling
/// it already-in-place reports the item done, leaves it where it was, and tells
/// the user it moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_into_a_case_differing_folder_on_one_volume_really_moves() {
    let volume = one_volume();
    volume.create_directory(Path::new("/DCIM")).await.unwrap();
    volume.create_directory(Path::new("/dcim")).await.unwrap();
    volume
        .create_file(Path::new("/DCIM/photo.jpg"), b"pixels")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    move_within_same_volume_with_progress(
        events.clone(),
        "op-volume-move-case-differing-folder",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/DCIM/photo.jpg")],
        Path::new("/dcim"),
        &duplicate_config(),
    )
    .await
    .expect("moving between differently-cased folders must succeed");

    assert_eq!(
        children(&volume, "/dcim").await,
        vec!["photo.jpg"],
        "the file arrives in the destination folder"
    );
    assert_eq!(read_all(&volume, "/dcim/photo.jpg").await, b"pixels");
    assert!(
        children(&volume, "/DCIM").await.is_empty(),
        "and it leaves the folder it came from"
    );
    // The already-in-place branch is the only thing on this path that speaks per
    // source (`move_same.rs`), so a real move passing through it silently is
    // exactly the regression: its `Done` / `source_removed: false` pair would be
    // the only word the user got about a file that never moved.
    assert!(
        events.inner.source_items_done.lock_ignore_poison().is_empty(),
        "nothing may report this item already in place"
    );
    let complete = events.inner.complete.lock_ignore_poison();
    assert_eq!(complete[0].files_processed, 1, "the move counts as work done");
}

/// A folder moved into its own parent must never reach `rename_merge_directory`,
/// which threads the destination down through recursion and would rename every
/// leaf onto itself or shuffle it aside.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_a_folder_into_its_own_parent_on_one_volume_leaves_the_subtree_alone() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_directory(Path::new("/photos/docs")).await.unwrap();
    volume.create_directory(Path::new("/photos/docs/sub")).await.unwrap();
    volume
        .create_file(Path::new("/photos/docs/a.txt"), b"alpha")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/photos/docs/sub/b.txt"), b"beta")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    move_within_same_volume_with_progress(
        events.clone(),
        "op-volume-move-folder-in-place",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/photos/docs")],
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("moving a folder in place must succeed");

    assert_eq!(children(&volume, "/photos").await, vec!["docs"], "no sibling appeared");
    assert_eq!(
        subtree(&volume, "/photos/docs").await,
        vec!["a.txt", "sub", "sub/b.txt"],
        "and nothing inside was renamed or shuffled aside"
    );
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
}

/// A mixed batch: the item already at the destination stays put, the one from
/// elsewhere on the same volume moves. The old all-or-nothing shape would refuse
/// both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mixed_move_batch_on_one_volume_leaves_the_local_source_and_moves_the_other() {
    let volume = one_volume();
    volume.create_directory(Path::new("/photos")).await.unwrap();
    volume.create_directory(Path::new("/elsewhere")).await.unwrap();
    volume
        .create_file(Path::new("/photos/photo.jpg"), b"pixels")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/elsewhere/notes.txt"), b"words")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Rename, true));

    move_within_same_volume_with_progress(
        events.clone(),
        "op-volume-move-mixed",
        &state,
        Arc::clone(&volume),
        &[
            PathBuf::from("/photos/photo.jpg"),
            PathBuf::from("/elsewhere/notes.txt"),
        ],
        Path::new("/photos"),
        &duplicate_config(),
    )
    .await
    .expect("a mixed move batch must succeed");

    assert_eq!(children(&volume, "/photos").await, vec!["notes.txt", "photo.jpg"]);
    assert_eq!(read_all(&volume, "/photos/photo.jpg").await, b"pixels");
    assert!(
        !volume.exists(Path::new("/elsewhere/notes.txt")).await,
        "the outside source moved"
    );
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
    assert_eq!(events.inner.complete.lock_ignore_poison()[0].files_processed, 2);
}
