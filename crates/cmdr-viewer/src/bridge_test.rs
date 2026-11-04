//! The browser's bridge polls a volume call once: a ready answer passes through, and one
//! that would wait is a typed refusal, never a hang.

use std::future;

use cmdr_fs::volume::VolumeError;

use super::bridge::poll_once;

#[test]
fn a_ready_call_answers_on_its_first_poll() {
    assert_eq!(poll_once(future::ready(Ok::<_, VolumeError>(7))).ok(), Some(7));
    // The poll tells a paced backend it can't wait, and only for its own length.
    let can_wait = poll_once(future::poll_fn(|_| {
        std::task::Poll::Ready(Ok::<_, VolumeError>(cmdr_fs::rt::caller_can_wait()))
    }));
    assert_eq!(can_wait.ok(), Some(false));
    assert!(cmdr_fs::rt::caller_can_wait());
    assert!(matches!(
        poll_once(future::ready(Err::<(), _>(VolumeError::NotFound("/x".into())))),
        Err(VolumeError::NotFound(_))
    ));
}

#[test]
fn a_call_that_would_wait_is_refused() {
    assert!(matches!(
        poll_once(future::pending::<Result<(), VolumeError>>()),
        Err(VolumeError::NotSupported)
    ));
}
