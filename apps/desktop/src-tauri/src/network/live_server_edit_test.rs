//! `live_server_edit`'s cells: what an edit to a connected place asks the
//! server, what it refuses, and what installing it changes.
//!
//! Protocol-free, like the module: `InMemoryVolume` stands in for the server,
//! keyed by app paths the way a real remote volume spells them. The Docker cells
//! that run the same flow over a real session are in `sftp_volume_wiring_test.rs`
//! and `webdav_volume_wiring_test.rs`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::{AcceptedEdit, ConnectedPlace, PlaceEdit, check};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::manager::VolumeManager;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::volume_broadcast::{self, VolumeRootChanged};

const PREFIX: &str = "sftp://ada@nas.local:22";

/// Roomy for an in-memory stat on a loaded machine.
const BUDGET: Duration = Duration::from_secs(2);

/// Short, for the cells where asking the server at all would be the bug: a stat
/// on a hanging session comes back `Unreachable` fast instead of passing.
const NO_TIME: Duration = Duration::from_millis(50);

fn app(remote: &str) -> String {
    format!("{PREFIX}{remote}")
}

fn leaf(remote: &str) -> String {
    Path::new(remote)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A server holding `/srv/data` with `tmp/` and `photos/` folders and a
/// `notes.txt` file, as a volume rooted at the server-side `root` sees it.
fn server_at(name: &str, root: &str) -> Arc<dyn Volume> {
    let dir = |remote: &str| FileEntry::new(leaf(remote), app(remote), true, false);
    Arc::new(
        InMemoryVolume::with_entries(
            name,
            vec![
                dir("/srv/data"),
                dir("/srv/data/tmp"),
                dir("/srv/data/photos"),
                FileEntry::new(leaf("/srv/data/notes.txt"), app("/srv/data/notes.txt"), false, false),
            ],
        )
        .with_root(app(root)),
    )
}

/// How the wiring builds a successor, over the same server.
fn over_the_same_server(name: &str, root: &Path) -> Arc<dyn Volume> {
    server_at(name, &root.to_string_lossy())
}

/// A volume whose session is gone: every stat answers `DeviceDisconnected`, or
/// never answers at all.
struct DeadSession {
    name: String,
    root: PathBuf,
    hangs: bool,
}

fn dead_session(name: &str, root: &str, hangs: bool) -> Arc<dyn Volume> {
    Arc::new(DeadSession {
        name: name.to_string(),
        root: PathBuf::from(app(root)),
        hangs,
    })
}

impl Volume for DeadSession {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::DeviceDisconnected("dead".to_string())) })
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::DeviceDisconnected("dead".to_string())) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        let hangs = self.hangs;
        Box::pin(async move {
            if hangs {
                std::future::pending::<()>().await;
            }
            Err(VolumeError::DeviceDisconnected("dead".to_string()))
        })
    }
}

fn place(id: &str, live: &Arc<dyn Volume>, saved_start_folder: Option<&str>) -> ConnectedPlace {
    ConnectedPlace {
        volume_id: id.to_string(),
        live: Arc::clone(live),
        app_prefix: PREFIX.to_string(),
        saved_start_folder: saved_start_folder.map(str::to_string),
    }
}

fn registered(id: &str, live: &Arc<dyn Volume>) -> VolumeManager {
    let manager = VolumeManager::new();
    manager.register(id, Arc::clone(live));
    manager
}

fn refusal(outcome: Result<AcceptedEdit, SavedServerOutcome>) -> Option<SavedServerOutcome> {
    outcome.err()
}

// ── Accepted ─────────────────────────────────────────────────────────

/// The reported bug's own shape: a place connected at `/srv/data/tmp` widened to
/// `/srv/data`. The live registry serves the wider root, the switcher is asked
/// to republish, and the panes hear where the root and the landing went.
#[tokio::test]
async fn a_wider_root_the_server_has_is_installed_and_announced() {
    let id = "live-edit-wider-root";
    let live = server_at("ada@nas.local", "/srv/data/tmp");
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let accepted = check(&place(id, &live, None), &edit, over_the_same_server, BUDGET)
        .await
        .unwrap_or_else(|refusal| panic!("the server has /srv/data, refused with {refusal:?}"));
    let _recorders = volume_broadcast::recorder_test_lock();
    let asked_before = volume_broadcast::volumes_changed_requests();
    accepted.install(&manager);

    let installed = manager.get(id).expect("still registered");
    assert_eq!(installed.root(), Path::new(&app("/srv/data")));
    assert!(
        volume_broadcast::volumes_changed_requests() > asked_before,
        "the switcher hears about the new root"
    );
    assert_eq!(
        volume_broadcast::volume_root_changes(id),
        vec![VolumeRootChanged {
            volume_id: id.to_string(),
            old_root: app("/srv/data/tmp"),
            new_root: app("/srv/data"),
            old_landing: app("/srv/data/tmp"),
            new_landing: app("/srv/data"),
        }]
    );
}

