//! How much one `read_next_window` call takes from a download, against a
//! scripted one. Real USB streams a held-open `GetObject` as 64 KiB reads, but the
//! virtual device hands a whole object over as ONE chunk, so the bound that keeps
//! a held-open transfer's memory flat is only observable here. The same goes for
//! a time-paced window longer than the call limit, which the virtual device's
//! instant transfers never produce.

use std::collections::VecDeque;

use bytes::Bytes;

use super::file_ops::{ChunkSource, pull_window};

const CHUNK: usize = 64 * 1024;
const WINDOW: usize = 4 * CHUNK;

/// A download that delivers `chunks` in order. `window_ends` marks the chunk
/// counts after which the session is free (a windowed read's window boundaries);
/// empty means one held-open transfer, busy until the last chunk is followed by
/// the end.
struct Scripted {
    chunks: VecDeque<Bytes>,
    delivered: usize,
    window_ends: Vec<usize>,
    ended: bool,
}

impl Scripted {
    fn held_open(chunk_count: usize) -> Self {
        Self::windowed(chunk_count, Vec::new())
    }

    fn windowed(chunk_count: usize, window_ends: Vec<usize>) -> Self {
        Self {
            chunks: (0..chunk_count).map(|i| Bytes::from(vec![i as u8; CHUNK])).collect(),
            delivered: 0,
            window_ends,
            ended: false,
        }
    }
}

impl ChunkSource for Scripted {
    async fn next_chunk(&mut self) -> Option<Result<Bytes, mtp_rs::Error>> {
        match self.chunks.pop_front() {
            Some(chunk) => {
                self.delivered += 1;
                Some(Ok(chunk))
            }
            None => {
                self.ended = true;
                None
            }
        }
    }

    fn holds_session(&self) -> bool {
        !self.ended && !self.window_ends.contains(&self.delivered)
    }
}

#[tokio::test]
async fn a_held_open_transfer_comes_back_one_window_at_a_time() {
    let mut source = Scripted::held_open(10);
    let mut sizes = Vec::new();
    loop {
        let window = pull_window(&mut source, WINDOW, WINDOW).await.expect("scripted chunks");
        if window.is_empty() {
            break;
        }
        sizes.push(window.len());
        if !source.ended {
            assert!(
                source.holds_session(),
                "mid-transfer the session stays busy between calls"
            );
        }
    }
    assert_eq!(
        sizes,
        vec![WINDOW, WINDOW, 2 * CHUNK],
        "each call stops at the window, never buffering the rest of the file"
    );
    assert!(!source.holds_session(), "EOF frees the session");
}

#[tokio::test]
async fn a_windowed_read_returns_at_each_window_boundary() {
    // Windows of three chunks: the session frees after chunks 3, 6, and 9.
    let mut source = Scripted::windowed(9, vec![3, 6, 9]);
    for _ in 0..3 {
        let window = pull_window(&mut source, WINDOW, WINDOW).await.expect("scripted chunks");
        assert_eq!(window.len(), 3 * CHUNK, "a call ends where the device's window ends");
        assert!(!source.holds_session(), "between windows the session is free");
    }
    let eof = pull_window(&mut source, WINDOW, WINDOW).await.expect("scripted chunks");
    assert!(eof.is_empty(), "EOF reads as an empty window");
}

#[tokio::test]
async fn a_window_longer_than_the_call_limit_spans_calls_and_keeps_the_session() {
    // A time-paced window on a fast link: ten chunks in one window, then a
    // two-chunk one. Each call stays within the limit, the session stays busy
    // until the long window ends, and the next window starts a fresh call.
    let mut source = Scripted::windowed(12, vec![10, 12]);
    let mut calls = Vec::new();
    loop {
        let window = pull_window(&mut source, WINDOW, WINDOW).await.expect("scripted chunks");
        if window.is_empty() {
            break;
        }
        calls.push((window.len(), source.holds_session()));
    }
    assert_eq!(
        calls,
        vec![(WINDOW, true), (WINDOW, true), (2 * CHUNK, false), (2 * CHUNK, false),],
        "the call limit bounds memory; the window, not the call, decides when the device is free"
    );
}

#[tokio::test]
async fn a_short_window_hands_back_no_more_memory_than_it_holds() {
    // The capacity is a guess at the call limit; a time-paced window that ends
    // after one chunk must not carry the rest of that guess along with it.
    let mut source = Scripted::windowed(1, vec![1]);
    let window = pull_window(&mut source, 64 * CHUNK, 64 * CHUNK)
        .await
        .expect("scripted chunks");
    assert_eq!(window.len(), CHUNK);
    assert!(
        window.capacity() < 2 * CHUNK,
        "capacity {} kept for a {}-byte window",
        window.capacity(),
        window.len()
    );
}

#[tokio::test]
async fn an_error_mid_window_surfaces_instead_of_a_short_window() {
    struct Failing(bool);
    impl ChunkSource for Failing {
        async fn next_chunk(&mut self) -> Option<Result<Bytes, mtp_rs::Error>> {
            if self.0 {
                return Some(Err(mtp_rs::Error::Cancelled));
            }
            self.0 = true;
            Some(Ok(Bytes::from(vec![0; CHUNK])))
        }

        fn holds_session(&self) -> bool {
            true
        }
    }
    let outcome = pull_window(&mut Failing(false), WINDOW, WINDOW).await;
    assert!(
        matches!(outcome, Err(mtp_rs::Error::Cancelled)),
        "the bytes before the failure are not handed out as a window"
    );
}
