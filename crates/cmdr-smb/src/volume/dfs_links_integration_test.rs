//! Every call family through a DFS link inside the share (`dfs_links.rs`):
//! transfers, listings, stats, scans, and mutations, against
//! smb2's own DFS fixtures: `smb-dfs-root` (port 10456) holds a share `dfs`
//! whose `data` folder is an `msdfs:` link to `smb-dfs-target`'s `files` share
//! (port 10457).
//!
//! These fixtures live in smb2's INTERNAL Docker stack, not the consumer stack
//! Cmdr vendors, so the cells are opt-in: they need the two containers started
//! by hand from the smb2 repo,
//! `docker compose -f crates/smb2/tests/docker/internal/docker-compose.yml up -d --build smb-dfs-root smb-dfs-target`,
//! then `CMDR_SMB_DFS_FIXTURE=1 cargo nextest run -p cmdr-smb --run-ignored all -E 'test(smb_dfs_)'`.
//!
//! ❗ `#[ignore]` alone doesn't keep them out of a gating run: the integration
//! lane runs EVERY ignored cell in `package(cmdr-smb)` against the consumer
//! stack, which has no DFS containers. Without the variable each cell says
//! "skipped" and passes ([`dfs_fixture_not_requested`]).
//!
//! ❗ The link's target is another host, so every cell that follows it allows the
//! pair first ([`dfs_volume_with_host`]); `smb_dfs_a_link_to_another_host_*` pin
//! what happens without that.

use super::session_build::TEST_DFS_TARGET_OVERRIDES;
use super::test_support::*;
use super::*;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::listings::{ListingHost, RecordingListings};
use cmdr_fs::volume::{DirectoryChange, StreamLength, WriteMode};
use std::path::Path;

const DFS_ROOT_PORT: u16 = 10456;
const DFS_TARGET_ADDR: &str = "127.0.0.1:10457";

/// The variable that says smb2's DFS containers are up (this file's header).
const DFS_FIXTURE_ENV: &str = "CMDR_SMB_DFS_FIXTURE";

/// Answers true, having said why, when nobody asked for the DFS cells: the
/// shared fixture lane selects them with the rest of the package, and its stack
/// has no DFS containers. Asked for and unreachable still FAILS, in
/// [`dfs_volume`].
#[allow(
    clippy::print_stderr,
    reason = "a skip notice belongs in the test runner's captured output, where nextest attributes it to the cell"
)]
fn dfs_fixture_not_requested() -> bool {
    if std::env::var_os(DFS_FIXTURE_ENV).is_some_and(|v| !v.is_empty()) {
        return false;
    }
    eprintln!("skipped: set {DFS_FIXTURE_ENV}=1 once smb2's DFS containers are up (see this file's header)");
    true
}

/// A volume on the DFS root share, with the link's target name pointed at the
/// target container's port-mapped address.
async fn dfs_volume() -> SmbVolume {
    dfs_volume_with_host(VolumeHost::detached()).await
}

/// [`dfs_volume`] reporting to `host`.
async fn dfs_volume_with_host(host: VolumeHost) -> SmbVolume {
    TEST_DFS_TARGET_OVERRIDES
        .lock_ignore_poison()
        .get_or_insert_with(Default::default)
        .insert("smb-dfs-target".to_string(), DFS_TARGET_ADDR.to_string());
    assert!(crate::LinkConsents::allow("127.0.0.1", "smb-dfs-target"));
    let params = SmbConnectionParams::new("127.0.0.1", "dfs", DFS_ROOT_PORT, None, None);
    connect_smb_volume(
        "dfs",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &cmdr_fs::volume::smb_volume_id("127.0.0.1", DFS_ROOT_PORT, "dfs"),
        params,
        host,
    )
    .await
    .unwrap_or_else(|e| {
        panic!("Couldn't reach smb2's smb-dfs-root at 127.0.0.1:{DFS_ROOT_PORT}; see this file's header ({e:?})")
    })
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 % 251) as u8).collect()
}

