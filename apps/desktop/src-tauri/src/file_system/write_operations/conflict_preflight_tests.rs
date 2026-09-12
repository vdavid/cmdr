//! Part of `conflict_preflight.rs`, split out as a `#[path]` child so the
//! module itself stays readable. `super::` here is `conflict_preflight`.

use super::{
    Deadline, SourceItemInput, VolumeScanError, merge_source_types_from_stats, scan_volume_for_conflicts_within,
};
use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
use crate::file_system::{InMemoryVolume, LocalPosixVolume, SourceItemInfo};
use crate::test_support::WedgedVolume;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The check must answer, or say it couldn't, within its budget — including
/// when the volumes it asks have stopped answering entirely.
///
/// A user watching `Checking for conflicts...` over a share whose connection
/// had dropped waited it out for minutes. Each leg owning its own timeout is
/// not enough: the legs run in sequence, so the promise is their SUM, and a
/// leg with no timeout at all (the volume resolves) makes it unbounded.
#[tokio::test]
async fn a_check_against_volumes_that_never_answer_gives_up_inside_its_budget() {
    let budget = Duration::from_millis(300);
    let _source = TestVolumeRegistration::install(
        "wedged-source",
        Arc::new(WedgedVolume::new("WedgedSource")) as Arc<dyn Volume>,
    );
    let _dest = TestVolumeRegistration::install(
        "wedged-dest",
        Arc::new(WedgedVolume::new("WedgedDest")) as Arc<dyn Volume>,
    );

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        scan_volume_for_conflicts_within(
            Deadline::new(budget),
            String::from("wedged-dest"),
            vec![SourceItemInput {
                name: String::from("holiday.mov"),
                size: 0,
                modified: None,
                is_directory: false,
            }],
            String::from("/incoming"),
            Some(String::from("wedged-source")),
            Some(vec![String::from("/media/holiday.mov")]),
        ),
    )
    .await
    .expect("the check must come back on its own, not be cut off by the test");

    let err = outcome.expect_err("a check that never reached either volume cannot report 'no conflicts'");
    assert!(
        matches!(err, VolumeScanError::TimedOut),
        "and it says WHY by variant, so the dialog can offer a retry without reading a sentence"
    );
    assert!(
        started.elapsed() < budget * 4,
        "the whole check owes one budget, not one per leg: took {:?}",
        started.elapsed()
    );
}

/// A source that won't answer costs its OWN leg, never the destination's.
///
/// The source stat is an optional refinement — the code says so, and falls
/// back to the caller's name-only items when it fails. But it used to share
/// one budget with the destination scan, so a source that never answered
/// spent all 30 s and the destination scan then failed instantly, reporting
/// `couldn't read the destination (timed out)` for a destination it had
/// never asked. That is what a user saw on 2026-08-27 (`ERR-AYVM4`): the
/// source was one 119k-file folder, and the recursive walk the scan used to
/// run could never have finished inside any budget.
#[tokio::test]
async fn a_source_that_never_answers_still_lets_the_destination_answer() {
    let dest_dir = tempfile::tempdir().expect("dest dir");
    std::fs::write(dest_dir.path().join("photo.jpg"), b"theirs").expect("write dest");
    let _source = TestVolumeRegistration::install(
        "starving-source",
        Arc::new(WedgedVolume::new("WedgedSource")) as Arc<dyn Volume>,
    );
    let _dest = TestVolumeRegistration::install(
        "starving-dest",
        Arc::new(LocalPosixVolume::new("Dest", dest_dir.path())) as Arc<dyn Volume>,
    );

    let budget = Duration::from_millis(900);
    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(budget),
        String::from("starving-dest"),
        vec![input("photo.jpg")],
        dest_dir.path().to_string_lossy().into_owned(),
        Some(String::from("starving-source")),
        Some(vec![String::from("/media/photo.jpg")]),
    )
    .await
    .expect("the destination is fine, so the check answers rather than blaming it");

    assert_eq!(
        conflicts.len(),
        1,
        "the clash is real and the dialog still has to say so, name-only hints or not"
    );
}

