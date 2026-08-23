//! The typed vocabulary of the agent store: token-backed enums for the classification
//! columns of `main.db`, and the `token_enum!` macro that declares them.
//!
//! Every classification field the store persists or crosses a boundary with is a typed
//! enum here, never a substring branch. Each carries a compact,
//! stable, human-readable snake_case **token** stored as TEXT, so the DB stays
//! `sqlite3`-inspectable and the enum ↔ storage mapping lives in exactly one place.
//! Renaming a token is a schema change; renaming a variant is free.
//!
//! `AgentRole` (the `messages.role` column) and `ProviderTag` (the `cost_meter.provider`
//! column) are token-backed too, but they live in [`super::llm::types`] because the LLM
//! seam owns them; this module carries the store-only enums.
//!
//! The `token_enum!` macro mirrors the operation log's (`operation_log/types.rs`): the
//! two are deliberately separate copies of a tiny code-generator so each durable store
//! stays self-contained, with no cross-subsystem macro coupling.

/// Declare a token-backed enum once: the variants, their stable DB tokens, `as_token`,
/// and `from_token`. Keeps the two directions in lockstep so they can't drift. The
/// serde/specta wire form (camelCase, for IPC + `bindings.ts`) is SEPARATE from the DB
/// `as_token` (stable snake_case): callers cross IPC as this typed enum, never a string;
/// the store reads/writes via the tokens.
macro_rules! token_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $( $(#[$vmeta:meta])* $variant:ident => $token:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, specta::Type)]
        #[serde(rename_all = "camelCase")]
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
    /// How a conversation was started, stored in the nullable `conversations.origin`
    /// column. NULL means the user started it (the v1 case); a non-null token records a
    /// programmatic origin. Kept as a column (not a migration) so a future
    /// notification-spawned thread is an additive token, not a schema change (spec §3).
    /// v1 never writes a non-null origin; `Notification` is the forward-compat surface
    /// the column exists to hold.
    pub enum ConversationOrigin {
        Notification => "notification",
        /// The one reserved row, created by migration v8 and never shown: it holds what
        /// quiet wakes spent, after their own threads are deleted. It needs a token of its
        /// own rather than masquerading as a wake thread, because the session list hides it
        /// by exactly this token and the rail's thread icon reads the same set.
        QuietWakes => "quiet_wakes",
    }
}

token_enum! {
    /// What a proposal group asks to do, stored in `proposals.verb`.
    ///
    /// A group is exactly ONE call to ONE executor, so the verb decides which executor runs it,
    /// what the group binds besides its ops, whether its ops carry their own destinations, and
    /// how far an approved group can be taken back. All three ride on `GroupIntent`
    /// (`agent/store/proposals/`), which pairs the verb with them so a wrong combination can't
    /// be built.
    pub enum ProposalVerb {
        Move => "move",
        Copy => "copy",
        Trash => "trash",
        Delete => "delete",
        Rename => "rename",
        Compress => "compress",
        Extract => "extract",
    }
}

token_enum! {
    /// Where a group sits in its lifecycle, stored in `proposals.status`.
    ///
    /// `Pending` is the only mutable state: the agent may re-propose a pending group, and
    /// the claim transaction only ever moves a group out of it. Everything else is frozen to
    /// the agent — including `Interrupted`, which is the user's to re-approve or discard.
    pub enum ProposalStatus {
        /// Proposed, waiting for the user. No expiry: it waits as long as it takes.
        Pending => "pending",
        /// The user approved it and the claim transaction handed its ops to the queue.
        Approved => "approved",
        /// Approved, but the app restarted before execution finished, so nothing here knows
        /// what ran. Frozen: the user re-approves (minting a new group) or discards.
        Interrupted => "interrupted",
        /// Execution FINISHED, whichever way it ended: the operation ran to completion, or
        /// the user cancelled it, or it failed. The distinction this carries is "no longer in
        /// flight" versus "we lost track of it" (`Interrupted` above), which is why it is written
        /// when the operation SETTLES rather than only when it succeeds. What happened to
        /// each source is the per-op statuses' job; a cancelled group keeps `pending` rows
        /// for the ops nothing ever reached, and that is the honest record.
        Completed => "completed",
        /// The user said no.
        Rejected => "rejected",
    }
}

token_enum! {
    /// Where one op sits, stored in `proposal_ops.status`. Per-op statuses are what make a
    /// partial apply ("run 11, skip 3") possible.
    pub enum OpStatus {
        /// Part of the group's live op set: what a claim binds and an executor will run.
        Pending => "pending",
        /// The user deselected it at review, so it's outside the accepted set. Kept as a row
        /// (never deleted) so the decision record says what was offered, not just what ran.
        Excluded => "excluded",
        /// It ran and did what it said.
        Done => "done",
        /// The executor passed over it (a fingerprint mismatch, a conflict resolution).
        Skipped => "skipped",
        /// It ran and didn't succeed.
        Failed => "failed",
    }
}

token_enum! {
    /// How far an approved group can be taken back, stored in `proposals.reversible`. A FACT
    /// the review dialog discloses, never a reason to refuse a group: per the guiding
    /// principle, an irreversible group is the user's to approve.
    pub enum Reversibility {
        /// The operation log's `RestoreMove` puts it back: move, trash, rename.
        RestoreMove => "restore_move",
        /// Undone by deleting what was written: copy, and a compress that created a new
        /// archive.
        DeleteWhatWasWritten => "delete_what_was_written",
        /// Nothing takes it back: a permanent delete, or a compress that overwrote an
        /// existing archive (the seed is unconditional and the prior bytes are gone).
        Irreversible => "irreversible",
    }
}
