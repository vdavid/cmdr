//! Integration tests for what a Stop-mode conflict prompt SAYS about the two
//! sides of a local clash.
//!
//! **Consent contract pinned here.** A blanket policy never replaces one kind of
//! entry with another; only an Overwrite answered on a prompt does, because that
//! prompt names both kinds (`conflict::resolution_for_clash`). That's only
//! consent if the prompt tells the truth: the kinds it names must be the kinds
//! the engine is about to act on, and the incoming side must be the real
//! incoming item. Every clash shape the three local engines can raise is driven
//! end to end here, and the emitted `write-conflict` event is checked against
//! the fixture.
//!
//! The responder answers Skip, so no fixture is changed by the prompt it raised.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use super::super::types::{ConflictResolution, KnownSize, WriteConflictEvent, WriteOperationConfig};
use super::conflict_responder_test_support::ConflictResponderSink;
use super::copy::copy_files_with_progress_inner;
use super::move_op::move_files_with_progress_inner;
use crate::host::{BufferedLeaves, ExpectedTotals, OperationIndex};
use crate::testing::TestOperations;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;
use cmdr_operation_log::types::SearchCoverageReason;

fn temp(name: &str) -> TestDir {
    TestDir::new(&format!("conflict_prompt_sides_{}_{}", name, uuid::Uuid::new_v4()))
}

fn ask_each() -> WriteOperationConfig {
    WriteOperationConfig {
        conflict_resolution: ConflictResolution::Stop,
        ..Default::default()
    }
}

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
}

enum Engine {
    Copy,
    Move,
}

/// Runs one copy or move of `sources` into `dst_root` under "Ask for each",
/// answers every prompt with Skip, and returns the prompts it raised.
fn prompts_for(
    engine: &TestOperations,
    driver: Engine,
    sources: &[std::path::PathBuf],
    dst_root: &Path,
) -> Vec<WriteConflictEvent> {
    let state = state(engine);
    let events = ConflictResponderSink::new(&state, ConflictResolution::Skip, false);
    let run = match driver {
        Engine::Copy => copy_files_with_progress_inner,
        Engine::Move => move_files_with_progress_inner,
    };
    run(&events, "op-prompt-sides", &state, sources, dst_root, &ask_each()).expect("the operation should succeed");
    events.inner.conflicts.lock_ignore_poison().clone()
}

fn the_one_prompt(prompts: Vec<WriteConflictEvent>) -> WriteConflictEvent {
    assert_eq!(prompts.len(), 1, "expected exactly one prompt, got {prompts:#?}");
    prompts.into_iter().next().expect("length checked above")
}

/// Source: a folder `thing/` holding `sentinel.txt`. Destination: a FILE `thing`.
fn folder_over_file_fixture(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    fs::write(src_root.join("thing/sentinel.txt"), "source-sentinel").unwrap();
    fs::write(dst_root.join("thing"), "precious user bytes").unwrap();
    (src_root, dst_root)
}

// ============================================================================
// folder → file
// ============================================================================

/// The copy engine meets the blocking file while creating a CHILD's parent, so
/// the pair it holds is the child and the file. The prompt must still describe
/// the folder that's arriving: its own path, and a folder.
#[test]
fn a_copied_folder_meeting_a_file_is_described_as_the_folder() {
    let engine = TestOperations::new();
    let dir = temp("copy_folder_over_file");
    let (src_root, dst_root) = folder_over_file_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Copy, &[src_root.join("thing")], &dst_root));

    assert!(prompt.source_is_directory, "a whole folder is arriving: {prompt:#?}");
    assert!(!prompt.destination_is_directory, "a file is in the way: {prompt:#?}");
    assert_eq!(
        prompt.source_path,
        src_root.join("thing").display().to_string(),
        "the incoming side is the source folder, not the file in the way"
    );
    assert_eq!(prompt.destination_path, dst_root.join("thing").display().to_string());
    assert_eq!(
        prompt.destination_size,
        Some(KnownSize::Exact {
            bytes: "precious user bytes".len() as u64
        }),
        "the existing side is still the file"
    );
}

/// The same clash one level down, inside a folder that merges: the prompt names
/// the nested source folder that maps onto the blocking file, not the merge root.
#[test]
fn a_nested_copied_folder_meeting_a_file_is_described_as_that_folder() {
    let engine = TestOperations::new();
    let dir = temp("copy_nested_folder_over_file");
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(src_root.join("album/raw")).unwrap();
    fs::write(src_root.join("album/raw/DSC0001.arw"), "raw bytes").unwrap();
    fs::create_dir_all(dst_root.join("album")).unwrap();
    fs::write(dst_root.join("album/raw"), "a file named raw").unwrap();

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Copy, &[src_root.join("album")], &dst_root));

    assert!(prompt.source_is_directory, "a whole folder is arriving: {prompt:#?}");
    assert!(!prompt.destination_is_directory, "a file is in the way: {prompt:#?}");
    assert_eq!(prompt.source_path, src_root.join("album/raw").display().to_string());
    assert_eq!(
        prompt.destination_path,
        dst_root.join("album/raw").display().to_string()
    );
}

#[test]
fn a_moved_folder_meeting_a_file_is_described_as_the_folder() {
    let engine = TestOperations::new();
    let dir = temp("move_folder_over_file");
    let (src_root, dst_root) = folder_over_file_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Move, &[src_root.join("thing")], &dst_root));

    assert!(prompt.source_is_directory, "{prompt:#?}");
    assert!(!prompt.destination_is_directory, "{prompt:#?}");
    assert_eq!(prompt.source_path, src_root.join("thing").display().to_string());
}

