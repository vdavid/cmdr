//! Duplicating in place: copying an item into the folder it already lives in,
//! and moving one there.
//!
//! Both engines answer the same question — "would this land on the source
//! itself?" — by `dev+ino`, per TOP-LEVEL source, before any per-file work
//! starts. Copy redirects the whole subtree to a free ` (N)` name; move writes
//! nothing and calls the item done. Neither consults the conflict machinery,
//! which is the point: every answer it can give for a self-collision either
//! destroys the original or refuses the user's intent.
//!
//! Most copy tests drive the operation through a [`ConflictResponderSink`]
//! scripted to `Rename` rather than a bare collector. A self-collision must
//! raise NO prompt at all, and the sink lets that be an assertion (`conflicts`
//! is empty) instead of a Stop-mode deadlock when the rule regresses.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use super::super::types::{
    CollectorEventSink, ConflictResolution, OperationEventSink, SourceItemOutcome, WriteOperationConfig,
    WriteSourceItemDoneEvent,
};
use super::super::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent, WriteSettledEvent,
};
use super::conflict_responder_test_support::ConflictResponderSink;
use super::copy::copy_files_with_progress_inner;
use super::move_op::move_files_with_progress_inner;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("self_collision_{}_{}", name, uuid::Uuid::new_v4()))
}

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// The inode of `path`, the identity that survives a rename and doesn't survive
/// a set-aside-and-replace. The assertions that use it are the anchor against
/// the `Overwrite` hazard: same name and same bytes prove nothing on their own.
fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).expect("path should exist").ino()
}

