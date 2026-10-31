//! [`HarnessError`]: why a harness call didn't do what it was asked.

use std::io;
use std::path::PathBuf;

use super::facts::Refusal;
use super::runner::TOOL_DEADLINE;

/// Why a harness call didn't do what it was asked. `call` is the command line, for
/// the test's failure message.
#[derive(Debug)]
pub enum HarnessError {
    /// The tool couldn't be started.
    CouldNotStart {
        /// The command line.
        call: String,
        /// The spawn error.
        error: io::Error,
    },
    /// The tool didn't answer within the deadline and was SIGKILLed.
    TimedOut {
        /// The command line.
        call: String,
    },
    /// The tool exited nonzero, or a signal ended it.
    Failed {
        /// The command line.
        call: String,
        /// The exit code, or `None` for a signal.
        code: Option<i32>,
        /// The tool's stderr, trimmed.
        stderr: String,
    },
    /// The runner refused to touch a disk it couldn't prove is this image's.
    Refused {
        /// The command line, or the step that needed the proof.
        call: String,
        /// Why.
        refusal: Refusal,
    },
    /// The tool answered with a plist the harness couldn't read, or without a field
    /// it needs.
    Unreadable {
        /// The command line.
        call: String,
        /// What was missing or malformed.
        detail: String,
    },
    /// A detach answered success, but `hdiutil info` still listed the image
    /// [`TOOL_DEADLINE`] later.
    StillListed {
        /// The backing file.
        image: PathBuf,
    },
    /// A local file operation failed (the image's scratch directory, say).
    Io(io::Error),
}

impl HarnessError {
    /// The refusal, when the runner refused.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused { refusal, .. } => Some(refusal),
            _ => None,
        }
    }

    /// Whether `hdiutil create` answered that something holds the image it's building.
    ///
    /// The tool offers only text here: the same exit code 1 covers every create failure
    /// (verified on macOS 27.0.1, a read-only hold on the transient `/dev/rdiskN`,
    /// 2026-10-31). The runner pins `LC_ALL=C`, and `real_images` snapshots the line.
    pub(super) fn says_resource_busy(&self) -> bool {
        match self {
            Self::Failed { stderr, .. } => stderr
                .lines()
                .any(|line| line == "hdiutil: create failed - Resource busy"),
            _ => false,
        }
    }
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CouldNotStart { call, error } => write!(f, "`{call}` couldn't start: {error}"),
            Self::TimedOut { call } => write!(f, "`{call}` didn't answer within {TOOL_DEADLINE:?} and was killed"),
            Self::Failed { call, code, stderr } => write!(f, "`{call}` exited with {code:?}: {stderr}"),
            Self::Refused { call, refusal } => write!(f, "refused `{call}`: {refusal}"),
            Self::Unreadable { call, detail } => write!(f, "couldn't read what `{call}` answered: {detail}"),
            Self::StillListed { image } => write!(
                f,
                "`hdiutil info` still lists {} {TOOL_DEADLINE:?} after its detach answered success",
                image.display()
            ),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<io::Error> for HarnessError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
