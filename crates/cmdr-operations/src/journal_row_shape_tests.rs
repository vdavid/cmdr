//! The SHAPE of the rows each kind journals, through the real capture paths, resolved
//! to full paths: which side a row's source and dest sit on, which rows are created in
//! place, and what a folder unit's `size` means. The Operation log's item tree and its
//! selection resolve every row by these shapes, so a capture change that moves a row
//! to the other side shows up here first. Counts and roles alone live in
//! `journal_capture_tests.rs` and `journal_capture_volume_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::delete::delete_files_with_progress_inner;
use super::event_sinks::CollectorEventSink;
use super::journal;
use super::state::WriteOperationState;
use super::transfer::copy::copy_files_with_progress_inner;
use super::types::{VolumeCopyConfig, WriteOperationConfig};
use crate::testing::{TestOperationLog, TestOperations};

use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_operation_log::store::{OperationItemRow, open_read_connection, read_operation_items, reconstruct_dir_path};
use cmdr_operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind, RowRole};

/// One journaled row with its interned dirs resolved: `(volume, full path)` per side.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shape {
    entry_type: EntryType,
    row_role: RowRole,
    source: (String, PathBuf),
    dest: Option<(String, PathBuf)>,
    size: Option<i64>,
}

fn side(conn: &rusqlite::Connection, dir_id: i64, name: &str) -> (String, PathBuf) {
    let volume: String = conn
        .query_row("SELECT volume_id FROM dirs WHERE dir_id = ?1", [dir_id], |r| r.get(0))
        .expect("dir volume");
    let dir = reconstruct_dir_path(conn, dir_id).expect("dir path");
    (volume, Path::new(&dir).join(name))
}

/// Every row of `op_id`, in `seq` order, resolved.
fn shapes(db: &Path, op_id: &str) -> Vec<Shape> {
    let conn = open_read_connection(db).expect("read conn");
    let rows: Vec<OperationItemRow> = read_operation_items(&conn, op_id, 10_000).expect("items");
    rows.iter()
        .map(|r| Shape {
            entry_type: r.entry_type,
            row_role: r.row_role,
            source: side(&conn, r.source_dir_id, &r.source_name),
            dest: match (r.dest_dir_id, &r.dest_name) {
                (Some(id), Some(name)) => Some(side(&conn, id, name)),
                _ => None,
            },
            size: r.size,
        })
        .collect()
}

/// Whether a row names a folder the operation created: source == dest.
fn created_in_place(s: &Shape) -> bool {
    s.dest.as_ref() == Some(&s.source)
}

fn local(path: PathBuf) -> (String, PathBuf) {
    ("root".to_string(), path)
}

fn journaled() -> (TestOperations, TestOperationLog) {
    let log = TestOperationLog::new("journal-row-shape");
    (TestOperations::journaling_into(&log), log)
}

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::ZERO))
}

/// A temp dir by its canonical path, so a recorded path compares equal (macOS's
/// `/var` is a link to `/private/var`).
fn work() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("work");
    let root = std::fs::canonicalize(dir.path()).expect("canonical");
    (dir, root)
}

/// `src/a.txt` and `src/sub/b.txt` under `root`.
fn source_tree(root: &Path) -> PathBuf {
    let src = root.join("src");
    std::fs::create_dir_all(src.join("sub")).expect("mk src");
    std::fs::write(src.join("a.txt"), b"aaa").expect("a");
    std::fs::write(src.join("sub").join("b.txt"), b"bbbb").expect("b");
    src
}

/// A folder copy: one FILE row per leaf (source = the original, dest = where it
/// landed), plus one DIR row per folder it made, created in place on the DEST side.
#[test]
fn a_folder_copy_journals_leaves_across_and_its_folders_in_place_on_the_dest_side() {
    let (engine, log) = journaled();
    let (_keep, root) = work();
    let src = source_tree(&root);
    let dst = root.join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let st = state(&engine);
    journal::open_local_op(st.scope(), "op", OpKind::Copy, Initiator::User, 0, Some("root"));
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op",
        &st,
        std::slice::from_ref(&src),
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("copy");
    journal::finalize_op(st.scope(), OpKind::Copy, ExecutionStatus::Done);

    let rows = shapes(log.db(), "op");
    assert!(rows.iter().all(|r| r.row_role == RowRole::RollbackUnit), "{rows:#?}");
    let mut files: Vec<_> = rows.iter().filter(|r| r.entry_type == EntryType::File).collect();
    files.sort_by(|a, b| a.source.cmp(&b.source));
    assert_eq!(
        files
            .iter()
            .map(|r| (r.source.clone(), r.dest.clone(), r.size))
            .collect::<Vec<_>>(),
        vec![
            (local(src.join("a.txt")), Some(local(dst.join("src/a.txt"))), Some(3)),
            (
                local(src.join("sub/b.txt")),
                Some(local(dst.join("src/sub/b.txt"))),
                Some(4)
            ),
        ],
        "{rows:#?}"
    );
    let mut dirs: Vec<_> = rows.iter().filter(|r| r.entry_type == EntryType::Dir).collect();
    dirs.sort_by(|a, b| a.source.cmp(&b.source));
    assert!(dirs.iter().all(|r| created_in_place(r)), "{rows:#?}");
    assert_eq!(
        dirs.iter().map(|r| r.source.clone()).collect::<Vec<_>>(),
        vec![local(dst.join("src")), local(dst.join("src/sub"))],
        "{rows:#?}"
    );
}

