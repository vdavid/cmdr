//! A search from a server pane (SFTP or WebDAV) answers for THAT server, and
//! honestly.
//!
//! No drive index can serve a server (`BackendKind::can_be_indexed`), so the
//! answer is "not covered", on both paths a search takes. It must never fall
//! through to the Mac's boot disk, which is what a server's scheme path did
//! before routing knew it. And a live run must not walk it either: a walk stands
//! an index up for the volume it walks, and that is building server indexing by
//! the back door.

use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{BackendKind, InMemoryVolume, sftp_app_root, sftp_volume_id};
use cmdr_index::Index;
use cmdr_index::testing::host::test_lock;

use super::live_drive::{pattern_query_for, settled};
use super::*;
use crate::file_system::index_provider::AppVolumeProvider;
use crate::file_system::volume::manager::get_volume_manager;
use crate::search::live::{self, WalkEnding};

/// How long the live cell waits for its answer: a run that walks nothing
/// settles at once, so anything near this is the bug.
const ANSWER_BUDGET: std::time::Duration = std::time::Duration::from_secs(6);

/// An SFTP account registered the way a connected server is, with one file on
/// it. Unregistered on drop.
struct Server {
    volume_id: String,
    /// `sftp://ada@nas.local:22/srv/data`, the spelling a server pane holds.
    root: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.volume_id);
    }
}

fn a_connected_sftp_server(username: &str) -> Server {
    let volume_id = sftp_volume_id("nas.local", 22, username);
    let root = format!("{}/srv/data", sftp_app_root("nas.local", 22, username));
    let entries = vec![FileEntry::new(
        "report.txt".into(),
        format!("{root}/report.txt"),
        false,
        false,
    )];
    get_volume_manager().register(
        &volume_id,
        Arc::new(
            InMemoryVolume::with_entries("NAS", entries)
                .with_root(&root)
                .with_backend_kind(BackendKind::Sftp),
        ),
    );
    Server { volume_id, root }
}

#[test]
fn a_server_scope_routes_to_the_server_never_the_boot_disk() {
    let root = format!("{}/srv/data", sftp_app_root("nas.local", 22, "ada"));
    let target = resolve_target(&pattern_query_for(&root, "report")).expect("one volume");
    assert_eq!(target.volume_id, sftp_volume_id("nas.local", 22, "ada"));
    assert!(target.from_scope);
}

/// The index-only answer (the dialog's auto-apply) for a server pane is the
/// honest gap, naming the server.
#[test]
fn the_index_only_answer_for_a_server_is_not_covered() {
    let _serialized = test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (_index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::new(AppVolumeProvider))
        .install_for_test();
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let server = a_connected_sftp_server("ada-index-only");

    let result = run_blocking(pattern_query_for(&server.root, "report")).expect("the search");

    assert_eq!(result.target_volume_id, server.volume_id);
    assert!(result.entries.is_empty(), "nothing from the Mac, and nothing invented");
    assert_eq!(result.uncovered_scopes, vec![server.root.clone()]);
}

/// The live run (Enter, and every MCP search) over a server walks nothing,
/// leaves no index behind, and says the scope isn't covered. It doesn't read as
/// an interrupted walk: nothing stopped, there was nothing it could walk.
#[test]
fn a_live_search_on_a_server_walks_nothing_and_says_it_is_not_covered() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (_index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::new(AppVolumeProvider))
        .install_for_test();
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let server = a_connected_sftp_server("ada-live");

    let answer = run_live_collected(pattern_query_for(&server.root, "report"), ANSWER_BUDGET).expect("the live search");

    assert_eq!(answer.target_volume_id, server.volume_id);
    assert!(
        answer.entries.is_empty(),
        "a server is never walked: {:?}",
        answer.entries
    );
    let coverage = settled(&answer.ending);
    assert_eq!(coverage.walk, WalkEnding::NothingToWalk, "nothing stopped short");
    assert_eq!(coverage.uncovered_scopes, vec![server.root.clone()]);
    assert!(
        !data.path().join(format!("index-{}.db", server.volume_id)).exists(),
        "no walk stood an index up for the server"
    );
}
