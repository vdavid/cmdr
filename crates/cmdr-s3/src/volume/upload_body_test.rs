//! What a body hands the transport, and when it stops: a cancel, a refused
//! last piece, and a pause held or set aside.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::volume::liveness::Liveness;
use cmdr_fs::volume::scan_stop::TestScanStop;
use cmdr_fs::volume::{ScanStop, ScanStopSignal};
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use super::{BodyWatch, Halt, HaltSlot, LastPieceAsk, PauseHold, buffered_body};

const MIB: usize = 1024 * 1024;

/// What one drained body did: the bytes it handed over, whether it failed, and
/// why it stopped itself.
struct Drained {
    sent: u64,
    failed: bool,
    halted: Option<Halt>,
}

fn watch(stop: CancellationToken, pause: PauseHold, halted: &HaltSlot, last_piece: Option<LastPieceAsk>) -> BodyWatch {
    BodyWatch {
        handed: Arc::new(AtomicU64::new(0)),
        stop,
        liveness: Arc::new(Liveness::new()),
        pause,
        halted: halted.clone(),
        last_piece,
    }
}

fn unpaused() -> PauseHold {
    PauseHold::new(ScanStop::none(), Duration::from_secs(5))
}

fn pausable(signal: &Arc<TestScanStop>, hold: Duration) -> PauseHold {
    PauseHold::new(ScanStop::new(Arc::clone(signal) as Arc<dyn ScanStopSignal>), hold)
}

async fn drain(len: usize, stop: CancellationToken, pause: PauseHold, last_piece: Option<LastPieceAsk>) -> Drained {
    let halted = HaltSlot::default();
    let mut body = buffered_body(Bytes::from(vec![3u8; len]), watch(stop, pause, &halted, last_piece));
    let mut sent = 0u64;
    let mut failed = false;
    while let Some(piece) = body.next().await {
        match piece {
            Ok(bytes) => sent += bytes.len() as u64,
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    Drained {
        sent,
        failed,
        halted: halted.get(),
    }
}

/// A go-ahead channel that answers every last-piece ask with `go`.
fn answering(go: bool) -> LastPieceAsk {
    let (ask, mut asks) = tokio::sync::mpsc::channel::<tokio::sync::oneshot::Sender<bool>>(1);
    tokio::spawn(async move {
        while let Some(reply) = asks.recv().await {
            let _ = reply.send(go);
        }
    });
    ask
}

#[tokio::test]
async fn a_body_goes_out_in_pieces_and_can_be_sent_again() {
    let bytes = Bytes::from(vec![3u8; 2 * MIB + 5]);
    for _ in 0..2 {
        let halted = HaltSlot::default();
        let w = watch(CancellationToken::new(), unpaused(), &halted, None);
        let handed = Arc::clone(&w.handed);
        let pieces: Vec<_> = buffered_body(bytes.clone(), w).collect().await;
        assert_eq!(pieces.len(), 3);
        assert_eq!(handed.load(Ordering::Relaxed), bytes.len() as u64);
    }
}

/// ❗ The last piece waits for the upload's go-ahead, which is where a Cancel
/// arrives: refused, the body fails short of its length and S3 publishes
/// nothing.
#[tokio::test]
async fn a_refused_last_piece_never_goes_out() {
    let drained = drain(2 * MIB, CancellationToken::new(), unpaused(), Some(answering(false))).await;
    assert!(drained.failed);
    assert_eq!(drained.sent, MIB as u64, "everything but the last piece went out");
}

#[tokio::test]
async fn an_allowed_last_piece_goes_out() {
    let drained = drain(2 * MIB, CancellationToken::new(), unpaused(), Some(answering(true))).await;
    assert!(!drained.failed);
    assert_eq!(drained.sent, 2 * MIB as u64);
}

#[tokio::test]
async fn a_cancelled_body_sends_nothing_more() {
    let stop = CancellationToken::new();
    stop.cancel();
    let drained = drain(2 * MIB, stop, unpaused(), None).await;
    assert_eq!((drained.sent, drained.failed), (0, true));
}

/// A pause that ends within the hold costs nothing: the body carries on where
/// it stood.
#[tokio::test(start_paused = true)]
async fn a_pause_shorter_than_the_hold_lets_the_body_carry_on() {
    let signal = TestScanStop::new();
    signal.pause();
    let resumer = Arc::clone(&signal);
    tokio::spawn(async move {
        // allowed-test-sleep: virtual time on a paused clock; a pause of 1 s
        // against a 5 s hold is the subject.
        tokio::time::sleep(Duration::from_secs(1)).await;
        resumer.resume();
    });
    let drained = drain(
        2 * MIB,
        CancellationToken::new(),
        pausable(&signal, Duration::from_secs(5)),
        None,
    )
    .await;
    assert!(!drained.failed);
    assert_eq!(drained.sent, 2 * MIB as u64);
    assert_eq!(drained.halted, None);
}

/// A pause that outlasts the hold sets the request aside before the server's
/// idle timeout can fail it.
#[tokio::test(start_paused = true)]
async fn a_pause_past_the_hold_sets_the_body_aside() {
    let signal = TestScanStop::new();
    signal.pause();
    let drained = drain(
        2 * MIB,
        CancellationToken::new(),
        pausable(&signal, Duration::from_secs(5)),
        None,
    )
    .await;
    assert!(drained.failed);
    assert_eq!(drained.sent, 0, "nothing goes out while paused");
    assert_eq!(drained.halted, Some(Halt::SetAside));
}

/// A Cancel that lands while the body is paused stops it as a stop, ❌ never
/// as a set-aside that would be sent again.
#[tokio::test(start_paused = true)]
async fn a_cancel_while_paused_stops_the_body() {
    let signal = TestScanStop::new();
    signal.pause();
    let stopper = Arc::clone(&signal);
    tokio::spawn(async move {
        // allowed-test-sleep: virtual time on a paused clock; the cancel has to
        // land inside the hold.
        tokio::time::sleep(Duration::from_secs(1)).await;
        stopper.stop();
    });
    let drained = drain(
        2 * MIB,
        CancellationToken::new(),
        pausable(&signal, Duration::from_secs(5)),
        None,
    )
    .await;
    assert!(drained.failed);
    assert_eq!(drained.halted, Some(Halt::Stopped));
}
