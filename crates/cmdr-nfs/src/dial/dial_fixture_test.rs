//! The protocol layer against the real knfsd fixture
//! (`apps/desktop/test/nfs-servers/README.md`): every cell is `#[ignore]`d, and
//! the integration lane runs them.

use std::collections::HashSet;

use super::*;
use crate::proto::nfs3::{FileType, ListedEntry, ListingEvent, Stable};
use crate::transport::tap;

const FIXTURE: &str = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)";

fn fixture_server() -> NfsServer {
    let port = |name: &str, default: u16| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    NfsServer {
        host: "127.0.0.1".to_string(),
        port: None,
        portmap_port: Some(port("NFS_FIXTURE_PORTMAP_PORT", 15480)),
    }
}

/// The seeded tree's owner, which root-squash leaves the files to.
const SEEDED: NfsIdentity = NfsIdentity::new(1000, 1000);

async fn mount_insecure(typed: &str) -> MountedExport {
    mount(
        &fixture_server(),
        typed,
        &SEEDED,
        &DialOptions::PRODUCTION,
        &CancellationToken::new(),
    )
    .await
    .unwrap_or_else(|err| panic!("mounting {typed}: {err:?}"))
}

async fn list(mounted: &MountedExport, dir: &FileHandle) -> Vec<ListedEntry> {
    let mut entries = Vec::new();
    mounted
        .nfs
        .list_dir(dir, 64 * 1024, |event| match event {
            ListingEvent::Entries(batch) => entries.extend(batch),
            ListingEvent::Restarted => entries.clear(),
        })
        .await
        .unwrap();
    entries
}

