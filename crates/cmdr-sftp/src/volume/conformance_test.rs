//! The shared `Volume` conformance promises, against a real OpenSSH server.
//!
//! ❗ **The server here is `sftp-fixture-openssh`, and that is the point.** It is
//! stock OpenSSH, so it advertises `posix-rename@openssh.com` — the extension
//! that is DEFINED to replace a rename's destination atomically, and that the
//! obvious call reaches for. The same cells against `sftp-fixture-noposixrename`
//! would pass while a clobbering rename shipped, because a server without the
//! extension refuses an occupied destination all by itself.
//!
//! These live apart from the rest of the Docker suite because they assert
//! something different in kind: not what this backend does, but that it keeps the
//! five contracts `volume::conformance` holds every backend to. SFTP has no
//! in-process double, and the answers that matter are the server's.

use std::path::Path;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::conformance;

use super::SftpVolume;
use super::testing::*;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// The stock server, plus a scratch directory of this cell's own.
///
/// Every cell in this binary shares one export, so a fixed directory name would
/// have two of them renaming each other's files.
async fn stock_server_with_scratch(what: &str) -> (SftpVolume, String) {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir(what);
    clean_scratch(&volume, &dir).await;
    volume.create_directory(Path::new(&dir)).await.expect(FIXTURE);
    (volume, dir)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_forceless_rename_refuses_an_existing_destination_on_a_posix_rename_server() {
    // ❗ THE data-safety cell of the write path. `Fs::rename` sends
    // `posix-rename@openssh.com` whenever the server offers it, and this server
    // does — so wiring `Volume::rename` straight to it would silently replace
    // `target.txt` here and hand every conflict prompt in the app a destroyed
    // file instead of a question.
    let (volume, dir) = stock_server_with_scratch("rename-no-clobber").await;
    let source = format!("{dir}/source.txt");
    let target = format!("{dir}/target.txt");
    volume.create_file(Path::new(&source), b"source").await.expect(FIXTURE);
    volume
        .create_file(Path::new(&target), b"the user's target file")
        .await
        .expect(FIXTURE);

    conformance::assert_rename_refuses_an_existing_destination(&volume, Path::new(&source), Path::new(&target)).await;

    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn create_file_refuses_to_clobber() {
    // The refusal is `SSH_FXF_EXCL`'s, so this is the cell that would notice the
    // exclusive open being swapped for a plain create, or for a stat-then-write.
    let (volume, dir) = stock_server_with_scratch("create-file-no-clobber").await;
    let notes = format!("{dir}/notes.txt");
    volume
        .create_file(Path::new(&notes), b"the user's notes")
        .await
        .expect(FIXTURE);

    conformance::assert_create_file_refuses_to_clobber(&volume, Path::new(&notes), b"new").await;

    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn create_directory_all_reports_an_existing_directory_honestly() {
    // ❗ `Created` is a promise the transfer driver SPENDS: on it, it skips the
    // per-file destination conflict probe for everything it writes inside. SFTP
    // v3 answers a mkdir over an existing directory with the same catch-all code
    // it answers a full disk with, so the honesty here rests entirely on the
    // probe that resolves it.
    let (volume, dir) = stock_server_with_scratch("mkdir-p-honesty").await;
    let album = format!("{dir}/album");
    volume.create_directory(Path::new(&album)).await.expect(FIXTURE);

    conformance::assert_create_directory_all_reports_an_existing_dir_honestly(&volume, Path::new(&album)).await;

    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn delete_leaves_a_non_empty_directory_intact() {
    let (volume, dir) = stock_server_with_scratch("delete-non-recursive").await;
    let album = format!("{dir}/album");
    volume.create_directory(Path::new(&album)).await.expect(FIXTURE);
    volume
        .create_file(Path::new(&format!("{album}/keep.txt")), b"content")
        .await
        .expect(FIXTURE);

    conformance::assert_delete_leaves_a_non_empty_dir_intact(&volume, Path::new(&album), "keep.txt").await;

    let _ = volume.delete(Path::new(&format!("{album}/keep.txt"))).await;
    let _ = volume.delete(Path::new(&album)).await;
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn writability_matches_the_mutations_offered() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("writability");
    clean_scratch(&volume, &dir).await;

    conformance::assert_writability_matches_the_mutations_offered(&volume, Path::new(&dir)).await;

    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn export_matches_the_bytes_offered() {
    // ❗ The cell that says this backend can be COPIED FROM at all. Every method
    // the copy engine calls on a source — `open_read_stream`, `read_range`,
    // `scan_for_copy` — is implemented and works here, so nothing else in the
    // suite notices when the one declaration that gates them is missing:
    // `copy_between_volumes` refuses a source answering `supports_export() ==
    // false` before it reads a byte, and logs nothing on the way out.
    let (volume, dir) = stock_server_with_scratch("export-handshake").await;
    let file = format!("{dir}/exported.txt");
    let content = b"the bytes a copy would move";
    volume.create_file(Path::new(&file), content).await.expect(FIXTURE);

    conformance::assert_export_matches_the_bytes_offered(&volume, Path::new(&file), content).await;

    clean_scratch(&volume, &dir).await;
}
