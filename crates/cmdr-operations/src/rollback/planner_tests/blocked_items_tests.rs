//! Undoing a copy that carried what refuses an `unlink` onto what it landed: a
//! Locked file, a `deny delete` ACL, a Locked `0500` folder. Each item is
//! verified against its snapshot first, then the rollback clears it, or opens the
//! folder holding it, for the one removal (`crates/cmdr-operations/src/rollback.rs`,
//! `delete_clearing_blockers`). macOS only: Linux has no flags to carry and no
//! `deny` ACL entries.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::testing::metadata_fixture::{LOCKED_FLAG, MetadataFixture, add_deny_delete_acl, set_flags, set_mode};

use super::test_support::*;
use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::{OpKind, RollbackState};

/// `/` as a local volume, so the seeded rows can name the fixture's real paths.
fn register_root(rig: &Rig) {
    rig.vm.register(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::host::testing::pane_order_host(),
        )) as Arc<dyn Volume>,
    );
}

/// Writes the copy's bytes with the mtime its journal row snapshots.
fn write_copy(path: &Path) {
    std::fs::write(path, b"COPY").expect("write");
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(MT as i64, 0)).expect("pin mtime");
}

/// Locked and a `deny delete` ACL each refuse the `unlink`, even for the owner:
/// each copy is cleared, then removed.
#[tokio::test]
async fn a_copy_rollback_removes_a_locked_and_a_deny_delete_copy() {
    let rig = Rig::new();
    let fixture = MetadataFixture::new("rollback_blocked");
    let locked = fixture.join("locked.txt");
    let acl = fixture.join("acl.txt");
    write_copy(&locked);
    write_copy(&acl);
    set_flags(&locked, LOCKED_FLAG);
    add_deny_delete_acl(&acl);
    register_root(&rig);
    let unit = |seq: i64, copy: &Path| {
        let source = fixture.join("src").join(copy.file_name().expect("a name"));
        file_unit(
            seq,
            "root",
            &source.to_string_lossy(),
            "root",
            &copy.to_string_lossy(),
            4,
        )
    };
    rig.seed(
        "op",
        OpKind::Copy,
        "root",
        Some("root"),
        RollbackState::Rollbackable,
        vec![unit(0, &locked), unit(1, &acl)],
    );
    rig.put_originals("op").await;

    let report = rig.rollback("op").await;

    assert_eq!(report.reversed, 2, "both copies are undone: {report:?}");
    assert!(!locked.exists() && !acl.exists());
}

/// A Locked `0500` folder refuses its child's departure, so the folder is opened
/// for that one removal and closed again, then removed itself.
#[tokio::test]
async fn a_copy_rollback_removes_a_locked_read_only_folder_and_its_child() {
    let rig = Rig::new();
    let fixture = MetadataFixture::new("rollback_blocked_folder");
    let vault = fixture.join("vault");
    std::fs::create_dir(&vault).expect("mkdir");
    let copy = vault.join("a.txt");
    write_copy(&copy);
    set_mode(&vault, 0o500);
    set_flags(&vault, LOCKED_FLAG);
    register_root(&rig);
    let source = fixture.join("src/vault/a.txt");
    rig.seed(
        "op",
        OpKind::Copy,
        "root",
        Some("root"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "root", &source.to_string_lossy(), "root", &copy.to_string_lossy(), 4),
            dir_unit(1, "root", &vault.to_string_lossy()),
        ],
    );
    rig.put_originals("op").await;

    let report = rig.rollback("op").await;

    assert_eq!(report.reversed, 2, "the copy and its folder are undone: {report:?}");
    assert!(!vault.exists());
}
