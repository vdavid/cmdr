//! The bytes an upload sends, as the [`UploadBody`] the transport streams, and
//! how a body answers the operation's pause.
//!
//! ❗ **Every upload body is already in memory** ([`buffered_body`]): a PUT's
//! whole object (at most one part, 69 MiB) or one part of a multipart upload.
//! So any request can be sent again from the start, and a source that turns out
//! shorter or longer than it promised is caught before a byte goes out.
//!
//! ❗ **A pause stops the body between pieces** ([`PauseHold`]). The engine's
//! pause parks the SOURCE stream, which a buffered upload drained long before
//! its bytes go out, so the body asks the operation's own signal
//! (`VolumeReadStream::stop_signal`) before every piece. A pause that ends
//! within [`PAUSE_HOLD`](super::writes::PAUSE_HOLD) costs nothing; one that
//! outlasts it sets the request aside ([`Halt::SetAside`]) before the server's
//! idle timeout can fail it, and the sender sends it again whole once resumed.
//!
//! ❗ **A PUT's last piece waits for a go-ahead** ([`LastPieceAsk`]): the upload
//! asks its progress callback, which is where a user's Cancel arrives, before
//! the byte that would let S3 publish goes out. Without it, a cancel landing
//! between two progress ticks lost to a fast finish and published the object
//! the user had just called off.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::volume::ScanStop;
use cmdr_fs::volume::liveness::Liveness;
use tokio_util::sync::CancellationToken;

use crate::transport::UploadBody;

/// How much of a buffered body goes out per piece: small enough that progress,
/// the silence watch, and a pause hear about it every few hundred milliseconds
/// even on a slow link.
const PIECE: usize = 1024 * 1024;

/// How a body asks whether its last piece may go out: it sends a reply slot,
/// and the upload answers `true` to go on, `false` to stop. A dropped slot or a
/// closed channel is a stop.
pub(super) type LastPieceAsk = tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<bool>>;

/// Why a body stopped on its own side, for the sender to read once the request
/// is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Halt {
    /// A pause outlasted the hold: the request is dropped, to go again whole
    /// once resumed.
    SetAside = 1,
    /// The operation stopped (Cancel) while the body was paused.
    Stopped = 2,
}

/// Where a body leaves its [`Halt`]. One per request.
#[derive(Clone, Default)]
pub(super) struct HaltSlot(Arc<AtomicU8>);

impl HaltSlot {
    fn set(&self, halt: Halt) {
        self.0.store(halt as u8, Ordering::Relaxed);
    }

    /// Why the body stopped itself, `None` when it didn't.
    pub(super) fn get(&self) -> Option<Halt> {
        match self.0.load(Ordering::Relaxed) {
            1 => Some(Halt::SetAside),
            2 => Some(Halt::Stopped),
            _ => None,
        }
    }
}

/// The operation's pause as an upload honours it: no request starts while
/// paused, and one in flight holds for `hold` before it's set aside.
#[derive(Clone, Debug)]
pub(super) struct PauseHold {
    stop: ScanStop,
    hold: Duration,
}

/// What a body does after asking [`PauseHold::hold`].
#[derive(Debug, PartialEq, Eq)]
enum Holding {
    Go,
    SetAside,
    Stopped,
}

impl PauseHold {
    pub(super) fn new(stop: ScanStop, hold: Duration) -> Self {
        Self { stop, hold }
    }

    /// Parks while the operation is paused, before a request goes out.
    /// Answers `true` when the operation stopped (Cancel) instead.
    pub(super) async fn stopped(&self) -> bool {
        self.stop.should_stop().await
    }

    /// Between two pieces of a body in flight: on at once when not paused, on
    /// after a pause shorter than the hold, else set aside.
    async fn hold(&self) -> Holding {
        tokio::select! {
            biased;
            stopped = self.stop.should_stop() => if stopped { Holding::Stopped } else { Holding::Go },
            () = tokio::time::sleep(self.hold) => Holding::SetAside,
        }
    }
}

/// What one buffered body needs besides its bytes.
pub(super) struct BodyWatch {
    /// This attempt's bytes onto the wire, for progress.
    pub handed: Arc<AtomicU64>,
    /// Cancels the body at its next piece.
    pub stop: CancellationToken,
    pub liveness: Arc<Liveness>,
    pub pause: PauseHold,
    pub halted: HaltSlot,
    /// A PUT's go-ahead for its last piece; `None` for a part, which publishes
    /// nothing.
    pub last_piece: Option<LastPieceAsk>,
}

/// Whether the upload lets the last piece go out.
async fn last_piece_may_go(ask: &LastPieceAsk) -> bool {
    let (reply, answer) = tokio::sync::oneshot::channel();
    if ask.send(reply).await.is_err() {
        return false;
    }
    answer.await.unwrap_or(false)
}

/// `bytes` as a request body, a piece at a time, counting each piece handed
/// over into `watch.handed`. Cheap to build again from the same `bytes` for a
/// second attempt.
///
/// Fails the body (which aborts the request) when `watch.stop` is cancelled,
/// when a pause outlasts its hold or ends in a stop (left in `watch.halted`),
/// and when the last piece's go-ahead says no.
pub(super) fn buffered_body(bytes: Bytes, watch: BodyWatch) -> UploadBody {
    let watch = Arc::new(watch);
    Box::pin(futures_util::stream::unfold(bytes, move |mut rest| {
        let watch = Arc::clone(&watch);
        async move {
            if watch.stop.is_cancelled() {
                return Some((Err(std::io::Error::other("cancelled")), Bytes::new()));
            }
            if rest.is_empty() {
                return None;
            }
            match watch.pause.hold().await {
                Holding::Go => {}
                Holding::SetAside => {
                    watch.halted.set(Halt::SetAside);
                    return Some((Err(std::io::Error::other("set aside for a pause")), Bytes::new()));
                }
                Holding::Stopped => {
                    watch.halted.set(Halt::Stopped);
                    return Some((Err(std::io::Error::other("stopped while paused")), Bytes::new()));
                }
            }
            let piece = rest.split_to(PIECE.min(rest.len()));
            // ❗ The piece that lets S3 publish goes out only with the
            // upload's go-ahead.
            if rest.is_empty()
                && let Some(ask) = &watch.last_piece
                && !last_piece_may_go(ask).await
            {
                return Some((Err(std::io::Error::other("cancelled")), Bytes::new()));
            }
            watch.handed.fetch_add(piece.len() as u64, Ordering::Relaxed);
            // A piece handed over is the server draining the socket, so it's
            // there: a server that answers nothing until the body is in would
            // otherwise look silent for the whole upload.
            watch.liveness.heard();
            Some((Ok(piece), rest))
        }
    }))
}

#[cfg(test)]
#[path = "upload_body_test.rs"]
mod upload_body_test;