/// A rename moves no folder, so ❗ nothing is asked of the server: the session
/// here never answers, and a stat would come back `Unreachable`.
#[tokio::test]
async fn a_rename_alone_swaps_the_instance_without_asking_the_server() {
    let id = "live-edit-rename";
    let live = dead_session("ada@nas.local", "/srv/data", true);
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "Naspolya",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let accepted = check(
        &place(id, &live, None),
        &edit,
        |name: &str, root: &Path| dead_session(name, &root.to_string_lossy(), true),
        NO_TIME,
    )
    .await
    .unwrap_or_else(|refusal| panic!("a rename needs no folder confirmed, refused with {refusal:?}"));
    accepted.install(&manager);

    let installed = manager.get(id).expect("still registered");
    assert_eq!(installed.name(), "Naspolya");
    assert_eq!(installed.root(), Path::new(&app("/srv/data")));
    assert!(
        volume_broadcast::volume_root_changes(id).is_empty(),
        "neither the root nor the landing moved"
    );
}

/// ❗ An edit that moves neither the root nor the landing asks the server
/// NOTHING and installs nothing, even with a start folder set. The
/// "reconnect automatically" switch is edited through this same save, and it's
/// most needed exactly when the session is down.
#[tokio::test]
async fn an_edit_that_moves_no_folder_asks_the_server_nothing() {
    let id = "live-edit-switch-only";
    let live = dead_session("ada@nas.local", "/srv/data", true);
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: Some("/srv/data/photos"),
    };

    let accepted = check(
        &place(id, &live, Some("/srv/data/photos")),
        &edit,
        |_: &str, _: &Path| panic!("nothing moved, so no successor is built"),
        NO_TIME,
    )
    .await
    .unwrap_or_else(|refusal| panic!("nothing needed confirming, refused with {refusal:?}"));
    accepted.install(&manager);

    assert!(Arc::ptr_eq(&manager.get(id).expect("still registered"), &live));
    assert!(volume_broadcast::volume_root_changes(id).is_empty());
}

/// A new start folder moves the landing and keeps the instance: the root is
/// the same, so the registry has nothing to swap.
#[tokio::test]
async fn a_start_folder_alone_moves_the_landing_and_keeps_the_instance() {
    let id = "live-edit-start-folder";
    let live = server_at("ada@nas.local", "/srv/data");
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: Some("/srv/data/photos"),
    };

    let accepted = check(&place(id, &live, None), &edit, over_the_same_server, BUDGET)
        .await
        .unwrap_or_else(|refusal| panic!("the server has photos/, refused with {refusal:?}"));
    accepted.install(&manager);

    assert!(Arc::ptr_eq(&manager.get(id).expect("still registered"), &live));
    assert_eq!(
        volume_broadcast::volume_root_changes(id),
        vec![VolumeRootChanged {
            volume_id: id.to_string(),
            old_root: app("/srv/data"),
            new_root: app("/srv/data"),
            old_landing: app("/srv/data"),
            new_landing: app("/srv/data/photos"),
        }]
    );
}

/// The old landing is the saved start folder, and clearing it lands the place
/// back on its root.
#[tokio::test]
async fn clearing_the_start_folder_lands_on_the_root() {
    let id = "live-edit-clear-start-folder";
    let live = server_at("ada@nas.local", "/srv/data");
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let accepted = check(
        &place(id, &live, Some("/srv/data/photos")),
        &edit,
        over_the_same_server,
        BUDGET,
    )
    .await
    .unwrap_or_else(|refusal| panic!("clearing asks nothing of the server, refused with {refusal:?}"));
    accepted.install(&manager);

    let changes = volume_broadcast::volume_root_changes(id);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].old_landing, app("/srv/data/photos"));
    assert_eq!(changes[0].new_landing, app("/srv/data"));
}

