//! The `diskutil` (macOS) / `umount` (Linux) subprocess an eject or an SMB
//! disconnect runs, and what its run means.
//!
//! [`run`] reports HOW the tool ended as a [`ToolOutcome`], keeping the exit
//! status the wire type has no room for, so the log lines can name it. [`settle`]
//! turns that into the caller's answer, and [`settle_with_retries`] (what
//! `run_teardown` calls) runs the tool again after a transient refusal and writes
//! the tool's log lines.

use std::time::Duration;

use super::EjectError;

/// How long one run of the tool gets. ❗ Hitting it doesn't cancel the unmount,
/// which may still land afterwards.
///
/// 30 s because a REFUSAL can be slow to arrive: after `unmount(2)` answers EBUSY,
/// `diskarbitrationd` scans every process with `proc_listpidspath(PROC_ALL_PIDS,
/// PATH_IS_VOLUME)` to name the dissenter before it answers (`DARequest.c`), and
/// that scan's time follows system load. `diskutil eject` refusals of a held APFS
/// disk image at load 2.2–2.5 took 20.4, 1.8, 0.5, 27.8, and 0.6 s (verified on
/// macOS 26.6.2, `diskutil eject` against a DMG held open, 2026-09-12). At 15 s two
/// of those five read as a false `TimedOut`, skipped the retry, and lost the stderr
/// naming the holder.
const TOOL_TIMEOUT: Duration = Duration::from_secs(30);

/// Which teardown the tool performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnmountVerb {
    /// `diskutil eject`: powers a USB drive down, detaches a disk image.
    Eject,
    /// `diskutil unmount`: an SMB share, which has no hardware to power down.
    Unmount,
}

impl std::fmt::Display for UnmountVerb {
    /// The command as a log line reads it. Linux runs `umount` for both verbs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(target_os = "macos")]
        let command = match self {
            Self::Eject => "diskutil eject",
            Self::Unmount => "diskutil unmount",
        };
        #[cfg(target_os = "linux")]
        let command = "umount";
        f.write_str(command)
    }
}

/// How one run of the tool ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ToolOutcome {
    /// Exit status 0.
    Succeeded,
    /// The tool ran and turned the unmount down.
    Exited {
        /// The exit code, or `None` when a signal ended the tool.
        code: Option<i32>,
        /// The tool's stderr, trimmed. Usually names the process holding the drive.
        stderr: String,
    },
    /// The tool couldn't be started at all (not on `PATH`, say).
    CouldNotStart {
        /// The spawn error.
        detail: String,
    },
    /// No answer within [`TOOL_TIMEOUT`].
    TimedOut,
    /// The blocking task running the tool panicked.
    TaskFailed {
        /// The join error.
        detail: String,
    },
}

impl std::fmt::Display for ToolOutcome {
    /// For the log line only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Succeeded => f.write_str("exited cleanly"),
            Self::Exited {
                code: Some(code),
                stderr,
            } => write!(f, "exit status {code}, stderr: {stderr}"),
            Self::Exited { code: None, stderr } => write!(f, "ended by a signal, stderr: {stderr}"),
            Self::CouldNotStart { detail } => write!(f, "couldn't start: {detail}"),
            Self::TimedOut => write!(f, "no answer within {} s", TOOL_TIMEOUT.as_secs()),
            Self::TaskFailed { detail } => write!(f, "the task running it failed: {detail}"),
        }
    }
}

/// Runs the tool against `mount_path` on the blocking pool, under [`TOOL_TIMEOUT`].
pub(super) async fn run(verb: UnmountVerb, mount_path: &str) -> ToolOutcome {
    let path = mount_path.to_string();
    within_tool_timeout(async move {
        tokio::task::spawn_blocking(move || run_blocking(verb, &path))
            .await
            .unwrap_or_else(|join_err| ToolOutcome::TaskFailed {
                detail: join_err.to_string(),
            })
    })
    .await
}

/// Bounds one run of the tool by [`TOOL_TIMEOUT`]. On expiry the run's future is
/// dropped, which only drops the JOIN HANDLE: the blocking thread and the unmount
/// it asked for run on. A seam, so tests hand in a run that answers late or never.
async fn within_tool_timeout(run: impl Future<Output = ToolOutcome>) -> ToolOutcome {
    tokio::time::timeout(TOOL_TIMEOUT, run)
        .await
        .unwrap_or(ToolOutcome::TimedOut)
}