fn named<'a>(entries: &'a [ListedEntry], name: &str) -> &'a ListedEntry {
    entries
        .iter()
        .find(|entry| &entry.name[..] == name.as_bytes())
        .unwrap_or_else(|| panic!("no {name} in the listing"))
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_fixture_lists_both_exports() {
    let _ = FIXTURE;
    let exports = list_exports(&fixture_server(), &DialOptions::PRODUCTION, &CancellationToken::new())
        .await
        .unwrap();
    assert!(exports.contains(&"/secure".to_string()), "{exports:?}");
    assert!(exports.contains(&"/insecure".to_string()), "{exports:?}");
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_insecure_export_mounts_and_lists_the_seeded_root_byte_exact() {
    let mounted = mount_insecure("/insecure").await;
    assert_eq!(mounted.export, "/insecure");
    assert_eq!(mounted.folder, None);
    assert_eq!(mounted.root_attributes.file_type, FileType::Directory);
    assert!(mounted.fsinfo.rtpref > 0 && mounted.fsinfo.wtpref > 0);

    let root = list(&mounted, &mounted.root.clone()).await;
    let names: HashSet<&[u8]> = root.iter().map(|entry| &entry.name[..]).collect();
    for expected in [
        "hello.txt",
        "ten-bytes.txt",
        "zero-bytes.txt",
        "photos",
        "empty-dir",
        "a file with spaces.txt",
        "café.txt",
        "cafe\u{301}.txt",
        "link-inside",
        "link-outside",
        "large.bin",
        "oddnames",
    ] {
        assert!(names.contains(expected.as_bytes()), "missing {expected}");
    }
    assert!(!names.contains(&b"."[..]) && !names.contains(&b".."[..]));
    assert_eq!(named(&root, "hello.txt").attributes.uid, 0);
    assert_eq!(named(&root, "ten-bytes.txt").attributes.uid, 1000);
    assert_eq!(named(&root, "ten-bytes.txt").attributes.size, 10);
    assert_eq!(named(&root, "photos").attributes.file_type, FileType::Directory);
    assert_eq!(named(&root, "link-inside").attributes.file_type, FileType::Symlink);

    let target = mounted.nfs.readlink(&named(&root, "link-inside").handle).await.unwrap();
    assert_eq!(&target[..], b"photos/2024 summer/beach.txt");

    // The non-UTF-8 name comes through as the server's bytes; dropping it is
    // the volume's call.
    let odd = list(&mounted, &named(&root, "oddnames").handle).await;
    assert!(odd.iter().any(|entry| &entry.name[..] == b"caf\xe9.txt"));
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn reads_land_at_their_offsets_and_a_read_past_the_end_says_eof() {
    let mounted = mount_insecure("/insecure").await;
    let large = mounted.nfs.lookup(&mounted.root, b"large.bin").await.unwrap();
    for line in [0u64, 1_000, 13_107_199] {
        let read = mounted.nfs.read(&large.handle, line * 16, 16).await.unwrap();
        assert_eq!(&read.data[..], format!("{line:015}\n").as_bytes());
    }
    let size = large.attributes.unwrap().size;
    assert_eq!(size, 200 * 1024 * 1024);
    let tail = mounted.nfs.read(&large.handle, size - 16, 1024).await.unwrap();
    assert_eq!(tail.data.len(), 16);
    assert!(tail.eof);
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn many_reads_pipelined_on_one_connection_each_get_their_own_bytes() {
    let mounted = mount_insecure("/insecure").await;
    let large = mounted.nfs.lookup(&mounted.root, b"large.bin").await.unwrap();
    let reads = (0..64u64).map(|i| {
        let nfs = mounted.nfs.clone();
        let handle = large.handle.clone();
        async move { (i, nfs.read(&handle, i * 65_536, 65_536).await.unwrap()) }
    });
    for (i, read) in futures_util::future::join_all(reads).await {
        assert_eq!(read.data.len(), 65_536);
        let first_line = i * 65_536 / 16;
        assert_eq!(&read.data[..16], format!("{first_line:015}\n").as_bytes());
    }
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_typed_folder_under_the_export_becomes_the_start_folder() {
    let mounted = mount_insecure("/insecure/photos/2024 summer/").await;
    assert_eq!(mounted.export, "/insecure");
    assert_eq!(mounted.folder.as_deref(), Some("/photos/2024 summer"));
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_missing_export_is_export_not_found() {
    let err = mount(
        &fixture_server(),
        "/no-such-export",
        &SEEDED,
        &DialOptions::PRODUCTION,
        &CancellationToken::new(),
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(err, NfsConnectError::ExportNotFound), "{err:?}");
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_secure_export_through_a_published_port_is_refused_by_number() {
    // A published port rewrites the source port past 1023 (README § "Reaching
    // /secure"), so knfsd answers `MNT3ERR_ACCES` whatever we bind: from a
    // reserved port the export refused us, from none it needed one.
    let err = mount(
        &fixture_server(),
        "/secure",
        &SEEDED,
        &DialOptions::PRODUCTION,
        &CancellationToken::new(),
    )
    .await
    .err()
    .unwrap();
    assert!(
        matches!(
            err,
            NfsConnectError::ExportRefused | NfsConnectError::ReservedPortRequired
        ),
        "{err:?}"
    );
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn the_secure_export_accepts_a_reserved_port_on_the_direct_path() {
    // Linux only: the container's own IP keeps the source port
    // (`NFS_FIXTURE_DIRECT_HOST`, README § "Reaching /secure").
    let Ok(direct) = std::env::var("NFS_FIXTURE_DIRECT_HOST") else {
        return;
    };
    let server = NfsServer::new(&direct, None);
    let mounted = mount(
        &server,
        "/secure",
        &SEEDED,
        &DialOptions::PRODUCTION,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(matches!(mounted.nfs.connection().reserved_port(), ReservedPort::Bound(port) if port < 1024));
    let root = list(&mounted, &mounted.root.clone()).await;
    assert!(root.iter().any(|entry| &entry.name[..] == b"hello.txt"));
}

#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn a_write_and_commit_agree_on_the_verifier_and_the_bytes_read_back() {
    let mounted = mount_insecure("/insecure").await;
    let name = format!("m1-write-{}.bin", random_u32());
    let created = mounted
        .nfs
        .create(
            &mounted.root,
            name.as_bytes(),
            &crate::proto::nfs3::CreateHow::Guarded(crate::proto::nfs3::Sattr3 {
                mode: Some(0o644),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    let handle = match created.handle {
        Some(handle) => handle,
        None => mounted.nfs.lookup(&mounted.root, name.as_bytes()).await.unwrap().handle,
    };
    let write = mounted
        .nfs
        .write(&handle, 0, Stable::Unstable, b"0123456789")
        .await
        .unwrap();
    assert_eq!(write.count, 10);
    let commit = mounted.nfs.commit(&handle, 0, 0).await.unwrap();
    if write.committed == Stable::Unstable {
        assert_eq!(commit.verifier, write.verifier);
    }
    let read = mounted.nfs.read(&handle, 0, 64).await.unwrap();
    assert_eq!(&read.data[..], b"0123456789");

    // A guarded create of a taken name is refused atomically.
    let again = mounted
        .nfs
        .create(
            &mounted.root,
            name.as_bytes(),
            &crate::proto::nfs3::CreateHow::Guarded(Default::default()),
        )
        .await
        .unwrap_err();
    assert_eq!(again, NfsError::Status(NfsStat3::Exist));
    mounted.nfs.remove(&mounted.root, name.as_bytes()).await.unwrap();
    let gone = mounted.nfs.lookup(&mounted.root, name.as_bytes()).await.unwrap_err();
    assert_eq!(gone, NfsError::Status(NfsStat3::NoEnt));
}

/// Writes every reply the protocol layer decodes, as hex, into
/// `$NFS_CAPTURE_DIR` (`<program>-<procedure>-<n>.hex`): how
/// `proto/testdata/knfsd/` was made. Does nothing without the variable.
#[tokio::test]
#[ignore = "needs the NFS fixture stack: nfs-servers/start.sh (nfs-fixture)"]
async fn capture_knfsd_replies_as_decoding_fixtures() {
    let Ok(dir) = std::env::var("NFS_CAPTURE_DIR") else {
        return;
    };
    tap::start();
    let options = DialOptions::PRODUCTION;
    let cancel = CancellationToken::new();
    list_exports(&fixture_server(), &options, &cancel).await.unwrap();
    let mounted = mount_insecure("/insecure").await;
    let nfs = &mounted.nfs;
    list(&mounted, &mounted.root.clone()).await;
    let hello = nfs.lookup(&mounted.root, b"hello.txt").await.unwrap();
    nfs.read(&hello.handle, 0, 4096).await.unwrap();
    nfs.access(&hello.handle, crate::proto::nfs3::access::READ)
        .await
        .unwrap();
    let link = nfs.lookup(&mounted.root, b"link-inside").await.unwrap();
    nfs.readlink(&link.handle).await.unwrap();
    nfs.readdir(&mounted.root, 0, [0; 8], 8192).await.unwrap();
    nfs.fsstat(&mounted.root).await.unwrap();
    nfs.pathconf(&mounted.root).await.unwrap();
    let _ = nfs.lookup(&mounted.root, b"no-such-file").await;
    let _ = mount(&fixture_server(), "/secure", &SEEDED, &options, &cancel).await;

    let mut seen = std::collections::HashMap::<(u32, u32), usize>::new();
    for (procedure, record) in tap::take() {
        let key = (procedure.program.number, procedure.number);
        let n = seen.entry(key).or_default();
        *n += 1;
        let hex: String = record.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = format!("{dir}/{}-{}-{}.hex", key.0, key.1, n);
        std::fs::write(&path, hex).unwrap();
    }
}
