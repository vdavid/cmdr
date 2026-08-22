//! The window's reassembly, against a server double that misbehaves on purpose.
//!
//! Every cell here is about the arithmetic: what the window does with a short
//! answer, with completions that arrive backwards, and with a file that ends
//! before its own size said it would. The real crate hazard — an engine that
//! advances its offset by the length it ASKED for — needs a real server, and
//! lives in `integration_test.rs`.
//!
//! ❗ Nothing here sleeps. Completion order is steered by how many times a read
//! yields before answering, which on the test runtime is exact rather than
//! probable.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{ChunkWindow, PositionedRead};
use cmdr_fs::volume::VolumeError;

/// The bytes every cell reads, shared with the Docker suites so the two sides
/// assert against the same shape: each 16-byte line holds its own line number, so
/// a hole or a duplicated span is visible in the output rather than merely
/// changing its length.
use crate::volume::testing::fixture_large_bytes as self_describing;

/// A server that answers positioned reads, as awkwardly as a cell asks it to.
#[derive(Clone)]
struct ScriptedServer {
    data: Arc<Vec<u8>>,
    /// The most bytes any one answer carries. A short-reading server in one
    /// number.
    max_answer: usize,
    /// How many times a read at this offset yields before answering. The test
    /// runtime polls every in-flight read once per round, so a higher count
    /// finishes later — which is how a cell orders completions exactly.
    yields: Arc<dyn Fn(u64) -> usize + Send + Sync>,
    completions: Arc<Mutex<Vec<u64>>>,
    in_flight: Arc<AtomicUsize>,
    peak_in_flight: Arc<AtomicUsize>,
}

impl ScriptedServer {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data: Arc::new(data),
            max_answer: usize::MAX,
            yields: Arc::new(|_| 0),
            completions: Arc::new(Mutex::new(Vec::new())),
            in_flight: Arc::new(AtomicUsize::new(0)),
            peak_in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn answering_at_most(mut self, bytes: usize) -> Self {
        self.max_answer = bytes;
        self
    }

    fn yielding(mut self, yields: impl Fn(u64) -> usize + Send + Sync + 'static) -> Self {
        self.yields = Arc::new(yields);
        self
    }

    /// The offsets whose answers landed, in the order they landed.
    fn completion_order(&self) -> Vec<u64> {
        self.completions
            .lock()
            .expect("a test double's lock is never poisoned")
            .clone()
    }

    fn peak_in_flight(&self) -> usize {
        self.peak_in_flight.load(Ordering::Relaxed)
    }
}

impl PositionedRead for ScriptedServer {
    async fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        let now = self.in_flight.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_in_flight.fetch_max(now, Ordering::Relaxed);
        for _ in 0..(self.yields)(offset) {
            tokio::task::yield_now().await;
        }
        let start = usize::try_from(offset).expect("a test double's file fits in memory");
        let answer = if start >= self.data.len() {
            Vec::new()
        } else {
            let end = (start + len.min(self.max_answer)).min(self.data.len());
            self.data[start..end].to_vec()
        };
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        self.completions
            .lock()
            .expect("a test double's lock is never poisoned")
            .push(offset);
        Ok(answer)
    }
}

/// Drains a window into one buffer, the way both production callers do.
async fn drain(window: &mut ChunkWindow<ScriptedServer>) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = window.next_chunk().await {
        let chunk = chunk.expect("the double never fails a read");
        out.extend_from_slice(&chunk.bytes);
        if chunk.at_eof {
            break;
        }
    }
    out
}

#[tokio::test]
async fn chunks_land_in_file_order_however_they_complete() {
    // The last chunk answers first and the first answers last, which is what a
    // window over a real link does whenever one request is unlucky.
    let data = self_describing(8 * 1024);
    let server = ScriptedServer::new(data.clone()).yielding(|offset| 8 - (offset as usize / 1024));
    let mut window = ChunkWindow::new(server.clone(), 0, data.len() as u64, 8, 1024);

    let read = drain(&mut window).await;

    assert_eq!(read, data, "the window must reassemble the file in offset order");
    let order = server.completion_order();
    assert_eq!(
        order,
        vec![7168, 6144, 5120, 4096, 3072, 2048, 1024, 0],
        "the double was told to answer backwards, so the cell is only meaningful if it did"
    );
}

#[tokio::test]
async fn a_short_answer_is_filled_rather_than_skipped() {
    // ❗ The shape of the crate hazard, in arithmetic: a server that answers with
    // less than it was asked for must leave neither a hole nor a duplicate.
    let data = self_describing(20 * 1024);
    let server = ScriptedServer::new(data.clone()).answering_at_most(100);
    let mut window = ChunkWindow::new(server, 0, data.len() as u64, 4, 4096);

    let read = drain(&mut window).await;

    assert_eq!(read.len(), data.len(), "every byte must arrive exactly once");
    assert_eq!(read, data, "and in its own place");
}

#[tokio::test]
async fn a_file_that_ends_early_reports_what_exists() {
    // A file truncated under a running copy: the window was told 10 000 bytes and
    // the server has 3 000. ❌ Never invent the difference.
    let data = self_describing(3_000);
    let server = ScriptedServer::new(data.clone());
    let mut window = ChunkWindow::new(server, 0, 10_000, 4, 1024);

    let read = drain(&mut window).await;

    assert_eq!(read, data);
}

#[tokio::test]
async fn an_empty_file_yields_nothing_and_ends() {
    let server = ScriptedServer::new(Vec::new());
    let mut window = ChunkWindow::new(server, 0, 0, 4, 1024);

    assert!(window.next_chunk().await.is_none());
}

#[tokio::test]
async fn a_range_that_starts_mid_file_reads_only_that_range() {
    // What `read_range` asks for: a `.zip`'s central directory is a window at the
    // tail, and reading one byte before or after it is a wrong answer.
    let data = self_describing(8 * 1024);
    let server = ScriptedServer::new(data.clone());
    let mut window = ChunkWindow::new(server, 5_000, 6_500, 4, 1024);

    let read = drain(&mut window).await;

    assert_eq!(read, data[5_000..6_500]);
}

#[tokio::test]
async fn the_window_keeps_its_whole_depth_in_flight() {
    // ❌ The guard against a serial loop shipped "to optimize later": at 255 KiB a
    // chunk, serial is one round trip per chunk and roughly 4 MB/s on any link
    // with latency in it.
    let data = self_describing(64 * 1024);
    // Every read yields once, so the window has a chance to fill before any of
    // them answers.
    let server = ScriptedServer::new(data.clone()).yielding(|_| 1);
    let mut window = ChunkWindow::new(server.clone(), 0, data.len() as u64, 8, 1024);

    let read = drain(&mut window).await;

    assert_eq!(read, data);
    assert_eq!(
        server.peak_in_flight(),
        8,
        "the window must keep its configured depth of reads outstanding"
    );
}