fn run_blocking(verb: UnmountVerb, mount_path: &str) -> ToolOutcome {
    #[cfg(target_os = "macos")]
    let output = {
        let diskutil_verb = match verb {
            UnmountVerb::Eject => "eject",
            UnmountVerb::Unmount => "unmount",
        };
        std::process::Command::new("diskutil")
            .args([diskutil_verb, mount_path])
            .output()
    };
    // Linux: `umount` covers the SMB and removable cases; the physical-drive
    // power-down `diskutil eject` does is rare on Linux dev machines.
    #[cfg(target_os = "linux")]
    let output = {
        let _ = verb;
        std::process::Command::new("umount").arg(mount_path).output()
    };
    match output {
        Err(e) => ToolOutcome::CouldNotStart { detail: e.to_string() },
        Ok(output) if output.status.success() => ToolOutcome::Succeeded,
        Ok(output) => ToolOutcome::Exited {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
    }
}

/// What a finished run means for the caller.
#[derive(Debug)]
pub(super) enum Settled {
    /// The tool did it.
    Done,
    /// The tool didn't say yes, but the volume is no longer in the mount table, so
    /// the goal is met: something else unmounted it first, or the unmount a
    /// timeout didn't cancel landed.
    AlreadyGone,
    /// It didn't happen, and the volume is still mounted.
    Refused(EjectError),
}

/// What a finished run means for the caller. Pure: `still_mounted` is the
/// mount-table read, asked only when the tool didn't succeed.
///
/// ❗ "No longer mounted" is the one evidence that overrides a failure, and it
/// comes from the OS mount table: ❌ never a probe of the mount root (a `statfs`
/// on a hung network mount blocks 30–120 s), ❌ never the tool's stderr
/// (`error-string-match`). The registry would be too late: the unmount
/// notification can land milliseconds after the tool exits.
///
/// Of the failures that stay failures, only a tool that EXITED with a code turned
/// the unmount down, so only that is an [`EjectError::UnmountRefused`]: a tool
/// that never started or died to a signal would make "something is still using
/// this drive" a lie.
pub(super) fn settle(outcome: &ToolOutcome, verb: UnmountVerb, still_mounted: impl FnOnce() -> bool) -> Settled {
    let error = match outcome {
        ToolOutcome::Succeeded => return Settled::Done,
        ToolOutcome::Exited { code: Some(_), stderr } => EjectError::UnmountRefused {
            detail: format!("{verb}: {stderr}"),
        },
        ToolOutcome::TimedOut => EjectError::TimedOut,
        ToolOutcome::Exited { code: None, .. } | ToolOutcome::CouldNotStart { .. } | ToolOutcome::TaskFailed { .. } => {
            EjectError::Unexpected {
                detail: format!("{verb}: {outcome}"),
            }
        }
    };
    if still_mounted() {
        Settled::Refused(error)
    } else {
        Settled::AlreadyGone
    }
}

/// Whether `mount_path` is still in the OS mount table, read without touching the
/// mount itself. A table that can't be read answers "still mounted": reading it
/// as gone would turn a real refusal into a silent success.
pub(super) fn is_still_mounted(mount_path: &str) -> bool {
    #[cfg(target_os = "macos")]
    let listed = crate::volumes::is_mount_point(mount_path);
    #[cfg(target_os = "linux")]
    let listed = crate::file_system::linux_mounts::is_mount_point(mount_path);
    listed.unwrap_or(true)
}

/// The pause before each retry of a refused unmount: at most three more runs,
/// about 3 s in all. A refusal is often a TRANSIENT hold: LaunchServices (`lsd`)
/// registering an `.app` a pane just showed holds the volume for under a second
/// (`volume/DETAILS.md` § "Eject").
pub(super) const REFUSAL_RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_millis(1000),
    Duration::from_millis(1500),
];

/// How long after the first attempt a retry may still START.
///
/// Retries exist for sub-second holds (`lsd`), and a slow ANSWER comes from the
/// daemon's dissenter scan ([`TOOL_TIMEOUT`]), not from a long hold, so a retry
/// after a slow refusal can still succeed: at 30 s even the worst measured refusal
/// (27.8 s) gets one. It caps the tool phase at `RETRY_BUDGET` + [`TOOL_TIMEOUT`]
/// (60 s: one last run may start just inside the budget and hit the timeout), where
/// four runs with no budget could reach about 2 minutes.
pub(super) const RETRY_BUDGET: Duration = Duration::from_secs(30);

