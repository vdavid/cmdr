use super::*;
use cmdr_operation_log::store::{open_read_connection, read_operation_items, recent_operations};
use cmdr_operation_log::types::RollbackState;
use cmdr_operations::testing::TestOperationLog;

use crate::file_system::tags::TAGS_XATTR;
use crate::file_system::tags_io::{apply_merge, clear_tags, read_for_merge};
use cmdr_fs::entry::TagRef;

fn tag(name: &str, color: u8) -> TagRef {
    TagRef {
        name: name.to_string(),
        color,
    }
}

#[test]
fn a_file_written_twice_journals_its_first_before_and_last_after() {
    let mut writes = TagWrites::default();
    writes.record("v", "/a", Some(b"one"), b"two".to_vec());
    writes.record("v", "/a", Some(b"two"), b"three".to_vec());
    let rows = writes.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].tags,
        Some(TagsChange {
            before: b"one".to_vec(),
            after: b"three".to_vec()
        })
    );
}

#[test]
fn a_file_back_where_it_started_journals_nothing() {
    let mut writes = TagWrites::default();
    writes.record("v", "/a", Some(b"one"), b"two".to_vec());
    writes.record("v", "/a", Some(b"two"), b"one".to_vec());
    assert!(writes.rows().is_empty());
}

#[test]
fn an_untagged_file_journals_the_empty_array_as_its_before() {
    let mut writes = TagWrites::default();
    writes.record("v", "/a", None, b"red".to_vec());
    let rows = writes.rows();
    assert_eq!(
        rows[0].tags.as_ref().map(|tags| tags.before.clone()),
        Some(no_tags_value()),
        "an undo writes the empty array back, never removes the attribute"
    );
}

/// A real `set` and `clear` on a local disk journal each file's exact prior bytes,
/// colorless tags and an undecodable value included, under the initiator the caller
/// named, as a rollbackable `tag` operation.
#[tokio::test]
async fn a_tag_write_journals_each_files_exact_prior_value() {
    const DISK: &str = "tags-journal-test-disk";
    let disk = cmdr_local::LocalPosixVolume::local_folder("Disk", "/", cmdr_fs::volume::host::VolumeHost::detached());
    crate::file_system::volume::manager::get_volume_manager().register(DISK, std::sync::Arc::new(disk));
    let dir = crate::test_support::TestDir::new(&format!("tags_journal_{}", std::process::id()));
    let custom = dir.join("custom.txt");
    let garbled = dir.join("garbled.txt");
    std::fs::write(&custom, b"x").unwrap();
    std::fs::write(&garbled, b"x").unwrap();
    let custom_before = encode_tags_plist(&[tag("Important", 0), tag("Red", 6)]).unwrap();
    xattr::set(&custom, TAGS_XATTR, &custom_before).unwrap();
    xattr::set(&garbled, TAGS_XATTR, b"garbage").unwrap();
    let custom_path = custom.to_string_lossy().into_owned();
    let garbled_path = garbled.to_string_lossy().into_owned();

    let log = TestOperationLog::new("tags-journal");
    let mut writes = TagWrites::default();
    let (current, _skipped) = read_for_merge(Some(DISK), vec![custom_path.clone()]).await;
    apply_merge(current, |_| vec![tag("Blue", 4)], &mut writes)
        .await
        .unwrap();
    clear_tags(Some(DISK), vec![garbled_path.clone()], &mut writes)
        .await
        .unwrap();
    let op_id = writes.journal(log.log(), Initiator::AiClient, true).expect("journaled");

    let conn = open_read_connection(log.db()).expect("read conn");
    let op = recent_operations(&conn, 10)
        .unwrap()
        .into_iter()
        .find(|op| op.op_id == op_id)
        .expect("the tag op");
    assert_eq!(op.kind, OpKind::Tag);
    assert_eq!(op.initiator, Initiator::AiClient);
    assert_eq!(op.rollback_state, RollbackState::Rollbackable);
    assert_eq!(op.source_volume_id.as_deref(), Some(DISK));
    let items = read_operation_items(&conn, &op_id, 10).unwrap();
    assert_eq!(items.len(), 2);
    let custom_row = items[0].tags.as_ref().expect("tag values");
    assert_eq!(
        custom_row.before, custom_before,
        "colorless and colored tags, byte for byte"
    );
    assert_eq!(Some(custom_row.after.clone()), xattr::get(&custom, TAGS_XATTR).unwrap());
    let garbled_row = items[1].tags.as_ref().expect("tag values");
    assert_eq!(
        garbled_row.before, b"garbage",
        "a clear keeps even a value it can't decode"
    );
}
