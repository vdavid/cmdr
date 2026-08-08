//! Self-tests for the fault injector. A test double that lies about WHEN it
//! fails is worse than no double at all: every suite built on it would be
//! asserting against a scenario that never happened.

use super::*;
use crate::file_system::volume::InMemoryVolume;

fn io(message: &str) -> VolumeError {
    VolumeError::IoError {
        message: message.to_string(),
        raw_os_error: None,
    }
}

async fn seeded() -> Arc<InMemoryVolume> {
    let vol = Arc::new(InMemoryVolume::new("Faulty"));
    vol.create_directory(Path::new("/dir")).await.unwrap();
    vol.create_file(Path::new("/dir/a.txt"), b"AAA").await.unwrap();
    vol.create_file(Path::new("/dir/b.txt"), b"BBB").await.unwrap();
    vol
}

/// An unarmed wrapper is invisible: every method still answers what the inner
/// volume answers.
#[tokio::test]
async fn an_unarmed_wrapper_forwards_everything() {
    let inner = seeded().await;
    let faulty: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&inner)).arc();

    assert_eq!(faulty.name(), inner.name());
    assert_eq!(faulty.root(), inner.root());
    assert!(faulty.is_directory(Path::new("/dir")).await.unwrap());
    assert!(faulty.exists(Path::new("/dir/a.txt")).await);
    assert_eq!(faulty.list_directory(Path::new("/dir"), None).await.unwrap().len(), 2);
    assert_eq!(
        faulty.get_metadata(Path::new("/dir/a.txt")).await.unwrap().size,
        Some(3)
    );
}

/// The Nth call fails and the (N-1)th doesn't. Off by one here would put every
/// downstream suite's failure in the wrong place.
#[tokio::test]
async fn the_armed_call_fails_and_the_one_before_it_does_not() {
    let inner = seeded().await;
    let faulty: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(FaultyOp::IsDirectory, 2, io("stat gave up"))
        .arc();

    assert!(
        faulty.is_directory(Path::new("/dir")).await.is_ok(),
        "the first call must forward"
    );
    assert!(
        matches!(faulty.is_directory(Path::new("/dir")).await, Err(VolumeError::IoError { .. })),
        "the second call is the armed one"
    );
    assert!(
        faulty.is_directory(Path::new("/dir")).await.is_ok(),
        "the third call forwards again — one armed fault means one failure"
    );
}

/// Counting is per-operation: calls to one method never advance another's
/// counter, so a driver that lists before it writes can't disarm the write
/// fault by accident.
#[tokio::test]
async fn each_operation_counts_its_own_calls() {
    let inner = seeded().await;
    let faulty: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(FaultyOp::Delete, 1, io("delete refused"))
        .arc();

    // Three unrelated calls first.
    let _ = faulty.list_directory(Path::new("/dir"), None).await;
    let _ = faulty.get_metadata(Path::new("/dir/a.txt")).await;
    let _ = faulty.is_directory(Path::new("/dir")).await;

    assert!(
        matches!(faulty.delete(Path::new("/dir/a.txt")).await, Err(VolumeError::IoError { .. })),
        "the FIRST delete is the armed one, whatever else ran before it"
    );
    assert!(
        inner.exists(Path::new("/dir/a.txt")).await,
        "a failed delete removes nothing"
    );
}

/// Both spellings of an operation share one counter, because a driver picking
/// `delete_with_cancel` over `delete` is an implementation detail the test
/// shouldn't have to know.
#[tokio::test]
async fn the_cancel_flavored_spelling_shares_the_plain_ones_counter() {
    let inner = seeded().await;
    let faulty: Arc<dyn Volume> = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(FaultyOp::Delete, 2, io("delete refused"))
        .arc();

    assert!(faulty.delete(Path::new("/dir/a.txt")).await.is_ok());
    assert!(matches!(
        faulty.delete_with_cancel(Path::new("/dir/b.txt"), None).await,
        Err(VolumeError::IoError { .. })
    ));
}

/// A lied-about type is what `is_directory` and the listing report, while the
/// entry keeps behaving as what it really is. That gap IS the fault class this
/// whole area exists to defend against: a directory answered as a file streams
/// as one and picks the destructive cleanup branch.
#[tokio::test]
async fn a_lied_about_type_is_what_the_stat_reports() {
    let wrapper = FaultyVolume::wrapping(seeded().await);
    let vol = Arc::clone(wrapper.inner());
    assert!(vol.is_directory(Path::new("/dir")).await.unwrap());

    // Reached through the wrapper, so a grid cell can arm a fault AND lie about
    // a type on the same volume.
    wrapper.inner().set_reported_type(Path::new("/dir"), false);

    assert!(
        !vol.is_directory(Path::new("/dir")).await.unwrap(),
        "the stat must report the lie"
    );
    assert!(
        !vol.get_metadata(Path::new("/dir")).await.unwrap().is_directory,
        "and so must the metadata"
    );
    assert_eq!(
        vol.list_directory(Path::new("/dir"), None).await.unwrap().len(),
        2,
        "while the children are still really there — the lie is the metadata, not the store"
    );
}