async fn upload(vol: &SmbVolume, path: &str, bytes: Vec<u8>, mode: WriteMode) {
    let len = bytes.len() as u64;
    let written = vol
        .write_from_stream(
            Path::new(path),
            mode,
            StreamLength::Known(len),
            inline_read_stream(bytes),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap_or_else(|e| panic!("an upload through the link lands: {e:?}"));
    assert_eq!(written.bytes, len);
}

/// One dead link refuses the paths behind it and nothing else (vdavid/smb2#28):
/// the link's target name points at a closed port, the share stays `Direct`, and
/// a path outside the link still answers.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_dead_link_target_refuses_the_path_and_leaves_the_share_up() {
    if dfs_fixture_not_requested() {
        return;
    }
    TEST_DFS_TARGET_OVERRIDES
        .lock_ignore_poison()
        .get_or_insert_with(Default::default)
        .insert("smb-dfs-target".to_string(), "127.0.0.1:1".to_string());
    assert!(crate::LinkConsents::allow("127.0.0.1", "smb-dfs-target"));
    let params = SmbConnectionParams::new("127.0.0.1", "dfs", DFS_ROOT_PORT, None, None);
    let vol = connect_smb_volume(
        "dfs",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &cmdr_fs::volume::smb_volume_id("127.0.0.1", DFS_ROOT_PORT, "dfs"),
        params,
        VolumeHost::detached(),
    )
    .await
    .expect("the share itself connects");

    let refused = vol.list_directory(Path::new("data"), None).await;
    assert!(
        matches!(refused, Err(VolumeError::LinkTargetUnreachable(_))),
        "a listing behind the dead link refuses that path: {refused:?}"
    );
    let stat = vol.get_metadata(Path::new("data/hello.txt")).await;
    assert!(
        matches!(stat, Err(VolumeError::LinkTargetUnreachable(_))),
        "so does a stat: {stat:?}"
    );

    assert_eq!(vol.inner.state.state(), ConnectionState::Direct, "the share stays up");
    vol.list_directory(Path::new(""), None)
        .await
        .expect("a path outside the link still answers");
}

/// Whether `result` is the typed refusal naming both servers.
#[track_caller]
fn assert_needs_consent<T: std::fmt::Debug>(result: Result<T, VolumeError>, server: &str, what: &str) {
    match result {
        Err(VolumeError::LinkNeedsConsent(sign_in)) => {
            assert_eq!(sign_in.server, server, "{what}");
            assert_eq!(sign_in.target, "smb-dfs-target", "{what}");
        }
        other => panic!("expected a consent refusal from {what}: {other:?}"),
    }
}