/// A saved start folder the LIVE root doesn't hold (the store and the session
/// drifted apart) can't have been where the place landed, so the old landing is
/// the old root.
#[tokio::test]
async fn a_saved_start_folder_outside_the_live_root_isnt_the_old_landing() {
    let id = "live-edit-drifted-start-folder";
    let live = server_at("ada@nas.local", "/srv/data/tmp");
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let accepted = check(
        &place(id, &live, Some("/srv/data/photos")),
        &edit,
        over_the_same_server,
        BUDGET,
    )
    .await
    .unwrap_or_else(|refusal| panic!("the server has /srv/data, refused with {refusal:?}"));
    accepted.install(&manager);

    let changes = volume_broadcast::volume_root_changes(id);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].old_landing, app("/srv/data/tmp"));
}

/// A place that disconnected between the check and the install has no live
/// root to move: the store already holds the edit, and nothing is announced.
#[tokio::test]
async fn a_place_that_disconnected_before_the_install_announces_nothing() {
    let id = "live-edit-gone-before-install";
    let live = server_at("ada@nas.local", "/srv/data/tmp");
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let accepted = check(&place(id, &live, None), &edit, over_the_same_server, BUDGET)
        .await
        .unwrap_or_else(|refusal| panic!("the server has /srv/data, refused with {refusal:?}"));
    manager.unregister(id);
    accepted.install(&manager);

    assert!(manager.get(id).is_none(), "an install never registers a place");
    assert!(volume_broadcast::volume_root_changes(id).is_empty());
}

// ── Refused, and nothing moves ───────────────────────────────────────

/// ❗ The live session is asked BEFORE anything moves: a root the server
/// doesn't have is refused, and the registry and the panes keep what they had.
#[tokio::test]
async fn a_root_the_server_doesnt_have_is_refused_and_nothing_moves() {
    let id = "live-edit-missing-root";
    let live = server_at("ada@nas.local", "/srv/data/tmp");
    let manager = registered(id, &live);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/elsewhere",
        start_folder: None,
    };

    let outcome = check(&place(id, &live, None), &edit, over_the_same_server, BUDGET).await;

    assert_eq!(refusal(outcome), Some(SavedServerOutcome::RootNotFound));
    assert!(Arc::ptr_eq(&manager.get(id).expect("still registered"), &live));
    assert!(volume_broadcast::volume_root_changes(id).is_empty());
}

/// A root that names a file is no root.
#[tokio::test]
async fn a_root_that_is_a_file_is_refused() {
    let id = "live-edit-file-root";
    let live = server_at("ada@nas.local", "/srv/data");
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data/notes.txt",
        start_folder: None,
    };

    let outcome = check(&place(id, &live, None), &edit, over_the_same_server, BUDGET).await;

    assert_eq!(refusal(outcome), Some(SavedServerOutcome::RootNotFound));
}

#[tokio::test]
async fn a_start_folder_the_server_doesnt_have_is_refused() {
    let id = "live-edit-missing-start-folder";
    let live = server_at("ada@nas.local", "/srv/data");
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: Some("/srv/data/gone"),
    };

    let outcome = check(&place(id, &live, None), &edit, over_the_same_server, BUDGET).await;

    assert_eq!(refusal(outcome), Some(SavedServerOutcome::StartFolderNotFound));
}

/// A dropped session can confirm nothing, so the edit is refused as
/// `Unreachable` rather than saved unchecked.
#[tokio::test]
async fn a_session_that_dropped_confirms_nothing() {
    let id = "live-edit-dropped-session";
    let live = dead_session("ada@nas.local", "/srv/data/tmp", false);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let outcome = check(
        &place(id, &live, None),
        &edit,
        |name: &str, root: &Path| dead_session(name, &root.to_string_lossy(), false),
        BUDGET,
    )
    .await;

    assert_eq!(refusal(outcome), Some(SavedServerOutcome::Unreachable));
}

/// A session that never answers runs out of the budget instead of holding the
/// Save button forever.
#[tokio::test]
async fn a_session_that_never_answers_runs_out_of_budget() {
    let id = "live-edit-silent-session";
    let live = dead_session("ada@nas.local", "/srv/data/tmp", true);
    let edit = PlaceEdit {
        label: "ada@nas.local",
        remote_root: "/srv/data",
        start_folder: None,
    };

    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        check(
            &place(id, &live, None),
            &edit,
            |name: &str, root: &Path| dead_session(name, &root.to_string_lossy(), true),
            NO_TIME,
        ),
    )
    .await
    .expect("the check honors its budget");

    assert_eq!(refusal(outcome), Some(SavedServerOutcome::Unreachable));
}
