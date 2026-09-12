//! Deadlines for the eject pipeline's blocking steps, so no eject can hang
//! forever.
//!
//! A disk image whose backing FILE lives on a hung SMB share looks like a local
//! ejectable volume, yet its `statfs`/NSURL lookup and its index stop can block
//! for 30–120 s, or for good. Without a deadline its flight never lands, and the
//! in-flight join strands every later click on a spinner (`in_flight`).
//! `volume/DETAILS.md` § "Eject" has the deadlines and why.

use std::time::Duration;

use super::{EjectError, EjectStep};

/// How long the ejectability lookup gets (`statfs` + NSURL on macOS, the mount
/// list on Linux). It's a read, but one that may wake a sleeping disk, so it sits
/// above the 2 s read tier.
pub(super) const EJECTABILITY_CHECK_DEADLINE: Duration = Duration::from_secs(5);

/// How long stopping the drive's index gets. It drains the index writer, which
/// the index documents as taking seconds.
pub(super) const INDEX_STOP_DEADLINE: Duration = Duration::from_secs(15);

/// How long a device provider's eject gets. MTP closes its session when the last
/// handle drops, which a wedged phone can stall.
pub(super) const DEVICE_EJECT_DEADLINE: Duration = Duration::from_secs(15);

/// Runs `work` under `deadline`, answering [`EjectError::NotResponding`] for
/// `step` when it doesn't finish in time.
///
/// `work` runs in its own task and the deadline races its join handle
/// (`deadline::timeout_detached_typed`), so expiry DETACHES it: a blocking thread
/// inside can't be cancelled and runs on to its own end, which is logged at
/// `warn`. ❗ The caller must not run anything that depended on the step: it
/// returns this error. `work` is a future rather than a closure so a test can hand
/// in one that never resolves and let a paused clock run out the deadline.
pub(super) async fn within_deadline<T: Send + 'static>(
    step: EjectStep,
    volume_id: &str,
    deadline: Duration,
    work: impl Future<Output = T> + Send + 'static,
) -> Result<T, EjectError> {
    crate::deadline::timeout_detached_typed(
        deadline,
        || {
            log::warn!(
                target: "eject",
                "{step} for {volume_id} didn't finish within {} s; answering NotResponding and leaving its task to run on",
                deadline.as_secs()
            );
            EjectError::NotResponding { step }
        },
        |detail| EjectError::Unexpected {
            detail: format!("{step}: {detail}"),
        },
        async move { Ok(work.await) },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::super::{EjectError, EjectStep, ejecting_volume_ids, in_flight, stop_index_then_unmount};
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test(start_paused = true)]
    async fn a_stuck_ejectability_check_answers_not_responding_at_its_deadline() {
        // A disk image whose backing FILE lives on a hung SMB share looks like a
        // local ejectable volume, and its statfs/NSURL lookup can block for good.
        let started = tokio::time::Instant::now();
        let result = within_deadline(
            EjectStep::EjectabilityCheck,
            "vol-cmdr-test-stuck-check",
            EJECTABILITY_CHECK_DEADLINE,
            std::future::pending::<bool>(),
        )
        .await;

        assert!(
            matches!(
                result,
                Err(EjectError::NotResponding {
                    step: EjectStep::EjectabilityCheck
                })
            ),
            "a stuck check must not be guessed as `NotEjectable`, got {result:?}"
        );
        assert_eq!(started.elapsed(), EJECTABILITY_CHECK_DEADLINE);
    }

    #[tokio::test(start_paused = true)]
    async fn an_answer_inside_the_deadline_passes_through() {
        let result = within_deadline(
            EjectStep::EjectabilityCheck,
            "vol-cmdr-test-quick-check",
            EJECTABILITY_CHECK_DEADLINE,
            async { true },
        )
        .await;
        assert!(matches!(result, Ok(true)), "got {result:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_stuck_index_stop_never_reaches_the_unmount_and_the_flight_lands() {
        let vid = "vol-cmdr-test-stuck-index-stop";
        let unmount_ran = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&unmount_ran);

        let result = in_flight::join_or_start(vid, move || async move {
            stop_index_then_unmount(
                vid,
                std::future::pending::<Result<(), EjectError>>(),
                move || async move {
                    observed.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await
        })
        .await;

        assert!(
            matches!(
                result,
                Err(EjectError::NotResponding {
                    step: EjectStep::IndexStop
                })
            ),
            "got {result:?}"
        );
        assert!(
            !unmount_ran.load(Ordering::SeqCst),
            "an index that might still hold the volume must never meet an unmount (FSKit can wedge)"
        );
        assert!(
            !ejecting_volume_ids().contains(&vid.to_string()),
            "the flight lands, so later clicks aren't stranded on a spinner"
        );
    }
}
