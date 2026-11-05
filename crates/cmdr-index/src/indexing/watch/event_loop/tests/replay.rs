//! The cold-start replay's own exits: whatever way it ends, it leaves the writer
//! committing.

use std::sync::Arc;

use super::*;
use crate::RescanReason;
use crate::indexing::hold::VolumeWork;
use crate::indexing::lifecycle::cover;
use crate::indexing::store::ROOT_ID;
use crate::indexing::watch::event_loop::ReplayChunks;
use crate::indexing::watch::event_loop::replay_cost::ReplayBudget;
use crate::indexing::writer::{IndexWriter, WriteMessage};

const SINCE: u64 = 4_000_000_000;

/// The incident (2026-11-03): a replay that ran over its budget returned before its
/// `CommitTransaction`, so the writer stayed inside its `BEGIN IMMEDIATE` for the
/// rest of the process. The fallback rescan and the live loop then wrote into a
/// transaction no reader could see: about 1,000 "root still not in DB after upsert"
/// warnings a minute, sizes and search frozen for days, and a stored replay position
/// that never advanced, so every launch replayed the same span and leaked again.
#[tokio::test(flavor = "multi_thread")]
async fn a_replay_that_falls_back_over_budget_leaves_the_writer_committing() {
    let volume_id = "replay-over-budget";
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("replay.db");
    IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    let (watcher, watched) = tokio::sync::mpsc::unbounded_channel();
    let (fallback_tx, fallback_rx) = tokio::sync::oneshot::channel();
    let work = VolumeWork::for_test(volume_id);

    let replay = crate::indexing::host::runtime::spawn(run_replay_event_loop(
        watched,
        writer.clone(),
        crate::NoopEventSink::shared(),
        ReplayConfig {
            volume_id: volume_id.to_string(),
            space: IndexPathSpace::root(),
            since_event_id: SINCE,
            estimated_total: None,
            heal_after_replay: false,
            work: work.clone(),
            ground: cover::Claim::take(volume_id, vec!["/".to_string()], cover::Holder::Rewriting),
            // Nothing to spend: the first delivered event already costs more.
            budget: ReplayBudget::with_limit(0),
            chunks: ReplayChunks::SHIPPING,
        },
        fallback_tx,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(true)),
    ));
    let created = dir.path().join("created.txt");
    watcher
        .send(make_event(
            &created.to_string_lossy(),
            SINCE + 1,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        ))
        .expect("send");
    replay
        .await
        .expect("the replay task ends")
        .expect("an over-budget replay is an ordinary exit");
    assert_eq!(
        fallback_rx.await.expect("the replay asks for a fallback"),
        RescanReason::ReplayOverflow
    );

    // What the fallback rescan does next: write a row and flush.
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "written-after-the-fallback".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .expect("send upsert");
    writer.flush().await.expect("flush");

    let reader = IndexStore::open_read_connection(&db_path).expect("read connection");
    assert!(
        store::resolve_path(&reader, "/written-after-the-fallback")
            .expect("readable")
            .is_some(),
        "a row written after the fallback must be visible to readers, so no transaction is left open"
    );

    drop(watcher);
    writer.shutdown();
    drop(work);
}