/// Child names of a directory, sorted.
fn dir_children(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("dir should be readable")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Every path under `root`, relative and sorted, so a subtree can be compared
/// whole (and a stray ` (N)` inside it can't hide).
fn subtree(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("dir should be readable") {
            let path = entry.unwrap().path();
            out.push(
                path.strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .to_string(),
            );
            if path.is_dir() {
                walk(&path, root, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// Requests rollback the instant the copy reports its first finished item —
/// after the duplicate has landed, and before the post-loop intent check reads
/// it. Everything else forwards to an inner collector the test asserts on.
struct RollBackOnFirstItem {
    /// A responder rather than a bare collector, for the same reason the copy
    /// tests use one: a regressed rule would park this test on a Stop prompt
    /// nobody answers instead of failing.
    inner: ConflictResponderSink,
    state: Arc<WriteOperationState>,
}

/// One-line forwarders for the events this sink doesn't care about.
macro_rules! forward_to_inner {
    ($($method:ident($event:ty)),+ $(,)?) => {
        $(fn $method(&self, event: $event) { self.inner.$method(event); })+
    };
}

impl OperationEventSink for RollBackOnFirstItem {
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
        self.state.intent.store(
            super::super::state::OperationIntent::RollingBack as u8,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    forward_to_inner!(
        emit_progress(WriteProgressEvent),
        emit_complete(WriteCompleteEvent),
        emit_cancelled(WriteCancelledEvent),
        emit_error(WriteErrorEvent),
        emit_conflict(WriteConflictEvent),
        emit_conflict_resolved(WriteConflictResolvedEvent),
        emit_scan_progress(ScanProgressEvent),
        emit_scan_conflict(ConflictInfo),
        emit_dry_run_complete(DryRunResult),
        emit_settled(WriteSettledEvent),
    );
}

// ============================================================================
// Copy
// ============================================================================

/// The headline case: pasting a file into its own folder duplicates it, with no
/// prompt and no policy question.
#[test]
fn duplicating_a_file_in_place_lands_a_numbered_copy_beside_it() {
    let tmp = create_temp_dir("file");
    let folder = tmp.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();
    let original_ino = inode(&original);

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, true);
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-file",
        &state,
        std::slice::from_ref(&original),
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("duplicating in place must succeed");

    assert_eq!(
        dir_children(&folder),
        vec!["photo (1).jpg", "photo.jpg"],
        "the duplicate lands beside the original"
    );
    assert_eq!(fs::read(folder.join("photo (1).jpg")).unwrap(), b"pixels");
    assert_eq!(fs::read(&original).unwrap(), b"pixels", "the original keeps its bytes");
    assert_eq!(
        inode(&original),
        original_ino,
        "the original file itself is untouched: same inode, so no set-aside-and-replace happened"
    );
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "a self-collision is not a conflict, so nothing may prompt"
    );
}

/// Duplicating the same file twice continues the series instead of nesting into
/// it, and leaves both earlier files alone.
#[test]
fn duplicating_a_file_twice_continues_the_series() {
    let tmp = create_temp_dir("twice");
    let folder = tmp.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();

    for op_id in ["op-duplicate-twice-1", "op-duplicate-twice-2"] {
        let state = make_state();
        let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, true);
        copy_files_with_progress_inner(
            &events,
            op_id,
            &state,
            std::slice::from_ref(&original),
            &folder,
            &WriteOperationConfig::default(),
        )
        .expect("duplicating in place must succeed");
        assert!(
            events.inner.conflicts.lock_ignore_poison().is_empty(),
            "neither duplicate may prompt"
        );
    }

    assert_eq!(
        dir_children(&folder),
        vec!["photo (1).jpg", "photo (2).jpg", "photo.jpg"],
        "the second duplicate continues the series and both earlier files survive"
    );
}

/// A folder duplicated in place becomes a sibling `docs (1)/` holding the whole
/// subtree. This is the case that forces the question to be asked per TOP-LEVEL
/// source: every leaf of a same-folder folder copy is its own self-collision, so
/// a per-leaf rule would scatter `a (1).txt` through the original instead.
#[test]
fn duplicating_a_folder_in_place_lands_a_sibling_copy_and_never_touches_the_original() {
    let tmp = create_temp_dir("folder");
    let parent = tmp.join("parent");
    let docs = parent.join("docs");
    fs::create_dir_all(docs.join("sub")).unwrap();
    fs::write(docs.join("a.txt"), b"alpha").unwrap();
    fs::write(docs.join("sub/b.txt"), b"beta").unwrap();

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, true);
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-folder",
        &state,
        std::slice::from_ref(&docs),
        &parent,
        &WriteOperationConfig::default(),
    )
    .expect("duplicating a folder in place must succeed");

    assert_eq!(
        dir_children(&parent),
        vec!["docs", "docs (1)"],
        "the copy lands as a sibling of the original folder"
    );
    assert_eq!(
        subtree(&parent.join("docs (1)")),
        vec!["a.txt", "sub", "sub/b.txt"],
        "the duplicate holds the full subtree"
    );
    assert_eq!(
        subtree(&docs),
        vec!["a.txt", "sub", "sub/b.txt"],
        "nothing lands inside the original: no `a (1).txt` anywhere under it"
    );
    assert_eq!(fs::read(parent.join("docs (1)/sub/b.txt")).unwrap(), b"beta");
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "duplicating a folder in place raises no conflict, per leaf or otherwise"
    );
}

/// An empty folder has no files at all, so nothing in the per-file loop speaks
/// for it: the redirect has to be in place before the scanned-dirs pass runs.
#[test]
fn duplicating_an_empty_folder_in_place_lands_an_empty_sibling() {
    let tmp = create_temp_dir("empty-folder");
    let parent = tmp.join("parent");
    let docs = parent.join("docs");
    fs::create_dir_all(&docs).unwrap();

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, true);
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-empty-folder",
        &state,
        std::slice::from_ref(&docs),
        &parent,
        &WriteOperationConfig::default(),
    )
    .expect("duplicating an empty folder in place must succeed");

    assert_eq!(dir_children(&parent), vec!["docs", "docs (1)"]);
    assert!(parent.join("docs (1)").is_dir(), "the duplicate is a directory");
    assert!(
        dir_children(&parent.join("docs (1)")).is_empty(),
        "and it's as empty as the original"
    );
}