/// A source pasted into the folder it already lives in is a request to
/// DUPLICATE it, and both engines answer it by silently auto-renaming
/// (`write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in
/// place)"). The pre-flight matches destination entries by NAME, so without
/// the filter every source of a same-folder copy comes back as its own
/// conflict, and the dialog announces a conflict count, shows the
/// overwrite/skip/rename radios, and hands the backend a pre-known-conflict
/// list naming every source.
#[tokio::test]
async fn a_same_folder_copy_finds_no_conflicts() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("photo.jpg"), b"bytes").expect("write source");
    std::fs::create_dir(dir.path().join("docs")).expect("create source dir");
    let _volume = TestVolumeRegistration::install(
        "self-collision-scan",
        Arc::new(LocalPosixVolume::new("Duplicates", dir.path())) as Arc<dyn Volume>,
    );

    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(Duration::from_secs(5)),
        String::from("self-collision-scan"),
        vec![input("photo.jpg"), input("docs")],
        dir.path().to_string_lossy().into_owned(),
        Some(String::from("self-collision-scan")),
        Some(vec![
            dir.path().join("photo.jpg").to_string_lossy().into_owned(),
            dir.path().join("docs").to_string_lossy().into_owned(),
        ]),
    )
    .await
    .expect("the scan answers");

    assert!(
        conflicts.is_empty(),
        "an item landing on itself is a duplicate, not a conflict, but the scan reported {conflicts:?}"
    );
}

/// The other half of the same rule: a DIFFERENT file of the same name is
/// still in the way, and the dialog still has to say so.
#[tokio::test]
async fn a_different_file_of_the_same_name_is_still_a_conflict() {
    let source_dir = tempfile::tempdir().expect("source dir");
    let dest_dir = tempfile::tempdir().expect("dest dir");
    std::fs::write(source_dir.path().join("photo.jpg"), b"mine").expect("write source");
    std::fs::write(dest_dir.path().join("photo.jpg"), b"theirs").expect("write dest");
    let _source = TestVolumeRegistration::install(
        "self-collision-scan-source",
        Arc::new(LocalPosixVolume::new("Source", source_dir.path())) as Arc<dyn Volume>,
    );
    let _dest = TestVolumeRegistration::install(
        "self-collision-scan-dest",
        Arc::new(LocalPosixVolume::new("Dest", dest_dir.path())) as Arc<dyn Volume>,
    );

    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(Duration::from_secs(5)),
        String::from("self-collision-scan-dest"),
        vec![input("photo.jpg")],
        dest_dir.path().to_string_lossy().into_owned(),
        Some(String::from("self-collision-scan-source")),
        Some(vec![source_dir.path().join("photo.jpg").to_string_lossy().into_owned()]),
    )
    .await
    .expect("the scan answers");

    assert_eq!(conflicts.len(), 1, "a real clash still reaches the dialog");
    assert_eq!(conflicts[0].source_path, "photo.jpg");
}

/// A source reached through a symlinked parent is the same file, and the
/// LOCAL engine (which a both-local transfer routes to) settles it with
/// `dev+ino`. A folded-path comparison would miss it and the dialog would
/// invent a conflict the engine then silently duplicates past.
#[tokio::test]
async fn a_source_reached_through_a_symlinked_parent_finds_no_conflicts() {
    let dir = tempfile::tempdir().expect("temp dir");
    let real = dir.path().join("real");
    std::fs::create_dir(&real).expect("create real dir");
    std::fs::write(real.join("photo.jpg"), b"bytes").expect("write source");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("symlink the parent");
    let _volume = TestVolumeRegistration::install(
        "self-collision-scan-symlink",
        Arc::new(LocalPosixVolume::new("Duplicates", dir.path())) as Arc<dyn Volume>,
    );

    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(Duration::from_secs(5)),
        String::from("self-collision-scan-symlink"),
        vec![input("photo.jpg")],
        real.to_string_lossy().into_owned(),
        Some(String::from("self-collision-scan-symlink")),
        Some(vec![link.join("photo.jpg").to_string_lossy().into_owned()]),
    )
    .await
    .expect("the scan answers");

    assert!(
        conflicts.is_empty(),
        "the symlinked route names the same file, so it is a duplicate: got {conflicts:?}"
    );
}

/// The cross-volume arm of the same rule. No backend out here offers an
/// inode, so identity is one volume, the same parent, and a folded LEAF, and
/// that fold is what makes a case-differing name (an SMB share, a macOS
/// volume) count. A case-differing PARENT deliberately does not: see
/// `transfer/volume/item_identity.rs::is_the_same_volume_path`.
#[tokio::test]
async fn a_same_folder_copy_on_a_remote_volume_finds_no_conflicts() {
    let volume = Arc::new(InMemoryVolume::new("Device")) as Arc<dyn Volume>;
    volume.create_directory(Path::new("/photos")).await.expect("create dir");
    volume
        .create_file(Path::new("/photos/photo.jpg"), b"pixels")
        .await
        .expect("create file");
    let _registered = TestVolumeRegistration::install("self-collision-scan-device", Arc::clone(&volume));

    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(Duration::from_secs(5)),
        String::from("self-collision-scan-device"),
        vec![input("photo.jpg")],
        String::from("/photos"),
        Some(String::from("self-collision-scan-device")),
        Some(vec![String::from("/photos/Photo.JPG")]),
    )
    .await
    .expect("the scan answers");

    assert!(
        conflicts.is_empty(),
        "the folded leaf names the same item, so it is a duplicate: got {conflicts:?}"
    );
}