/// The security line: a link to another host, with nobody's consent. Every call
/// family refuses that path alone, typed, the other host accepts not one
/// connection (so no sign-in could have gone there), and the share stays up.
///
/// Connects as `localhost`, so the pair differs from the one [`dfs_volume`] allows.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_link_to_another_host_hears_nothing_without_consent() {
    if dfs_fixture_not_requested() {
        return;
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = listener.local_addr().unwrap().to_string();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&accepted);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            drop(socket);
        }
    });
    TEST_DFS_TARGET_OVERRIDES
        .lock_ignore_poison()
        .get_or_insert_with(Default::default)
        .insert("smb-dfs-target".to_string(), target_addr);
    let params = SmbConnectionParams::new("localhost", "dfs", DFS_ROOT_PORT, None, None);
    let vol = connect_smb_volume(
        "dfs",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &cmdr_fs::volume::smb_volume_id("localhost", DFS_ROOT_PORT, "dfs"),
        params,
        VolumeHost::detached(),
    )
    .await
    .expect("the share itself connects");
    let server = "localhost";

    assert_needs_consent(vol.list_directory(Path::new("data"), None).await, server, "a listing");
    assert_needs_consent(vol.get_metadata(Path::new("data/hello.txt")).await, server, "a stat");
    assert_needs_consent(vol.is_directory(Path::new("data/subdir")).await, server, "is_directory");
    assert_needs_consent(vol.entry_kind(Path::new("data/subdir")).await, server, "entry_kind");
    assert!(!vol.exists(Path::new("data/hello.txt")).await, "exists can't say yes");
    assert_needs_consent(
        vol.attributes_impl(Path::new("data/hello.txt")).await,
        server,
        "attributes",
    );
    assert_needs_consent(
        vol.read_range(Path::new("data/hello.txt"), 0, 4).await,
        server,
        "read_range",
    );
    assert_needs_consent(
        vol.open_random_reader(Path::new("data/hello.txt")).await.map(|_| ()),
        server,
        "a random reader",
    );
    assert_needs_consent(
        vol.open_read_stream(Path::new("data/hello.txt")).await.map(|_| ()),
        server,
        "a streamed read",
    );
    assert_needs_consent(
        vol.open_read_stream_with_hint(Path::new("data/hello.txt"), Some(23))
            .await
            .map(|_| ()),
        server,
        "a hinted read",
    );
    assert_needs_consent(
        vol.write_from_stream(
            Path::new("data/upload.bin"),
            WriteMode::CreateNew,
            StreamLength::Known(5 * 1024 * 1024),
            inline_read_stream(payload(5 * 1024 * 1024)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .map(|w| w.bytes),
        server,
        "a streamed upload",
    );
    assert_needs_consent(
        vol.create_file(Path::new("data/small.bin"), b"hi").await,
        server,
        "a one-frame create",
    );
    assert_needs_consent(vol.create_directory(Path::new("data/new")).await, server, "a mkdir");
    assert_needs_consent(vol.delete(Path::new("data/hello.txt")).await, server, "a delete");
    assert_needs_consent(
        vol.rename(Path::new("data/hello.txt"), Path::new("data/other.txt"), false)
            .await,
        server,
        "a rename",
    );
    assert_needs_consent(vol.scan_for_copy(Path::new("data/subdir")).await, server, "a copy scan");

    vol.begin_scan_session().await;
    let listed = vol
        .list_directory_for_scan(
            Path::new("data/subdir"),
            None,
            &cmdr_fs::volume::ScanListingClock::start(),
        )
        .await;
    let prefetched = vol
        .open_read_stream_for_scan(Path::new("data/subdir/nested.txt"), Some(12))
        .await
        .map(|_| ());
    vol.end_scan_session().await;
    assert_needs_consent(listed, server, "an index walk");
    assert_needs_consent(prefetched, server, "an enrichment prefetch");

    assert!(
        vol.watch_listing(Path::new("data/subdir")).is_none(),
        "no link was followed, so nothing starts a watch there"
    );

    assert_eq!(accepted.load(Ordering::SeqCst), 0, "the other host heard nothing");
    assert_eq!(vol.inner.state.state(), ConnectionState::Direct, "the share stays up");
    vol.list_directory(Path::new(""), None)
        .await
        .expect("a path outside the link still answers");
}

/// Taking consent back stops a link that already worked on its next call, and
/// allowing it again brings it back.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_link_to_another_host_stops_when_consent_is_forgotten() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    vol.list_directory(Path::new("data"), None).await.expect("allowed");

    assert!(crate::LinkConsents::forget("127.0.0.1", "smb-dfs-target"));
    assert_needs_consent(
        vol.list_directory(Path::new("data"), None).await,
        "127.0.0.1",
        "a known link, forgotten",
    );
    assert_eq!(vol.inner.state.state(), ConnectionState::Direct, "the share stays up");

    assert!(crate::LinkConsents::allow("127.0.0.1", "smb-dfs-target"));
    vol.list_directory(Path::new("data"), None)
        .await
        .expect("allowed again");
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_file_behind_a_link_downloads_streamed_and_hinted() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let expected = b"Hello from DFS target!\n".to_vec();

    let streamed = drain(vol.open_read_stream(Path::new("data/hello.txt")).await.unwrap()).await;
    assert_eq!(
        streamed, expected,
        "a streamed read through the link reads the target's file"
    );

    let hinted = drain(
        vol.open_read_stream_with_hint(Path::new("data/hello.txt"), Some(expected.len() as u64))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        hinted, expected,
        "a hinted read through the link falls through to the link stream"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_uploads_through_a_link_land_on_the_target_streamed_and_single_frame() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let folder = run_folder();
    vol.create_directory(Path::new(&folder)).await.unwrap();
    let big_path = &format!("{folder}/big.bin");
    let small_path = &format!("{folder}/small.bin");

    // Bigger than a fresh connection's one-frame limit, so it streams, and
    // spans several of the link download's windows on the way back.
    let big = payload(5 * 1024 * 1024 + 123);
    upload(&vol, big_path, big.clone(), WriteMode::CreateOrReplace).await;
    let small = payload(4096);
    upload(&vol, small_path, small.clone(), WriteMode::CreateOrReplace).await;

    let big_back = drain(vol.open_read_stream(Path::new(big_path)).await.unwrap()).await;
    assert_eq!(big_back.len(), big.len());
    assert!(big_back == big, "the streamed upload reads back byte for byte");
    let small_back = drain(vol.open_read_stream(Path::new(small_path)).await.unwrap()).await;
    assert_eq!(small_back, small, "the one-frame upload reads back byte for byte");

    for path in [big_path, small_path, &folder] {
        vol.delete(Path::new(path)).await.unwrap();
    }
}

/// A folder name of this run's own inside the link, so runs never collide.
fn run_folder() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("data/cmdr-dfs-{}-{nanos}", std::process::id())
}