/// One clipboard, two folders: the item already at the destination duplicates,
/// the one from elsewhere copies normally, and neither prompts. The guard this
/// rule replaces was an operation-level verdict, so it refused the whole batch
/// over the one item.
#[test]
fn a_mixed_batch_duplicates_the_local_source_and_copies_the_other() {
    let tmp = create_temp_dir("mixed");
    let folder = tmp.join("folder");
    let elsewhere = tmp.join("elsewhere");
    fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    let already_there = folder.join("photo.jpg");
    let from_elsewhere = elsewhere.join("notes.txt");
    fs::write(&already_there, b"pixels").unwrap();
    fs::write(&from_elsewhere, b"words").unwrap();

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, true);
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-mixed",
        &state,
        &[already_there.clone(), from_elsewhere.clone()],
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("a mixed batch must succeed");

    assert_eq!(
        dir_children(&folder),
        vec!["notes.txt", "photo (1).jpg", "photo.jpg"],
        "the self-collision duplicates and the outside source copies"
    );
    assert_eq!(fs::read(folder.join("notes.txt")).unwrap(), b"words");
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "neither item prompts"
    );
}

/// The lexical `source.parent() == destination` test missed a symlinked parent.
/// `is_same_file` compares `dev+ino` through `fs::metadata`, which FOLLOWS
/// symlinks, so the same file reached by another route still counts.
#[test]
fn a_source_reached_through_a_symlinked_parent_is_a_self_collision() {
    let tmp = create_temp_dir("symlink");
    let real = tmp.join("real");
    fs::create_dir_all(&real).unwrap();
    let original = real.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();
    let link = tmp.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let original_ino = inode(&original);

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, true);
    let via_link = link.join("photo.jpg");
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-symlinked-parent",
        &state,
        std::slice::from_ref(&via_link),
        &real,
        &WriteOperationConfig::default(),
    )
    .expect("duplicating through a symlinked parent must succeed");

    assert_eq!(dir_children(&real), vec!["photo (1).jpg", "photo.jpg"]);
    assert_eq!(inode(&original), original_ino, "the original is untouched");
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
}

/// The data-safety pin. Under `Overwrite`, a self-collision that reached the
/// conflict machinery would send the original through set-aside-and-delete:
/// same name, same bytes, NEW inode, with hard links broken and birth time
/// reset. The rule has to win before the policy is ever consulted.
#[test]
fn duplicating_under_overwrite_still_duplicates_and_never_replaces_the_original() {
    let tmp = create_temp_dir("overwrite");
    let folder = tmp.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();
    let original_ino = inode(&original);

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    copy_files_with_progress_inner(
        &*events,
        "op-duplicate-overwrite",
        &state,
        std::slice::from_ref(&original),
        &folder,
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            ..Default::default()
        },
    )
    .expect("duplicating under Overwrite must succeed");

    assert_eq!(dir_children(&folder), vec!["photo (1).jpg", "photo.jpg"]);
    assert_eq!(
        inode(&original),
        original_ino,
        "Overwrite must not have replaced the original with a fresh inode"
    );
    assert_eq!(fs::read(&original).unwrap(), b"pixels");
}

/// The same pin for the latch: an `Overwrite` answered "apply to all" on an
/// earlier, genuine conflict in the same batch must not reach the duplicate.
#[test]
fn duplicating_under_an_apply_to_all_overwrite_latch_never_replaces_the_original() {
    let tmp = create_temp_dir("latch");
    let folder = tmp.join("folder");
    let elsewhere = tmp.join("elsewhere");
    fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    // Sorts before `photo.jpg`, so the latch is set by the time the duplicate
    // would be asked about.
    let clashing = elsewhere.join("a-report.txt");
    fs::write(&clashing, b"the incoming one").unwrap();
    fs::write(folder.join("a-report.txt"), b"the one already there").unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();
    let original_ino = inode(&original);

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Overwrite, true);
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-latch",
        &state,
        &[clashing.clone(), original.clone()],
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("the batch must succeed");

    assert_eq!(
        events.inner.conflicts.lock_ignore_poison().len(),
        1,
        "only the genuine clash prompts"
    );
    assert_eq!(
        fs::read(folder.join("a-report.txt")).unwrap(),
        b"the incoming one",
        "the latched Overwrite applied to the genuine clash"
    );
    assert_eq!(
        dir_children(&folder),
        vec!["a-report.txt", "photo (1).jpg", "photo.jpg"]
    );
    assert_eq!(
        inode(&original),
        original_ino,
        "the latch must not have reached the duplicate"
    );
}

