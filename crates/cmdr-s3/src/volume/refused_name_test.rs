//! A name the provider refuses to store is `VolumeError::InvalidName` before a
//! single request goes out, on every write path: an upload, New File, New
//! Folder, a rename's destination, and a server-side copy's destination.
//! Verified live (`live_hostile_names_round_trip`, 2026-10-02): GCS refuses
//! CR/LF in a key (`400 InvalidObjectName`, a bodyless 400 to the
//! no-overwrite HEAD), B2 any control character (`400 InvalidRequest`, its
//! catch-all, so a code can't tell it apart).

use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::volume::{ServerCopyProgress, Volume, VolumeError, VolumeReadStream, WriteMode};

use super::fake_s3::FakeS3;
use super::testing::BytesSource;
use crate::params::S3Provider;

struct NoProgress;

impl ServerCopyProgress for NoProgress {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

async fn every_write_path(provider: S3Provider, name: &str) {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("source.txt", 5);
    let volume = s3.volume_for(provider);
    let at = |key: &str| volume.root().join(key);
    let source = BytesSource::new(b"hello".to_vec());
    let length = source.total_size();
    let uploaded = volume
        .write_from_stream(&at(name), WriteMode::CreateNew, length, Box::new(source), &|_| {
            ControlFlow::Continue(())
        })
        .await;
    let created = volume.create_file(&at(name), b"hello").await;
    let folder = volume.create_directory(&at(name)).await;
    let renamed = volume.rename(&at("source.txt"), &at(name), false).await;
    let copied = volume
        .copy_on_server(&volume, &at("source.txt"), &at(name), WriteMode::CreateNew, &NoProgress)
        .await;
    for (what, outcome) in [
        ("upload", uploaded.map(|_| ())),
        ("New File", created),
        ("New Folder", folder),
        ("rename", renamed),
        ("server copy", copied.map(|_| ())),
    ] {
        assert!(
            matches!(&outcome, Err(VolumeError::InvalidName(_))),
            "{what} of {name:?}: {outcome:?}"
        );
    }
    assert!(s3.object(name).is_none());
    assert!(s3.object("source.txt").is_some(), "the rename's source stays");
    assert_eq!(s3.requests_about(name), 0, "nothing was sent about the refused name");
}

#[tokio::test(flavor = "multi_thread")]
async fn gcs_refuses_a_line_break_before_sending_anything() {
    every_write_path(S3Provider::Gcs, "line\nbreak.txt").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn b2_refuses_a_control_character_before_sending_anything() {
    every_write_path(
        S3Provider::B2 {
            region: "eu-central-003".into(),
        },
        "tab\there.txt",
    )
    .await;
}

/// The same names are fine where the provider stores them.
#[tokio::test(flavor = "multi_thread")]
async fn r2_stores_a_tab_in_a_name() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = s3.volume();
    let created = volume.create_file(&volume.root().join("tab\there.txt"), b"hello").await;
    assert!(created.is_ok(), "{created:?}");
    assert_eq!(s3.object("tab\there.txt").map(|s| s.len), Some(5));
}
