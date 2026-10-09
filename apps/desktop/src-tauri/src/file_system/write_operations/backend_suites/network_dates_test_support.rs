//! The date scenarios every backend owes, written once: a copy keeps the
//! source's modification date, onto the server and off it, for files and for
//! the folders it creates.
//!
//! These run the app's whole pipeline, so they catch what the per-backend
//! conformance cells (`cmdr_fs::volume::conformance`'s two date assertions)
//! can't: a staging rename, a wrapper stream, or an engine path that drops the
//! date between the two ends. The contract:
//! `../transfer/volume/DETAILS.md` § "Copies keep the source's date". Cells stay
//! per backend, named for their lane, like `network_transfer_test_support.rs`'s.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::conformance::{SOURCE_DATE_SECS, assert_write_from_stream_keeps_the_source_date};

use super::network_transfer_test_support::{clean_deep, run_copy, self_describing_bytes};
use crate::file_system::volume::LocalPosixVolume;
use crate::test_support::TestDir;

/// What `path` lists as its modification date, which the scenarios insist on.
async fn listed_date(volume: &dyn Volume, path: &Path, what: &str) -> u64 {
    volume
        .get_metadata(path)
        .await
        .unwrap_or_else(|e| panic!("{what}: {} must be stattable, got {e:?}", path.display()))
        .modified_at
        .unwrap_or_else(|| panic!("{what}: {} lists no modification date at all", path.display()))
}

