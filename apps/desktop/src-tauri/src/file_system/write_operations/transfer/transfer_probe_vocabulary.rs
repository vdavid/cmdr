//! What a probe row and a driver can SAY: the phases, the role, and the row
//! identity, with their rendering and their round trip through the `AtomicU8`
//! each is stored in.
//!
//! Pure value types, deliberately apart from the live table in
//! `transfer_probe.rs`: nothing here takes a lock, holds an `Arc`, or reads the
//! registry, so a phase's meaning can be read and tested without the watchdog
//! around it. `transfer_probe.rs` re-exports all four, so every caller still
//! names them as `transfer_probe::<item>`.

use crate::file_system::write_operations::types::TransferWaitReason;

/// What a single copy task is doing. Ordinals are stable only within a build;
/// nothing persists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskPhase {
    /// Spawned into the window, not yet doing I/O.
    Spawned = 0,
    /// Opening the source stream (a device round-trip on MTP / SMB).
    OpeningSource = 1,
    /// Actively piping chunks.
    Streaming = 2,
    /// Parked between windows because the user paused.
    ParkedPause = 3,
    /// Parked between windows for foreground work on the SOURCE device
    /// (unbounded by design).
    ParkedSourceYield = 4,
    /// Parked between windows for foreground work on the DESTINATION share
    /// (hard-capped; it holds an open write handle).
    ParkedDestYield = 5,
    /// Past the last byte: safe-replace finalize, journal, cleanup.
    Finalizing = 6,
    /// Resolving a nested conflict inside a directory source (may be waiting on
    /// the human).
    ResolvingConflict = 7,
    /// Between attempts at the same file: a transport blip took the last one out
    /// and the backoff is running (`retry.rs`).
    WaitingToRetry = 8,
    /// Walking a directory source's tree: listing a level and resolving what is
    /// already there, before any of its files can be handed to the window.
    ///
    /// On a cross-share merge this is a full network round trip per level, and
    /// on a tree of many small folders it is most of the transfer's wall clock.
    /// Without a phase of its own the walk reported `spawned`, and a dump of a
    /// perfectly healthy transfer read as a task that had never started
    /// (`ERR-AYVM4`, 54 s in).
    Walking = 9,
}

impl TaskPhase {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::OpeningSource => "opening-source",
            Self::Streaming => "streaming",
            Self::ParkedPause => "parked(pause)",
            Self::ParkedSourceYield => "parked(source-yield)",
            Self::ParkedDestYield => "parked(dest-yield)",
            Self::Finalizing => "finalizing",
            Self::ResolvingConflict => "resolving-conflict",
            Self::WaitingToRetry => "waiting-to-retry",
            Self::Walking => "walking",
        }
    }

    /// What a task in this phase is waiting on, or `None` when the phase means
    /// "working" and so explains nothing about a stall.
    ///
    /// `ParkedPause` maps to `None` on purpose: the pause is reported from the
    /// operation's pause gate, which is authoritative, and a task can still be
    /// mid-chunk when the gate flips.
    pub(super) const fn wait_reason(self) -> Option<TransferWaitReason> {
        match self {
            Self::ParkedDestYield => Some(TransferWaitReason::Destination),
            Self::ParkedSourceYield => Some(TransferWaitReason::Source),
            Self::ResolvingConflict => Some(TransferWaitReason::Conflict),
            Self::Spawned
            | Self::OpeningSource
            | Self::Streaming
            | Self::ParkedPause
            | Self::Finalizing
            // A backoff is our own doing, not a wait on a device or a person, and
            // it is over in a second or less. The dump names the phase; the UI
            // keeps whatever reason the stall itself produced.
            | Self::WaitingToRetry
            // Walking is WORKING — listing levels and resolving what's there.
            // It waits on the source and the destination in turn, so naming
            // either one would be a guess.
            | Self::Walking => None,
        }
    }

    /// May the watchdog abort a task sitting in this phase when nothing has moved
    /// for a very long time?
    ///
    /// Only the two phases that mean "inside a backend call, waiting on the wire".
    /// Every park is deliberate and self-limiting — a pause ends when the user
    /// resumes, a yield when foreground drains (and the destination yield is
    /// hard-capped), a conflict when the human answers, a retry backoff on its own
    /// timer — so aborting one would break something that was working as designed.
    pub(super) const fn is_abortable_on_stall(self) -> bool {
        matches!(self, Self::OpeningSource | Self::Streaming)
    }

    pub(super) const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::OpeningSource,
            2 => Self::Streaming,
            3 => Self::ParkedPause,
            4 => Self::ParkedSourceYield,
            5 => Self::ParkedDestYield,
            6 => Self::Finalizing,
            7 => Self::ResolvingConflict,
            8 => Self::WaitingToRetry,
            9 => Self::Walking,
            _ => Self::Spawned,
        }
    }
}

