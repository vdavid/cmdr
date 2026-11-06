//! The home-folder guard: the rule itself, then each start that must refuse.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::testing::TestDir;
use cmdr_operation_log::types::Initiator;

use super::super::OperationEventSink;
use super::super::event_sinks::{CollectorEventSink, CollectorScanPreviewSink};
use super::super::mutation_error::MutationError;
use super::super::rename::{BulkRenameRow, RenameStartError};
use super::super::source_binding::SourceFingerprint;
use super::super::testing::TestOperations;
use super::super::types::{
    HomeFolderAction, HomeFolderRelation, ScanPreviewPurpose, ScanPreviewRefusal, VolumeCopyConfig,
    WriteOperationConfig, WriteOperationError,
};
use super::{HomeFolderHit, home_folder_hit};

fn hit_on_paper(home: &str, source: &str) -> Option<HomeFolderRelation> {
    home_folder_hit(Path::new(home), &[PathBuf::from(source)], false).map(|hit| hit.relation)
}

#[test]
fn the_home_folder_itself_is_refused_however_it_is_spelled() {
    for source in [
        "/Users/me",
        "/Users/me/",
        "/Users/me//",
        "/Users/./me",
        "/Users/me/Documents/..",
        "~",
        "~/",
    ] {
        assert_eq!(
            hit_on_paper("/Users/me", source),
            Some(HomeFolderRelation::IsHome),
            "source: {source}"
        );
    }
}

#[test]
fn every_folder_above_the_home_folder_is_refused() {
    for source in ["/Users", "/Users/", "/", "/Users/me/.."] {
        assert_eq!(
            hit_on_paper("/Users/me", source),
            Some(HomeFolderRelation::HoldsHome),
            "the home folder is inside {source}"
        );
    }
}

#[test]
fn other_folders_stay_deletable() {
    for source in [
        "/Users/other",
        "/Users/me/Documents",
        "~/Documents",
        "/Users/meme",
        "/Users/Shared",
        "~other",
    ] {
        assert_eq!(hit_on_paper("/Users/me", source), None, "deletable: {source}");
    }
}

#[test]
fn the_refusal_names_the_source_with_its_tilde_expanded() {
    let hit = home_folder_hit(Path::new("/Users/me"), &[PathBuf::from("~")], false);
    assert_eq!(
        hit,
        Some(HomeFolderHit {
            path: "/Users/me".into(),
            relation: HomeFolderRelation::IsHome,
        })
    );
}

#[test]
fn one_guarded_source_refuses_the_whole_batch() {
    let sources = [PathBuf::from("/Users/me/a.txt"), PathBuf::from("/Users")];
    let hit = home_folder_hit(Path::new("/Users/me"), &sources, false).expect("refused");
    assert_eq!(hit.path, "/Users");
}

/// A home folder on disk, `<root>/Users/me`, and a second user beside it.
fn disk_home(label: &str) -> (TestDir, PathBuf) {
    let root = TestDir::new(label);
    let home = root.join("Users/me");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(root.join("Users/other")).expect("other home");
    (root, home)
}

#[cfg(unix)]
#[test]
fn a_path_through_a_symlinked_folder_reaches_the_home_folder() {
    let (root, home) = disk_home("home-guard-symlinked-parent");
    let link = root.join("people");
    std::os::unix::fs::symlink(root.join("Users"), &link).expect("symlink");

    let hit = home_folder_hit(&home, &[link.join("me")], true).map(|hit| hit.relation);
    assert_eq!(hit, Some(HomeFolderRelation::IsHome));
    let other = home_folder_hit(&home, &[link.join("other")], true);
    assert_eq!(other, None, "another user's home stays deletable through the link too");
}

#[cfg(unix)]
#[test]
fn a_symlink_pointing_at_the_home_folder_stays_deletable() {
    // Deleting the link removes the link, never the folder it points at.
    let (root, home) = disk_home("home-guard-symlink-itself");
    let link = root.join("my-home");
    std::os::unix::fs::symlink(&home, &link).expect("symlink");

    assert_eq!(home_folder_hit(&home, &[link], true), None);
}