/// A file copied onto the server keeps the date it had on local disk, through
/// the app's whole pipeline: the staging name, the final rename, and the
/// checkpoint wrapper around the source stream all sit between the two ends.
///
/// `tolerance` is the server's clock granularity, as for
/// `conformance::assert_write_from_stream_keeps_the_source_date` (which pins
/// the destination half alone, in the backend's own crate).
pub(super) async fn a_copy_onto_the_server_keeps_the_source_date(
    remote: Arc<dyn Volume>,
    dir: PathBuf,
    tolerance: Duration,
) {
    let local_dir = TestDir::new("network_dated_onto_server");
    let local_file = local_dir.join("dated.bin");
    std::fs::write(&local_file, self_describing_bytes(4_000, "dated.bin")).expect("seed the local file");
    let source_date = std::time::UNIX_EPOCH + Duration::from_secs(SOURCE_DATE_SECS);
    std::fs::File::options()
        .write(true)
        .open(&local_file)
        .and_then(|file| file.set_modified(source_date))
        .expect("date the local file into the past");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));

    run_copy(
        "dated-onto-server",
        Arc::clone(&local),
        vec![PathBuf::from("dated.bin")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    let landed = listed_date(remote.as_ref(), &dir.join("dated.bin"), "after the copy").await;
    let off_by = landed.abs_diff(SOURCE_DATE_SECS);
    assert!(
        off_by <= tolerance.as_secs(),
        "the copy on the server must keep the local file's date ({SOURCE_DATE_SECS}); it lists {landed}, {off_by} s off"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A file copied off the server keeps the date the server lists for it.
///
/// The seed is a dated write through the server's own `write_from_stream`,
/// asserted as it goes, so a server that can't store a date fails at the seed
/// with a sentence saying so, rather than here with a date nobody chose.
pub(super) async fn a_copy_off_the_server_keeps_the_source_date(
    remote: Arc<dyn Volume>,
    dir: PathBuf,
    tolerance: Duration,
) {
    let on_server = dir.join("dated.txt");
    assert_write_from_stream_keeps_the_source_date(remote.as_ref(), &on_server, tolerance).await;
    a_copy_off_the_server_keeps_the_date_it_lists(Arc::clone(&remote), on_server).await;
    clean_deep(remote.as_ref(), &dir).await;
}

/// The date a scenario's copied folder carries: 2021-01-29 08:30:15 UTC.
const FOLDER_DATE_SECS: u64 = SOURCE_DATE_SECS;
/// The folder inside it, dated apart so a stamp on the wrong level can't pass.
const INNER_FOLDER_DATE_SECS: u64 = 1_577_836_800; // 2020-01-01

/// `album/` with a file and `inner/` (holding a file of its own) on local disk
/// under `root`, both folders dated after their contents, the way an old folder
/// lists.
fn seed_dated_local_folder(root: &Path) {
    let album = root.join("album");
    let inner = album.join("inner");
    std::fs::create_dir_all(&inner).expect("seed the local folders");
    std::fs::write(album.join("photo.bin"), self_describing_bytes(3_000, "photo.bin")).expect("seed a file");
    std::fs::write(inner.join("deep.bin"), self_describing_bytes(2_000, "deep.bin")).expect("seed a file");
    date_local_folder(&inner, INNER_FOLDER_DATE_SECS);
    date_local_folder(&album, FOLDER_DATE_SECS);
}

fn date_local_folder(path: &Path, secs: u64) {
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(secs as i64, 0))
        .unwrap_or_else(|e| panic!("date {}: {e}", path.display()));
}

fn assert_dated(listed: u64, expected: u64, tolerance: Duration, what: &str) {
    let off_by = listed.abs_diff(expected);
    assert!(
        off_by <= tolerance.as_secs(),
        "{what} must keep its source folder's date ({expected}); it lists {listed}, {off_by} s off"
    );
}

/// A folder copied onto the server keeps its date, and so does the folder
/// inside it, through the app's whole pipeline. Writing each child bumps its
/// folder's date on the server, so this only passes when the copy dates every
/// folder AFTER its contents landed.
///
/// `tolerance` as for [`a_copy_onto_the_server_keeps_the_source_date`].
pub(super) async fn copied_folders_onto_the_server_keep_their_dates(
    remote: Arc<dyn Volume>,
    dir: PathBuf,
    tolerance: Duration,
) {
    let local_dir = TestDir::new("network_dated_folders_onto_server");
    seed_dated_local_folder(&local_dir);
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));

    run_copy(
        "dated-folders-onto-server",
        Arc::clone(&local),
        vec![PathBuf::from("album")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;

    let album = dir.join("album");
    let listed = listed_date(remote.as_ref(), &album, "the copied folder").await;
    assert_dated(listed, FOLDER_DATE_SECS, tolerance, "the copied folder on the server");
    let listed = listed_date(remote.as_ref(), &album.join("inner"), "the folder inside it").await;
    assert_dated(
        listed,
        INNER_FOLDER_DATE_SECS,
        tolerance,
        "the folder inside it on the server",
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A folder copied off the server keeps the date the server lists for it, and
/// so does the folder inside it.
///
/// The seed lands through a plain copy onto the server, then the server's own
/// [`Volume::set_modified`] ages both folders (deepest first), so this pins the
/// copy-off half on its own.
pub(super) async fn copied_folders_off_the_server_keep_their_dates(remote: Arc<dyn Volume>, dir: PathBuf) {
    let seed_dir = TestDir::new("network_dated_folders_seed");
    seed_dated_local_folder(&seed_dir);
    let seed: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Seed", &*seed_dir));
    run_copy(
        "dated-folders-seed",
        seed,
        vec![PathBuf::from("album")],
        Arc::clone(&remote),
        dir.clone(),
    )
    .await;
    let album = dir.join("album");
    let at = |secs: u64| std::time::UNIX_EPOCH + Duration::from_secs(secs);
    for (folder, secs) in [
        (album.join("inner"), INNER_FOLDER_DATE_SECS),
        (album.clone(), FOLDER_DATE_SECS),
    ] {
        remote
            .set_modified(&folder, at(secs))
            .await
            .unwrap_or_else(|e| panic!("fixture: date {} on the server, got {e:?}", folder.display()));
    }
    let listed_album = listed_date(remote.as_ref(), &album, "the seed").await;
    let listed_inner = listed_date(remote.as_ref(), &album.join("inner"), "the seed").await;

    let local_dir = TestDir::new("network_dated_folders_off_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));
    run_copy(
        "dated-folders-off-server",
        Arc::clone(&remote),
        vec![album],
        Arc::clone(&local),
        PathBuf::from(""),
    )
    .await;

    // Local disk keeps nanoseconds, so only the server's rounding can put the
    // two a second apart.
    let landed = listed_date(local.as_ref(), Path::new("album"), "after the copy").await;
    assert_dated(
        landed,
        listed_album,
        Duration::from_secs(1),
        "the copied folder on local disk",
    );
    let landed = listed_date(local.as_ref(), Path::new("album/inner"), "after the copy").await;
    assert_dated(
        landed,
        listed_inner,
        Duration::from_secs(1),
        "the folder inside it on local disk",
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A file already on the server, listed with a date at least a day old, keeps
/// that date when copied off it.
///
/// For a server that can't store a date (Apache `mod_dav`), whose fixture dates
/// a file by its own means; [`a_copy_off_the_server_keeps_the_source_date`]
/// seeds one through the server for everything else. Leaves `on_server` alone.
pub(super) async fn a_copy_off_the_server_keeps_the_date_it_lists(remote: Arc<dyn Volume>, on_server: PathBuf) {
    let source_date = listed_date(remote.as_ref(), &on_server, "the seed").await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is past 1970")
        .as_secs();
    assert!(
        source_date + 24 * 60 * 60 <= now,
        "fixture precondition: {} must list a date at least a day old, so a copy stamping \"now\" can't pass; it lists {source_date}",
        on_server.display()
    );
    let name = on_server
        .file_name()
        .expect("the seed is a file, so it has a name")
        .to_owned();

    let local_dir = TestDir::new("network_dated_off_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Local", &*local_dir));
    run_copy(
        "dated-off-server",
        Arc::clone(&remote),
        vec![on_server],
        Arc::clone(&local),
        PathBuf::from(""),
    )
    .await;

    // Local disk keeps nanoseconds, so only the server's own rounding of the
    // date it reports can put the two a second apart.
    let landed = listed_date(local.as_ref(), Path::new(&name), "after the copy").await;
    let off_by = landed.abs_diff(source_date);
    assert!(
        off_by <= 1,
        "the copy on local disk must keep the date the server lists ({source_date}); it lists {landed}, {off_by} s off"
    );
}
