//! A search placed on a volume that isn't connected answers from that volume's
//! index on disk, at the root its last walk recorded.
//!
//! The case a path can't express: a saved share's last root (`/Volumes/testshare`)
//! routes to the boot disk once nothing is mounted there, so only the caller's
//! named volume reaches its index. The walk has nothing to read, so the answer
//! is the index's rows and a lower bound.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_index::Index;
use cmdr_index::store::{IndexStore, ROOT_ID};
use cmdr_index::testing::host::test_lock;

use super::live_drive::pattern_query_for;
use super::*;
use crate::file_system::index_provider::AppVolumeProvider;
use crate::search::live::{self, AnswerEnding, WalkEnding};

const ANSWER_BUDGET: Duration = Duration::from_secs(6);

/// `index-{volume_id}.db` holding `/documents/invoices/hetzner.pdf`, last walked
/// at `volume_path`.
fn a_stopped_share_index(data: &Path, volume_id: &str, volume_path: &str) {
    let db_path = data.join(format!("index-{volume_id}.db"));
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "volume_path", volume_path).expect("meta");
    let papers = IndexStore::insert_entry_v2(&conn, ROOT_ID, "documents", true, false, None, None, None, None).unwrap();
    let invoices = IndexStore::insert_entry_v2(&conn, papers, "invoices", true, false, None, None, None, None).unwrap();
    IndexStore::insert_entry_v2(
        &conn,
        invoices,
        "hetzner.pdf",
        false,
        false,
        Some(10),
        Some(10),
        Some(1_700_000_000),
        None,
    )
    .unwrap();
}

#[test]
fn a_search_on_a_share_that_isnt_connected_answers_from_its_index_on_disk() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (_index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::new(AppVolumeProvider))
        .install_for_test();
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let volume_id = "smb-offline-testshare-test";
    let root = "/Volumes/cmdr-offline-testshare";
    a_stopped_share_index(data.path(), volume_id, root);

    let query = pattern_query_for(root, "hetzner");
    let answer = run_live_collected(query, Some(volume_id), ANSWER_BUDGET).expect("the search runs");
    volumes::forget_volume_for_test(volume_id);

    assert_eq!(answer.target_volume_id, volume_id, "the named volume's index answers");
    let paths: Vec<&str> = answer.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [format!("{root}/documents/invoices/hetzner.pdf")],
        "rows are app paths under the last root"
    );
    let AnswerEnding::Settled(coverage) = &answer.ending else {
        panic!("a run with nothing to walk settles at once");
    };
    assert_eq!(
        coverage.walk,
        WalkEnding::Interrupted,
        "nothing walked the unmounted share, so the list is a lower bound"
    );
}
