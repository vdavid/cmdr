//! The `diskutil` (macOS) / `umount` (Linux) subprocess an eject or an SMB
//! disconnect runs, and what its run means.
//!
//! [`run`] reports HOW the tool ended as a [`ToolOutcome`], keeping the exit
//! status the wire type has no room for, so `run_teardown` (the one place a
//! refusal is logged) can name it. [`settle`] turns that into the caller's answer.

use std::time::Duration;

use super::EjectError;

/// How long the tool gets. ❗ Hitting it doesn't cancel the unmount, which may
/// still land afterwards.
const TOOL_TIMEOUT: Duration = Duration::from_secs(15);

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
    match tokio::time::timeout(
        TOOL_TIMEOUT,
        tokio::task::spawn_blocking(move || run_blocking(verb, &path)),
    )
    .await
    {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(join_err)) => ToolOutcome::TaskFailed {
            detail: join_err.to_string(),
        },
        Err(_elapsed) => ToolOutcome::TimedOut,
    }
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

/// What a finished run means for the caller. Pure.
///
/// Only a tool that EXITED with a code turned the unmount down, so only that is
/// an [`EjectError::UnmountRefused`]: a tool that never started or died to a
/// signal would make "something is still using this drive" a lie.
pub(super) fn settle(outcome: &ToolOutcome, verb: UnmountVerb) -> Result<(), EjectError> {
    match outcome {
        ToolOutcome::Succeeded => Ok(()),
        ToolOutcome::Exited { code: Some(_), stderr } => Err(EjectError::UnmountRefused {
            detail: format!("{verb}: {stderr}"),
        }),
        ToolOutcome::TimedOut => Err(EjectError::TimedOut),
        ToolOutcome::Exited { code: None, .. } | ToolOutcome::CouldNotStart { .. } | ToolOutcome::TaskFailed { .. } => {
            Err(EjectError::Unexpected {
                detail: format!("{verb}: {outcome}"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_that_says_no_is_an_unmount_refusal_carrying_its_stderr() {
        let outcome = ToolOutcome::Exited {
            code: Some(1),
            stderr: "Unmount was dissented by PID 51419 (/bin/sleep)".to_string(),
        };
        let error = settle(&outcome, UnmountVerb::Eject).unwrap_err();
        assert!(
            matches!(error, EjectError::UnmountRefused { ref detail } if detail.contains("dissented by PID 51419")),
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
        let error = settle(&outcome, UnmountVerb::Unmount).unwrap_err();
        assert!(matches!(error, EjectError::Unexpected { .. }), "got {error:?}");
    }

    #[test]
    fn a_timeout_stays_a_timeout() {
        assert!(matches!(
            settle(&ToolOutcome::TimedOut, UnmountVerb::Eject),
            Err(EjectError::TimedOut)
        ));
    }

    #[test]
    fn a_clean_exit_is_done() {
        assert!(settle(&ToolOutcome::Succeeded, UnmountVerb::Eject).is_ok());
    }

    #[test]
    fn the_log_line_names_the_exit_status_and_the_stderr() {
        let outcome = ToolOutcome::Exited {
            code: Some(1),
            stderr: "Failed to find disk /Volumes/X".to_string(),
        };
        let line = outcome.to_string();
        assert!(line.contains("exit status 1"), "{line}");
        assert!(line.contains("Failed to find disk /Volumes/X"), "{line}");
    }
}
