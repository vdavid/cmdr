//! A local copy whose destination tree holds a LINK to a folder where the
//! incoming tree has a real folder (#294).
//!
//! The copy loop walks FILES and creates their parents on the way, and its
//! "does the parent exist?" test follows links. So a link standing where a
//! folder is about to merge read as that folder, and every file below it landed
//! in the link's target: a folder the user never picked, whose same-named files
//! an Overwrite then replaced. A link is a leaf at every level below the
//! destination, and a folder meeting one is a type clash for the resolver.
//!
//! The destination ITSELF, and anything above it, may be a link: the person
//! navigated there, so that is the folder they picked.

#![cfg(unix)]

use super::super::conflict_responder_test_support::ConflictResponderSink;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

const OUTSIDE: &str = "OUTSIDE THE SELECTION";

fn temp(name: &str) -> TestDir {
    TestDir::new(&format!("copy_dest_link_{}_{}", name, uuid::Uuid::new_v4()))
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

fn policy(conflict_resolution: ConflictResolution) -> WriteOperationConfig {
    WriteOperationConfig {
        conflict_resolution,
        ..Default::default()
    }
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// The folder outside the selection: one file the incoming tree also names,
/// and a real subfolder the incoming tree also has.
fn plant_target(root: &Path) -> PathBuf {
    let target = root.join("outside/target");
    fs::create_dir_all(target.join("deeper")).unwrap();
    fs::write(target.join("inside.txt"), OUTSIDE).unwrap();
    target
}

/// The incoming tree, `src/album/`: a plain file, and a REAL folder `link/`
/// holding a file the target also has, one it doesn't, a file one level further
/// down (whose parent exists for real inside the target), and an empty folder
/// (which only the scanned-dirs pass lands).
fn plant_incoming_album(root: &Path) -> PathBuf {
    let album = root.join("src/album");
    fs::create_dir_all(album.join("link/deeper")).unwrap();
    fs::create_dir_all(album.join("link/empty")).unwrap();
    fs::write(album.join("plain.txt"), "PLAIN").unwrap();
    fs::write(album.join("link/inside.txt"), "INCOMING").unwrap();
    fs::write(album.join("link/new.txt"), "NEW").unwrap();
    fs::write(album.join("link/deeper/leaf.txt"), "LEAF").unwrap();
    album
}

/// `dst/album/` exists and holds `link`, pointing at the target.
fn plant_album_holding_a_link(root: &Path, target: &Path) -> PathBuf {
    let dst = root.join("dst");
    fs::create_dir_all(dst.join("album")).unwrap();
    std::os::unix::fs::symlink(target, dst.join("album/link")).unwrap();
    dst
}

/// What every policy owes the folder the link points at: its file keeps its
/// bytes, and nothing new arrives at any depth.
fn assert_target_untouched(target: &Path, policy: &str) {
    assert_eq!(
        fs::read_to_string(target.join("inside.txt")).unwrap(),
        OUTSIDE,
        "{policy}: the link's target must keep its own file"
    );
    let mut names: Vec<String> = fs::read_dir(target)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["deeper", "inside.txt"],
        "{policy}: nothing may land in the link's target"
    );
    assert_eq!(
        fs::read_dir(target.join("deeper")).unwrap().count(),
        0,
        "{policy}: nothing may land deeper in the link's target either"
    );
}

/// Skip, and a BLANKET Overwrite (which never crosses types): the link stays
/// the link, its target is untouched, and the rest of the folder still merges.
fn a_copy_leaves_a_deep_dest_link_alone(resolution: ConflictResolution) {
    let policy_name = format!("{resolution:?}");
    let dir = temp(&policy_name);
    let target = plant_target(&dir);
    let album = plant_incoming_album(&dir);
    let dst = plant_album_holding_a_link(&dir, &target);

    let result = copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        &format!("op-local-deep-dest-link-{policy_name}"),
        &state(),
        std::slice::from_ref(&album),
        &dst,
        &policy(resolution),
    );
    assert!(result.is_ok(), "{policy_name}: expected Ok, got {result:?}");

    assert_target_untouched(&target, &policy_name);
    assert!(
        is_link(&dst.join("album/link")),
        "{policy_name}: the link is still the link"
    );
    assert_eq!(
        fs::read_to_string(dst.join("album/plain.txt")).unwrap(),
        "PLAIN",
        "{policy_name}: the rest of the folder merged"
    );
}

#[test]
fn a_skipping_copy_leaves_a_deep_dest_link_alone() {
    a_copy_leaves_a_deep_dest_link_alone(ConflictResolution::Skip);
}

#[test]
fn a_blanket_overwrite_copy_leaves_a_deep_dest_link_alone() {
    a_copy_leaves_a_deep_dest_link_alone(ConflictResolution::Overwrite);
}

/// Rename: the incoming folder lands beside the link under a free name, empty
/// subfolder included.
#[test]
fn a_renaming_copy_lands_beside_a_deep_dest_link() {
    let dir = temp("rename");
    let target = plant_target(&dir);
    let album = plant_incoming_album(&dir);
    let dst = plant_album_holding_a_link(&dir, &target);

    let result = copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-local-deep-dest-link-rename",
        &state(),
        std::slice::from_ref(&album),
        &dst,
        &policy(ConflictResolution::Rename),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(&target, "Rename");
    assert!(is_link(&dst.join("album/link")), "the link is still the link");
    let beside = dst.join("album/link (1)");
    assert_eq!(fs::read_to_string(beside.join("inside.txt")).unwrap(), "INCOMING");
    assert_eq!(fs::read_to_string(beside.join("new.txt")).unwrap(), "NEW");
    assert_eq!(fs::read_to_string(beside.join("deeper/leaf.txt")).unwrap(), "LEAF");
    assert!(beside.join("empty").is_dir(), "the empty folder follows the rename");
}

