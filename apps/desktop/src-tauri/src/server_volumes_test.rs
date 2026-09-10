//! Which server a remote path belongs to, and the one thing asking must never
//! do: connect.

use std::time::Duration;

use super::*;
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::webdav_known_servers::KnownWebdavServer;
use cmdr_fs::volume::BackendKind;

/// A host nobody else's cell will use, so this suite can share the
/// process-global store with whatever runs beside it.
///
/// ❗ **This suite owns `198.51.100.x` and `commands/servers_test.rs` owns
/// `192.0.2.x`, one reserved block each.** Both write the same process-global
/// `KNOWN` store, so under a thread-per-test runner (`cargo test --lib`, which
/// nextest's process-per-test hides) a shared address is one cell removing the
/// entry another is about to look up, or writing it with the other pin. Picking
/// a free last octet inside your OWN block is what keeps that impossible; a
/// number that merely looks unused across both files is how they collided.
///
/// ❗ Both blocks are reserved for documentation (RFC 5737) and routed nowhere,
/// so a dial against one hangs rather than reaching a stranger's machine. That
/// hang is what makes "the resolver never connects" assertable, which is why
/// these are literal addresses rather than `.test` names that fail DNS instantly.
fn saved_sftp(host: &str, pinned: bool) -> KnownSftpServer {
    KnownSftpServer {
        host: host.to_string(),
        port: 2222,
        username: "ada".to_string(),
        display_name: format!("{host} server"),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
        pinned,
        last_connected_at: "2026-09-06T00:00:00Z".to_string(),
    }
}

#[test]
fn a_pinned_saved_server_answers_for_its_own_paths_without_connecting() {
    let host = "198.51.100.11";
    sftp_known_servers::remember(saved_sftp(host, true));

    let started = std::time::Instant::now();
    let volume = server_volume_for_path(&format!("sftp://ada@{host}:2222/srv/data/photos"))
        .expect("a saved server answers for a path under its own root");

    assert_eq!(volume.id, cmdr_fs::volume::sftp_volume_id(host, 2222, "ada"));
    assert_eq!(volume.path, format!("sftp://ada@{host}:2222/srv/data"));
    assert_eq!(volume.fs_type.as_deref(), Some("sftp"));
    assert_eq!(volume.category, LocationCategory::Network);
    assert_eq!(
        volume.connection_state,
        Some(ConnectionState::Saved),
        "nothing is registered under that id, so the row is the greyed one"
    );
    assert!(!volume.is_ejectable, "a server has nothing to unplug");
    assert!(!volume.supports_trash);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "❗ resolving must never dial: this address is routed nowhere, so a connect would hang"
    );
}

/// ❗ Pins govern the SWITCHER, not identity. A tab restored onto an unpinned
/// server still has to find its way home.
#[test]
fn an_unpinned_saved_server_still_answers_for_its_paths() {
    let host = "198.51.100.12";
    sftp_known_servers::remember(saved_sftp(host, false));

    let volume = server_volume_for_path(&format!("sftp://ada@{host}:2222/srv/data"))
        .expect("an unpinned server is still where its paths live");

    assert_eq!(volume.id, cmdr_fs::volume::sftp_volume_id(host, 2222, "ada"));
    assert_eq!(volume.connection_state, Some(ConnectionState::Saved));
}

#[test]
fn a_saved_webdav_server_answers_for_its_own_paths() {
    let host = "198.51.100.13";
    webdav_known_servers::remember(KnownWebdavServer {
        url: format!("http://{host}:8080/dav/"),
        username: "ada".to_string(),
        display_name: "Docs".to_string(),
        remote_root: "/Photos".to_string(),
        start_folder: None,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-06T00:00:00Z".to_string(),
    });

    let volume = server_volume_for_path(&format!("webdav://ada@{host}:8080/Photos/2026"))
        .expect("a saved WebDAV server answers for a path under its own root");

    assert_eq!(volume.id, cmdr_fs::volume::webdav_volume_id(host, 8080, "ada"));
    assert_eq!(volume.path, format!("webdav://ada@{host}:8080/Photos"));
    assert_eq!(volume.fs_type.as_deref(), Some("webdav"));
}

