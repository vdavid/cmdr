//! A saved-server list that's there but doesn't read is never saved over: the next
//! connect's save lands only after the original is set aside, and one entry that
//! doesn't read costs only that entry, with a copy of the whole file kept.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::network::s3_known_places::{KnownS3PlacesStore, StoredS3PlacesFile, migrate};
use crate::network::sftp_known_servers::KnownSftpServersStore;
use crate::network::webdav_known_servers::KnownWebdavServersStore;
use crate::state_recovery::testing;
use crate::test_support::TestDir;

const SFTP: &str = r#"{
  "knownSftpServers": [
    { "host": "testnas.local", "port": 22, "username": "test_user_1", "displayName": "NAS", "remoteRoot": "/srv",
      "keyFile": null, "useAgent": true, "pinned": true, "lastConnectedAt": "2026-10-01T10:00:00Z" },
    { "port": 22, "username": "typo-no-host", "displayName": "", "remoteRoot": "/",
      "keyFile": null, "useAgent": true, "lastConnectedAt": "2026-10-01T10:00:00Z" }
  ]
}"#;

const WEBDAV: &str = r#"{
  "knownWebdavServers": [
    { "url": "https://dav.example.com/", "username": "test_user_1", "displayName": "Work", "remoteRoot": "/",
      "pinned": true, "lastConnectedAt": "2026-10-01T10:00:00Z" },
    { "url": 42 }
  ]
}"#;

const S3: &str = r#"{
  "knownS3Places": [
    { "provider": { "kind": "aws", "region": "eu-west-1" }, "accessKeyId": "AKIAONE", "bucket": "photos",
      "pinned": true, "lastConnectedAt": "2026-10-01T10:00:00Z" },
    { "provider": { "kind": "from_a_newer_build", "zone": "x" }, "accessKeyId": "AKIATWO", "bucket": null,
      "lastConnectedAt": "2026-10-01T10:00:00Z" }
  ],
  "knownS3Accounts": [
    { "provider": { "kind": "aws", "region": "eu-west-1" }, "accessKeyId": "AKIAONE", "displayName": "Home" },
    { "provider": { "kind": "from_a_newer_build" }, "accessKeyId": "AKIATWO", "displayName": "Lab" }
  ]
}"#;

fn running_as_root() -> bool {
    // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail.
    unsafe { libc::geteuid() == 0 }
}

/// The one copy beside `path`, read back whatever its mode.
fn the_copy(path: &Path) -> Vec<u8> {
    let kept = testing::kept_copies(path);
    assert_eq!(kept.len(), 1, "one copy of {path:?}: {kept:?}");
    fs::set_permissions(&kept[0], fs::Permissions::from_mode(0o644)).expect("read the copy");
    fs::read(&kept[0]).expect("read the copy")
}

/// Reads each list as `read::<T>` would at launch, then saves what came back, as the next
/// connect would.
fn read_then_save(dir: &TestDir, prepare: impl Fn(&Path)) {
    let sftp = dir.join("known-sftp-servers.json");
    let webdav = dir.join("webdav_known_servers.json");
    let s3 = dir.join("s3_known_places.json");
    for (path, bytes) in [(&sftp, SFTP), (&webdav, WEBDAV), (&s3, S3)] {
        fs::write(path, bytes).expect("write the saved list");
        prepare(path);
    }

    let sftp_store: KnownSftpServersStore = read(&sftp, StateFile::SftpServers);
    save(&sftp, "the known SFTP servers", &sftp_store);
    let webdav_store: KnownWebdavServersStore = read(&webdav, StateFile::WebdavServers);
    save(&webdav, "the known WebDAV servers", &webdav_store);
    let (s3_store, _) = migrate(read::<StoredS3PlacesFile>(&s3, StateFile::S3Places));
    save(&s3, "the known S3 places", &s3_store);

    assert_eq!(the_copy(&sftp), SFTP.as_bytes());
    assert_eq!(the_copy(&webdav), WEBDAV.as_bytes());
    assert_eq!(the_copy(&s3), S3.as_bytes());
}

#[test]
fn an_unreadable_list_survives_the_next_save() {
    if running_as_root() {
        return;
    }
    let notices = testing::capture_recoveries();
    let dir = TestDir::new("server_list_unreadable");
    read_then_save(&dir, |path| {
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("make it unreadable");
    });
    let files: Vec<StateFile> = notices.recoveries().iter().map(|r| r.file).collect();
    assert_eq!(
        files,
        vec![StateFile::SftpServers, StateFile::WebdavServers, StateFile::S3Places]
    );
}

#[test]
fn a_list_that_doesnt_parse_survives_the_next_save() {
    let _notices = testing::capture_recoveries();
    let dir = TestDir::new("server_list_unparsed");
    let sftp = dir.join("known-sftp-servers.json");
    fs::write(&sftp, b"{\"knownSftpServers\": [{\"trunc").expect("write");

    let store: KnownSftpServersStore = read(&sftp, StateFile::SftpServers);
    assert!(store.known_sftp_servers.is_empty());
    save(&sftp, "the known SFTP servers", &store);

    assert_eq!(the_copy(&sftp), b"{\"knownSftpServers\": [{\"trunc");
}

/// One entry from a newer build, or with a typo, costs that entry only: the rest read, and the
/// whole file is copied aside before the next save drops it.
#[test]
fn one_entry_that_doesnt_read_keeps_the_rest_and_a_copy() {
    let notices = testing::capture_recoveries();
    let dir = TestDir::new("server_list_partial");
    read_then_save(&dir, |_| {});

    let sftp: KnownSftpServersStore = read(&dir.join("known-sftp-servers.json"), StateFile::SftpServers);
    assert_eq!(sftp.known_sftp_servers.len(), 1);
    assert!(sftp.known_sftp_servers[0].pinned);
    let webdav: KnownWebdavServersStore = read(&dir.join("webdav_known_servers.json"), StateFile::WebdavServers);
    assert_eq!(webdav.known_webdav_servers.len(), 1);
    let s3: KnownS3PlacesStore = migrate(read(&dir.join("s3_known_places.json"), StateFile::S3Places)).0;
    assert_eq!(s3.known_s3_places.len(), 1);
    assert_eq!(s3.known_s3_places[0].bucket.as_deref(), Some("photos"));
    assert_eq!(s3.known_s3_accounts.len(), 1);
    assert_eq!(s3.known_s3_accounts[0].display_name, "Home");
    assert_eq!(notices.recoveries().len(), 3, "one notice per list");
}
