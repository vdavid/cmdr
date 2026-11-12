//! The feed's window: what may start, what it lets go of, and what it counts.

use super::{IndexFeed, Keep, Taken};
use bytes::Bytes;
use cmdr_fs::testing::wait_until;
use cmdr_fs::volume::ListingPhase;
use rustic_core::Id;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

fn id(n: u8) -> Id {
    format!("{n:064x}").parse().expect("a 64-digit hex id")
}

fn feed(files: &[(u8, u64)], window: u64) -> (Arc<IndexFeed>, watch::Receiver<Option<ListingPhase>>) {
    let (progress, seen) = watch::channel(None);
    let files = files.iter().map(|(n, size)| (id(*n), *size)).collect();
    (
        Arc::new(IndexFeed::new(files, window, CancellationToken::new(), progress)),
        seen,
    )
}

fn done_bytes(seen: &watch::Receiver<Option<ListingPhase>>) -> u64 {
    match *seen.borrow() {
        Some(ListingPhase::LoadingIndex { done_bytes, .. }) => done_bytes,
        None => 0,
    }
}

/// Asks for `n`'s bytes on a thread of its own, the way a rustic decoder does.
fn ask(feed: &Arc<IndexFeed>, n: u8) -> std::thread::JoinHandle<Bytes> {
    let feed = Arc::clone(feed);
    std::thread::spawn(move || match feed.take(&id(n)) {
        Ok(Taken::Bytes(bytes)) => bytes,
        Ok(Taken::NotFed) => panic!("the feed holds {n} for rustic"),
        Err(err) => panic!("the feed answered {err:?}"),
    })
}

/// Waits until the feed lets a read start, the way the load's reader does.
fn wait_for_start(feed: &IndexFeed) -> super::Start {
    let mut started = None;
    wait_until(Duration::from_secs(5), "a read starts", || {
        started = feed.next_read();
        started.is_some()
    });
    started.expect("waited for it")
}

/// Reading ahead stops once the window is full; a file a decoder waits on
/// starts anyway, so a full window never stalls rustic.
#[test]
fn a_full_window_stops_reading_ahead_but_never_an_asked_file() {
    let (feed, _seen) = feed(&[(1, 100), (2, 100), (3, 100)], 150);

    let first = feed.next_read().expect("the window starts empty");
    assert_eq!(first.index, 0);
    assert!(feed.next_read().is_none(), "a second 100 bytes won't fit 150");

    let decoder = ask(&feed, 3);
    let asked = wait_for_start(&feed);
    assert_eq!(asked.index, 2, "the asked file starts past the full window");
    assert_eq!(feed.add_chunk(2, 100), Keep::Going);
    feed.deliver(2, Bytes::from(vec![3; 100]));
    assert_eq!(decoder.join().expect("the decoder"), vec![3; 100]);
}

/// ❗ A read ahead that runs past its listed size (a listing that under-reports)
/// is let go of so the window holds what it budgeted; it isn't read ahead again,
/// rustic's ask reads it whole, and progress counts its bytes once.
#[test]
fn a_read_ahead_past_its_listed_size_is_let_go_and_read_again_when_asked() {
    let (feed, seen) = feed(&[(1, 10)], 1 << 20);
    let big = 10 + super::CAP_SLACK as usize + 1;

    let start = feed.next_read().expect("reads ahead");
    assert_eq!(feed.add_chunk(start.index, 10), Keep::Going);
    assert_eq!(
        feed.add_chunk(start.index, big - 10),
        Keep::Stop,
        "let go past the slack"
    );
    assert_eq!(done_bytes(&seen), 0, "the dropped bytes are taken back");
    assert!(feed.next_read().is_none(), "never read ahead again");

    let decoder = ask(&feed, 1);
    let again = wait_for_start(&feed);
    assert_eq!(
        feed.add_chunk(again.index, big),
        Keep::Going,
        "an asked file is never let go of"
    );
    feed.deliver(again.index, Bytes::from(vec![0; big]));
    assert_eq!(decoder.join().expect("the decoder").len(), big);
    assert_eq!(done_bytes(&seen), big as u64);
}

/// A decoder waiting on a file hears a failed read instead of waiting forever.
#[test]
fn a_waiting_decoder_hears_a_failed_read() {
    let (feed, _seen) = feed(&[(1, 10)], 1 << 20);
    let _reading = feed.next_read().expect("reads ahead");
    let waiting = {
        let feed = Arc::clone(&feed);
        std::thread::spawn(move || feed.take(&id(1)).map(|_| ()))
    };

    feed.fail(cmdr_fs::volume::VolumeError::DeviceDisconnected("testshare".into()));

    assert!(matches!(
        waiting.join().expect("the decoder"),
        Err(cmdr_fs::volume::VolumeError::DeviceDisconnected(_))
    ));
}

/// ❗ A read reserves its buffer from the listing before a byte arrives. A
/// hostile or broken listing (an object store's size field) must not make it
/// ask for more than the window: past `isize::MAX` that panics, and a huge
/// valid size can abort the whole app on a failed allocation.
#[test]
fn a_read_never_reserves_more_than_the_window_whatever_the_listing_says() {
    let (feed, _seen) = feed(&[(1, u64::MAX / 2)], 1 << 20);

    let start = feed.next_read().expect("reads ahead into an empty window");

    assert_eq!(start.reserve, 1 << 20);
}
