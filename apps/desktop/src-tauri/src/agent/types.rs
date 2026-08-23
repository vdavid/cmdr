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

/// What became of one proposal group once the user answered it.
///
/// ⚠️ **`Ran` is written when the operation SETTLES, never when the user clicked approve.** The
/// claim is only a claim: a group can be approved and then skip every file behind a fingerprint
/// mismatch, and an outcome recorded at claim time would teach the agent that the user wanted
/// something they never got. `suggested_ops/bridge/decorator.rs` is where the real answer lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProposalOutcomeKind {
    /// The user said no in the review.
    Rejected,
    /// The user approved it and the operation has since finished, however it finished.
    Ran { done: u32, skipped: u32, failed: u32 },
}

/// One answered proposal group: what was asked, over how much, and what the user did.
///
/// ⚠️ **Both a stored shape and a wire shape**, like the rest of this module's vocabulary: it
/// rides inside a `ConversationEvent`'s persisted JSON, inside the follow-up turn's persisted
/// user message, AND across IPC as a display block. So a field rename here is a change to data
/// already on disk, not a refactor.
///
/// ❌ **No rationale, no file names, no op paths.** A decision line goes into the agent's memory
/// ring, which rides the prefix of every later turn; the group's own display text (a path or a
/// pattern the user already saw) plus a count is the whole of what a lesson needs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecision {
    pub verb: ProposalVerb,
    /// The group's display name: a path or a pattern, so the user's own data rather than copy.
    pub what: String,
    /// The live op count the user was answering about.
    pub ops: u32,
    pub outcome: ProposalOutcomeKind,
}

/// What the user answered across one sweep.
///
/// ⚠️ **One sweep, ❌ never one group.** "Reject all" over an eight-group sweep is eight
/// rejections, and a follow-up turn each would be eight model calls serialized behind one
/// conversation lock. The sweep is the unit the user experienced as one decision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposalOutcomes {
    pub decisions: Vec<ProposalDecision>,
}

impl ProposalOutcomeKind {
    /// The stable token, for the memory ring's line and the anonymous analytics event.
    pub fn as_token(self) -> &'static str {
        match self {
            ProposalOutcomeKind::Rejected => "rejected",
            ProposalOutcomeKind::Ran { .. } => "ran",
        }
    }
}

impl ProposalDecision {
    /// The decision as the MODEL reads it: one line, terse, English.
    ///
    /// ❌ Never render this into the UI. It is what goes into the memory ring and into the
    /// follow-up turn's prompt; the rail says the same numbers in the user's own language.
    pub fn render(&self) -> String {
        let verb = self.verb.as_token();
        let ops = self.ops;
        match self.outcome {
            ProposalOutcomeKind::Rejected => format!("turned down: {verb} {ops} item(s) under {}", self.what),
            ProposalOutcomeKind::Ran { done, skipped, failed } => format!(
                "approved: {verb} {ops} item(s) under {} ({done} done, {skipped} skipped, {failed} failed)",
                self.what
            ),
        }
    }
}

impl ProposalOutcomes {
    /// The sweep as the MODEL reads it: what the user answered, and what to do about it. Empty
    /// renders empty, never a header saying nothing happened: that would spend budget to say
    /// nothing.
    ///
    /// ⚠️ **The instruction lives HERE, not in `SYSTEM_PROMPT`.** This block opens exactly one
    /// kind of turn, and the system prompt rides the cached prefix of every turn the rail and
    /// every wake ever run. A permanent tax on all of them, to steer one, is the wrong trade.
    ///
    /// ❌ Never render this into the UI. It is deliberately English and deliberately shaped for
    /// a prompt; the rail says the same numbers in the user's own language.
    pub fn render(&self) -> String {
        if self.decisions.is_empty() {
            return String::new();
        }
        let mut out = String::from("The user has answered suggestions you made:\n");
        for decision in &self.decisions {
            out.push_str(&format!("{}\n", decision.render()));
        }
        out.push_str(
            "Save what you should do differently next time with memory_write or memory_edit. \
             Then reply in one or two sentences, and ask a single short question only if their \
             reason would change what you suggest next.\n",
        );
        out
    }

    /// Every path the block mentions, in order. The follow-up message's FTS text: the display
    /// names are the user's own data rather than authored copy.
    pub fn paths(&self) -> Vec<&str> {
        self.decisions.iter().map(|decision| decision.what.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(outcome: ProposalOutcomeKind) -> ProposalDecision {
        ProposalDecision {
            verb: ProposalVerb::Trash,
            what: "/Users/dana/Downloads/*.dmg".to_string(),
            ops: 12,
            outcome,
        }
    }

    /// The one line that goes into the agent's memory ring. It has to name the verb, the size,
    /// and the place, because "the user said no" on its own teaches nothing actionable.
    #[test]
    fn a_decision_line_names_the_verb_the_count_and_the_place() {
        let line = decision(ProposalOutcomeKind::Rejected).render();

        assert!(line.contains("turned down"), "{line}");
        assert!(line.contains("trash"), "{line}");
        assert!(line.contains("12"), "{line}");
        assert!(line.contains("/Users/dana/Downloads/*.dmg"), "{line}");
    }

    /// ⚠️ An approval's line carries what RAN, not what was claimed. Without the tallies the
    /// agent would learn that the user got what they approved, which a fingerprint mismatch can
    /// make false for every file in the group.
    #[test]
    fn an_approval_line_carries_what_actually_ran() {
        let line = decision(ProposalOutcomeKind::Ran {
            done: 10,
            skipped: 2,
            failed: 0,
        })
        .render();

        assert!(line.contains("approved"), "{line}");
        assert!(line.contains("10 done, 2 skipped, 0 failed"), "{line}");
    }

    /// The follow-up turn's opener says what happened AND what to do about it: the instruction
    /// lives here rather than in `SYSTEM_PROMPT`, which every other turn would pay for.
    #[test]
    fn the_follow_up_prompt_asks_for_a_lesson_rather_than_only_reporting() {
        let rendered = ProposalOutcomes {
            decisions: vec![decision(ProposalOutcomeKind::Rejected)],
        }
        .render();

        assert!(rendered.contains("turned down: trash"), "{rendered}");
        assert!(rendered.contains("memory_write"), "{rendered}");
    }

    /// An empty block renders empty, never a header saying nothing happened: a turn that spent
    /// budget to report silence is exactly what the wake path already refuses to do.
    #[test]
    fn an_empty_block_says_nothing_at_all() {
        assert_eq!(ProposalOutcomes { decisions: Vec::new() }.render(), "");
    }
}