#[cfg(unix)]
#[test]
fn a_home_folder_given_through_a_symlink_still_guards_its_real_path() {
    // `$HOME` can itself run through a symlink (macOS's `/tmp` → `/private/tmp`
    // is one under every temp dir here).
    let (root, home) = disk_home("home-guard-home-via-link");
    let link = root.join("home-link");
    std::os::unix::fs::symlink(&home, &link).expect("symlink");
    let real_home = std::fs::canonicalize(&home).expect("canonical");

    let hit = home_folder_hit(&link, &[real_home], true).map(|hit| hit.relation);
    assert_eq!(hit, Some(HomeFolderRelation::IsHome));
}

fn engine_guarding(home: &Path) -> TestOperations {
    let home = home.to_path_buf();
    TestOperations::with(|builder| builder.home_folder(home))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_of_the_home_folder_refuses_before_starting() {
    let (_root, home) = disk_home("home-guard-delete");
    let engine = engine_guarding(&home);
    let events = Arc::new(CollectorEventSink::new());

    let refused = engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![home.clone()],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await;

    assert!(
        matches!(
            refused,
            Err(WriteOperationError::HomeFolder {
                relation: HomeFolderRelation::IsHome,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(home.exists(), "nothing was deleted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trash_of_a_folder_holding_the_home_folder_refuses_before_starting() {
    let (_root, home) = disk_home("home-guard-trash");
    let engine = engine_guarding(&home);
    let events = Arc::new(CollectorEventSink::new());
    let users = home.parent().expect("parent").to_path_buf();

    let refused = engine
        .trash_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![users.clone()],
            None,
            WriteOperationConfig::default(),
            Initiator::User,
            None,
        )
        .await;

    assert!(
        matches!(
            refused,
            Err(WriteOperationError::HomeFolder {
                relation: HomeFolderRelation::HoldsHome,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(users.exists(), "nothing was trashed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_preview_over_the_home_folder_refuses_without_walking() {
    let (_root, home) = disk_home("home-guard-preview");
    let engine = engine_guarding(&home);

    let refused = engine.start_scan_preview(
        Arc::new(CollectorScanPreviewSink::new()),
        vec![PathBuf::from(&home)],
        None,
        "root".into(),
        50,
        ScanPreviewPurpose::Delete,
        cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    );

    assert_eq!(
        refused.err(),
        Some(ScanPreviewRefusal::HomeFolder {
            path: home.display().to_string(),
            relation: HomeFolderRelation::IsHome,
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_preview_of_the_home_folder_still_walks() {
    // Copying the home folder somewhere is fine; only removing it isn't.
    let (_root, home) = disk_home("home-guard-copy-preview");
    let engine = engine_guarding(&home);
    let started = engine.start_scan_preview(
        Arc::new(CollectorScanPreviewSink::new()),
        vec![PathBuf::from(&home)],
        None,
        "root".into(),
        50,
        ScanPreviewPurpose::Transfer,
        cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    );
    let preview = started.expect("a copy preview starts");
    engine.cancel_scan_preview(&preview.preview_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_users_home_folder_stays_deletable() {
    let (root, home) = disk_home("home-guard-other-user");
    let engine = engine_guarding(&home);
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![root.join("Users/other")],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("another user's home deletes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_on_another_volume_is_not_the_home_folders_business() {
    let (_root, home) = disk_home("home-guard-other-volume");
    let engine = engine_guarding(&home);
    assert_eq!(engine.home_folder_hit("usb-1", &[home]), None);
}

fn refused_as(result: &Option<WriteOperationError>) -> Option<(HomeFolderRelation, HomeFolderAction)> {
    match result {
        Some(WriteOperationError::HomeFolder { relation, action, .. }) => Some((*relation, *action)),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guard_answers_each_action_before_anything_opens() {
    let (root, home) = disk_home("home-guard-query");
    let engine = engine_guarding(&home);
    let users = home.parent().expect("parent").to_path_buf();

    for action in [
        HomeFolderAction::Delete,
        HomeFolderAction::Move,
        HomeFolderAction::Rename,
    ] {
        assert_eq!(
            refused_as(&engine.home_folder_guard("root", std::slice::from_ref(&home), action)),
            Some((HomeFolderRelation::IsHome, action))
        );
        assert_eq!(
            refused_as(&engine.home_folder_guard("root", std::slice::from_ref(&users), action)),
            Some((HomeFolderRelation::HoldsHome, action))
        );
        assert!(
            engine
                .home_folder_guard("root", &[root.join("Users/other")], action)
                .is_none()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_disk_move_of_the_home_folder_refuses_before_starting() {
    let (root, home) = disk_home("home-guard-move");
    let engine = engine_guarding(&home);
    let events = Arc::new(CollectorEventSink::new());
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("destination");

    let refused = engine
        .move_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![home.clone()],
            elsewhere,
            WriteOperationConfig::default(),
            vec![],
            None,
            Initiator::User,
            None,
            None,
        )
        .await;

    assert!(
        matches!(
            refused,
            Err(WriteOperationError::HomeFolder {
                relation: HomeFolderRelation::IsHome,
                action: HomeFolderAction::Move,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(home.exists(), "nothing moved");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_onto_another_volume_of_a_folder_holding_the_home_refuses_before_starting() {
    // Copy-then-delete: the source side is what's guarded, so the destination
    // volume needn't even answer.
    let (_root, home) = disk_home("home-guard-cross-move");
    let engine = engine_guarding(&home);
    let events = Arc::new(CollectorEventSink::new());
    let users = home.parent().expect("parent").to_path_buf();

    let refused = engine
        .start_volume_move(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            "root".into(),
            vec![users.clone()],
            "usb-1".into(),
            "/backup".into(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;

    assert!(
        matches!(
            refused,
            Err(WriteOperationError::HomeFolder {
                relation: HomeFolderRelation::HoldsHome,
                action: HomeFolderAction::Move,
                ..
            })
        ),
        "{refused:?}"
    );
    assert!(users.exists(), "nothing moved");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_of_the_home_folder_refuses_at_the_preflight_and_the_rename() {
    let (_root, home) = disk_home("home-guard-rename");
    let engine = engine_guarding(&home);
    let renamed = home.with_file_name("someone-else");

    let preflight = engine
        .check_rename_permission_for_volume(home.clone(), "root".into())
        .await;
    assert!(
        matches!(
            preflight,
            Err(MutationError::HomeFolder {
                relation: HomeFolderRelation::IsHome,
                ..
            })
        ),
        "{preflight:?}"
    );

    let refused = engine
        .rename_managed(home.clone(), renamed.clone(), false, "root".into(), Initiator::User)
        .await;
    assert!(matches!(refused, Err(MutationError::HomeFolder { .. })), "{refused:?}");
    assert!(home.exists() && !renamed.exists(), "nothing renamed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_of_another_users_home_folder_goes_ahead() {
    let (root, home) = disk_home("home-guard-rename-other");
    let engine = engine_guarding(&home);
    let other = root.join("Users/other");
    engine
        .rename_managed(
            other.clone(),
            other.with_file_name("guest"),
            false,
            "root".into(),
            Initiator::User,
        )
        .await
        .expect("another user's home renames");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_rename_or_a_rename_by_move_of_the_home_folder_refuses() {
    let (_root, home) = disk_home("home-guard-bulk-rename");
    let engine = engine_guarding(&home);
    let events = Arc::new(CollectorEventSink::new());
    let row = BulkRenameRow {
        row_id: "row-1".into(),
        source: home.clone(),
        destination: home.with_file_name("renamed"),
        expected_fingerprint: SourceFingerprint::capture_local(&home).expect("fingerprint"),
    };

    let batch = engine
        .start_renames(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            "root".into(),
            vec![row],
            Initiator::User,
        )
        .await;
    assert!(
        matches!(
            &batch,
            Err(RenameStartError::Engine {
                error: WriteOperationError::HomeFolder {
                    action: HomeFolderAction::Rename,
                    ..
                }
            })
        ),
        "{:?}",
        batch.err()
    );

    let by_move = engine
        .start_rename_by_move(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            "root".into(),
            vec![(home.clone(), "renamed".into())],
            home.parent().expect("parent").display().to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(
        matches!(
            by_move,
            Err(WriteOperationError::HomeFolder {
                action: HomeFolderAction::Rename,
                ..
            })
        ),
        "{by_move:?}"
    );
    assert!(home.exists(), "nothing renamed");
}
