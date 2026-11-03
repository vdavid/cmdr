//! Undoing a Finder-tag write puts each file's prior tag value back, byte for byte, and
//! leaves alone a file whose tags changed since or that's gone
//! (`crates/cmdr-operation-log/DETAILS.md` § "A tag write restores the exact prior value").
//!
//! The planner treats the value as opaque bytes, so these cells use made-up blobs: the
//! codec lives in the app (`file_system/tags.rs`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::entry::{FINDER_TAGS_XATTR, TagsChange};
use cmdr_fs::volume::{InMemoryVolume, Volume, XattrReadMode};
use cmdr_operation_log::types::{EntryType, ItemOutcome, OpKind, RollbackState, RowRole, SkipReason};
use cmdr_operation_log::writer::JournalItem;

const BEFORE: &[u8] = b"Important\n0|Red\n6";
const AFTER: &[u8] = b"Blue\n4";

/// The row a tag write records for `path`: the file itself, no dest, and the value
/// before and after.
fn tagged(seq: i64, path: &str) -> JournalItem {
    let (dir, name) = split(path);
    JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "v".to_string(),
        source_dir: dir,
        source_name: name,
        dest_volume_id: None,
        dest_dir: None,
        dest_name: None,
        size: None,
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
        dest_identity: None,
        tags: Some(TagsChange {
            before: BEFORE.to_vec(),
            after: AFTER.to_vec(),
        }),
    }
}

async fn tags_on(vol: &InMemoryVolume, path: &str) -> Option<Vec<u8>> {
    vol.read_xattr_many(&[PathBuf::from(path)], FINDER_TAGS_XATTR, XattrReadMode::Verified)
        .await
        .remove(0)
        .expect("readable")
}

async fn set_tags(vol: &InMemoryVolume, path: &str, value: &[u8]) {
    vol.write_xattr(Path::new(path), FINDER_TAGS_XATTR, value)
        .await
        .expect("write the tags");
}

/// Two tagged files, both still carrying what the write left.
async fn tagged_pair(rig: &Rig) -> Arc<InMemoryVolume> {
    let v = Arc::new(InMemoryVolume::new("V"));
    for path in ["/a.txt", "/b.txt"] {
        put(&v, path, b"x").await;
        set_tags(&v, path, AFTER).await;
    }
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Tag,
        "v",
        None,
        RollbackState::Rollbackable,
        vec![tagged(0, "/a.txt"), tagged(1, "/b.txt")],
    );
    v
}

#[tokio::test]
async fn undoing_a_tag_write_restores_every_files_prior_tags_exactly() {
    let rig = Rig::new();
    let v = tagged_pair(&rig).await;

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (2, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(tags_on(&v, "/a.txt").await.as_deref(), Some(BEFORE));
    assert_eq!(tags_on(&v, "/b.txt").await.as_deref(), Some(BEFORE));
}

#[tokio::test]
async fn a_file_retagged_since_keeps_its_new_tags() {
    let rig = Rig::new();
    let v = tagged_pair(&rig).await;
    set_tags(&v, "/b.txt", b"Green\n2").await;

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 1), "{report:?}");
    assert_eq!(report.skips[0].reason, SkipReason::Drift);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(tags_on(&v, "/a.txt").await.as_deref(), Some(BEFORE));
    assert_eq!(
        tags_on(&v, "/b.txt").await.as_deref(),
        Some(&b"Green\n2"[..]),
        "left alone"
    );
}

#[tokio::test]
async fn a_file_gone_since_is_a_skip_never_a_restore() {
    let rig = Rig::new();
    let v = tagged_pair(&rig).await;
    v.delete(Path::new("/b.txt")).await.expect("delete");

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 1), "{report:?}");
    assert_eq!(report.skips[0].reason, SkipReason::Vanished);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
}

#[tokio::test]
async fn a_file_already_back_to_its_prior_tags_counts_as_reversed() {
    let rig = Rig::new();
    let v = tagged_pair(&rig).await;
    set_tags(&v, "/a.txt", BEFORE).await;

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (2, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
}

#[tokio::test]
async fn undoing_the_undo_puts_the_written_tags_back() {
    let rig = Rig::new();
    let v = tagged_pair(&rig).await;
    rig.rollback("op").await;

    let inverse = rig.read_op("inv-1");
    assert_eq!(inverse.kind, OpKind::Tag);
    assert_eq!(
        inverse.rollback_state,
        RollbackState::Rollbackable,
        "a tag undo can be undone"
    );

    let redo = rig.rollback_as("inv-1", "inv-2").await;

    assert_eq!((redo.reversed, redo.skipped), (2, 0), "{redo:?}");
    assert_eq!(tags_on(&v, "/a.txt").await.as_deref(), Some(AFTER));
    assert_eq!(tags_on(&v, "/b.txt").await.as_deref(), Some(AFTER));
}

#[tokio::test]
async fn a_tag_row_without_its_values_is_never_acted_on() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, "/a.txt", b"x").await;
    set_tags(&v, "/a.txt", AFTER).await;
    rig.register("v", v.clone());
    let row = JournalItem {
        tags: None,
        ..tagged(0, "/a.txt")
    };
    rig.seed("op", OpKind::Tag, "v", None, RollbackState::Rollbackable, vec![row]);

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "{report:?}");
    assert_eq!(tags_on(&v, "/a.txt").await.as_deref(), Some(AFTER), "untouched");
}
