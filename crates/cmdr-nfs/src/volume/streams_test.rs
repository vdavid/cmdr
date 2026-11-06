//! The window's reassembly against a double that short-reads and answers
//! backwards on purpose.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;

use super::*;

#[derive(Clone)]
struct Scripted {
    data: Arc<Vec<u8>>,
    /// At most this many bytes per answer.
    short: usize,
    /// Later offsets answer sooner, so completions arrive out of order.
    backwards: bool,
}

impl PositionedRead for Scripted {
    async fn read_at(&self, offset: u64, len: u32) -> Result<(Bytes, bool), VolumeError> {
        if self.backwards {
            let delay = 50u64.saturating_sub(offset / 1024);
            // allowed-test-sleep: the fake latency that makes completions arrive backwards.
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let from = (offset as usize).min(self.data.len());
        let to = (from + (len as usize).min(self.short)).min(self.data.len());
        Ok((Bytes::copy_from_slice(&self.data[from..to]), to == self.data.len()))
    }
}

async fn drain(window: &mut ChunkWindow<Scripted>) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = window.next_chunk().await {
        let chunk = chunk.unwrap();
        out.extend_from_slice(&chunk.bytes);
        if chunk.at_eof {
            break;
        }
    }
    out
}

fn data(len: usize) -> Arc<Vec<u8>> {
    Arc::new((0..len).map(|index| (index % 251) as u8).collect())
}

#[tokio::test(start_paused = true)]
async fn out_of_order_completions_come_back_in_file_order() {
    let data = data(40 * 1024 + 3);
    let reader = Scripted {
        data: Arc::clone(&data),
        short: usize::MAX,
        backwards: true,
    };
    let mut window = chunk_window(reader, 0, data.len() as u64, 8, 1024);
    assert!(drain(&mut window).await == *data);
}

#[tokio::test]
async fn a_short_answer_mid_file_is_asked_again_from_where_it_stopped() {
    let data = data(10_000);
    let reader = Scripted {
        data: Arc::clone(&data),
        short: 333,
        backwards: false,
    };
    let mut window = chunk_window(reader, 0, data.len() as u64, 4, 4096);
    assert!(drain(&mut window).await == *data);
}

#[tokio::test]
async fn a_file_that_shrank_ends_where_the_server_says() {
    let data = data(2_500);
    let reader = Scripted {
        data: Arc::clone(&data),
        short: usize::MAX,
        backwards: false,
    };
    // The open said 10,000 bytes; the file now holds 2,500.
    let mut window = chunk_window(reader, 0, 10_000, 4, 1024);
    assert!(drain(&mut window).await == *data);
}

#[test]
fn the_chunk_is_the_servers_preference_within_its_maximum_and_our_record_cap() {
    let info = |rtpref, rtmax| FsInfo {
        rtmax,
        rtpref,
        wtmax: 0,
        wtpref: 0,
        dtpref: 0,
        maxfilesize: 0,
        time_delta: NfsTime::default(),
        properties: 0,
    };
    assert_eq!(chunk_bytes(&info(131_072, 1_048_576)), 131_072);
    assert_eq!(chunk_bytes(&info(4 << 20, 4 << 20)), MAX_PAYLOAD as usize);
    assert_eq!(chunk_bytes(&info(65_536, 32_768)), 32_768);
    assert_eq!(chunk_bytes(&info(0, 0)), 65_536);
}