/// The dialog and the engine have to agree, so the pre-flight draws the
/// parent line in the same place: a source from a differently-cased folder is
/// a real clash, not a duplicate. Dropping it here would announce "no
/// conflicts" for a transfer the engine then prompts about.
#[tokio::test]
async fn a_source_from_a_case_differing_folder_is_still_a_conflict() {
    let volume = Arc::new(InMemoryVolume::new("Device")) as Arc<dyn Volume>;
    for dir in ["/photos", "/PHOTOS"] {
        volume.create_directory(Path::new(dir)).await.expect("create dir");
        volume
            .create_file(&Path::new(dir).join("photo.jpg"), b"pixels")
            .await
            .expect("create file");
    }
    let _registered = TestVolumeRegistration::install("self-collision-scan-case-parent", Arc::clone(&volume));

    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(Duration::from_secs(5)),
        String::from("self-collision-scan-case-parent"),
        vec![input("photo.jpg")],
        String::from("/photos"),
        Some(String::from("self-collision-scan-case-parent")),
        Some(vec![String::from("/PHOTOS/photo.jpg")]),
    )
    .await
    .expect("the scan answers");

    assert_eq!(
        conflicts.len(),
        1,
        "a differently-cased parent is another folder as far as we may say: got {conflicts:?}"
    );
}

/// Two volumes that happen to spell a path the same way hold two different
/// items, so the clash is real and the dialog still has to say so.
#[tokio::test]
async fn the_same_path_on_two_volumes_is_still_a_conflict() {
    let source = Arc::new(InMemoryVolume::new("Source")) as Arc<dyn Volume>;
    let dest = Arc::new(InMemoryVolume::new("Dest")) as Arc<dyn Volume>;
    for volume in [&source, &dest] {
        volume.create_directory(Path::new("/photos")).await.expect("create dir");
        volume
            .create_file(Path::new("/photos/photo.jpg"), b"pixels")
            .await
            .expect("create file");
    }
    let _source_reg = TestVolumeRegistration::install("self-collision-scan-two-source", source);
    let _dest_reg = TestVolumeRegistration::install("self-collision-scan-two-dest", dest);

    let conflicts = scan_volume_for_conflicts_within(
        Deadline::new(Duration::from_secs(5)),
        String::from("self-collision-scan-two-dest"),
        vec![input("photo.jpg")],
        String::from("/photos"),
        Some(String::from("self-collision-scan-two-source")),
        Some(vec![String::from("/photos/photo.jpg")]),
    )
    .await
    .expect("the scan answers");

    assert_eq!(conflicts.len(), 1, "two volumes, two items, one real clash");
}

fn input(name: &str) -> SourceItemInput {
    SourceItemInput {
        name: name.to_string(),
        size: 0,
        modified: None,
        is_directory: false,
    }
}

fn stat(path: &str, is_dir: bool, bytes: u64) -> (PathBuf, FileEntry) {
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    (
        PathBuf::from(path),
        FileEntry {
            size: Some(bytes),
            ..FileEntry::new(name, path.to_string(), is_dir, false)
        },
    )
}

fn item(name: &str) -> SourceItemInfo {
    SourceItemInfo {
        name: name.to_string(),
        size: 0,
        modified: None,
        is_directory: false,
    }
}

#[test]
fn overlays_real_directory_flag_onto_placeholder_items() {
    let mut items = vec![item("photos"), item("readme.txt")];
    let stats = vec![stat("/src/photos", true, 999_999), stat("/src/readme.txt", false, 42)];

    merge_source_types_from_stats(&mut items, &stats);

    // The directory item is now flagged as such; whatever size the stat
    // reported for it is deliberately NOT copied (a dir's conflict size is
    // meaningless).
    assert!(items[0].is_directory);
    assert_eq!(items[0].size, 0);
    // The file item gets its real size.
    assert!(!items[1].is_directory);
    assert_eq!(items[1].size, 42);
}

#[test]
fn keeps_caller_values_when_no_stat_hit() {
    let mut items = vec![SourceItemInfo {
        name: "ghost".to_string(),
        size: 7,
        modified: Some(123),
        is_directory: true,
    }];
    let stats = vec![stat("/src/other", false, 1)];

    merge_source_types_from_stats(&mut items, &stats);

    // No matching name → the caller's values survive untouched.
    assert!(items[0].is_directory);
    assert_eq!(items[0].size, 7);
    assert_eq!(items[0].modified, Some(123));
}