/// A "Skip all" the frontend pre-computed by NAME lists every source of a
/// same-folder paste, because every name is present at the destination. The
/// duplicate must still happen: the bulk-skip is about conflicts, and this
/// isn't one.
#[test]
fn a_pre_known_conflict_naming_the_source_itself_does_not_skip_the_duplicate() {
    let tmp = create_temp_dir("pre-known");
    let folder = tmp.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    copy_files_with_progress_inner(
        &*events,
        "op-duplicate-pre-known",
        &state,
        std::slice::from_ref(&original),
        &folder,
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Skip,
            pre_known_conflicts: vec!["photo.jpg".to_string()],
            ..Default::default()
        },
    )
    .expect("the duplicate must succeed");

    assert_eq!(
        dir_children(&folder),
        vec!["photo (1).jpg", "photo.jpg"],
        "a pre-known conflict naming the source itself must not silently no-op the duplicate"
    );
}

/// Rollback deletes what the operation created and nothing else. The duplicate
/// is entirely the operation's own work, so it goes; the original never can.
#[test]
fn rolling_back_a_duplicate_removes_the_copy_and_keeps_the_original() {
    let tmp = create_temp_dir("rollback");
    let folder = tmp.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();
    let original_ino = inode(&original);

    let state = make_state();
    let events = RollBackOnFirstItem {
        inner: ConflictResponderSink::new(&state, ConflictResolution::Rename, true),
        state: Arc::clone(&state),
    };
    copy_files_with_progress_inner(
        &events,
        "op-duplicate-rollback",
        &state,
        std::slice::from_ref(&original),
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("a rolled-back copy reports through the cancelled event, not an error");

    assert_eq!(
        dir_children(&folder),
        vec!["photo.jpg"],
        "rollback removed the duplicate"
    );
    assert_eq!(inode(&original), original_ino, "and left the original alone");
    assert_eq!(fs::read(&original).unwrap(), b"pixels");
    assert!(
        events.inner.inner.conflicts.lock_ignore_poison().is_empty(),
        "the duplicate that got rolled back was never a conflict either"
    );
}

/// Regression anchor: a DIFFERENT file arriving under a name the destination
/// already holds is still an ordinary conflict, prompt and all.
#[test]
fn a_genuine_conflict_still_raises_the_normal_flow() {
    let tmp = create_temp_dir("genuine");
    let folder = tmp.join("folder");
    let elsewhere = tmp.join("elsewhere");
    fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(folder.join("photo.jpg"), b"the one already there").unwrap();
    let incoming = elsewhere.join("photo.jpg");
    fs::write(&incoming, b"a different photo").unwrap();

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Rename, false);
    copy_files_with_progress_inner(
        &events,
        "op-genuine-conflict",
        &state,
        std::slice::from_ref(&incoming),
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("the copy must succeed");

    assert_eq!(
        events.inner.conflicts.lock_ignore_poison().len(),
        1,
        "two different files sharing a name is still a conflict"
    );
    assert_eq!(dir_children(&folder), vec!["photo (1).jpg", "photo.jpg"]);
    assert_eq!(fs::read(folder.join("photo.jpg")).unwrap(), b"the one already there");
    assert_eq!(fs::read(folder.join("photo (1).jpg")).unwrap(), b"a different photo");
}

// ============================================================================
// Move
// ============================================================================

/// Moving an item into the folder it already lives in is already done. Nothing
/// is written, nothing is renamed aside, and the item reports itself finished.
#[test]
fn moving_a_file_into_its_own_folder_leaves_it_alone() {
    let tmp = create_temp_dir("move-file");
    let folder = tmp.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let original = folder.join("photo.jpg");
    fs::write(&original, b"pixels").unwrap();
    let original_ino = inode(&original);

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Skip, true);
    move_files_with_progress_inner(
        &events,
        "op-move-in-place",
        &state,
        std::slice::from_ref(&original),
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("moving in place must succeed");

    assert_eq!(dir_children(&folder), vec!["photo.jpg"], "no `photo (1).jpg` appeared");
    assert_eq!(inode(&original), original_ino, "the file was not touched at all");
    assert!(
        events.inner.conflicts.lock_ignore_poison().is_empty(),
        "an item already where it was asked to go raises no conflict"
    );
    let items = events.inner.source_items_done.lock_ignore_poison();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_path, original.display().to_string());
    assert_eq!(items[0].outcome, SourceItemOutcome::Done, "and it reports itself done");
    assert!(!items[0].source_removed, "the source is still exactly where it was");
    let complete = events.inner.complete.lock_ignore_poison();
    assert_eq!(complete[0].files_processed, 1, "the item counts toward the total");
    assert_eq!(complete[0].files_skipped, 0, "it wasn't skipped, it was already there");
}