/// A prefix nothing saved matches is `None`, ❌ never the boot disk: that is the
/// whole reason a remote path carries a scheme.
#[test]
fn an_unknown_prefix_is_nobodys_volume() {
    assert!(server_volume_for_path("sftp://nobody@198.51.100.99:22/srv/data").is_none());
    assert!(server_volume_for_path("webdav://nobody@198.51.100.99:80/dav").is_none());
}

/// The trap a raw string prefix compare falls into: a sibling root whose name
/// merely starts with a saved one's.
#[test]
fn a_sibling_root_is_not_this_servers_path() {
    let host = "198.51.100.14";
    sftp_known_servers::remember(saved_sftp(host, true));

    assert!(
        server_volume_for_path(&format!("sftp://ada@{host}:2222/srv/data-1/photos")).is_none(),
        "`/srv/data-1` is a different tree, however much of `/srv/data` it spells"
    );
}

/// ❗ Asking whether a path exists on a SAVED server nobody connected answers
/// "couldn't tell", ❌ never a confident `false`: nothing has looked, and every
/// caller reads `false` as "deleted" and walks the pane off the server.
#[tokio::test]
async fn path_exists_on_a_saved_server_nobody_connected_answers_that_it_couldnt_tell() {
    let host = "198.51.100.48";
    sftp_known_servers::remember(saved_sftp(host, true));

    let answer = crate::commands::file_system::path_exists(
        Some(cmdr_fs::volume::sftp_volume_id(host, 2222, "ada")),
        format!("sftp://ada@{host}:2222/srv/data/photos"),
    )
    .await;

    assert!(
        answer.timed_out && !answer.data,
        "a server that isn't connected can't say either way; got {answer:?}"
    );
}

/// ❗ Listing a SAVED server nobody connected answers `NotConnected`, ❌ never the
/// `NotFound` an unknown id gets: the user would read "Path not found" for a folder
/// that's fine, and the frontend reads `NotFound` as "this folder was deleted".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_a_saved_server_nobody_connected_says_it_is_not_connected() {
    use crate::file_system::listing::caching_test_support::{TestListingGuard, unique_test_id};
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::listing::streaming::{
        CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
    };

    let host = "198.51.100.49";
    sftp_known_servers::remember(saved_sftp(host, true));

    let listing = TestListingGuard::adopt(unique_test_id("saved-server-listing"));
    let events: std::sync::Arc<dyn ListingEventSink> = std::sync::Arc::new(CollectorListingEventSink::new());
    let state = std::sync::Arc::new(StreamingListingState {
        cancel: tokio_util::sync::CancellationToken::new(),
    });
    let pane_path = format!("sftp://ada@{host}:2222/srv/data/photos");
    let outcome = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        &cmdr_fs::volume::sftp_volume_id(host, 2222, "ada"),
        std::path::Path::new(&pane_path),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(
        matches!(outcome, Err(cmdr_fs::volume::VolumeError::NotConnected(_))),
        "a saved server nobody connected isn't connected yet, and nothing was deleted; got {outcome:?}"
    );
}

