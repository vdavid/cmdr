//! The proposal spine in `main.db`: sweeps, the reviewable groups inside them, their ops,
//! and the server-owned acceptance record the claim transaction binds against.
//!
//! Three levels, and the middle one is one executor call:
//!
//! - **Sweep** (`proposal_sets`): one agent wake's output. Display and provenance only.
//! - **Group** (`proposals`): the reviewable, approvable, executable unit. Exactly ONE call
//!   to ONE executor, which is why `source_volume_id` lives here and not on the sweep.
//! - **Op** (`proposal_ops`): one path, which may be a file or a whole directory.
//!
//! ## What this module owns, and what it refuses to own
//!
//! Persistence and the lifecycle machine, nothing else: it takes a [`NewGroup`] and hands
//! back ids, pages, counts, and typed refusals. Selector resolution, analytics, and anything
//! that reads the drive index live a layer up in `agent/suggested_ops/`, so this module keeps
//! `rusqlite` as its only real dependency.
//!
//! ## The shapes that can't be built wrong
//!
//! [`GroupIntent`] pairs each verb with the target its executor requires AND the op shape it
//! takes, so a trash group carrying a destination folder, or a move group whose ops each
//! carry their own destination, is unrepresentable rather than rejected at runtime. The
//! store never validates the pairing because it never can be wrong.
//!
//! Depth (the claim transaction's exact order, the digest, the recovery sweep, the DDL
//! rationale): `DETAILS.md`.

mod claim;
mod complete;
mod read;
mod recovery;
mod write;

#[cfg(test)]
mod tests;

pub use claim::{
    AcceptanceOutcome, ClaimOutcome, ClaimRefusal, ClaimedGroup, OpBinding, RejectOutcome, claim_group_for_execution,
    live_binding, record_acceptance, reject_group,
};
pub use complete::{CompleteOutcome, mark_group_completed, record_op_outcome};
pub use read::{
    GroupSummary, ProposalOp, ProposalSweep, count_ops, count_pending, get_group, get_sweep, list_groups, page_ops,
    rejected_groups_since,
};
pub use recovery::recover_interrupted_groups;
pub use write::{
    NewGroup, NewOp, NewRename, NewSweep, OpSnapshot, ReproposeOutcome, create_group, create_sweep, repropose_group,
};

use crate::agent::types::{ProposalStatus, ProposalVerb, Reversibility};
use crate::location::Location;

/// A place a group may write into: a [`Location`] proven not to be inside an archive.
///
/// Copy or move INTO a zip, and move OUT of one, plan their work from their own `WalkDir`
/// rather than the per-source engine, so there is nowhere to apply a source binding. The
/// write engine refuses a bound transfer that lands on one of those routes, which is the
/// right backstop and the wrong layer on its own: a user-started copy-into-zip works while
/// an approved one refuses, and it refuses AFTER the user approved, which is exactly the
/// agent-specific execution behaviour the guiding principle forbids.
///
/// So the constraint lives at the PROPOSAL boundary instead, where a refusal costs the agent
/// a retry and costs the user nothing. A group with such a destination cannot be built, the
/// same way a trash group carrying a destination cannot.
///
/// Extract is unaffected: its SOURCES are archive-inner, its destination an ordinary folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableDestination(Location);

impl WritableDestination {
    /// The destination, or `None` when a ROUTE serves it: inside an archive, or
    /// inside a repo's virtual `.git` trees. Neither takes a write, so a proposal
    /// aimed there is one the user would approve and the backend would then refuse.
    ///
    /// No I/O, and deliberately permissive at the one edge that matters: the
    /// archive FILE itself is a perfectly good compress target and an ordinary file
    /// to copy, so only a path continuing INSIDE one is refused.
    /// `path_routes_over_its_parent` draws that line already.
    pub fn new(location: Location) -> Option<Self> {
        let routed =
            crate::file_system::volume::manager::path_routes_over_its_parent(std::path::Path::new(&location.path));
        (!routed).then_some(Self(location))
    }

    /// The location it wraps.
    pub fn location(&self) -> &Location {
        &self.0
    }
}

/// What a group asks for: the verb, the target that verb's executor binds, and the ops in
/// the shape that verb takes.
///
/// One enum rather than a verb plus loose optional fields, because the pairing is exact:
/// `trash_files_start` takes raw paths and no target, `move_files_start` takes one shared
/// destination directory, and `start_bulk_rename` is the one executor whose rows carry their
/// own destinations (it refuses a row whose source and destination parents differ, so its
/// group binds the shared PARENT rather than a target).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupIntent {
    /// Move every source into one shared destination directory.
    Move {
        destination: WritableDestination,
        sources: Vec<NewOp>,
    },
    /// Copy every source into one shared destination directory.
    Copy {
        destination: WritableDestination,
        sources: Vec<NewOp>,
    },
    /// Extract archive-inner sources into one shared destination directory.
    Extract {
        destination: WritableDestination,
        sources: Vec<NewOp>,
    },
    /// Compress every source into one target archive.
    Compress {
        archive: WritableDestination,
        sources: Vec<NewOp>,
        /// Whether that archive already exists. It decides reversibility: creating one is
        /// undone by deleting it, overwriting one is not undoable at all (the seed is
        /// unconditional and the prior bytes aren't retained).
        overwrites_existing: bool,
    },
    /// Rename sources in place. Every source shares `parent`, and each op carries the name it
    /// becomes.
    Rename { parent: String, renames: Vec<NewRename> },
    /// Move every source to the trash. Binds no target at all.
    Trash { sources: Vec<NewOp> },
    /// Delete every source permanently. Binds no target, and nothing takes it back.
    Delete { sources: Vec<NewOp> },
}