/// A folder moved into its own parent must not self-merge. `merge_move_directory`
/// threads the destination down through recursion, so reaching it would rename
/// every leaf onto itself or shuffle it aside to `name (1)`.
#[test]
fn moving_a_folder_into_its_own_parent_leaves_the_subtree_alone() {
    let tmp = create_temp_dir("move-folder");
    let parent = tmp.join("parent");
    let docs = parent.join("docs");
    fs::create_dir_all(docs.join("sub")).unwrap();
    fs::write(docs.join("a.txt"), b"alpha").unwrap();
    fs::write(docs.join("sub/b.txt"), b"beta").unwrap();

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Skip, true);
    move_files_with_progress_inner(
        &events,
        "op-move-folder-in-place",
        &state,
        std::slice::from_ref(&docs),
        &parent,
        &WriteOperationConfig::default(),
    )
    .expect("moving a folder in place must succeed");

    assert_eq!(dir_children(&parent), vec!["docs"], "no sibling copy appeared");
    assert_eq!(
        subtree(&docs),
        vec!["a.txt", "sub", "sub/b.txt"],
        "and nothing inside was renamed or shuffled aside"
    );
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
}

/// A mixed move batch: the item already at the destination stays put, the one
/// from elsewhere moves.
#[test]
fn a_mixed_move_batch_leaves_the_local_source_and_moves_the_other() {
    let tmp = create_temp_dir("move-mixed");
    let folder = tmp.join("folder");
    let elsewhere = tmp.join("elsewhere");
    fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    let already_there = folder.join("photo.jpg");
    let from_elsewhere = elsewhere.join("notes.txt");
    fs::write(&already_there, b"pixels").unwrap();
    fs::write(&from_elsewhere, b"words").unwrap();
    let original_ino = inode(&already_there);

    let state = make_state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Skip, true);
    move_files_with_progress_inner(
        &events,
        "op-move-mixed",
        &state,
        &[already_there.clone(), from_elsewhere.clone()],
        &folder,
        &WriteOperationConfig::default(),
    )
    .expect("a mixed move batch must succeed");

    assert_eq!(dir_children(&folder), vec!["notes.txt", "photo.jpg"]);
    assert_eq!(
        inode(&already_there),
        original_ino,
        "the item already there is untouched"
    );
    assert!(!from_elsewhere.exists(), "the outside source moved");
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
    assert_eq!(events.inner.complete.lock_ignore_poison()[0].files_processed, 2);
}
