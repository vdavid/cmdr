//! `hdiutil create`, which attaches the image it's building and can lose it to whoever
//! opens that device first.
//!
//! The create attaches its new image, runs `newfs` on it, and detaches it. A process
//! with the transient `/dev/rdiskN` open at that detach makes the tool answer "Resource
//! busy", and the image STAYS attached, unmounted, under the path we asked for. So a bare
//! retry can't work: the leftover and its file have to go first, and the leftover's
//! detach going through is also the signal that the holder let go.

use std::path::Path;
use std::time::Duration;

use super::facts::Refusal;
use super::runner::Call;
use super::{DiskImageSession, HarnessError, holds_within};

/// How many times one image gets created before its busy answer stands. A holder that
/// outlasts this isn't passing through.
const MAX_CREATE_ATTEMPTS: u32 = 4;

/// How long a failed create's leftover gets to go.
const CREATE_SETTLE: Duration = Duration::from_secs(10);

impl DiskImageSession {
    /// Runs `call`, a [`Call::Create`] for `image_path`. A busy answer is retried once the
    /// failed attempt's leftover is gone, and any failed attempt leaves nothing behind.
    pub(super) fn create_image(&self, image_path: &Path, call: Call<'_>) -> Result<(), HarnessError> {
        retrying_busy_create(
            || self.run(image_path, call).map(drop),
            || self.settle_failed_create(image_path),
        )
    }

    /// Clears what a failed create left: its attachment, once a plain detach goes through,
    /// then its file, so the next attempt doesn't answer "File exists".
    ///
    /// ❌ Never `-force`: a holder that won't let go is what this waits out, and past
    /// [`CREATE_SETTLE`] the leftover is `reclaim_orphans`' to take at the next `acquire`.
    pub(super) fn settle_failed_create(&self, image_path: &Path) -> Result<(), HarnessError> {
        let mut still_busy = None;
        let detached = holds_within(CREATE_SETTLE, || match self.detach_leftover(image_path) {
            Ok(()) => true,
            Err(error) => {
                still_busy = Some(error);
                false
            }
        });
        if !detached {
            return Err(still_busy.unwrap_or_else(|| HarnessError::StillListed {
                image: image_path.to_path_buf(),
            }));
        }
        match std::fs::remove_file(image_path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    fn detach_leftover(&self, image_path: &Path) -> Result<(), HarnessError> {
        let whole = match self.whole_of(image_path) {
            Ok(whole) => whole,
            Err(error) if error.refusal() == Some(&Refusal::ImageNotAttached) => return Ok(()),
            Err(error) => return Err(error),
        };
        let detach = Call::Detach {
            whole: &whole,
            force: false,
        };
        match self.run(image_path, detach) {
            Ok(_) => self.until_unlisted(image_path),
            Err(error) if error.refusal() == Some(&Refusal::ImageNotAttached) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// The attempts of one create: `settle` runs after every failure, and only a busy answer
/// that settled earns another attempt.
fn retrying_busy_create(
    mut create: impl FnMut() -> Result<(), HarnessError>,
    mut settle: impl FnMut() -> Result<(), HarnessError>,
) -> Result<(), HarnessError> {
    let mut attempt = 1;
    loop {
        let Err(error) = create() else {
            return Ok(());
        };
        let settled = settle();
        if let Err(settle_error) = &settled {
            log::warn!(
                target: "testing::disk_images",
                "couldn't clear what a failed `hdiutil create` left: {settle_error}"
            );
        }
        if !(settled.is_ok() && error.says_resource_busy() && attempt < MAX_CREATE_ATTEMPTS) {
            return Err(error);
        }
        log::info!(
            target: "testing::disk_images",
            "`hdiutil create` answered busy on attempt {attempt} and its leftover is gone, so it's trying again"
        );
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn busy() -> HarnessError {
        HarnessError::Failed {
            call: "hdiutil create".to_string(),
            code: Some(1),
            stderr: "hdiutil: WARNING: 'hdiutil create' is deprecated.\nhdiutil: create failed - Resource busy"
                .to_string(),
        }
    }

    fn no_space() -> HarnessError {
        HarnessError::Failed {
            call: "hdiutil create".to_string(),
            code: Some(1),
            stderr: "hdiutil: create failed - No space left on device".to_string(),
        }
    }

    /// Runs the loop against `answers`, one per attempt (`None` once they run out reads as
    /// success), and returns the outcome with how many creates and settles it took.
    fn drive(
        answers: Vec<HarnessError>,
        settle: impl Fn() -> Result<(), HarnessError>,
    ) -> (Result<(), HarnessError>, u32, u32) {
        let (creates, settles) = (Cell::new(0), Cell::new(0));
        let mut answers = answers.into_iter();
        let result = retrying_busy_create(
            || {
                creates.set(creates.get() + 1);
                answers.next().map_or(Ok(()), Err)
            },
            || {
                settles.set(settles.get() + 1);
                settle()
            },
        );
        (result, creates.get(), settles.get())
    }

    #[test]
    fn a_create_that_works_settles_nothing() {
        let (result, creates, settles) = drive(vec![], || Ok(()));
        assert!(result.is_ok(), "got {result:?}");
        assert_eq!((creates, settles), (1, 0));
    }

    #[test]
    fn a_busy_create_is_retried_once_its_leftover_is_gone() {
        let (result, creates, settles) = drive(vec![busy(), busy()], || Ok(()));
        assert!(result.is_ok(), "got {result:?}");
        assert_eq!((creates, settles), (3, 2));
    }

    #[test]
    fn a_create_that_stays_busy_gives_up_with_the_tools_answer() {
        let answers = (0..MAX_CREATE_ATTEMPTS + 2).map(|_| busy()).collect();
        let (result, creates, settles) = drive(answers, || Ok(()));
        assert!(
            result.as_ref().is_err_and(HarnessError::says_resource_busy),
            "got {result:?}"
        );
        assert_eq!((creates, settles), (MAX_CREATE_ATTEMPTS, MAX_CREATE_ATTEMPTS));
    }

    /// Any failure can leave an attachment, so it's cleared even without a retry.
    #[test]
    fn any_other_failure_stands_at_once_and_still_clears_what_it_left() {
        let (result, creates, settles) = drive(vec![no_space()], || Ok(()));
        assert!(
            result.as_ref().is_err_and(|error| !error.says_resource_busy()),
            "got {result:?}"
        );
        assert_eq!((creates, settles), (1, 1));
    }

    /// A leftover that's still there would answer the retry with "File exists", which
    /// would bury the busy answer that explains the failure.
    #[test]
    fn a_busy_create_whose_leftover_wont_go_stands_with_the_busy_answer() {
        let (result, creates, settles) = drive(vec![busy()], || {
            Err(HarnessError::StillListed {
                image: "/tmp/image.dmg".into(),
            })
        });
        assert!(
            result.as_ref().is_err_and(HarnessError::says_resource_busy),
            "got {result:?}"
        );
        assert_eq!((creates, settles), (1, 1));
    }

    #[test]
    fn only_the_tools_busy_line_reads_as_busy() {
        assert!(busy().says_resource_busy());
        assert!(!no_space().says_resource_busy());
        let about_something_else = HarnessError::Failed {
            call: "hdiutil detach".to_string(),
            code: Some(16),
            stderr: "hdiutil: couldn't eject \"disk5\" - Resource busy".to_string(),
        };
        assert!(!about_something_else.says_resource_busy());
        let timed_out = HarnessError::TimedOut {
            call: "hdiutil create failed - Resource busy".to_string(),
        };
        assert!(!timed_out.says_resource_busy());
    }
}
