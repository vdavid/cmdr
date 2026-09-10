//! `Index::cover` refuses a volume no drive index can serve.
//!
//! A cover walk stands an index up for whatever it walks, so without this door a
//! walk pointed at a server (SFTP, WebDAV) would build that server an index DB,
//! which `start_volume` refuses to do for the same volume. The search layer gates
//! its own walks too; this holds whatever the caller.

use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{BackendKind, InMemoryVolume, sftp_app_root, sftp_volume_id};
use tokio_util::sync::CancellationToken;

use crate::indexing::handle::{Index, IndexError};
use crate::indexing::host::volumes::FakeVolumeProvider;
use crate::indexing::read::coverage::CoverageDimension;

#[test]
fn a_cover_walk_refuses_a_server_and_builds_no_index_for_it() {
    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let root = format!("{}/srv/data", sftp_app_root("nas.local", 22, "ada"));
    let volume_id = sftp_volume_id("nas.local", 22, "ada");
    let entries = vec![FileEntry::new(
        "report.txt".into(),
        format!("{root}/report.txt"),
        false,
        false,
    )];
    let volumes = FakeVolumeProvider::shared();
    volumes.register(
        &volume_id,
        Arc::new(
            InMemoryVolume::with_entries("NAS", entries)
                .with_root(&root)
                .with_backend_kind(BackendKind::Sftp),
        ),
    );
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .install_for_test();

    let database = data.path().join(format!("index-{volume_id}.db"));
    match index.cover(
        &volume_id,
        vec![root.clone()],
        CoverageDimension::Listing,
        CancellationToken::new(),
    ) {
        Ok(walk) => {
            while walk.next_batch().is_some() {}
            let _ = walk.finish();
            panic!(
                "a server was walked, and its index database exists: {}",
                database.exists()
            );
        }
        Err(error) => assert!(
            matches!(error, IndexError::NotIndexable { .. }),
            "the refusal is typed: {error:?}"
        ),
    }
    assert!(!database.exists(), "no index database is built for a server");
}
