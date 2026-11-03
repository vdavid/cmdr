//! A failed write's abort: one abort answers the write at once, and the rounds
//! that confirm it (list, abort again while a late part brings the upload back)
//! run afterwards, ending early when the volume is gone.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::time::Duration;

use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{Volume, VolumeError, VolumeReadStream, WriteMode};

use super::fake_s3::FakeS3;
use super::multipart_upload::{UploadEnd, settle_abort};
use super::testing::{BytesSource, wait_for_the_ledger_to_settle};

const MIB: usize = 1024 * 1024;

/// Deadline for a wait on a real condition: generous, since a healthy run
/// meets it at once.
const WAIT: Duration = Duration::from_secs(5);

/// ❗ The write answers once its one abort is in, ❌ never after the confirming
/// listing: a server slow to list held every cancelled or failed upload up to
/// four rounds before. The listing comes afterwards and still settles it.
///
/// No wall clock: the listing is held until the write has answered, so the
/// ordering is the assertion. A write that waited on its listing would never
/// answer, which nextest's per-test cap reports as a hang by this name. A 5 s
/// deadline here timed out under a loaded `pnpm check` though the write never
/// waits (#896: starved to one tokio worker beside 400 spinning threads, it
/// answered in 2–4 s with the listing held for 20 s).
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_upload_answers_before_its_abort_is_confirmed() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let volume = s3.volume();
    volume.set_part_floor(5 * MIB as u64);
    s3.taken_before_next_part("raced.bin");
    s3.hold_upload_listings();
    let source = BytesSource::new(vec![7u8; 12 * MIB]);
    let length = source.total_size();

    let outcome = volume
        .write_from_stream(
            &volume.root().join("raced.bin"),
            WriteMode::CreateNew,
            length,
            Box::new(source),
            &|_| ControlFlow::Continue(()),
        )
        .await;
    assert!(matches!(outcome, Err(VolumeError::AlreadyExists(_))), "got {outcome:?}");
    assert_eq!(s3.open_uploads(), 0, "the one abort already ended the upload");

    // The rounds' confirming listing reaches the server and waits there, so
    // the write above answered with its confirmation still to come.
    wait_until_async(WAIT, "the confirming listing to be held", || {
        s3.held_upload_listings() > 0
    })
    .await;
    s3.release_upload_listings();
    // The background rounds confirm it, and only then forget its record.
    wait_for_the_ledger_to_settle(&volume, "raced").await;
}

/// A server, scripted: each abort's answer and each listing's, in turn. `None`
/// in place of an answer is a volume no longer there to ask.
#[derive(Default)]
struct Scripted {
    aborts: RefCell<VecDeque<Option<bool>>>,
    listings: RefCell<VecDeque<Option<Option<bool>>>>,
    aborts_sent: Cell<usize>,
}

impl UploadEnd for Scripted {
    async fn abort(&self) -> Option<bool> {
        self.aborts_sent.set(self.aborts_sent.get() + 1);
        self.aborts.borrow_mut().pop_front().unwrap_or(Some(true))
    }

    async fn still_listed(&self) -> Option<Option<bool>> {
        self.listings.borrow_mut().pop_front().unwrap_or(Some(Some(false)))
    }
}

/// A part landing after the first abort brings the upload back: the rounds
/// abort again, and a listing without it settles it. The paused clock stands
/// in for the gap between rounds.
#[tokio::test(start_paused = true)]
async fn an_upload_a_late_part_brought_back_is_aborted_again() {
    let server = Scripted {
        listings: RefCell::new(VecDeque::from([Some(Some(true)), Some(Some(false))])),
        ..Scripted::default()
    };

    assert!(settle_abort(&server, Some(true)).await, "settled on the second round");
    assert_eq!(
        server.aborts_sent.get(),
        1,
        "one more abort after the first one sent inline"
    );
}

/// ❗ A volume retired or disconnected while the rounds wait ends them at once,
/// ❌ never asked again through a session that's gone: the record stays for
/// the next connect's sweep.
#[tokio::test(start_paused = true)]
async fn rounds_whose_volume_is_gone_stop_and_leave_the_upload_for_the_sweep() {
    let server = Scripted {
        listings: RefCell::new(VecDeque::from([Some(Some(true))])),
        aborts: RefCell::new(VecDeque::from([None])),
        ..Scripted::default()
    };

    assert!(!settle_abort(&server, Some(true)).await, "not settled");
    assert_eq!(server.aborts_sent.get(), 1, "and nothing asked after the volume went");
}
