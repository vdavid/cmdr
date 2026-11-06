//! The read side against the real knfsd fixture
//! (`apps/desktop/test/nfs-servers/README.md`). Every cell is `#[ignore]`d;
//! the integration lane runs them with `--run-ignored only`.

use std::time::Instant;

use cmdr_fs::volume::{EntryKind, StreamLength, Volume, VolumeError};

use super::testing::*;
use super::{NfsIdentitySource, guess_identity, list_exports};
use crate::errors::NfsConnectError;

const SOURCE_DATE_SECS: u64 = 1_611_909_015;

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_seeded_root_lists_byte_exact_with_owners_dates_and_links() {
    let volume = connect_insecure().await;
    let entries = volume.list_directory(&app_path(&volume, "/"), None).await.unwrap();
    // Other cells' scratch folders live at this root too, from this run or any
    // worktree's (`SCRATCH_PREFIX`): working space, not the seeded tree.
    let mut names: Vec<&str> = entries
        .iter()
        .map(|entry| entry.name.as_str())
        .filter(|name| !name.starts_with(SCRATCH_PREFIX))
        .collect();
    names.sort_unstable();
    let mut expected = vec![
        "a file with spaces.txt",
        "caf\u{e9}.txt",
        "cafe\u{301}.txt",
        "dated.txt",
        "empty-dir",
        "five-bytes.txt",
        "group-only",
        "hello.txt",
        "large.bin",
        "link-inside",
        "link-outside",
        "nested",
        "oddnames",
        "photos",
        "ten-bytes.txt",
        "zero-bytes.txt",
    ];
    expected.sort_unstable();
    assert_eq!(names, expected, "both café spellings, by their own bytes");

    let by_name = |name: &str| entries.iter().find(|entry| entry.name == name).unwrap();
    assert_eq!(by_name("ten-bytes.txt").size, Some(10));
    assert_eq!(by_name("zero-bytes.txt").size, Some(0));
    assert_eq!(by_name("large.bin").size, Some(FIXTURE_LARGE_LEN));
    assert_eq!(by_name("dated.txt").modified_at, Some(SOURCE_DATE_SECS));
    assert_eq!(by_name("hello.txt").owner, "0");
    assert_eq!(by_name("photos").owner, "1000");
    assert!(by_name("photos").is_directory);
    assert!(by_name("link-inside").is_symlink && by_name("link-outside").is_symlink);

    let odd = volume
        .list_directory(&app_path(&volume, "/oddnames"), None)
        .await
        .unwrap();
    assert_eq!(
        odd.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["fine.txt"]
    );
    let deep = volume
        .list_directory(&app_path(&volume, "/nested/deep/deeper"), None)
        .await
        .unwrap();
    assert_eq!(deep.len(), 1);
    assert!(
        volume
            .list_directory(&app_path(&volume, "/empty-dir"), None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_large_file_streams_byte_exact_through_the_window() {
    let volume = connect_insecure().await;
    let started = Instant::now();
    let mut stream = volume
        .open_read_stream(&app_path(&volume, FIXTURE_LARGE_FILE))
        .await
        .unwrap();
    assert_eq!(stream.total_size(), StreamLength::Known(FIXTURE_LARGE_LEN));
    let mut offset = 0u64;
    // FNV-1a over the whole stream, beside the per-chunk byte compare: the
    // compare says WHERE a fault is, the digest that the run saw every byte.
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let mut expected_digest: u64 = 0xcbf2_9ce4_8422_2325;
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.unwrap();
        let expected = fixture_large_bytes_at(offset, chunk.len());
        assert!(chunk == expected, "the bytes at offset {offset} are not the file's");
        for (&got, &want) in chunk.iter().zip(&expected) {
            digest = (digest ^ u64::from(got)).wrapping_mul(0x100_0000_01b3);
            expected_digest = (expected_digest ^ u64::from(want)).wrapping_mul(0x100_0000_01b3);
        }
        offset += chunk.len() as u64;
    }
    assert_eq!(offset, FIXTURE_LARGE_LEN);
    assert_eq!(digest, expected_digest);
    let secs = started.elapsed().as_secs_f64();
    log::info!(target: "nfs", "streamed 200 MiB in {secs:.2} s ({:.0} MB/s)", 200.0 * 1.048_576 / secs);
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn read_range_answers_exactly_at_three_offsets() {
    let volume = connect_insecure().await;
    let path = app_path(&volume, FIXTURE_LARGE_FILE);
    for (offset, len) in [
        (0u64, 4096usize),
        (123_456_789, 3_000_000),
        (FIXTURE_LARGE_LEN - 100, 1000),
    ] {
        let read = volume.read_range(&path, offset, len).await.unwrap();
        let expect_len = len.min((FIXTURE_LARGE_LEN - offset) as usize);
        assert_eq!(read.len(), expect_len, "read_range({offset}, {len})");
        assert!(
            read == fixture_large_bytes_at(offset, expect_len),
            "read_range({offset}, {len}) bytes"
        );
    }
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn links_resolve_inside_the_export_and_stop_at_its_edge() {
    let volume = connect_insecure().await;
    let inside = app_path(&volume, "/link-inside");
    assert_eq!(volume.read_range(&inside, 0, 100).await.unwrap(), b"sun\n");
    assert_eq!(volume.entry_kind(&inside).await.unwrap(), EntryKind::Symlink);
    let outside = app_path(&volume, "/link-outside");
    assert!(volume.get_metadata(&outside).await.unwrap().is_symlink);
    assert!(matches!(
        volume.open_read_stream(&outside).await.err(),
        Some(VolumeError::NotSupported)
    ));
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_typed_folder_splits_into_the_export_and_a_start_folder() {
    let volume = connect(fixture_params("/insecure/photos/2024 summer")).await.unwrap();
    let params = volume.params();
    assert_eq!(params.export(), "/insecure");
    assert_eq!(params.start_folder.as_deref(), Some("/photos/2024 summer"));
    assert_eq!(volume.volume_id(), fixture_params("/insecure").volume_id());
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_missing_export_is_export_not_found() {
    assert!(matches!(
        connect(fixture_params("/nope")).await.err(),
        Some(NfsConnectError::ExportNotFound)
    ));
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_secure_export_through_a_published_port_is_refused_from_a_reserved_one() {
    // A published port rewrites the source port (README § "Reaching /secure"),
    // so knfsd refuses whatever we bind. From a port we DID reserve, that's the
    // export's refusal, ❌ not a missing reserved port.
    assert!(matches!(
        connect(fixture_params(FIXTURE_SECURE_EXPORT)).await.err(),
        Some(NfsConnectError::ExportRefused)
    ));
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_secure_export_lists_through_a_reserved_port_on_the_direct_path() {
    let Some(server) = fixture_direct_server() else {
        // `NFS_FIXTURE_DIRECT_HOST` is set on the Linux lane only (README §
        // "Reaching /secure"); the refusal cell above holds macOS.
        return;
    };
    let params = crate::params::NfsConnectionParams::new(server, FIXTURE_SECURE_EXPORT, FIXTURE_IDENTITY);
    let volume = connect(params).await.expect("/secure mounts from a reserved port");
    let entries = volume.list_directory(&app_path(&volume, "/"), None).await.unwrap();
    assert!(entries.iter().any(|entry| entry.name == "hello.txt"));
    assert_eq!(
        volume
            .read_range(&app_path(&volume, "/ten-bytes.txt"), 0, 64)
            .await
            .unwrap(),
        b"0123456789",
        "every NFS call on the connection kept the reserved port, ❌ not just the MNT"
    );
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_server_lists_both_exports_and_guesses_the_files_owner() {
    let exports = list_exports(&fixture_server(), Default::default()).await.unwrap();
    assert!(exports.contains(&FIXTURE_INSECURE_EXPORT.to_string()), "{exports:?}");
    assert!(exports.contains(&FIXTURE_SECURE_EXPORT.to_string()), "{exports:?}");

    let this_mac = crate::params::NfsIdentity::new(501, 20);
    let guess = guess_identity(&fixture_server(), FIXTURE_INSECURE_EXPORT, this_mac, Default::default())
        .await
        .unwrap();
    assert_eq!(guess.source, NfsIdentitySource::MostCommonOwner);
    assert_eq!(
        (guess.identity.uid, guess.identity.gid),
        (FIXTURE_IDENTITY.uid, FIXTURE_IDENTITY.gid),
        "the files' 1000, ❌ not the root-owned export root's 0"
    );
    assert_eq!(guess.identity.other_gids, vec![2001], "the group on `group-only/`");
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_file_readable_only_through_another_group_opens_once_the_place_lists_that_group() {
    let path = "/group-only/secret.txt";
    let outside = connect_insecure().await;
    let refused = outside.read_range(&app_path(&outside, path), 0, 64).await.err();
    assert!(
        matches!(refused, Some(VolumeError::PermissionDenied { .. })),
        "{refused:?}"
    );

    let mut params = fixture_params(FIXTURE_INSECURE_EXPORT);
    params.identity.other_gids = vec![2001];
    let member = connect(params).await.unwrap();
    assert_eq!(
        member.read_range(&app_path(&member, path), 0, 64).await.unwrap(),
        b"through a group\n"
    );
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn space_and_write_access_come_from_the_server() {
    let volume = connect_insecure().await;
    match volume.get_space_info().await.unwrap() {
        cmdr_fs::volume::SpaceInfo::Bounded {
            total_bytes,
            available_bytes,
            ..
        } => assert!(total_bytes > 0 && available_bytes <= total_bytes),
        other => panic!("FSSTAT has a total: {other:?}"),
    }
    assert_eq!(
        volume.write_access_at(&app_path(&volume, "/")).await,
        cmdr_fs::volume::WriteAccess::Writable
    );
}

/// A copy that waited out a long outage carries a download on from an offset
/// (`Volume::resume_read_stream`), on the file its first read saw; one written
/// over since (a new change time) is `SourceChanged`, before a byte is read.
#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_read_resumes_on_its_named_file_and_refuses_a_rewritten_one() {
    let volume = connect_insecure().await;
    let dir = scratch_dir(&volume, "resume").await;
    let path = dir.join("resume.bin");
    let bytes: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    volume.create_file(&path, &bytes).await.unwrap();

    let first = volume.open_read_stream(&path).await.unwrap();
    let version = first.source_version().expect("a lookup names the file's version");
    drop(first);
    let mut rest = volume
        .resume_read_stream(&path, &version, 100_000)
        .await
        .expect("the same file resumes");
    assert_eq!(
        rest.total_size(),
        StreamLength::Known(bytes.len() as u64),
        "the whole file's size"
    );
    let mut read = Vec::new();
    while let Some(chunk) = rest.next_chunk().await {
        read.extend_from_slice(&chunk.unwrap());
    }
    assert!(read == bytes[100_000..], "exactly the bytes from the offset");

    volume.delete(&path).await.unwrap();
    volume.create_file(&path, &bytes).await.unwrap();
    match volume.resume_read_stream(&path, &version, 100_000).await {
        Err(VolumeError::SourceChanged(_)) => {}
        Err(other) => panic!("expected SourceChanged, got {other:?}"),
        Ok(_) => panic!("expected SourceChanged, got a stream"),
    }
    clean_deep(&volume, &dir).await;
}