impl GroupIntent {
    /// The stored verb token's variant.
    pub fn verb(&self) -> ProposalVerb {
        match self {
            GroupIntent::Move { .. } => ProposalVerb::Move,
            GroupIntent::Copy { .. } => ProposalVerb::Copy,
            GroupIntent::Extract { .. } => ProposalVerb::Extract,
            GroupIntent::Compress { .. } => ProposalVerb::Compress,
            GroupIntent::Rename { .. } => ProposalVerb::Rename,
            GroupIntent::Trash { .. } => ProposalVerb::Trash,
            GroupIntent::Delete { .. } => ProposalVerb::Delete,
        }
    }

    /// How far an approved group of this intent can be taken back. A fact the review dialog
    /// DISCLOSES; per the guiding principle it is never a reason to refuse a group.
    pub fn reversibility(&self) -> Reversibility {
        match self {
            GroupIntent::Move { .. } | GroupIntent::Trash { .. } | GroupIntent::Rename { .. } => {
                Reversibility::RestoreMove
            }
            GroupIntent::Copy { .. } | GroupIntent::Extract { .. } => Reversibility::DeleteWhatWasWritten,
            GroupIntent::Compress {
                overwrites_existing, ..
            } => {
                if *overwrites_existing {
                    Reversibility::Irreversible
                } else {
                    Reversibility::DeleteWhatWasWritten
                }
            }
            GroupIntent::Delete { .. } => Reversibility::Irreversible,
        }
    }

    /// The target this intent binds, as the two columns store it: a path plus the volume it
    /// lives on. A rename binds its shared parent on the group's own source volume, so it
    /// carries no volume of its own; trash and delete bind nothing.
    pub(super) fn stored_destination(&self) -> (Option<&str>, Option<&str>) {
        match self {
            GroupIntent::Move { destination, .. }
            | GroupIntent::Copy { destination, .. }
            | GroupIntent::Extract { destination, .. } => {
                let destination = destination.location();
                (Some(destination.path.as_str()), Some(destination.volume_id.as_str()))
            }
            GroupIntent::Compress { archive, .. } => {
                let archive = archive.location();
                (Some(archive.path.as_str()), Some(archive.volume_id.as_str()))
            }
            GroupIntent::Rename { parent, .. } => (Some(parent.as_str()), None),
            GroupIntent::Trash { .. } | GroupIntent::Delete { .. } => (None, None),
        }
    }

    /// How many ops this intent carries. `COUNT`-free: it's the in-memory list's length.
    pub fn op_count(&self) -> usize {
        match self {
            GroupIntent::Move { sources, .. }
            | GroupIntent::Copy { sources, .. }
            | GroupIntent::Extract { sources, .. }
            | GroupIntent::Compress { sources, .. }
            | GroupIntent::Trash { sources }
            | GroupIntent::Delete { sources } => sources.len(),
            GroupIntent::Rename { renames, .. } => renames.len(),
        }
    }

    /// Every op flattened into the row shape: source path, the per-op destination (rename
    /// only), and the creation snapshot.
    pub(super) fn rows(&self) -> Box<dyn Iterator<Item = (&str, Option<&str>, Option<&OpSnapshot>)> + '_> {
        match self {
            GroupIntent::Move { sources, .. }
            | GroupIntent::Copy { sources, .. }
            | GroupIntent::Extract { sources, .. }
            | GroupIntent::Compress { sources, .. }
            | GroupIntent::Trash { sources }
            | GroupIntent::Delete { sources } => Box::new(
                sources
                    .iter()
                    .map(|op| (op.source_path.as_str(), None, op.snapshot.as_ref())),
            ),
            GroupIntent::Rename { renames, .. } => Box::new(renames.iter().map(|op| {
                (
                    op.source_path.as_str(),
                    Some(op.new_name.as_str()),
                    op.snapshot.as_ref(),
                )
            })),
        }
    }
}

/// One group as stored, header only. Its ops are read paged (`page_ops`) and counted with
/// `COUNT(*)` (`count_ops`) — a group of 60 000 is legitimate, so nothing loads them to
/// answer a question about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalGroup {
    pub id: i64,
    pub set_id: i64,
    pub seq: i64,
    pub verb: ProposalVerb,
    pub status: ProposalStatus,
    pub source_volume_id: String,
    /// The shared destination dir, rename parent, or archive path; `None` for trash/delete.
    pub destination: Option<String>,
    /// Which volume `destination` lives on. `None` for a rename (the source volume's) and for
    /// the verbs that bind no target at all.
    pub destination_volume_id: Option<String>,
    pub reversible: Reversibility,
    pub display_name: String,
    pub rationale: Option<String>,
    /// The selector this group froze, as JSON, when a selector produced it. Display and
    /// provenance only: ❌ a selector is NEVER re-resolved.
    pub selector: Option<String>,
    pub created_at: i64,
    /// When the group left `pending`.
    pub decided_at: Option<i64>,
}

/// Decode a stored token, or report which column held what.
pub(super) fn decode_token<T>(
    column: &'static str,
    token: String,
    parse: fn(&str) -> Option<T>,
) -> Result<T, super::AgentStoreError> {
    parse(&token).ok_or(super::AgentStoreError::Decode { column, value: token })
}
