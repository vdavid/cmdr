//! The streamed reads whose last byte is out and whose CLOSE isn't answered yet.
//!
//! A streamed read ends at its last byte, ahead of its CLOSE's answer
//! (`read_stream.rs`), and it rides the transfer session while a delete rides the
//! main one: two TCP connections, so nothing orders that CLOSE against the delete.
//! A server takes a file with a handle still open as DELETE PENDING, which keeps it
//! listed and keeps its folder refusing removal until the handle closes (verified on
//! the Docker guest share, Samba 4.20.6, by holding a read open across a delete and
//! a listing, 2026-10-31). So `delete_impl` waits here for that one path: a delete
//! that returned means the file is gone, which is what an undo's "is this folder
//! empty?" reads next (`DirNotEmpty`, #638).
//!
//! Only a read that delivered its last byte registers, so a delete never waits on a
//! download still moving bytes, and a path nobody is closing costs one map lookup.
//! A read stopped early queues its CLOSE on drop with nobody to wait for its answer,
//! and stays out of this.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use tokio::sync::Notify;

/// One share's closing reads, counted per SMB path: two readers of one file can be
/// closing at once.
#[derive(Default)]
pub(super) struct ClosingReads {
    paths: Mutex<HashMap<String, usize>>,
    /// Wakes the deletes waiting on a path when any path's last CLOSE lands.
    settled: Notify,
}

impl ClosingReads {
    /// Registers a read of `smb_path` whose CLOSE is on its way, until the returned
    /// guard drops. Called before the last byte is handed out, so a consumer that
    /// has every byte can't reach a delete ahead of this.
    pub(super) fn begin(self: &Arc<Self>, smb_path: &str) -> ClosingRead {
        *self.paths.lock_ignore_poison().entry(smb_path.to_string()).or_default() += 1;
        ClosingRead {
            reads: Arc::clone(self),
            smb_path: smb_path.to_string(),
        }
    }

    /// Returns once no read of `smb_path` is waiting on its CLOSE. Bounded by smb2,
    /// which bounds every wait itself: a dead connection fails the CLOSE, and the
    /// read's task then drops its guard.
    pub(super) async fn settled(&self, smb_path: &str) {
        loop {
            // Armed before the check, so a guard dropping between the two can't be
            // missed.
            let notified = self.settled.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !self.paths.lock_ignore_poison().contains_key(smb_path) {
                return;
            }
            notified.await;
        }
    }
}

/// A read's stake in [`ClosingReads`], held by its task until the CLOSE's answer.
pub(super) struct ClosingRead {
    reads: Arc<ClosingReads>,
    smb_path: String,
}

impl Drop for ClosingRead {
    fn drop(&mut self) {
        let mut paths = self.reads.paths.lock_ignore_poison();
        let last = match paths.get_mut(&self.smb_path) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => {
                paths.remove(&self.smb_path);
                true
            }
            None => false,
        };
        drop(paths);
        if last {
            self.reads.settled.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_path_nobody_is_closing_is_settled_at_once() {
        let reads = Arc::new(ClosingReads::default());
        let other = reads.begin("other.bin");
        reads.settled("page.png").await;
        drop(other);
    }

    #[tokio::test]
    async fn a_delete_waits_for_every_reader_of_its_path() {
        let reads = Arc::new(ClosingReads::default());
        let first = reads.begin("page.png");
        let second = reads.begin("page.png");
        let settled = reads.settled("page.png");
        tokio::pin!(settled);

        drop(first);
        tokio::select! {
            biased;
            () = &mut settled => panic!("the second reader still holds the file open"),
            () = std::future::ready(()) => {}
        }

        drop(second);
        settled.await;
        assert!(reads.paths.lock_ignore_poison().is_empty(), "nothing stays registered");
    }
}