/// What the DRIVER (the loop that fills and drains the concurrency window) is
/// doing. Distinguishing this from the tasks is the point: in the incident the
/// driver stopped after a destination `get_metadata` pre-check with six of eight
/// slots free, and nothing recorded that.
///
/// ❗ EVERY driver owes this, the two serial ones (`volume/copy_serial.rs`,
/// `volume/move.rs`) as much as the concurrent one: a driver that never advances
/// it reports `starting` for the whole transfer and its dump says nothing at
/// all. Where each is set: `volume/DETAILS.md` § driver phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DriverPhase {
    Starting = 0,
    /// Running the destination pre-check for the next source, before it can be
    /// spawned (concurrent) or streamed inline (serial).
    PreparingNext = 1,
    /// Window full or sources exhausted: awaiting the next task to finish.
    AwaitingTasks = 2,
    /// Loop finished; running cleanup, rollback, or finalize.
    PostLoop = 3,
    /// A SERIAL driver is streaming one source itself, so unlike
    /// [`Self::AwaitingTasks`] there is no window to drain: the wedge can only
    /// be in the rows below, and a reader should go straight to them.
    TransferringSource = 4,
    /// Parked on a PERSON, with nothing else running until they answer.
    /// Unbounded by design, so a dump naming it has explained the whole stall.
    ResolvingConflict = 5,
}

impl DriverPhase {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::PreparingNext => "preparing-next",
            Self::AwaitingTasks => "awaiting-tasks",
            Self::PostLoop => "post-loop",
            Self::TransferringSource => "transferring-source",
            Self::ResolvingConflict => "resolving-conflict",
        }
    }

    pub(super) const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::PreparingNext,
            2 => Self::AwaitingTasks,
            3 => Self::PostLoop,
            4 => Self::TransferringSource,
            5 => Self::ResolvingConflict,
            _ => Self::Starting,
        }
    }
}

/// What an in-flight row IS, which is what makes the dump's `in_flight=X/Y`
/// arithmetic add up. Rationale: `volume/DETAILS.md` § task roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRole {
    /// One file's byte copy: a merge leaf, or a top-level FILE source. These are
    /// what the `strategy.rs::FileWindow` width bounds, so they are what the
    /// dump measures against it.
    File,
    /// A directory source's walker: it lists levels and hands each file to the
    /// window, holding no slot of its own. Counted apart.
    Walker,
}

/// Which row of the in-flight table this is, and where it sits in the work.
///
/// A top-level source renders as `#<position in the source list>`; a leaf its
/// walker hands to the window renders as `#<that source>.<leaf>`. So the number
/// is unique within one dump AND says which source is producing the work, which
/// a flat counter can't. Depth doesn't enter it: one walker numbers every leaf
/// of its whole subtree from a single counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRow {
    source: usize,
    /// `None` for the source's own row, whether that source is a file or a
    /// walker.
    leaf: Option<usize>,
}

impl TaskRow {
    /// The row for the top-level source at `index` in the operation's source list.
    pub const fn source(index: usize) -> Self {
        Self {
            source: index,
            leaf: None,
        }
    }

    /// The row for the `nth` leaf this source's walker has handed to the window.
    pub const fn leaf(self, nth: usize) -> Self {
        Self {
            source: self.source,
            leaf: Some(nth),
        }
    }

    pub fn label(self) -> String {
        match self.leaf {
            Some(leaf) => format!("#{}.{leaf}", self.source),
            None => format!("#{}", self.source),
        }
    }
}
