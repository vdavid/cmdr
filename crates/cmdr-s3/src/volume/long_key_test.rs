//! A key at S3's 1,024-byte ceiling, against a fake that refuses a longer
//! listing prefix the way B2 does (`400 InvalidRequest`, live, 2026-10-02:
//! `live_hostile_names_round_trip`). Every "is this a folder?" listing asks
//! for `<key>/`, one byte past the ceiling, which no key can match, so the
//! volume answers it without asking and the object stays reachable.

use std::time::Duration;

use cmdr_fs::volume::{RenameWork, Volume, VolumeError};

use super::fake_s3::FakeS3;

/// An ASCII key of exactly 1,024 bytes (so its wire spelling is the same).
fn ceiling_key() -> String {
    format!("long/{}", "k".repeat(1024 - 5))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_at_the_ceiling_stats_tallies_renames_and_deletes() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let key = ceiling_key();
    s3.seed(&key, 42);
    let volume = s3.volume();
    let path = volume.root().join(&key);

    let stat = volume.get_metadata(&path).await;
    assert!(
        matches!(&stat, Ok(e) if e.size == Some(42) && !e.is_directory),
        "{stat:?}"
    );
    let tally = volume.tally_subtree(&path, 10).await;
    assert!(matches!(&tally, Ok(t) if t.files == 1 && t.bytes == 42), "{tally:?}");
    let work = volume.rename_work(&path).await;
    assert!(matches!(work, Ok(RenameWork::OneCall)), "{work:?}");
    let deleted = volume.delete(&path).await;
    assert!(deleted.is_ok(), "{deleted:?}");
    assert!(s3.object(&key).is_none(), "the object is gone");
    assert!(
        s3.listed().iter().all(|prefix| prefix.len() <= 1024),
        "never a prefix no key can match: {:?}",
        s3.listed().iter().map(String::len).collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_key_at_the_ceiling_is_not_found() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = s3.volume();
    let path = volume.root().join(ceiling_key());
    let stat = volume.get_metadata(&path).await;
    assert!(matches!(stat, Err(VolumeError::NotFound(_))), "{stat:?}");
    let listed = volume.list_directory(&path, None).await;
    assert!(matches!(listed, Err(VolumeError::NotFound(_))), "{listed:?}");
    let deleted = volume.delete(&path).await;
    assert!(matches!(deleted, Err(VolumeError::NotFound(_))), "{deleted:?}");
}
