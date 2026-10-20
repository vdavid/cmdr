//! The typed vocabulary of the operation log: the enums that classify an
//! operation and its items, and their stable DB tokens.
//!
//! Every classification field the journal stores or crosses a boundary with
//! (`kind`, `initiator`, `execution_status`, `rollback_state`,
//! `not_rollbackable_reason`, per-item `outcome`, `entry_type`, `row_role`,
//! `search_coverage`, `search_coverage_reason`, the `archive_edit` subkind) is a
//! typed enum here, never a substring branch. Each
//! carries a compact, stable, human-readable **token** stored as TEXT in the DB.
//!
//! The tokens are a serialization contract, not a display string: they stay
//! `sqlite3`-inspectable and are the ONE place enum ↔ storage mapping lives.
//! Renaming a token is a schema change (needs a migration to rewrite stored
//! rows); renaming a *variant* is free. Tokens are lowercase snake_case so a DB
//! browser reads them plainly.
//!
//! `from_token` returns `None` on an unknown value — a row written by a newer
//! schema, or genuine corruption. Readers turn that into a typed store error
//! rather than guessing.

/// A macro to declare a token-backed enum once: the variants, their stable DB
/// tokens, `as_token`, and `from_token`. Keeps the two directions in lockstep so
/// they can't drift.
macro_rules! token_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $( $(#[$vmeta:meta])* $variant:ident => $token:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
        // The serde/specta wire form (camelCase, for IPC + `bindings.ts`) is
        // SEPARATE from the DB `as_token` (stable snake_case). Callers cross IPC
        // as this typed enum, never a string; the store
        // reads/writes via the tokens below.
        #[serde(rename_all = "camelCase")]
        #[allow(missing_docs, reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship")]
        $vis enum $name { $( $(#[$vmeta])* $variant ),+ }

        impl $name {
            /// The stable DB token for this variant.
            pub fn as_token(self) -> &'static str {
                match self { $( $name::$variant => $token ),+ }
            }

            /// Parse a stored token back to the variant, or `None` if unknown.
            pub fn from_token(token: &str) -> Option<Self> {
                match token { $( $token => Some($name::$variant), )+ _ => None }
            }
        }
    };
}

token_enum! {
    /// The durable operation taxonomy. It is deliberately coarser than the live
    /// `WriteOperationType`: compress and zip edit have distinct progress
    /// identities but share `ArchiveEdit` here and are distinguished by
    /// [`ArchiveSubkind`].
    pub enum OpKind {
        Copy => "copy",
        Move => "move",
        Delete => "delete",
        Trash => "trash",
        Rename => "rename",
        CreateFolder => "create_folder",
        CreateFile => "create_file",
        ArchiveEdit => "archive_edit",
    }
}

token_enum! {
    /// The `archive_edit` subkind, supplied by the capturing driver (compress vs
    /// zip-inner edit). Stored only when `kind = ArchiveEdit`; the live operation
    /// may carry the more specific `WriteOperationType::Compress`.
    pub enum ArchiveSubkind {
        Compress => "compress",
        Edit => "edit",
        Extract => "extract",
    }
}

token_enum! {
    /// Who initiated the operation. `AgentEdited` is mixed provenance: the
    /// in-app agent proposed the batch and the user retyped at least one name while reviewing
    /// it, so crediting the agent alone would be a lie about who chose those names.
    pub enum Initiator {
        User => "user",
        AiClient => "ai_client",
        Agent => "agent",
        AgentEdited => "agent_edited",
    }
}

token_enum! {
    /// The operation's lifecycle axis, mirrored from the manager's
    /// `LifecycleStatus`. Independent of [`RollbackState`].
    pub enum ExecutionStatus {
        Queued => "queued",
        Running => "running",
        Done => "done",
        Failed => "failed",
        Canceled => "canceled",
    }
}

token_enum! {
    /// Whether and how the operation can be / has been reversed. Independent
    /// of [`ExecutionStatus`]. `RollingBack` is the transient in-flight guard
    /// (rollback); a fresh op sits at `NotRollbackable` until finalize proves otherwise.
    pub enum RollbackState {
        NotRollbackable => "not_rollbackable",
        Rollbackable => "rollbackable",
        RollingBack => "rolling_back",
        RolledBack => "rolled_back",
        PartiallyRolledBack => "partially_rolled_back",
    }
}

token_enum! {
    /// Why an operation is not rollbackable, set alongside
    /// `RollbackState::NotRollbackable`. A nullable column: `None` when the
    /// op is rollbackable. Cross-volume disconnection is NOT here — that's
    /// computed at rollback time from mount state, never stored.
    pub enum NotRollbackableReason {
        /// A copy/move overwrote existing files; the originals are gone.
        Overwrote => "overwrote",
        /// A permanent delete can't be restored.
        PermanentDelete => "permanent_delete",
        /// A compress overwrote a prior archive; the prior bytes aren't retained.
        ArchiveOverwrite => "archive_overwrite",
        /// Zip-inner editing rollback isn't supported yet (v1).
        ZipEditUnsupported => "zip_edit_unsupported",
        /// A `rollback_unit` row was dropped/errored, so the journal is an
        /// incomplete record of what to reverse (the completeness downgrade).
        JournalIncomplete => "journal_incomplete",
        /// A move merged a source folder into a folder that already existed, so
        /// the one directory row names a destination holding files the operation
        /// never touched. Renaming it back would carry those files away with it.
        DirectoryMerge => "directory_merge",
        /// A cross-FS move resolved a conflict at the FINAL destination, after
        /// its rows were already journaled against the staging area — so the
        /// recorded destinations aren't where the files landed.
        StagedConflictResolved => "staged_conflict_resolved",
    }
}

token_enum! {
    /// Whether the journal holds every leaf of the operation (search honesty).
    /// `Full` requires the drive index to have been present AND
    /// current for the whole subtree.
    pub enum SearchCoverage {
        Full => "full",
        TopLevelOnly => "top_level_only",
    }
}

token_enum! {
    /// Why coverage is only `TopLevelOnly`, set when `search_coverage =
    /// top_level_only`. Kept distinct so the future agent can tell a
    /// too-big-to-index subtree from a stale index.
    pub enum SearchCoverageReason {
        /// The subtree exceeded the per-op `search_only` leaf cap.
        Capped => "capped",
        /// No drive index covered the subtree.
        IndexAbsent => "index_absent",
        /// The index covered the subtree but wasn't current.
        IndexStale => "index_stale",
        /// The volume's index phase wasn't `Live`.
        VolumeNotLive => "volume_not_live",
        /// A `search_only` leaf row was dropped/errored (the completeness downgrade).
        SearchRowIncomplete => "search_row_incomplete",
    }
}

token_enum! {
    /// Whether an item row is a file or a directory. Directories the op created
    /// are first-class rows so a `seq DESC` rollback removes files before the
    /// dirs that held them.
    pub enum EntryType {
        File => "file",
        Dir => "dir",
    }
}

token_enum! {
    /// An item row's role. `RollbackUnit` rows are the reversal
    /// units and are also searchable; `SearchOnly` rows exist purely so leaf
    /// search hits inside a top-level move/trash unit, and are never reversed.
    pub enum RowRole {
        RollbackUnit => "rollback_unit",
        SearchOnly => "search_only",
    }
}

token_enum! {
    /// The per-item outcome. A canceled/failed op keeps `Done` rows for what it
    /// reached — exactly what a rollback needs.
    pub enum ItemOutcome {
        Done => "done",
        Skipped => "skipped",
        Failed => "failed",
        RolledBack => "rolled_back",
    }
}

token_enum! {
    /// Why a rollback left one item alone rather than reversing it — the rollback
    /// engine's per-item verdict, and the vocabulary of the nullable
    /// `operation_items.rollback_skip_reason` column.
    ///
    /// It is also the vocabulary the IN-FLIGHT reversals speak — the one a cancel
    /// runs over a transfer's own ledger (`cmdr_operations::reversal`),
    /// which reports its verdicts on the `write-cancelled` event rather than storing
    /// them. ❌ Don't add a parallel enum for those; the two reversals answer the same
    /// question and a user shouldn't meet two vocabularies for it.
    ///
    /// Stored ONLY by the rollback engine, ONLY on the original op's item rows, and
    /// ONLY alongside [`ItemOutcome::Skipped`]. Everything else reads NULL, which means
    /// "reason not recorded" and never a default: a pre-v2 row, a mutation path that
    /// recorded its own `Skipped` outcome, and the inverse op's mirror rows all have no
    /// rollback verdict to report. `AlreadyGone` counts as reversed (the desired end
    /// state already holds), so it lands `RolledBack` and is never stored.
    ///
    /// This is reporting fidelity only: no variant may change whether an item is
    /// skipped, retried, or forced.
    pub enum SkipReason {
        /// A precondition field couldn't be verified (a recorded snapshot field whose
        /// live counterpart is absent, or no snapshot field at all) — fail safe, never
        /// operate on an unprovable file.
        UnverifiablePrecondition => "unverifiable_precondition",
        /// The item changed since the op (size/mtime drift) — never touch a changed file.
        Drift => "drift",
        /// The restore target is occupied by a DIFFERENT entry — the pinned
        /// non-destructive policy skips rather than overwrite.
        RestoreTargetOccupied => "restore_target_occupied",
        /// A directory the undo would remove isn't empty (a file was added since) — the
        /// create-folder / copied-dir recheck.
        DirNotEmpty => "dir_not_empty",
        /// The thing to reverse is already gone and the desired end state already holds
        /// (a copy already removed, an item already restored), so this is an idempotent
        /// no-op success.
        AlreadyGone => "already_gone",
        /// The item is gone from where the operation left it AND absent from where it came
        /// from (the trash was emptied, or it was deleted or moved away since): there's
        /// nothing to put back. A real skip, unlike `AlreadyGone`, because the end state the
        /// undo wanted (the item home again) does NOT hold.
        Vanished => "vanished",
        /// A backend error prevented reversing this item.
        Failed => "failed",
        /// The item was verified as this operation's own, but it wears a lock this
        /// process can't clear (`schg`, which only root can), so it can't be removed.
        /// A user flag or an ACL the copy carried over is cleared, never reported here.
        Locked => "locked",
    }
}

/// One operation's id: the managed operation's UUIDv7
/// ([`new_operation_id`](crate::new_operation_id)), reused as its journal row's
/// key. The journal's API takes this, so an id can't be swapped with a volume
/// id or a path where both are strings; a plain `String` only crosses IPC and
/// sits in the store's rows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(String);

impl OperationId {
    /// The id as text, for the store, a log line, or the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for OperationId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for OperationId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Lets a map keyed by `OperationId` answer a lookup by the `&str` a wire row
/// or a progress path carries, without minting an id to ask. Sound because the
/// id hashes and compares exactly as its text does.
impl std::borrow::Borrow<str> for OperationId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant round-trips through its token, and tokens are unique within
    /// an enum. This guards the stable-token serialization contract: a fumbled
    /// token would silently corrupt reads of stored rows.
    #[test]
    fn tokens_round_trip_and_are_unique() {
        macro_rules! check {
            ($ty:ident, [$($v:ident),+]) => {{
                let all = [$($ty::$v),+];
                let mut seen = std::collections::HashSet::new();
                for variant in all {
                    let token = variant.as_token();
                    assert!(seen.insert(token), "duplicate token {token:?} in {}", stringify!($ty));
                    assert_eq!($ty::from_token(token), Some(variant), "round-trip {token:?}");
                }
                assert_eq!($ty::from_token("definitely-not-a-token"), None);
            }};
        }

        check!(
            OpKind,
            [Copy, Move, Delete, Trash, Rename, CreateFolder, CreateFile, ArchiveEdit]
        );
        check!(ArchiveSubkind, [Compress, Edit, Extract]);
        check!(Initiator, [User, AiClient, Agent, AgentEdited]);
        check!(ExecutionStatus, [Queued, Running, Done, Failed, Canceled]);
        check!(
            RollbackState,
            [
                NotRollbackable,
                Rollbackable,
                RollingBack,
                RolledBack,
                PartiallyRolledBack
            ]
        );
        check!(
            NotRollbackableReason,
            [
                Overwrote,
                PermanentDelete,
                ArchiveOverwrite,
                ZipEditUnsupported,
                JournalIncomplete,
                DirectoryMerge,
                StagedConflictResolved
            ]
        );
        check!(SearchCoverage, [Full, TopLevelOnly]);
        check!(
            SearchCoverageReason,
            [Capped, IndexAbsent, IndexStale, VolumeNotLive, SearchRowIncomplete]
        );
        check!(EntryType, [File, Dir]);
        check!(RowRole, [RollbackUnit, SearchOnly]);
        check!(ItemOutcome, [Done, Skipped, Failed, RolledBack]);
        check!(
            SkipReason,
            [
                UnverifiablePrecondition,
                Drift,
                RestoreTargetOccupied,
                DirNotEmpty,
                AlreadyGone,
                Vanished,
                Failed,
                Locked
            ]
        );
    }
}
