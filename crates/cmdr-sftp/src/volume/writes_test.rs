//! The write window's bookkeeping, against a double that behaves the way a real
//! server sometimes does.
//!
//! A server may take fewer bytes than it was offered, and the window's chunks
//! complete in whatever order they complete. Both are ordinary; both corrupt a
//! file if the offsets are wrong. The double makes them happen on purpose.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cmdr_fs::volume::VolumeError;

use super::*;

/// A remote file that lives in a `HashMap<offset, byte>`, so what lands where is
/// exactly inspectable.
#[derive(Clone)]
struct ScriptedWriter {
    bytes: Arc<Mutex<HashMap<u64, u8>>>,
    /// The most bytes any one request will take. `None` takes everything.
    takes_at_most: Option<usize>,
    /// Requests answered before one fails, `None` for a writer that never does.
    fails_after: Option<Arc<Mutex<usize>>>,
}

impl ScriptedWriter {
    fn new() -> Self {
        Self {
            bytes: Arc::new(Mutex::new(HashMap::new())),
            takes_at_most: None,
            fails_after: None,
        }
    }

    fn taking_at_most(mut self, n: usize) -> Self {
        self.takes_at_most = Some(n);
        self
    }

    fn failing_after(mut self, n: usize) -> Self {
        self.fails_after = Some(Arc::new(Mutex::new(n)));
        self
    }

    /// What the file holds, as a contiguous run from 0. Panics on a hole, which
    /// is the failure this whole module exists to catch.
    fn contents(&self) -> Vec<u8> {
        let bytes = self.bytes.lock().expect("no test panics while holding this");
        (0..bytes.len() as u64)
            .map(|offset| {
                *bytes
                    .get(&offset)
                    .unwrap_or_else(|| panic!("a hole at offset {offset}: {} bytes landed", bytes.len()))
            })
            .collect()
    }
}

impl PositionedWrite for ScriptedWriter {
    async fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<usize, VolumeError> {
        if let Some(left) = &self.fails_after {
            let mut left = left.lock().expect("no test panics while holding this");
            if *left == 0 {
                return Err(VolumeError::StorageFull {
                    message: "the scripted writer stopped here".to_string(),
                });
            }
            *left -= 1;
        }
        let take = self.takes_at_most.map_or(bytes.len(), |most| most.min(bytes.len()));
        let mut file = self.bytes.lock().expect("no test panics while holding this");
        for (index, byte) in bytes[..take].iter().enumerate() {
            file.insert(offset + index as u64, *byte);
        }
        Ok(take)
    }
}

/// The bytes a source of `len` hands over, self-describing so a misplaced run
/// is visible.
fn source_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[tokio::test]
async fn a_server_that_takes_four_bytes_at_a_time_still_gets_the_whole_chunk() {
    // ❗ The bug this catches: advancing the offset by the length ASKED for
    // rather than the length taken. Every request after the first would then
    // land past a gap, and the file would hold a hole plus a duplicate.
    let mut writer = ScriptedWriter::new().taking_at_most(4);
    let bytes = source_bytes(1000);

    let written = write_all_at(&mut writer, 0, &bytes)
        .await
        .expect("the double never fails here");

    assert_eq!(written, 1000);
    assert_eq!(writer.contents(), bytes, "the chunk landed byte for byte");
}

#[tokio::test]
async fn a_server_that_accepts_nothing_fails_instead_of_spinning() {
    // A zero-byte answer repeated is a hang, not progress, and a hung upload is
    // worse than a failed one: nothing tells the user it stopped.
    let mut writer = ScriptedWriter::new().taking_at_most(0);
    let outcome = write_all_at(&mut writer, 0, &source_bytes(16)).await;
    assert!(matches!(outcome, Err(VolumeError::IoError { .. })), "got {outcome:?}");
}

#[tokio::test]
async fn a_write_lands_where_it_was_told_to_rather_than_where_the_handle_was() {
    // N clones of one handle each write their own part at once, so the offset
    // has to travel with the request. A writer trusting a shared cursor would
    // interleave the two runs.
    let mut first = ScriptedWriter::new();
    let mut second = first.clone();
    let bytes = source_bytes(32);

    write_all_at(&mut second, 32, &bytes)
        .await
        .expect("the double never fails");
    write_all_at(&mut first, 0, &bytes)
        .await
        .expect("the double never fails");

    let expected: Vec<u8> = bytes.iter().chain(bytes.iter()).copied().collect();
    assert_eq!(first.contents(), expected);
}

#[tokio::test]
async fn a_failure_partway_through_stops_rather_than_writing_the_rest() {
    let mut writer = ScriptedWriter::new().taking_at_most(4).failing_after(2);
    let outcome = write_all_at(&mut writer, 0, &source_bytes(64)).await;
    assert!(
        matches!(outcome, Err(VolumeError::StorageFull { .. })),
        "got {outcome:?}"
    );
}
