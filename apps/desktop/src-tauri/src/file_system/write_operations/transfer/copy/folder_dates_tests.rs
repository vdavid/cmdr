//! A local-to-local copy keeps each copied folder's date, the way the
//! cross-volume engine does (`../volume/folder_dates.rs`). The contract:
//! `../volume/DETAILS.md` § "Copies keep the source's date".

use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;

/// 2021-01-29 08:30:15 UTC.
const FOLDER_DATE: i64 = 1_611_909_015;
/// 2020-01-01, so a stamp on the wrong level can't pass.
const INNER_FOLDER_DATE: i64 = 1_577_836_800;
/// 2019-01-01, for a folder holding nothing at all.
const EMPTY_FOLDER_DATE: i64 = 1_546_300_800;

fn date(path: &Path, secs: i64) {
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(secs, 0)).expect("date the folder");
}

fn mtime(path: &Path) -> i64 {
    filetime::FileTime::from_last_modification_time(&fs::metadata(path).expect("stat")).unix_seconds()
}

/// `album/` with a file, `inner/` holding a file, and an empty `empty/`, each
/// folder dated after its contents landed.
fn seed(src_dir: &Path) -> PathBuf {
    let album = src_dir.join("album");
    fs::create_dir_all(album.join("inner")).unwrap();
    fs::create_dir_all(album.join("empty")).unwrap();
    fs::write(album.join("photo.bin"), b"photo").unwrap();
    fs::write(album.join("inner/deep.bin"), b"deep").unwrap();
    date(&album.join("inner"), INNER_FOLDER_DATE);
    date(&album.join("empty"), EMPTY_FOLDER_DATE);
    date(&album, FOLDER_DATE);
    album
}

fn copy(album: &Path, dst_dir: &Path) {
    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-folder-dates",
        &state,
        std::slice::from_ref(&album.to_path_buf()),
        dst_dir,
        &WriteOperationConfig::default(),
    );
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[test]
fn a_copied_folder_keeps_its_date_and_so_do_the_folders_inside_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&dst_dir).unwrap();
    let album = seed(&tmp.path().join("src"));

    copy(&album, &dst_dir);

    assert_eq!(mtime(&dst_dir.join("album")), FOLDER_DATE);
    assert_eq!(mtime(&dst_dir.join("album/inner")), INNER_FOLDER_DATE);
    assert_eq!(mtime(&dst_dir.join("album/empty")), EMPTY_FOLDER_DATE);
}

/// A merge into a folder that was already there leaves that folder's date to
/// the filesystem: it's the user's folder, and the copy only added to it.
#[test]
fn a_merge_leaves_the_existing_folders_date_alone() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(dst_dir.join("album")).unwrap();
    let album = seed(&tmp.path().join("src"));

    copy(&album, &dst_dir);

    assert_ne!(mtime(&dst_dir.join("album")), FOLDER_DATE);
    assert_eq!(mtime(&dst_dir.join("album/inner")), INNER_FOLDER_DATE);
}