/// What a run of the tool is aimed at, for its log lines.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target<'a> {
    /// The volume being torn down.
    pub volume_id: &'a str,
    /// Eject or unmount.
    pub verb: UnmountVerb,
    /// The mount root the tool runs against.
    pub mount_path: &'a str,
}

/// Runs the tool until it settles, retrying a refused unmount after each pause in
/// [`REFUSAL_RETRY_BACKOFF`], as long as the retry would start inside
/// [`RETRY_BUDGET`] of the first attempt.
///
/// Every run re-settles, so a volume that left the mount table between runs
/// counts as done. Only [`EjectError::UnmountRefused`] is retried: a timeout
/// already waited [`TOOL_TIMEOUT`] and its unmount may still land, and a tool that
/// couldn't start won't start next time. `run_tool` and `still_mounted` are
/// closures, so tests drive a scripted tool on a paused clock.
///
/// Logs on target `eject`: each intermediate refusal at `info` with its attempt
/// number and outcome (so the logs show how often holds are transient), the final
/// refusal once at `warn` with the attempt count and the time since the first
/// attempt, and a success at `info`, with the count when it took retries.
pub(super) async fn settle_with_retries<T, Fut, M>(
    target: Target<'_>,
    mut run_tool: T,
    mut still_mounted: M,
) -> Result<(), EjectError>
where
    T: FnMut() -> Fut,
    Fut: Future<Output = ToolOutcome>,
    M: FnMut() -> bool,
{
    let Target {
        volume_id,
        verb,
        mount_path,
    } = target;
    let mut attempts: usize = 1;
    let first_attempt = tokio::time::Instant::now();
    loop {
        let outcome = run_tool().await;
        let pause = REFUSAL_RETRY_BACKOFF.get(attempts - 1).copied();
        match (settle(&outcome, verb, &mut still_mounted), pause) {
            (Settled::Done, _) => {
                if attempts == 1 {
                    log::info!(target: "eject", "`{verb}` succeeded for {volume_id} at {mount_path}");
                } else {
                    log::info!(
                        target: "eject",
                        "`{verb}` succeeded for {volume_id} at {mount_path} on attempt {attempts}"
                    );
                }
                return Ok(());
            }
            (Settled::AlreadyGone, _) => {
                log::info!(
                    target: "eject",
                    "`{verb}` for {volume_id} at {mount_path} didn't go through on attempt {attempts} ({outcome}), but it's no longer mounted, so it counts as done"
                );
                return Ok(());
            }
            (Settled::Refused(EjectError::UnmountRefused { .. }), Some(pause))
                if first_attempt.elapsed() + pause < RETRY_BUDGET =>
            {
                log::info!(
                    target: "eject",
                    "`{verb}` for {volume_id} at {mount_path} was refused on attempt {attempts} ({outcome}); retrying in {} ms",
                    pause.as_millis()
                );
                tokio::time::sleep(pause).await;
                attempts += 1;
            }
            (Settled::Refused(error), _) => {
                log::warn!(
                    target: "eject",
                    "`{verb}` for {volume_id} at {mount_path} didn't go through (attempts: {attempts}, {:.1} s since the first): {outcome}",
                    first_attempt.elapsed().as_secs_f64()
                );
                return Err(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::VecDeque;

    fn still_mounted() -> bool {
        true
    }

    fn gone() -> bool {
        false
    }

    fn refusal(settled: Settled) -> EjectError {
        match settled {
            Settled::Refused(error) => error,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn dissented() -> ToolOutcome {
        ToolOutcome::Exited {
            code: Some(1),
            stderr: "Unmount was dissented by PID 51419 (/bin/sleep)".to_string(),
        }
    }

    #[test]
    fn a_tool_that_says_no_is_an_unmount_refusal_carrying_its_stderr() {
        let error = refusal(settle(&dissented(), UnmountVerb::Eject, still_mounted));
        let expected_detail = format!(
            "{}: Unmount was dissented by PID 51419 (/bin/sleep)",
            UnmountVerb::Eject
        );
        assert!(
            matches!(error, EjectError::UnmountRefused { ref detail } if *detail == expected_detail),
            "got {error:?}"
        );
    }

    #[test]
    fn a_tool_that_couldnt_start_is_unexpected_not_a_refusal() {
        // "Something is still using this drive" would be a lie when `diskutil`
        // never ran at all.
        let outcome = ToolOutcome::CouldNotStart {
            detail: "No such file or directory".to_string(),
        };
        let error = refusal(settle(&outcome, UnmountVerb::Unmount, still_mounted));
        assert!(matches!(error, EjectError::Unexpected { .. }), "got {error:?}");
    }

    #[test]
    fn a_timeout_stays_a_timeout_while_the_volume_is_still_mounted() {
        let error = refusal(settle(&ToolOutcome::TimedOut, UnmountVerb::Eject, still_mounted));
        assert!(matches!(error, EjectError::TimedOut), "got {error:?}");
    }

    #[test]
    fn a_clean_exit_is_done_without_reading_the_mount_table() {
        let settled = settle(&ToolOutcome::Succeeded, UnmountVerb::Eject, || {
            panic!("a clean exit must not read the mount table")
        });
        assert!(matches!(settled, Settled::Done), "got {settled:?}");
    }

    #[test]
    fn a_refusal_for_a_volume_no_longer_mounted_counts_as_already_gone() {
        // `diskutil eject` of a path that's already unmounted exits 1 with
        // "Failed to find disk", and so does one that lost a race with the
        // unmount it asked for. The person's goal is met either way.
        let outcome = ToolOutcome::Exited {
            code: Some(1),
            stderr: "Failed to find disk /Volumes/X".to_string(),
        };
        let settled = settle(&outcome, UnmountVerb::Eject, gone);
        assert!(matches!(settled, Settled::AlreadyGone), "got {settled:?}");
    }

    #[test]
    fn a_timeout_after_which_the_volume_is_gone_counts_as_already_gone() {
        // The unmount the timeout didn't cancel landed.
        let settled = settle(&ToolOutcome::TimedOut, UnmountVerb::Unmount, gone);
        assert!(matches!(settled, Settled::AlreadyGone), "got {settled:?}");
    }

    #[test]
    fn the_log_line_names_the_exit_status_and_the_stderr() {
        let outcome = ToolOutcome::Exited {
            code: Some(1),
            stderr: "Failed to find disk /Volumes/X".to_string(),
        };
        assert_eq!(
            outcome.to_string(),
            "exit status 1, stderr: Failed to find disk /Volumes/X"
        );
    }

    // ── Retrying a refused unmount ────────────────────────────────────

    fn target() -> Target<'static> {
        Target {
            volume_id: "vol-backup-00c0ffee00c0ffee",
            verb: UnmountVerb::Eject,
            mount_path: "/Volumes/Backup",
        }
    }

    /// A fake tool that answers `script` in order and counts its runs.
    fn scripted(script: Vec<ToolOutcome>, runs: &Cell<usize>) -> impl FnMut() -> std::future::Ready<ToolOutcome> + '_ {
        let mut script = VecDeque::from(script);
        move || {
            runs.set(runs.get() + 1);
            std::future::ready(
                script
                    .pop_front()
                    .expect("the tool must not run more often than the test scripted"),
            )
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_transient_hold_is_retried_until_the_eject_goes_through() {
        // `lsd` registering a freshly seen `.app` holds the volume for under a
        // second; the retry after the first pause finds it free.
        let runs = Cell::new(0);
        let started = tokio::time::Instant::now();
        let result = settle_with_retries(
            target(),
            scripted(vec![dissented(), ToolOutcome::Succeeded], &runs),
            still_mounted,
        )
        .await;

        assert!(result.is_ok(), "got {result:?}");
        assert_eq!(runs.get(), 2);
        assert_eq!(started.elapsed(), Duration::from_millis(500));
    }

    #[tokio::test(start_paused = true)]
    async fn a_hold_that_outlasts_every_retry_answers_the_last_refusal() {
        let runs = Cell::new(0);
        let started = tokio::time::Instant::now();
        let result = settle_with_retries(
            target(),
            scripted(vec![dissented(), dissented(), dissented(), dissented()], &runs),
            still_mounted,
        )
        .await;

        let expected_detail = format!(
            "{}: Unmount was dissented by PID 51419 (/bin/sleep)",
            UnmountVerb::Eject
        );
        assert!(
            matches!(result, Err(EjectError::UnmountRefused { ref detail }) if *detail == expected_detail),
            "got {result:?}"
        );
        assert_eq!(runs.get(), 4, "one run plus three retries");
        assert_eq!(
            started.elapsed(),
            Duration::from_secs(3),
            "a real hold reports about 3 s later"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_retry_that_finds_the_volume_gone_counts_as_done() {
        let runs = Cell::new(0);
        let mut mount_table = VecDeque::from([true, false]);
        let result = settle_with_retries(target(), scripted(vec![dissented(), dissented()], &runs), move || {
            mount_table
                .pop_front()
                .expect("the mount table must be read once per refused run")
        })
        .await;

        assert!(result.is_ok(), "got {result:?}");
        assert_eq!(runs.get(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_timeout_is_not_retried() {
        // It already waited 30 s, and the unmount it didn't cancel may still land.
        let runs = Cell::new(0);
        let result = settle_with_retries(
            target(),
            scripted(vec![ToolOutcome::TimedOut, ToolOutcome::Succeeded], &runs),
            still_mounted,
        )
        .await;

        assert!(matches!(result, Err(EjectError::TimedOut)), "got {result:?}");
        assert_eq!(runs.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_tool_that_couldnt_start_is_not_retried() {
        let runs = Cell::new(0);
        let outcome = ToolOutcome::CouldNotStart {
            detail: "No such file or directory".to_string(),
        };
        let result = settle_with_retries(
            target(),
            scripted(vec![outcome, ToolOutcome::Succeeded], &runs),
            still_mounted,
        )
        .await;

        assert!(matches!(result, Err(EjectError::Unexpected { .. })), "got {result:?}");
        assert_eq!(runs.get(), 1);
    }

    // ── Slow answers: the daemon's dissenter scan, the timeout, the budget ──

    /// A fake tool whose every run answers after its own delay, through the real
    /// [`within_tool_timeout`], and counts its runs.
    fn answering_after(
        script: Vec<(Duration, ToolOutcome)>,
        runs: &Cell<usize>,
    ) -> impl FnMut() -> std::pin::Pin<Box<dyn Future<Output = ToolOutcome>>> + '_ {
        let mut script = VecDeque::from(script);
        move || {
            runs.set(runs.get() + 1);
            let (delay, outcome) = script
                .pop_front()
                .expect("the tool must not run more often than the test scripted");
            Box::pin(within_tool_timeout(async move {
                tokio::time::sleep(delay).await;
                outcome
            }))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_refusal_that_answers_after_20_s_is_a_refusal_and_is_retried() {
        // `diskarbitrationd` scans every process for the dissenter before it
        // answers, and under load that took 20.4 and 27.8 s in a measured run. The
        // answer is still a refusal naming the holder, not a timeout.
        let runs = Cell::new(0);
        let started = tokio::time::Instant::now();
        let result = settle_with_retries(
            target(),
            answering_after(
                vec![
                    (Duration::from_secs(20), dissented()),
                    (Duration::ZERO, ToolOutcome::Succeeded),
                ],
                &runs,
            ),
            still_mounted,
        )
        .await;

        assert!(
            result.is_ok(),
            "the retry after a slow refusal can still succeed, got {result:?}"
        );
        assert_eq!(runs.get(), 2);
        assert_eq!(started.elapsed(), Duration::from_millis(20_500));
    }

    #[tokio::test(start_paused = true)]
    async fn the_retry_budget_stops_a_new_attempt_from_starting_past_its_limit() {
        // Refusals at 20 s and at 29.5 s: the next retry would START at 30.5 s,
        // past the 30 s budget, so the second refusal is the answer.
        let runs = Cell::new(0);
        let started = tokio::time::Instant::now();
        let result = settle_with_retries(
            target(),
            answering_after(
                vec![
                    (Duration::from_secs(20), dissented()),
                    (Duration::from_secs(9), dissented()),
                    (Duration::ZERO, ToolOutcome::Succeeded),
                ],
                &runs,
            ),
            still_mounted,
        )
        .await;

        assert!(
            matches!(result, Err(EjectError::UnmountRefused { .. })),
            "got {result:?}"
        );
        assert_eq!(runs.get(), 2, "no attempt may start once the budget is spent");
        assert_eq!(started.elapsed(), Duration::from_millis(29_500));
    }

    #[tokio::test(start_paused = true)]
    async fn a_truly_stuck_tool_still_answers_timed_out_at_30_s() {
        let started = tokio::time::Instant::now();
        let outcome = within_tool_timeout(std::future::pending::<ToolOutcome>()).await;
        assert_eq!(outcome, ToolOutcome::TimedOut);
        assert_eq!(started.elapsed(), Duration::from_secs(30));
    }
}
