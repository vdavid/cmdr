//! The shared `Volume` conformance promises, asserted against a REAL SMB
//! server (Docker-gated, like every `smb_integration_*` test).
//!
//! These live apart from `smb_transfer_semantics_test.rs` because they assert
//! something different in kind: not how a transfer behaves, but that this
//! backend keeps the four contracts `volume::conformance` holds every backend
//! to. SMB has no in-process double, so the answer they need is the server's
//! (`STATUS_DIRECTORY_NOT_EMPTY`, `STATUS_OBJECT_NAME_COLLISION`), never smb2's.
//!
//! Declared as a `#[cfg(test)]` submodule of `smb`; helpers come from
//! `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;

/// The shared `Volume::delete` non-recursion assertion, against a real SMB
/// server. Docker-gated like the rest of this file, because SMB has no
/// in-process double: the answer we need is the server's
/// (`STATUS_DIRECTORY_NOT_EMPTY`), not smb2's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_delete_honors_the_shared_non_recursion_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    let album = format!("{base}/album");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol.create_directory(Path::new(&album)).await.unwrap();
    smb_vol
        .create_file(Path::new(&format!("{album}/keep.txt")), b"content")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_delete_leaves_a_non_empty_dir_intact(
        smb_vol.as_ref(),
        Path::new(&album),
        "keep.txt",
    )
    .await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared `Volume::rename` no-clobber assertion, against a real SMB server.
///
/// Two mechanisms have to line up for SMB to keep this promise: the `stat`
/// pre-check in `SmbVolume::rename`, and smb2's `ReplaceIfExists == false` on
/// the wire behind it. The pre-check alone is a belief (a `stat` that fails for
/// any reason reads as "nothing there"), so what's really being asserted here is
/// that the server still refuses when the belief is wrong.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_rename_honors_the_shared_no_clobber_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let source = format!("{base}/source.txt");
    let target = format!("{base}/target.txt");
    smb_vol.create_file(Path::new(&source), b"source").await.unwrap();
    smb_vol
        .create_file(Path::new(&target), b"the user's target file")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_rename_refuses_an_existing_destination(
        smb_vol.as_ref(),
        Path::new(&source),
        Path::new(&target),
    )
    .await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared `Volume::create_file` no-clobber assertion, against a real SMB
/// server. The refusal is the server's `STATUS_OBJECT_NAME_COLLISION` on the
/// `FileCreate` disposition, so this is the assertion that would notice
/// `create_file_writer_exclusive` being swapped back for a plain writer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_create_file_honors_the_shared_no_clobber_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let notes = format!("{base}/notes.txt");
    smb_vol
        .create_file(Path::new(&notes), b"the user's notes")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_create_file_refuses_to_clobber(smb_vol.as_ref(), Path::new(&notes), b"new")
        .await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared `Volume::create_directory_all` honesty assertion, against a real
/// SMB server: the trait's default walk composed from SMB's own `exists` +
/// `create_directory`, over the wire.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_create_directory_all_honors_the_shared_honesty_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    let album = format!("{base}/album");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol.create_directory(Path::new(&album)).await.unwrap();

    cmdr_fs::volume::conformance::assert_create_directory_all_reports_an_existing_dir_honestly(
        smb_vol.as_ref(),
        Path::new(&album),
    )
    .await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared writability-declaration assertion, against a real SMB server:
/// `is_writable()` and what the share actually accepts say the same thing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_is_writable_honors_the_shared_declaration_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    cmdr_fs::volume::conformance::assert_writability_matches_the_mutations_offered(smb_vol.as_ref(), Path::new(&base))
        .await;

    ensure_clean(&smb_vol, &base).await;
}
