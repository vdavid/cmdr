//! A `DeleteObjects` request the server throttles goes again after a pause,
//! against a fake that answers the first ones `503 SlowDown`: the request is
//! idempotent, and one throttled batch of a thousand keys must not fail the
//! whole delete it belongs to (a 1,005-object delete stopped halfway on a
//! single blip, live on Wasabi, 2026-10-02).

use std::time::Duration;

use cmdr_fs::volume::Volume;

use super::fake_s3::FakeS3;

#[tokio::test(flavor = "multi_thread")]
async fn a_throttled_batch_delete_goes_again_and_lands() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let keys = ["doomed/a.txt", "doomed/b.txt", "doomed/c.txt"];
    for key in keys {
        s3.seed(key, 1);
    }
    s3.throttle_batch_deletes(2);
    let volume = s3.volume();
    let paths: Vec<_> = keys.iter().map(|key| volume.root().join(key)).collect();

    let results = volume.delete_files(&paths).await;

    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(
        s3.batch_deletes(),
        1,
        "one batch reached the store, after two throttled tries"
    );
    for key in keys {
        assert!(s3.object(key).is_none(), "{key} is gone");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_batch_still_throttled_after_its_retries_fails_each_key() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    s3.seed("doomed/a.txt", 1);
    s3.throttle_batch_deletes(usize::MAX);
    let volume = s3.volume();

    let results = volume.delete_files(&[volume.root().join("doomed/a.txt")]).await;

    assert!(matches!(results.as_slice(), [Err(_)]), "{results:?}");
    assert!(s3.object("doomed/a.txt").is_some(), "nothing was deleted");
}