/// An Overwrite a person ANSWERED for this clash: the prompt names the incoming
/// FOLDER landing on a non-folder, once, and the folder replaces the LINK. The
/// target keeps every byte.
#[test]
fn an_answered_overwrite_replaces_a_deep_dest_link_never_its_target() {
    let dir = temp("answered");
    let target = plant_target(&dir);
    let album = plant_incoming_album(&dir);
    let dst = plant_album_holding_a_link(&dir, &target);
    let state = state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false);

    let result = copy_files_with_progress_inner(
        &events,
        "op-local-deep-dest-link-answered",
        &state,
        std::slice::from_ref(&album),
        &dst,
        &policy(ConflictResolution::Stop),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    {
        let prompts = events.inner.conflicts.lock_ignore_poison();
        assert_eq!(prompts.len(), 1, "one prompt, for the folder meeting the link");
        assert!(
            prompts[0].source_is_directory && !prompts[0].destination_is_directory,
            "the prompt is a folder landing on a non-folder, got {:?}",
            prompts[0]
        );
        assert_eq!(prompts[0].source_path, album.join("link").display().to_string());
        assert_eq!(
            prompts[0].destination_path,
            dst.join("album/link").display().to_string()
        );
    }
    assert_target_untouched(&target, "answered Overwrite");
    let landed = dst.join("album/link");
    assert!(!is_link(&landed), "the folder replaced the link");
    assert_eq!(fs::read_to_string(landed.join("inside.txt")).unwrap(), "INCOMING");
    assert_eq!(fs::read_to_string(landed.join("new.txt")).unwrap(), "NEW");
    assert_eq!(fs::read_to_string(landed.join("deeper/leaf.txt")).unwrap(), "LEAF");
    assert!(landed.join("empty").is_dir());
    let mut names: Vec<String> = fs::read_dir(dst.join("album"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["link", "plain.txt"], "the set-aside link is dropped");
}

/// A folder holding nothing but empty folders never reaches the per-file loop,
/// so nobody is asked: the scanned-dirs pass leaves the link alone and creates
/// nothing through it.
#[test]
fn an_empty_incoming_folder_creates_nothing_through_a_deep_dest_link() {
    let dir = temp("empty_only");
    let target = plant_target(&dir);
    let album = dir.join("src/album");
    fs::create_dir_all(album.join("link/empty")).unwrap();
    let dst = plant_album_holding_a_link(&dir, &target);

    let result = copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-local-deep-dest-link-empty",
        &state(),
        std::slice::from_ref(&album),
        &dst,
        &policy(ConflictResolution::Overwrite),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(&target, "empty folders");
    assert!(is_link(&dst.join("album/link")), "the link is still the link");
}

/// The same clash at the TOP level: the selected folder's own name is a link in
/// the destination.
#[test]
fn a_copy_leaves_a_top_level_dest_link_alone() {
    let dir = temp("top_level");
    let target = plant_target(&dir);
    let incoming = dir.join("src/link");
    fs::create_dir_all(&incoming).unwrap();
    fs::write(incoming.join("inside.txt"), "INCOMING").unwrap();
    fs::write(incoming.join("new.txt"), "NEW").unwrap();
    let dst = dir.join("dst");
    fs::create_dir_all(&dst).unwrap();
    std::os::unix::fs::symlink(&target, dst.join("link")).unwrap();

    let result = copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-local-top-dest-link",
        &state(),
        std::slice::from_ref(&incoming),
        &dst,
        &policy(ConflictResolution::Overwrite),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_target_untouched(&target, "top level");
    assert!(is_link(&dst.join("link")), "the link is still the link");
}

/// The destination the person picked may itself be reached through a link: that
/// IS the folder they chose, so the copy merges into it as into any other.
#[test]
fn a_destination_that_is_itself_a_link_still_takes_the_copy() {
    let dir = temp("dest_is_link");
    let album = plant_incoming_album(&dir);
    let real = dir.join("real-dst");
    fs::create_dir_all(real.join("album")).unwrap();
    fs::write(real.join("album/kept.txt"), "KEPT").unwrap();
    let dst = dir.join("dst");
    std::os::unix::fs::symlink(&real, &dst).unwrap();

    let events = CollectorEventSink::new();
    let result = copy_files_with_progress_inner(
        &events,
        "op-local-dest-is-link",
        &state(),
        std::slice::from_ref(&album),
        &dst,
        &policy(ConflictResolution::Stop),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert!(events.conflicts.lock_ignore_poison().is_empty(), "nothing to ask");
    assert_eq!(fs::read_to_string(real.join("album/kept.txt")).unwrap(), "KEPT");
    assert_eq!(fs::read_to_string(real.join("album/plain.txt")).unwrap(), "PLAIN");
    assert_eq!(
        fs::read_to_string(real.join("album/link/deeper/leaf.txt")).unwrap(),
        "LEAF"
    );
    assert!(real.join("album/link/empty").is_dir());
}