// ============================================================================
// file → folder
// ============================================================================

fn file_over_folder_fixture(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(dst_root.join("notes")).unwrap();
    fs::write(src_root.join("notes"), "incoming file").unwrap();
    fs::write(dst_root.join("notes/precious.txt"), "precious user data").unwrap();
    (src_root, dst_root)
}

#[test]
fn a_copied_file_meeting_a_folder_is_described_as_a_file() {
    let engine = TestOperations::new();
    let dir = temp("copy_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Copy, &[src_root.join("notes")], &dst_root));

    assert!(!prompt.source_is_directory, "{prompt:#?}");
    assert!(prompt.destination_is_directory, "{prompt:#?}");
    assert_eq!(prompt.source_path, src_root.join("notes").display().to_string());
    assert_eq!(
        prompt.source_size,
        Some(KnownSize::Exact {
            bytes: "incoming file".len() as u64
        })
    );
}

/// An index that has counted only part of every folder: what a drive mid-first-
/// index answers.
struct PartlyCountedIndex;

impl OperationIndex for PartlyCountedIndex {
    fn expected_totals(&self, _sources: &[std::path::PathBuf]) -> Option<ExpectedTotals> {
        None
    }

    fn recursive_size(&self, _path: &Path) -> Option<KnownSize> {
        Some(KnownSize::AtLeast { bytes: 12_288 })
    }

    fn search_leaves(&self, _volume_id: &str, _abs_source: &Path, _cap: usize) -> BufferedLeaves {
        BufferedLeaves::downgraded(SearchCoverageReason::VolumeNotLive)
    }
}

/// The folder an Overwrite would delete must never be sized from a partial
/// count as if it were whole: the prompt carries the floor as a floor, and no
/// "(larger)" comparison is made against it (#528).
#[test]
fn a_partly_counted_folder_is_sized_as_a_lower_bound() {
    let engine = TestOperations::with(|builder| builder.index(Arc::new(PartlyCountedIndex)));
    let dir = temp("copy_file_over_partly_counted_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Copy, &[src_root.join("notes")], &dst_root));

    assert!(prompt.destination_is_directory, "{prompt:#?}");
    assert_eq!(prompt.destination_size, Some(KnownSize::AtLeast { bytes: 12_288 }));
    assert_eq!(
        prompt.size_difference, None,
        "no difference against a floor: {prompt:#?}"
    );
}

#[test]
fn a_moved_file_meeting_a_folder_is_described_as_a_file() {
    let engine = TestOperations::new();
    let dir = temp("move_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Move, &[src_root.join("notes")], &dst_root));

    assert!(!prompt.source_is_directory, "{prompt:#?}");
    assert!(prompt.destination_is_directory, "{prompt:#?}");
}

// ============================================================================
// Symlinks: a link is a leaf, whatever it points at
// ============================================================================
//
// The engines treat a link as a leaf, so a link to a folder meeting a real
// folder is a file→folder clash, and Overwrite replaces the whole folder with
// the link. A prompt that follows the link calls that "folder over folder",
// which the dialog renders as a plain clash with no warning.

/// Source: `docs` is a LINK to a folder elsewhere. Destination: a real folder
/// `docs/` with content.
fn link_over_folder_fixture(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    let target = dir.join("elsewhere");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("linked.txt"), "behind the link").unwrap();
    std::os::unix::fs::symlink(&target, src_root.join("docs")).unwrap();
    fs::create_dir_all(dst_root.join("docs")).unwrap();
    fs::write(dst_root.join("docs/precious.txt"), "precious user data").unwrap();
    (src_root, dst_root)
}

#[test]
fn a_copied_link_to_a_folder_meeting_a_folder_is_described_as_a_leaf() {
    let engine = TestOperations::new();
    let dir = temp("copy_link_over_folder");
    let (src_root, dst_root) = link_over_folder_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Copy, &[src_root.join("docs")], &dst_root));

    assert!(!prompt.source_is_directory, "the link is a leaf: {prompt:#?}");
    assert!(prompt.destination_is_directory, "{prompt:#?}");
}

#[test]
fn a_moved_link_to_a_folder_meeting_a_folder_is_described_as_a_leaf() {
    let engine = TestOperations::new();
    let dir = temp("move_link_over_folder");
    let (src_root, dst_root) = link_over_folder_fixture(&dir);

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Move, &[src_root.join("docs")], &dst_root));

    assert!(!prompt.source_is_directory, "the link is a leaf: {prompt:#?}");
    assert!(prompt.destination_is_directory, "{prompt:#?}");
}

/// The mirror: a real folder moving onto a LINK to a folder. Overwrite replaces
/// the link, so the existing side is a leaf.
#[test]
fn a_moved_folder_meeting_a_link_to_a_folder_is_described_as_folder_over_leaf() {
    let engine = TestOperations::new();
    let dir = temp("move_folder_over_link");
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    let target = dir.join("elsewhere");
    fs::create_dir_all(src_root.join("docs")).unwrap();
    fs::write(src_root.join("docs/incoming.txt"), "incoming").unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    std::os::unix::fs::symlink(&target, dst_root.join("docs")).unwrap();

    let prompt = the_one_prompt(prompts_for(&engine, Engine::Move, &[src_root.join("docs")], &dst_root));

    assert!(prompt.source_is_directory, "{prompt:#?}");
    assert!(!prompt.destination_is_directory, "the link is a leaf: {prompt:#?}");
}
