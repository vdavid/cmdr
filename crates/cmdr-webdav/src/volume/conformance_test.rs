//! The shared `Volume` conformance promises, against a real Apache `mod_dav`.
//!
//! ❗ **The server here is `webdav-fixture-apache`, and that is the point.** A
//! WebDAV `MOVE` defaults to `Overwrite: T`, and a `DELETE` on a collection
//! removes the whole tree, so wiring `Volume::rename` or `Volume::delete`
//! straight to the verb would silently clobber here and hand every conflict
//! prompt in the app a destroyed file instead of a question.
//!
//! These live apart from the rest of the Docker suite because they assert
//! something different in kind: not what this backend does, but that it keeps the
//! contracts `volume::conformance` holds every backend to. WebDAV has no
//! in-process double, and the answers that matter are the server's.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::conformance;

use super::WebdavVolume;
use super::testing::*;

const FIXTURE: &str = "webdav-servers/start.sh (webdav-fixture)";

/// The stock server, plus a scratch directory of this cell's own.
///
/// Every cell in this binary shares one export, so a fixed directory name would
/// have two of them renaming each other's files.
async fn stock_server_with_scratch() -> (WebdavVolume, PathBuf) {
    let volume = connect_fixture("APACHE", 13480).await;
    let dir = scratch_dir(&volume).await;
    (volume, dir)
}

/// Removes everything a cell built, deepest first, and the scratch dir itself.
async fn clean(volume: &WebdavVolume, dir: &Path) {
    fn remove<'a>(volume: &'a WebdavVolume, path: &'a Path) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Ok(entries) = volume.list_directory(path, None).await {
                for entry in entries {
                    remove(volume, &path.join(&entry.name)).await;
                }
            }
            let _ = volume.delete(path).await;
        })
    }
    remove(volume, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_forceless_rename_refuses_an_existing_destination() {
    // ❗ THE data-safety cell of the write path. `MOVE` without `Overwrite: F`
    // replaces the destination on every server that speaks the protocol.
    let (volume, dir) = stock_server_with_scratch().await;
    let source = dir.join("source.txt");
    let target = dir.join("target.txt");
    volume.create_file(&source, b"source").await.expect(FIXTURE);
    volume
        .create_file(&target, b"the user's target file")
        .await
        .expect(FIXTURE);

    conformance::assert_rename_refuses_an_existing_destination(&volume, &source, &target).await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn create_file_refuses_to_clobber() {
    // `PUT` overwrites by default; the refusal has to come from `If-None-Match: *`
    // or a probe, so this is the cell that notices either going missing.
    let (volume, dir) = stock_server_with_scratch().await;
    let notes = dir.join("notes.txt");
    volume.create_file(&notes, b"the user's notes").await.expect(FIXTURE);

    conformance::assert_create_file_refuses_to_clobber(&volume, &notes, b"new").await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn create_directory_all_reports_an_existing_directory_honestly() {
    // ❗ `Created` is a promise the transfer driver SPENDS: on it, it skips the
    // per-file destination conflict probe for everything it writes inside.
    // `MKCOL` on an existing collection answers 405, and the honesty here rests
    // on the mapper reading that as "already there" rather than as a refusal.
    let (volume, dir) = stock_server_with_scratch().await;
    let album = dir.join("album");
    volume.create_directory(&album).await.expect(FIXTURE);

    conformance::assert_create_directory_all_reports_an_existing_dir_honestly(&volume, &album).await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn delete_leaves_a_non_empty_directory_intact() {
    // ❗ `DELETE` on a collection is recursive by protocol (`Depth: infinity` is
    // the only depth it accepts), so this refusal is entirely the backend's.
    let (volume, dir) = stock_server_with_scratch().await;
    let album = dir.join("album");
    volume.create_directory(&album).await.expect(FIXTURE);
    volume
        .create_file(&album.join("keep.txt"), b"content")
        .await
        .expect(FIXTURE);

    conformance::assert_delete_leaves_a_non_empty_dir_intact(&volume, &album, "keep.txt").await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn writability_matches_the_mutations_offered() {
    let (volume, dir) = stock_server_with_scratch().await;
    // ❗ A name that is NOT there yet: the helper creates the directory itself
    // (or watches the create be refused) and removes it again, so handing it the
    // scratch dir trips its own precondition.
    let unborn = dir.join("unborn");

    conformance::assert_writability_matches_the_mutations_offered(&volume, &unborn).await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn export_matches_the_bytes_offered() {
    // ❗ The cell that says this backend can be COPIED FROM at all:
    // `copy_between_volumes` refuses a source answering `supports_export() ==
    // false` before it reads a byte, and logs nothing on the way out.
    let (volume, dir) = stock_server_with_scratch().await;
    let file = dir.join("exported.txt");
    let content = b"the bytes a copy would move";
    volume.create_file(&file, content).await.expect(FIXTURE);

    conformance::assert_export_matches_the_bytes_offered(&volume, &file, content).await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn not_found_carries_the_path() {
    // A 404 carries Apache's own HTML page and no path, and that is what the
    // frontend would render as the missing file's name unless the mapper puts
    // the path there.
    let (volume, dir) = stock_server_with_scratch().await;

    conformance::assert_not_found_carries_the_path(&volume, &dir.join("no-such-file.txt")).await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn conflict_scan_reads_a_missing_destination_as_empty() {
    // A `PROPFIND` on a collection nobody has created yet answers 404, and the
    // walk's `scan_conflicts` is what turns it into "nothing clashes" rather
    // than into a copy preview that won't open.
    let (volume, dir) = stock_server_with_scratch().await;

    conformance::assert_conflict_scan_reads_a_missing_destination_as_empty(&volume, &dir.join("not-created-yet")).await;

    clean(&volume, &dir).await;
}

/// The shared stop assertions, against a real Apache `mod_dav`.
///
/// ❗ Per entry: this backend's scan is `scan_walk`'s, so the boundary comes with
/// it. One `PROPFIND` per directory over a WAN link is the cost a Cancel has to
/// land inside of.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_batch_scan_stops_when_it_is_told_to() {
    let (volume, dir) = stock_server_with_scratch().await;
    volume.create_file(&dir.join("a.txt"), b"a").await.expect(FIXTURE);

    conformance::assert_batch_scan_stops_when_told(&volume, &dir).await;

    clean(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_batch_scan_asks_its_boundary_inside_the_walk() {
    let (volume, dir) = stock_server_with_scratch().await;
    volume.create_file(&dir.join("a.txt"), b"a").await.expect(FIXTURE);
    volume.create_file(&dir.join("b.txt"), b"bb").await.expect(FIXTURE);

    conformance::assert_batch_scan_asks_inside_the_walk(&volume, &dir, 3).await;

    clean(&volume, &dir).await;
}
