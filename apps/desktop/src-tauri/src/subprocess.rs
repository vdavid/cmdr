//! Running a child process under a deadline.
//!
//! **Why this exists.** `Command::output()` waits for the child to exit, with no
//! upper bound of any kind. That's fine for `sw_vers`; it is not fine for
//! anything reaching a network host. `smbutil view` against a share whose server
//! stopped answering never returns, and the share browser's spinner waited on it
//! for as long as the user was willing to watch.
//!
//! **Why a plain `tokio::time::timeout` around `spawn_blocking` isn't enough.**
//! That releases the caller and leaks everything else: the child keeps running,
//! and the blocking-pool thread parked in `wait()` is gone for good. Retry a few
//! times and the pool (512 threads, shared with every directory listing in the
//! app) starts running out. Here the deadline is applied to a `tokio::process`
//! child with `kill_on_drop`, so expiry actually ends the process.
//!
//! Which subprocesses need a deadline and which don't: `DETAILS.md` §
//! "Subprocesses run under a deadline".

use std::process::Output;
use std::time::Duration;
use tokio::process::Command;

/// Why a bounded subprocess produced no output.
#[derive(Debug)]
pub enum SubprocessError {
    /// The process couldn't be started, or died without a status. Carries the raw
    /// `io::Error` so a caller can still tell `NotFound` (the tool isn't
    /// installed) from the rest.
    Spawn(std::io::Error),
    /// It ran past its deadline and was killed. Carries the deadline so the
    /// caller can say how long it waited without repeating the constant.
    TimedOut { limit: Duration },
}

impl std::fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(err) => write!(f, "{err}"),
            Self::TimedOut { limit } => write!(f, "stopped answering after {:.0}s", limit.as_secs_f64()),
        }
    }
}

/// Runs `command` to completion, or kills it once `limit` is up.
///
/// `what` names the process in the log; keep it a short static label (`"smbutil
/// view"`), never a path or a URL, since the line ends up in every error-report
/// bundle.
///
/// A deadline that fires logs one WARN on the `subprocess` target. That line is
/// half the point of routing through here: a share browser that came back empty
/// used to leave no trace of the tool that never answered.
pub async fn output_within(what: &str, limit: Duration, command: &mut Command) -> Result<Output, SubprocessError> {
    // `spawn` INHERITS stdio, where `output` would have piped it. Every caller here
    // parses what the tool printed, so without this the output comes back empty and
    // a share list reads as "this server has no shares" — a silent wrong answer
    // rather than a visible failure. Pinned by the `echo` test below.
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Dropping the child sends SIGKILL. `wait_with_output` consumes the child, so
    // the timeout dropping that future IS the kill: no handle is left over to
    // forget about, and the expiry path can't be reached with the process alive.
    command.kill_on_drop(true);

    let started = std::time::Instant::now();
    let child = command.spawn().map_err(SubprocessError::Spawn)?;

    match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            log::debug!(
                target: "subprocess",
                "{what} exited {:?} after {:?}",
                output.status.code(),
                started.elapsed()
            );
            Ok(output)
        }
        Ok(Err(err)) => Err(SubprocessError::Spawn(err)),
        Err(_elapsed) => {
            log::warn!(
                target: "subprocess",
                "{what} didn't answer within {limit:?} and was stopped, so whatever asked for it gets nothing back."
            );
            Err(SubprocessError::TimedOut { limit })
        }
    }
}

/// A [`Command`] that inherits nothing from the user's locale, so English-only
/// output parsing holds wherever Cmdr runs.
///
/// Every tool whose stdout or stderr gets parsed goes through here. Without it a
/// Swedish `LANG` silently changes what the parser sees, and the parser fails in
/// a way that reads like the server being broken.
pub fn command_in_c_locale(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env("LC_ALL", "C").env("LANG", "C");
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_child_that_never_exits_is_killed_at_the_deadline() {
        let started = std::time::Instant::now();
        let result = output_within("sleep", Duration::from_millis(200), Command::new("sleep").arg("30")).await;

        assert!(
            matches!(result, Err(SubprocessError::TimedOut { .. })),
            "a child outliving its deadline must report the deadline, got {result:?}"
        );
        // The point of the bound is that the CALLER is released promptly. The
        // ceiling is generous on purpose: this asserts "not 30 seconds", not a
        // scheduler guarantee.
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the caller waited {:?}, so the deadline didn't release it",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_child_that_finishes_in_time_returns_its_output() {
        let result = output_within("echo", Duration::from_secs(10), Command::new("echo").arg("hi"))
            .await
            .expect("echo finishes well inside ten seconds");

        assert!(result.status.success());
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "hi");
    }

    #[tokio::test]
    async fn a_missing_program_reports_the_io_error_rather_than_the_deadline() {
        let result = output_within(
            "not-a-real-program",
            Duration::from_secs(10),
            &mut Command::new("cmdr-no-such-program-exists"),
        )
        .await;

        match result {
            Err(SubprocessError::Spawn(err)) => {
                assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("a missing program must surface as a spawn error, got {other:?}"),
        }
    }
}