/// ❗ A copy onto a SAVED server nobody connected is refused as not connected yet,
/// ❌ never the untyped "Destination volume not found": a transfer doesn't dial,
/// and the way through is opening the server, which that wording hides.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_onto_a_saved_server_nobody_connected_is_refused_as_not_connected() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::write_operations::CollectorEventSink;
    use crate::file_system::{VolumeCopyConfig, WriteOperationError, start_volume_copy};
    use crate::operation_log::types::Initiator;

    let host = "198.51.100.50";
    sftp_known_servers::remember(saved_sftp(host, true));
    let dir = crate::test_support::TestDir::new("copy_onto_saved_server");
    std::fs::write(dir.join("notes.txt"), b"notes").expect("seeding the local file");
    let source_id = format!("saved-server-copy-source-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(&source_id, std::sync::Arc::new(LocalPosixVolume::new("Local", &*dir)));

    let refused = start_volume_copy(
        std::sync::Arc::new(CollectorEventSink::new()),
        source_id.clone(),
        vec![std::path::PathBuf::from("notes.txt")],
        cmdr_fs::volume::sftp_volume_id(host, 2222, "ada"),
        format!("sftp://ada@{host}:2222/srv/data/photos"),
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .expect_err("a copy onto a saved server nobody connected is refused");
    get_volume_manager().unregister(&source_id);

    assert!(
        matches!(refused, WriteOperationError::DestinationNotConnected { .. }),
        "the saved server isn't connected yet; got {refused:?}"
    );
}

// ── The rows the switcher gets ───────────────────────────────────────

/// ❗ **Every saved server gets a row, and the row carries its own pin.** The
/// list is the app's registry of what a volume id MEANS: a hub Enter and a
/// restored tab both land on an id, and an id with no row is a volume the app
/// denies exists. The pin rides along as a field so the SWITCHER can apply the
/// cap (`navigation/volume-grouping.ts`) without the list hiding identities from
/// everything else.
#[test]
fn every_saved_server_gets_a_row_carrying_its_own_pin() {
    let pinned_host = "198.51.100.41";
    let unpinned_host = "198.51.100.42";
    sftp_known_servers::remember(saved_sftp(pinned_host, true));
    sftp_known_servers::remember(saved_sftp(unpinned_host, false));

    let mut rows = Vec::new();
    append_server_volumes(&mut rows);

    let pinned_id = cmdr_fs::volume::sftp_volume_id(pinned_host, 2222, "ada");
    let row = rows
        .iter()
        .find(|row| row.id == pinned_id)
        .expect("a pinned server has a row even with no session behind it");
    assert_eq!(row.category, LocationCategory::Network);
    assert_eq!(row.fs_type.as_deref(), Some("sftp"));
    assert_eq!(row.name, format!("{pinned_host} server"));
    assert_eq!(row.path, format!("sftp://ada@{pinned_host}:2222/srv/data"));
    assert_eq!(row.connection_state, Some(ConnectionState::Saved));
    assert_eq!(row.pinned, Some(true));
    assert!(
        !row.is_ejectable,
        "a server has nothing to unplug; its control says Disconnect"
    );
    assert!(!row.supports_trash);
    assert!(
        row.capabilities.is_none(),
        "❗ enrichment fills this from the registered volume afterwards, which is why the arm folds BEFORE it"
    );

    let unpinned_id = cmdr_fs::volume::sftp_volume_id(unpinned_host, 2222, "ada");
    let unpinned = rows
        .iter()
        .find(|row| row.id == unpinned_id)
        .expect("an unpinned server is still an id something can navigate to");
    assert_eq!(
        unpinned.pinned,
        Some(false),
        "the switcher is what hides it, and this field is how the switcher knows"
    );
    assert_eq!(unpinned.connection_state, Some(ConnectionState::Saved));
}

/// A row nothing saved carries `pinned: Some(false)`: it was never subject to
/// the cap, and it earns its switcher row through its live session instead.
#[test]
fn a_row_for_a_live_session_nothing_saved_reports_no_pin() {
    let volume_id = cmdr_fs::volume::sftp_volume_id("198.51.100.47", 2222, "ada");
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(
        &volume_id,
        std::sync::Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("Forgotten but live")
                .with_backend_kind(BackendKind::Sftp)
                .with_connection_state(ConnectionState::Direct),
        ),
    );

    let mut rows = Vec::new();
    append_server_volumes(&mut rows);
    let row = rows
        .iter()
        .find(|row| row.id == volume_id)
        .expect("a live volume has a row");
    assert_eq!(row.pinned, Some(false));

    manager.unregister(&volume_id);
}

