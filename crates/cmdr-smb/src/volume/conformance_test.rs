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

use super::test_support::*;
use super::*;
use cmdr_fs::volume::StreamLength;

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

/// The shared `write_from_stream(CreateNew)` no-clobber assertion, against a
/// real SMB server. `b"new"` fits one compound frame, so this is the
/// single-shot path, where no staged landing refuses on the write's behalf: the
/// refusal is the server's `STATUS_OBJECT_NAME_COLLISION` on
/// `write_file_compound_exclusive`'s `FileCreate`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_create_new_honors_the_shared_no_clobber_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let notes = format!("{base}/notes.txt");
    let fresh = format!("{base}/fresh.txt");
    smb_vol
        .create_file(Path::new(&notes), b"the user's notes")
        .await
        .unwrap();
    assert!(
        smb_vol.write_is_single_shot(StreamLength::Known(3)).await,
        "fixture precondition: the write under test must take the single-shot compound path"
    );

    cmdr_fs::volume::conformance::assert_write_from_stream_create_new_refuses_to_clobber(
        smb_vol.as_ref(),
        Path::new(&notes),
        Path::new(&fresh),
        b"new",
    )
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

/// The shared file-in-the-way assertion, against a real SMB server: the
/// trait's default walk, with the server's own answer for a create under a file.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_create_directory_all_honors_the_shared_file_in_the_way_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    let notes = format!("{base}/notes");
    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol
        .create_file(Path::new(&notes), b"the user's notes")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_create_directory_all_refuses_a_file_in_the_way(
        smb_vol.as_ref(),
        Path::new(&notes),
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

/// The shared export-handshake assertion, against a real SMB server: the bytes
/// stream back over smb2, and `supports_export()` says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_export_honors_the_shared_handshake_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let file = format!("{base}/exported.txt");
    let content = b"the bytes a copy would move";
    smb_vol.create_file(Path::new(&file), content).await.unwrap();

    cmdr_fs::volume::conformance::assert_export_matches_the_bytes_offered(smb_vol.as_ref(), Path::new(&file), content)
        .await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared date assertions, over a real share: a copy onto it keeps the
/// source's date, and a read off it reports the date it lists. The dated
/// source is a few bytes, so this is the one-frame compound write, stamped by
/// path once the frame has closed the handle.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_keeps_the_source_date_per_the_shared_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();

    let dated = format!("{base}/dated.txt");
    cmdr_fs::volume::conformance::assert_write_from_stream_keeps_the_source_date(
        smb_vol.as_ref(),
        Path::new(&dated),
        Duration::ZERO,
    )
    .await;
    cmdr_fs::volume::conformance::assert_read_stream_reports_the_listed_date(smb_vol.as_ref(), Path::new(&dated)).await;

    ensure_clean(&smb_vol, &base).await;
}

/// A folder dated after a file landed in it lists that date.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_set_modified_dates_a_folder() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();

    cmdr_fs::volume::conformance::assert_set_modified_dates_a_folder(
        smb_vol.as_ref(),
        Path::new(&format!("{base}/dated")),
        Duration::ZERO,
    )
    .await;

    ensure_clean(&smb_vol, &base).await;
}

/// A write too big for one frame keeps the source's date too: the streaming
/// writer stamps its OWN handle before `finish()`, since the server rewrites
/// the date when a writing handle closes and a path stamp while it's open
/// would lose to that close.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_streamed_write_keeps_the_source_date() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;
    smb_vol.create_directory(Path::new(&base)).await.unwrap();

    let max_write = smb_vol
        .negotiated_max_write()
        .await
        .expect("a connected volume has negotiated params");
    let size = max_write + 64 * 1024;
    assert!(
        !smb_vol.write_is_single_shot(StreamLength::Known(size)).await,
        "this cell only means something on the streaming writer"
    );
    let aged = format!("{base}/aged.bin");
    smb_vol
        .create_file(Path::new(&aged), &vec![0x5A; size as usize])
        .await
        .unwrap();
    age_in_the_container(&aged, cmdr_fs::volume::conformance::SOURCE_DATE_SECS);

    let stream = smb_vol.open_read_stream(Path::new(&aged)).await.unwrap();
    assert!(
        stream.modified_at().is_some(),
        "the aged source's stream must carry its 2021 date, or this cell proves nothing about the destination"
    );
    let copy = format!("{base}/copy.bin");
    smb_vol
        .write_from_stream(
            Path::new(&copy),
            cmdr_fs::volume::WriteMode::CreateNew,
            StreamLength::Known(size),
            stream,
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap();

    let listed = smb_vol
        .get_metadata(Path::new(&copy))
        .await
        .unwrap()
        .modified_at
        .expect("SMB lists `LastWriteTime` on every file");
    assert_eq!(
        listed,
        cmdr_fs::volume::conformance::SOURCE_DATE_SECS,
        "a streamed copy must keep the source's date (2021-01-29 08:30:15 UTC)"
    );

    ensure_clean(&smb_vol, &base).await;
}