fn names(entries: &[FileEntry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    names.sort();
    names
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_folder_behind_a_link_lists() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;

    let top = vol.list_directory(Path::new("data"), None).await.unwrap();
    let top_names = names(&top);
    assert!(
        top_names.contains(&"hello.txt".to_string()) && top_names.contains(&"subdir".to_string()),
        "the link's folder lists the target share, got {top_names:?}"
    );
    let hello = top.iter().find(|e| e.name == "hello.txt").unwrap();
    assert_eq!(hello.size, Some(23));
    assert!(
        hello.path.ends_with("data/hello.txt"),
        "an entry keeps the caller's path, link folder included, got {:?}",
        hello.path
    );

    let nested = vol.list_directory(Path::new("data/subdir"), None).await.unwrap();
    assert_eq!(
        names(&nested),
        vec!["nested.txt".to_string()],
        "a folder deeper in the link"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_stats_behind_a_link_describe_the_target() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;

    let file = vol.get_metadata(Path::new("data/hello.txt")).await.unwrap();
    assert_eq!((file.is_directory, file.size), (false, Some(23)), "get_metadata");
    assert!(
        vol.is_directory(Path::new("data/subdir")).await.unwrap(),
        "is_directory"
    );
    assert!(
        !vol.is_directory(Path::new("data/hello.txt")).await.unwrap(),
        "is_directory on a file"
    );
    assert!(vol.exists(Path::new("data/subdir/nested.txt")).await, "exists");
    assert!(
        !vol.exists(Path::new("data/no-such-file.txt")).await,
        "exists on a miss"
    );
    assert!(matches!(
        vol.get_metadata(Path::new("data/no-such-file.txt")).await,
        Err(VolumeError::NotFound(_))
    ));
    assert_eq!(
        vol.entry_kind(Path::new("data/subdir")).await.unwrap(),
        cmdr_fs::volume::EntryKind::Directory,
        "entry_kind"
    );
    let attributes = vol.attributes_impl(Path::new("data/hello.txt")).await.unwrap();
    assert!(!attributes.contains(smb2::FileAttributes::DIRECTORY), "attributes");
    assert_eq!(
        vol.read_range(Path::new("data/hello.txt"), 6, 4).await.unwrap(),
        b"from".to_vec(),
        "read_range"
    );
    let reader = vol.open_random_reader(Path::new("data/hello.txt")).await.unwrap();
    assert_eq!(reader.size(), 23, "open_random_reader");
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_copy_scan_walks_behind_a_link() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;

    let scan = vol.scan_for_copy(Path::new("data/subdir")).await.unwrap();
    assert_eq!((scan.file_count, scan.total_bytes), (1, 12), "a folder behind the link");

    let batch = vol
        .scan_for_copy_batch(&[PathBuf::from("data/hello.txt"), PathBuf::from("data/subdir")])
        .await
        .unwrap();
    assert_eq!(
        (batch.aggregate.file_count, batch.aggregate.total_bytes),
        (2, 35),
        "the batch scan's pipelined top-level stats follow the link too"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_mutations_behind_a_link_land_on_the_target() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let folder = run_folder();
    let file = format!("{folder}/note.txt");
    let renamed = format!("{folder}/renamed.txt");

    vol.create_directory(Path::new(&folder)).await.unwrap();
    vol.create_file(Path::new(&file), b"hi").await.unwrap();
    assert!(
        matches!(
            vol.create_file(Path::new(&file), b"again").await,
            Err(VolumeError::AlreadyExists(_))
        ),
        "create_file still refuses to clobber through the link"
    );
    vol.rename(Path::new(&file), Path::new(&renamed), false).await.unwrap();
    vol.create_file(Path::new(&file), b"second").await.unwrap();
    vol.rename(Path::new(&file), Path::new(&renamed), true).await.unwrap();
    assert_eq!(
        vol.read_range(Path::new(&renamed), 0, 6).await.unwrap(),
        b"second".to_vec(),
        "the forced rename swapped the new file in"
    );
    assert_eq!(
        names(&vol.list_directory(Path::new(&folder), None).await.unwrap()),
        vec!["renamed.txt".to_string()]
    );

    vol.delete(Path::new(&renamed)).await.unwrap();
    vol.delete(Path::new(&folder)).await.unwrap();
    assert!(
        !vol.exists(Path::new(&folder)).await,
        "the folder is gone from the target"
    );
}

/// Once a call has followed a link, a call under it goes straight to the
/// target: smb2's resolver answers from its referral cache, so the share's own
/// connection sends nothing, where each call used to pay a refused CREATE there
/// (twice: ours, then smb2's own first try).
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_call_under_a_known_link_skips_the_refused_round_trip() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    vol.list_directory(Path::new("data"), None).await.unwrap();

    let before = request_counts(&vol).await;
    let file = vol.get_metadata(Path::new("data/hello.txt")).await.unwrap();
    let nested = vol.list_directory(Path::new("data/subdir"), None).await.unwrap();
    let after = request_counts(&vol).await;

    assert_eq!(file.size, Some(23));
    assert_eq!(names(&nested), vec!["nested.txt".to_string()]);
    assert_eq!(
        after.0 - before.0,
        0,
        "the share's connection sends nothing for a path under a known link"
    );
}

/// A move between two folders behind one link lands on the target (smb2
/// 0.36.2 places a SET_INFO's destination inside the link), both with a free
/// destination and replacing a taken one. Before it, smb2 could follow a rename
/// only within one directory and refused this.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_move_between_folders_behind_a_link_lands() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let folder = run_folder();
    let inner = format!("{folder}/inner");
    let file = format!("{folder}/moved.txt");
    let moved = format!("{inner}/moved.txt");
    vol.create_directory(Path::new(&folder)).await.unwrap();
    vol.create_directory(Path::new(&inner)).await.unwrap();
    vol.create_file(Path::new(&file), b"first").await.unwrap();

    vol.rename(Path::new(&file), Path::new(&moved), false)
        .await
        .unwrap_or_else(|e| panic!("a move inside the link lands: {e:?}"));
    assert!(!vol.exists(Path::new(&file)).await, "the source is gone");
    assert_eq!(
        vol.read_range(Path::new(&moved), 0, 5).await.unwrap(),
        b"first".to_vec()
    );

    vol.create_file(Path::new(&file), b"again").await.unwrap();
    vol.rename(Path::new(&file), Path::new(&moved), true)
        .await
        .unwrap_or_else(|e| panic!("a forced move inside the link replaces: {e:?}"));
    assert_eq!(
        vol.read_range(Path::new(&moved), 0, 5).await.unwrap(),
        b"again".to_vec(),
        "the forced move swapped the new file in"
    );

    vol.delete(Path::new(&moved)).await.unwrap();
    vol.delete(Path::new(&inner)).await.unwrap();
    vol.delete(Path::new(&folder)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_foreign_spelling_behind_a_link_resolves() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    // Exact already: nothing to correct.
    assert_eq!(
        vol.find_stored_spelling(Path::new("data/subdir/nested.txt"), None)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_the_index_scan_pool_lists_behind_a_link() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    vol.begin_scan_session().await;
    assert!(
        vol.inner.scan_pool.read().await.is_some(),
        "the pool is open, so the listing rides a member"
    );
    let listed = vol
        .list_directory_for_scan(
            Path::new("data/subdir"),
            None,
            &cmdr_fs::volume::ScanListingClock::start(),
        )
        .await;
    let prefetched = vol
        .open_read_stream_for_scan(Path::new("data/subdir/nested.txt"), Some(12))
        .await;
    vol.end_scan_session().await;
    assert_eq!(names(&listed.unwrap()), vec!["nested.txt".to_string()]);
    assert_eq!(
        drain(prefetched.unwrap()).await,
        b"Nested file\n".to_vec(),
        "an enrichment prefetch behind the link streams through it"
    );
}

/// The share's CHANGE_NOTIFY watch runs on the share's own server, so it never
/// hears a change on a link's target: a listing behind a link isn't kept fresh,
/// and the fresh-listing oracle must not stand in for a read there.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_listing_behind_a_link_claims_no_watch() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    assert_eq!(vol.listing_watch_coverage(Path::new("")), WatchCoverage::EveryWriter);
    vol.list_directory(Path::new("data/subdir"), None).await.unwrap();
    assert_eq!(
        vol.listing_watch_coverage(Path::new("data/subdir")),
        WatchCoverage::None
    );
    assert_eq!(
        vol.listing_watch_coverage(Path::new("data/subdir/deeper")),
        WatchCoverage::None,
        "below a followed path too"
    );
    assert_eq!(
        vol.listing_watch_coverage(Path::new("")),
        WatchCoverage::EveryWriter,
        "the share itself stays watched"
    );
}

/// While a pane shows a folder behind a link, the link's target is watched:
/// another client's change there reaches the pane under the path the pane
/// keyed it on, the listing claims the watch, and once the last listing behind
/// the link closes the watch goes and the claim with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_shown_folder_behind_a_link_hears_another_clients_change() {
    if dfs_fixture_not_requested() {
        return;
    }
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder()
        .listings(Arc::clone(&listings) as Arc<dyn ListingHost>)
        .build();
    let vol = dfs_volume_with_host(host).await;
    let other_client = dfs_volume().await;
    let folder = run_folder();
    other_client.create_directory(Path::new(&folder)).await.unwrap();

    // The pane's read, which follows the link, then what the listing engine
    // asks for while the listing is open.
    vol.list_directory(Path::new(&folder), None).await.unwrap();
    let hold = vol
        .watch_listing(Path::new(&folder))
        .expect("a listing behind a link keeps a watch on it");
    cmdr_fs::testing::wait_until_async(Duration::from_secs(10), "the link watch is live", || {
        vol.listing_watch_coverage(Path::new(&folder)) == WatchCoverage::EveryWriter
    })
    .await;

    // The watch arms on its own session before it says it's live, so one write
    // is enough; the loop only rides out the server's notify fan-out.
    let pane_path = Path::new(TEST_MOUNT_ROOT).join(&folder);
    let name = "from-another-client.txt";
    other_client
        .create_file(Path::new(&format!("{folder}/{name}")), b"outside")
        .await
        .unwrap();
    cmdr_fs::testing::wait_until_async(
        Duration::from_secs(20),
        "another client's file reaches the pane",
        || {
            listings.changes().iter().any(|(_, parent, change)| {
                matches!(change, DirectoryChange::Added(entry) if entry.name == name) && *parent == pane_path
            })
        },
    )
    .await;

    drop(hold);
    assert_eq!(
        vol.listing_watch_coverage(Path::new(&folder)),
        WatchCoverage::None,
        "with no listing behind the link left, nothing watches it"
    );

    other_client
        .delete(Path::new(&format!("{folder}/{name}")))
        .await
        .unwrap();
    other_client.delete(Path::new(&folder)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_server_side_copy_through_a_link_asks_the_caller_to_stream() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let result = Volume::copy_within(
        &vol,
        Path::new("data/hello.txt"),
        Path::new("data/never.txt"),
        &|_, _| std::ops::ControlFlow::Continue(()),
    )
    .await;
    assert!(
        matches!(result, Err(VolumeError::NotSupported)),
        "the engine's 'stream it instead', got {result:?}"
    );
}