/// A pinned WebDAV server gets the same row, with its own `fs_type`.
///
/// ❗ The `fs_type` is load-bearing beyond display: the frontend's `volumeKindFor`
/// reads it AHEAD of the `category === 'network'` arm, so a row without it is an
/// SMB pane, with SMB's capability row and Open terminal firing on a
/// `webdav://` path.
#[test]
fn a_pinned_webdav_server_gets_a_row_of_its_own_kind() {
    let host = "198.51.100.43";
    webdav_known_servers::remember(KnownWebdavServer {
        url: format!("http://{host}:8080/dav/"),
        username: "ada".to_string(),
        display_name: "Docs".to_string(),
        remote_root: "/Photos".to_string(),
        start_folder: None,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-06T00:00:00Z".to_string(),
    });

    let mut rows = Vec::new();
    append_server_volumes(&mut rows);

    let row = rows
        .iter()
        .find(|row| row.id == cmdr_fs::volume::webdav_volume_id(host, 8080, "ada"))
        .expect("a pinned WebDAV server has a row");
    assert_eq!(row.fs_type.as_deref(), Some("webdav"));
    assert_eq!(row.path, format!("webdav://ada@{host}:8080/Photos"));
}

/// ❗ **A registered volume gets a row whether or not it is pinned**, and it is
/// the same id the `saved` row carried, so a tab survives the dial.
#[test]
fn a_registered_volume_gets_a_row_under_the_id_its_saved_row_carried() {
    let host = "198.51.100.44";
    sftp_known_servers::remember(saved_sftp(host, false));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");

    let mut before = Vec::new();
    append_server_volumes(&mut before);
    let saved_row = before
        .iter()
        .find(|row| row.id == volume_id)
        .expect("unpinned and unconnected is still a place, so still a row");
    assert_eq!(saved_row.connection_state, Some(ConnectionState::Saved));

    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(
        &volume_id,
        std::sync::Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("stand-in")
                .with_backend_kind(BackendKind::Sftp)
                .with_connection_state(ConnectionState::Direct),
        ),
    );

    let mut after = Vec::new();
    append_server_volumes(&mut after);
    let row = after
        .iter()
        .find(|row| row.id == volume_id)
        .expect("a live session always has a row, pinned or not");
    assert_eq!(row.connection_state, Some(ConnectionState::Direct));

    manager.unregister(&volume_id);
}

/// ❗ **The arm folds BEFORE enrichment**, so a live server reaches the frontend
/// with the capability surface its backend actually publishes rather than with
/// the pane's per-kind defaults. This is the cell that pins the ORDER, which is
/// the whole reason `volume_listing::complete` exists.
#[tokio::test]
async fn a_live_servers_row_comes_out_of_the_pipeline_enriched() {
    let host = "198.51.100.45";
    sftp_known_servers::remember(saved_sftp(host, true));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");

    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(
        &volume_id,
        std::sync::Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("stand-in")
                .with_backend_kind(BackendKind::Sftp)
                .with_connection_state(ConnectionState::Direct),
        ),
    );

    let volumes = crate::volume_listing::complete(Vec::new()).await;
    let row = volumes
        .iter()
        .find(|row| row.id == volume_id)
        .expect("a live server is in the published list");

    assert!(
        row.capabilities.is_some(),
        "❗ appended after enrichment, a server ships `capabilities: None` and the pane guesses"
    );
    assert_eq!(row.connection_state, Some(ConnectionState::Direct));

    manager.unregister(&volume_id);
}

/// ❗ **A live session is never a ghost.** `forget_server` drops the saved entry
/// without dropping the session, so a volume the stores no longer know about
/// still gets its row: one a pane can sit on while the switcher denies it exists
/// is worse than one the user can disconnect.
#[test]
fn a_registered_volume_nothing_saved_still_gets_a_row() {
    let volume_id = cmdr_fs::volume::sftp_volume_id("198.51.100.46", 2222, "ada");
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(
        &volume_id,
        std::sync::Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("Forgotten but live")
                .with_backend_kind(BackendKind::Sftp)
                .with_connection_state(ConnectionState::Direct)
                .with_root("sftp://ada@198.51.100.46:2222/srv/data"),
        ),
    );

    let mut rows = Vec::new();
    append_server_volumes(&mut rows);
    let row = rows
        .iter()
        .find(|row| row.id == volume_id)
        .expect("a live volume has a row whether or not anything saved it");

    assert_eq!(
        row.name, "Forgotten but live",
        "the volume's own name, since no entry has one"
    );
    assert_eq!(row.path, "sftp://ada@198.51.100.46:2222/srv/data");
    assert_eq!(row.fs_type.as_deref(), Some("sftp"));

    manager.unregister(&volume_id);
}