/// The read half of the shared date contract on its own, on both read paths:
/// the streamed download and the hinted one-frame compound read.
///
/// The file is aged inside the container (`touch -d`), so the read half is
/// pinned apart from this backend's own write half.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_read_stream_reports_the_listed_date_on_both_read_paths() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let aged = format!("{base}/aged.txt");
    smb_vol
        .create_file(Path::new(&aged), b"bytes from a file last changed in 2021\n")
        .await
        .unwrap();
    age_in_the_container(&aged, cmdr_fs::volume::conformance::SOURCE_DATE_SECS);

    cmdr_fs::volume::conformance::assert_read_stream_reports_the_listed_date(smb_vol.as_ref(), Path::new(&aged)).await;

    let size = smb_vol.get_metadata(Path::new(&aged)).await.unwrap().size;
    let hinted = smb_vol
        .open_read_stream_with_hint(Path::new(&aged), size)
        .await
        .unwrap();
    let reported = hinted
        .modified_at()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    assert_eq!(
        reported,
        Some(cmdr_fs::volume::conformance::SOURCE_DATE_SECS),
        "the one-frame compound read of {aged} must report the date the share lists"
    );

    ensure_clean(&smb_vol, &base).await;
}

/// Sets `share_relative`'s modification date to `unix_secs` from inside the
/// guest fixture container, the one way to age a file on a share that doesn't
/// depend on this backend setting dates.
fn age_in_the_container(share_relative: &str, unix_secs: u64) {
    let port = docker_guest_params().port;
    let container = docker(&["ps", "--filter", &format!("publish={port}"), "--format", "{{.Names}}"]);
    let container = container.lines().next().unwrap_or_default();
    assert!(
        !container.is_empty(),
        "no container publishes the guest SMB port {port}"
    );
    docker(&[
        "exec",
        container,
        "touch",
        "-d",
        &format!("@{unix_secs}"),
        &format!("/shares/public/{share_relative}"),
    ]);
}

/// One `docker` invocation, or a panic naming what could not be run.
fn docker(args: &[&str]) -> String {
    let out = std::process::Command::new("docker")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!("this cell ages a file inside the fixture container and needs the `docker` CLI: {e}")
        });
    assert!(
        out.status.success(),
        "`docker {}` did not run: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The shared `NotFound`-payload assertion, against a real SMB server: what the
/// frontend renders as the missing file's name really is its path, not the
/// server's NTSTATUS sentence.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_not_found_honors_the_shared_path_payload_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let missing = format!("{base}/no-such-file.txt");

    cmdr_fs::volume::conformance::assert_not_found_carries_the_path(smb_vol.as_ref(), Path::new(&missing)).await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared conflict-scan assertion, against a real SMB server: a destination
/// the paste would create answers "nothing clashes", not the server's
/// `STATUS_OBJECT_NAME_NOT_FOUND`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_conflict_scan_honors_the_shared_missing_destination_contract() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    let unborn = format!("{base}/not-created-yet");

    cmdr_fs::volume::conformance::assert_conflict_scan_reads_a_missing_destination_as_empty(
        smb_vol.as_ref(),
        Path::new(&unborn),
    )
    .await;

    ensure_clean(&smb_vol, &base).await;
}

/// The shared stop assertions, against a real SMB server.
///
/// ❗ This is the backend the ⚠️ was about: a share whose disks have spun down
/// answers a listing in seconds, and the whole scan used to be one call with no
/// boundary in it, so Cancel did nothing until the walk was over. The seam is per
/// entry, and per directory BEFORE its listing goes out.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_batch_scan_stops_when_it_is_told_to() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol
        .create_file(Path::new(&format!("{base}/a.txt")), b"a")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{base}/b.txt")), b"bb")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_batch_scan_stops_when_told(smb_vol.as_ref(), Path::new(&base)).await;

    ensure_clean(&smb_vol, &base).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_batch_scan_asks_its_boundary_inside_the_walk() {
    let smb_vol = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&smb_vol, &base).await;

    smb_vol.create_directory(Path::new(&base)).await.unwrap();
    smb_vol
        .create_directory(Path::new(&format!("{base}/nested")))
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{base}/a.txt")), b"a")
        .await
        .unwrap();
    smb_vol
        .create_file(Path::new(&format!("{base}/nested/c.txt")), b"ccc")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_batch_scan_asks_inside_the_walk(smb_vol.as_ref(), Path::new(&base), 3).await;

    ensure_clean(&smb_vol, &base).await;
}