/// A cross-volume folder move journals like a copy: per-leaf FILE rows from the
/// source volume to the dest volume, and the folders it made created in place on the
/// dest volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_folder_move_journals_like_a_copy() {
    let (engine, log) = journaled();
    let source = Arc::new(InMemoryVolume::new("Src").with_space_info(1_000_000, 900_000));
    source.create_directory(Path::new("/photos")).await.expect("mk");
    source.create_directory(Path::new("/photos/sub")).await.expect("mk");
    source.create_file(Path::new("/photos/a.txt"), b"aaa").await.expect("a");
    source
        .create_file(Path::new("/photos/sub/b.txt"), b"bbbb")
        .await
        .expect("b");
    let dest = Arc::new(InMemoryVolume::new("Dst").with_space_info(1_000_000, 900_000));

    let st = Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(
        st.scope(),
        "op",
        OpKind::Move,
        Initiator::User,
        "smb-src",
        Some("smb-dst"),
        0,
    );
    super::move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op",
        &st,
        source as Arc<dyn Volume>,
        &[PathBuf::from("/photos")],
        dest as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("volume move");
    journal::finalize_op(st.scope(), OpKind::Move, ExecutionStatus::Done);

    let on = |vol: &str, p: &str| (vol.to_string(), PathBuf::from(p));
    let rows = shapes(log.db(), "op");
    assert!(rows.iter().all(|r| r.row_role == RowRole::RollbackUnit), "{rows:#?}");
    let mut files: Vec<_> = rows
        .iter()
        .filter(|r| r.entry_type == EntryType::File)
        .map(|r| (r.source.clone(), r.dest.clone()))
        .collect();
    files.sort();
    assert_eq!(
        files,
        vec![
            (on("smb-src", "/photos/a.txt"), Some(on("smb-dst", "/photos/a.txt"))),
            (
                on("smb-src", "/photos/sub/b.txt"),
                Some(on("smb-dst", "/photos/sub/b.txt"))
            ),
        ],
        "{rows:#?}"
    );
    let mut dirs: Vec<_> = rows.iter().filter(|r| r.entry_type == EntryType::Dir).collect();
    dirs.sort_by(|a, b| a.source.cmp(&b.source));
    assert!(dirs.iter().all(|r| created_in_place(r)), "{rows:#?}");
    assert_eq!(
        dirs.iter().map(|r| r.source.clone()).collect::<Vec<_>>(),
        vec![on("smb-dst", "/photos"), on("smb-dst", "/photos/sub")],
        "{rows:#?}"
    );
}

/// A folder delete: one FILE row per leaf at its original path, no dest. Pinned as
/// whatever folder rows it writes today, so the tree's delete shape is known.
#[test]
fn a_folder_delete_journals_one_row_per_leaf_at_its_original_path() {
    let (engine, log) = journaled();
    let (_keep, root) = work();
    let src = source_tree(&root);

    let st = state(&engine);
    journal::open_local_op(st.scope(), "op", OpKind::Delete, Initiator::User, 0, None);
    delete_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op",
        &st,
        std::slice::from_ref(&src),
        &WriteOperationConfig::default(),
    )
    .expect("delete");
    journal::finalize_op(st.scope(), OpKind::Delete, ExecutionStatus::Done);

    let rows = shapes(log.db(), "op");
    let mut files: Vec<_> = rows
        .iter()
        .filter(|r| r.entry_type == EntryType::File)
        .map(|r| (r.row_role, r.source.clone(), r.dest.clone(), r.size))
        .collect();
    files.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        files,
        vec![
            (RowRole::RollbackUnit, local(src.join("a.txt")), None, Some(3)),
            (RowRole::RollbackUnit, local(src.join("sub/b.txt")), None, Some(4)),
        ],
        "{rows:#?}"
    );
    assert!(
        rows.iter().all(|r| r.entry_type == EntryType::File),
        "a delete journals its leaves only, no folder rows: {rows:#?}"
    );
}

// ── Same-FS move and trash: one folder unit plus `search_only` leaves ──────────
//
// The leaves come from the drive index; these install a canned enumeration (the
// test hook `journal_capture_tests.rs` uses too).

#[cfg(target_os = "macos")]
mod folder_units {
    use super::*;
    use crate::transfer::move_op::move_files_with_progress_inner;
    use cmdr_operation_log::types::SearchCoverage;

