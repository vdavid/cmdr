//! The browser's bridge polls a volume call once: a ready answer passes through, and one
//! that would wait is a typed refusal, never a hang.

use std::future;

use cmdr_fs::volume::VolumeError;

use super::bridge::poll_once;

#[test]
fn a_ready_call_answers_on_its_first_poll() {
    assert_eq!(poll_once(future::ready(Ok::<_, VolumeError>(7))).ok(), Some(7));
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
