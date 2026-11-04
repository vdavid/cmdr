//! Opening files out of a repo's `.git` snapshot and a restic backup: the app's routes and
//! materializer under the viewer, end to end through the app's viewer.

use super::app_viewer as viewer;
use super::{ViewerContentKind, ViewerError};

/// The viewer opens a file that lives in a repo's `.git` snapshot: the portal
/// volume streams the blob to a bounded temp and the viewer opens THAT, exactly
/// as it does for an entry inside a zip. Before the materialization covered
/// every route, `file_path.exists()` was false for a snapshot path and the open
/// answered `notFound`.
#[test]
fn opens_a_file_out_of_a_git_snapshot_and_reads_its_lines() {
    use crate::file_system::git;
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};
    use cmdr_local::LocalPosixVolume;
    use std::sync::Arc;

    let dir = temp_dir("viewer_session", "git_snapshot");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("notes.txt", b"first line\nsecond line\n", "initial");
    // (nextest isolates the process-global manager per test.)
    get_volume_manager().register(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Root",
            dir.to_str().unwrap(),
            crate::volume_host::host(),
        )),
    );
    git::wiring::set_virtual_portal_enabled(true);

    let snapshot = dir.join(".git/branches/main/notes.txt");
    let opened = viewer()
        .open_session(snapshot.to_str().unwrap(), "root")
        .expect("the snapshot file opens");
    assert_eq!(opened.initial_lines.rows[0].text, "first line");
    assert_eq!(opened.initial_lines.rows[1].text, "second line");
    // The session is named for the path the user asked for, not for the temp.
    assert!(opened.file_name.ends_with("notes.txt"));

    viewer().close_session(&opened.session_id).expect("close");

    // A directory in the snapshot is a directory, ❌ never an empty preview.
    assert!(matches!(
        viewer().open_session(dir.join(".git/branches/main").to_str().unwrap(), "root"),
        Err(ViewerError::IsDirectory)
    ));
    // And a path that isn't in the snapshot is honestly missing.
    assert!(matches!(
        viewer().open_session(dir.join(".git/branches/main/nope.txt").to_str().unwrap(), "root"),
        Err(ViewerError::NotFound { .. })
    ));

    cleanup(&dir);
}

/// A route has no positioned reads of its own to offer, so a picture in a snapshot renders
/// from the bounded temp, sniffed there like any local file.
#[test]
fn a_picture_in_a_restic_snapshot_renders_from_its_temp() {
    use crate::file_system::restic::test_repo::{SourceFile, backed_up_drive};

    const HEIC: &[u8] = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic";
    let _secrets = crate::test_support::isolate_secrets();
    let drive = tauri::async_runtime::block_on(async {
        let drive = backed_up_drive(
            "viewer-restic-media",
            &[SourceFile {
                path: "photo.heic",
                bytes: HEIC,
                mode: 0o644,
                modified: 1_600_000_000,
            }],
            &[],
        )
        .await;
        drive.unlock().await;
        drive
    });

    let file = drive.backed_up.join("photo.heic");
    let opened = viewer()
        .open_session(file.to_str().unwrap(), &drive.volume_id)
        .expect("the backed-up file opens");
    assert_eq!(opened.kind, ViewerContentKind::Image);
    assert!(!opened.media_from_volume, "a temp is a local file");
    viewer().close_session(&opened.session_id).expect("close");
}

/// The viewer opens a file inside a restic snapshot the same way: the view
/// streams it (blob by blob, out of the repo's packs) to a bounded temp, and the
/// viewer opens that.
#[test]
fn opens_a_file_out_of_a_restic_snapshot_and_reads_its_lines() {
    use crate::file_system::restic::test_repo::{SourceFile, backed_up_drive};

    let _secrets = crate::test_support::isolate_secrets();
    let drive = tauri::async_runtime::block_on(async {
        let drive = backed_up_drive(
            "viewer-restic",
            &[SourceFile {
                path: "notes.txt",
                bytes: b"first line\nsecond line\n",
                mode: 0o644,
                modified: 1_600_000_000,
            }],
            &[],
        )
        .await;
        drive.unlock().await;
        drive
    });

    let file = drive.backed_up.join("notes.txt");
    let opened = viewer()
        .open_session(file.to_str().unwrap(), &drive.volume_id)
        .expect("the backed-up file opens");
    assert_eq!(opened.initial_lines.rows[0].text, "first line");
    assert_eq!(opened.initial_lines.rows[1].text, "second line");
    assert!(opened.file_name.ends_with("notes.txt"));
    viewer().close_session(&opened.session_id).expect("close");

    assert!(matches!(
        viewer().open_session(drive.backed_up.to_str().unwrap(), &drive.volume_id),
        Err(ViewerError::IsDirectory)
    ));
    assert!(matches!(
        viewer().open_session(drive.backed_up.join("nope.txt").to_str().unwrap(), &drive.volume_id),
        Err(ViewerError::NotFound { .. })
    ));
}