    fn canned_leaves(names: &[&str]) {
        let leaves: Vec<_> = names
            .iter()
            .map(|n| crate::journal_search::Leaf {
                rel: PathBuf::from(n),
                entry_type: EntryType::File,
                size: Some(1),
                mtime: None,
            })
            .collect();
        crate::journal_search::test_hook::install(move |_path| {
            Some(crate::journal_search::BufferedLeaves {
                coverage: SearchCoverage::Full,
                reason: None,
                leaves: leaves.clone(),
            })
        });
    }

    /// The unit is ONE `dir` row, original → landed, whose `size` is the folder
    /// inode's length (never its bytes); each leaf is `search_only` with
    /// `source = original/rel` and `dest = landed/rel`.
    #[test]
    fn a_same_fs_folder_move_journals_one_unit_and_leaves_on_both_sides() {
        let (engine, log) = journaled();
        let (_keep, root) = work();
        let src = source_tree(&root);
        let dst = root.join("dst");
        std::fs::create_dir_all(&dst).expect("mk dst");
        let inode_len = std::fs::symlink_metadata(&src).expect("stat").len() as i64;

        canned_leaves(&["a.txt", "sub/b.txt"]);
        let st = state(&engine);
        journal::open_local_op(st.scope(), "op", OpKind::Move, Initiator::User, 0, Some("root"));
        move_files_with_progress_inner(
            &CollectorEventSink::new(),
            "op",
            &st,
            std::slice::from_ref(&src),
            &dst,
            &WriteOperationConfig::default(),
        )
        .expect("move");
        journal::finalize_op(st.scope(), OpKind::Move, ExecutionStatus::Done);
        crate::journal_search::test_hook::clear();

        let rows = shapes(log.db(), "op");
        let units: Vec<_> = rows.iter().filter(|r| r.row_role == RowRole::RollbackUnit).collect();
        assert_eq!(units.len(), 1, "{rows:#?}");
        assert_eq!(units[0].entry_type, EntryType::Dir);
        assert_eq!(units[0].source, local(src.clone()));
        assert_eq!(units[0].dest, Some(local(dst.join("src"))));
        assert_eq!(
            units[0].size,
            Some(inode_len),
            "a folder unit's size is its inode length, never the 7 bytes inside it"
        );
        let mut leaves: Vec<_> = rows
            .iter()
            .filter(|r| r.row_role == RowRole::SearchOnly)
            .map(|r| (r.source.clone(), r.dest.clone()))
            .collect();
        leaves.sort();
        assert_eq!(
            leaves,
            vec![
                (local(src.join("a.txt")), Some(local(dst.join("src/a.txt")))),
                (local(src.join("sub/b.txt")), Some(local(dst.join("src/sub/b.txt")))),
            ],
            "{rows:#?}"
        );
    }

    /// A trashed folder: one `dir` unit, original → its place in the Trash, and its
    /// `search_only` leaves rebased the same way.
    #[test]
    fn a_folder_trash_journals_one_unit_and_leaves_under_its_place_in_the_trash() {
        use crate::delete::trash::trash_files_with_progress;

        let (engine, log) = journaled();
        let (_keep, root) = work();
        let src = source_tree(&root);

        canned_leaves(&["a.txt", "sub/b.txt"]);
        let _cleanup = crate::testing::TrashCleanup::of(std::slice::from_ref(&src));
        let st = state(&engine);
        journal::open_local_op(st.scope(), "op", OpKind::Trash, Initiator::User, 0, None);
        let _ = trash_files_with_progress(&CollectorEventSink::new(), "op", &st, std::slice::from_ref(&src), None);
        journal::finalize_op(st.scope(), OpKind::Trash, ExecutionStatus::Done);
        crate::journal_search::test_hook::clear();

        let rows = shapes(log.db(), "op");
        let units: Vec<_> = rows.iter().filter(|r| r.row_role == RowRole::RollbackUnit).collect();
        assert_eq!(units.len(), 1, "{rows:#?}");
        assert_eq!(units[0].entry_type, EntryType::Dir);
        assert_eq!(units[0].source, local(src.clone()));
        let (trash_vol, in_trash) = units[0].dest.clone().expect("macOS reports the in-Trash path");
        assert_eq!(trash_vol, "root");
        assert!(in_trash.components().any(|c| c.as_os_str() == ".Trash"), "{in_trash:?}");
        let mut leaves: Vec<_> = rows
            .iter()
            .filter(|r| r.row_role == RowRole::SearchOnly)
            .map(|r| (r.source.clone(), r.dest.clone()))
            .collect();
        leaves.sort();
        assert_eq!(
            leaves,
            vec![
                (local(src.join("a.txt")), Some(local(in_trash.join("a.txt")))),
                (local(src.join("sub/b.txt")), Some(local(in_trash.join("sub/b.txt")))),
            ],
            "{rows:#?}"
        );
    }
}
