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
//! The `token_enum!` macro mirrors the operation log's (`crates/cmdr-operation-log/src/types.rs`): the
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
        /// Save a new server. Zero ops: the group's server draft is the whole proposal
        /// (`agent/store/proposals/DETAILS.md` § A server group).
        AddServer => "add_server",
        /// Change a saved server's editable fields. Zero ops, like `AddServer`.
        EditServer => "edit_server",
    }
}

impl ProposalVerb {
    /// Whether a group of this verb proposes a server rather than acting on files. A server
    /// group has no ops and no acceptance record, and takes its own claim.
    pub fn is_server(self) -> bool {
        matches!(self, ProposalVerb::AddServer | ProposalVerb::EditServer)
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
        /// A saved server's settings: forgotten again, or changed back, in Servers.
        ServerSettings => "server_settings",
    }
}

token_enum! {
    /// Which kind of server a server proposal describes, stored inside its draft.
    pub enum DraftProtocol {
        Smb => "smb",
        Sftp => "sftp",
        Webdav => "webdav",
        Nfs => "nfs",
        S3 => "s3",
    }
}

impl DraftProtocol {
    /// The protocol as a person names it, for the line the model reads.
    pub fn label(self) -> &'static str {
        match self {
            DraftProtocol::Smb => "SMB",
            DraftProtocol::Sftp => "SFTP",
            DraftProtocol::Webdav => "WebDAV",
            DraftProtocol::Nfs => "NFS",
            DraftProtocol::S3 => "S3",
        }
    }
}

token_enum! {
    /// What an approved server save left undone, stored beside the group's draft when the save
    /// wrote something but not everything. Absent on a whole save.
    pub enum ServerSaveNote {
        /// An SMB add saved the host but not its share row.
        ShareNotSaved => "share_not_saved",
    }
}

/// What became of one proposal group once the user answered it.
///
/// ⚠️ **`Ran` is written when the operation SETTLES, never when the user clicked approve.** The
/// claim is only a claim: a group can be approved and then skip every file behind a fingerprint
/// mismatch, and an outcome recorded at claim time would teach the agent that the user wanted
/// something they never got. `suggested_ops/bridge/decorator.rs` is where the real answer lands.
///
/// `Running` and `Interrupted` are the thread line's states BEFORE (or instead of) that answer,
/// and only the thread line's: ❌ neither ever reaches the memory ring, which learns from
/// `Ran` and `Rejected` alone (`agent/outcomes.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProposalOutcomeKind {
    /// The user said no in the review.
    Rejected,
    /// The user approved it and the operation has since finished, however it finished.
    Ran { done: u32, skipped: u32, failed: u32 },
    /// The user approved it and the operation is running. The thread line is rewritten in
    /// place when it settles.
    Running,
    /// The user approved it and the app quit before the operation settled. Written by the
    /// next launch, so a line never reads "running" after the run it named is gone.
    Interrupted,
    /// The user approved a server proposal and Cmdr saved it: all of it, unless `note` says
    /// what it left undone. A server group has no ops, so there are no tallies.
    Saved { note: Option<ServerSaveNote> },
}

/// One answered proposal group: what was asked, over how much, and what the user did.
///
/// ⚠️ **Both a stored shape and a wire shape**, like the rest of this module's vocabulary: it
/// rides inside a `ConversationEvent`'s persisted JSON, inside the follow-up turn's persisted
/// user message, AND across IPC as a display block. So a field rename here is a change to data
/// already on disk, not a refactor.
///
/// ❌ **No rationale, no file names, no op paths.** A decision line goes into the agent's memory
/// ring, which reaches every later thread; the group's own display text (a path or a
/// pattern the user already saw) plus a count is the whole of what a lesson needs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecision {
    pub verb: ProposalVerb,
    /// The group's display name: a path or a pattern, so the user's own data rather than copy.
    pub what: String,
    /// The live op count the user was answering about. 0 for a server group, which has none.
    pub ops: u32,
    pub outcome: ProposalOutcomeKind,
    /// The kind of server a server group proposed; absent for a file group. Optional on disk
    /// too, so a line written before servers could be proposed still reads.
    #[serde(default)]
    pub protocol: Option<DraftProtocol>,
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
            ProposalOutcomeKind::Running => "running",
            ProposalOutcomeKind::Interrupted => "interrupted",
            ProposalOutcomeKind::Saved { .. } => "saved",
        }
    }
}

impl ProposalDecision {
    /// The decision as the MODEL reads it: one line, terse, English.
    ///
    /// ❌ Never render this into the UI. It is what goes into the memory ring and into the
    /// follow-up turn's prompt; the rail says the same numbers in the user's own language.
    pub fn render(&self) -> String {
        if self.verb.is_server() {
            return self.render_server();
        }
        let verb = self.verb.as_token();
        let ops = self.ops;
        match self.outcome {
            ProposalOutcomeKind::Rejected => format!("turned down: {verb} {ops} item(s) under {}", self.what),
            ProposalOutcomeKind::Ran { done, skipped, failed } => format!(
                "approved: {verb} {ops} item(s) under {} ({done} done, {skipped} skipped, {failed} failed)",
                self.what
            ),
            ProposalOutcomeKind::Running => {
                format!("approved, still running: {verb} {ops} item(s) under {}", self.what)
            }
            ProposalOutcomeKind::Interrupted => format!(
                "approved, cut short by an app restart: {verb} {ops} item(s) under {}",
                self.what
            ),
            // Only a server group is saved, so a file group never reaches this arm.
            ProposalOutcomeKind::Saved { .. } => format!("approved: {verb} {ops} item(s) under {}", self.what),
        }
    }

    /// A server group's line: what was asked, about which server, and how it ended. No counts:
    /// a server group has no ops.
    fn render_server(&self) -> String {
        let server = match self.protocol {
            Some(protocol) => format!("the {} server {}", protocol.label(), self.what),
            None => format!("the server {}", self.what),
        };
        let asked = match self.verb {
            ProposalVerb::EditServer => format!("change {server}"),
            _ => format!("add {server}"),
        };
        match self.outcome {
            ProposalOutcomeKind::Rejected => format!("turned down: {asked}"),
            ProposalOutcomeKind::Saved { note: None } => match self.verb {
                ProposalVerb::EditServer => format!("approved: saved the changes to {server}"),
                _ => format!("approved: saved {server}"),
            },
            ProposalOutcomeKind::Saved {
                note: Some(ServerSaveNote::ShareNotSaved),
            } => format!("approved: saved {server}, not its share"),
            ProposalOutcomeKind::Running => format!("approved, still saving: {asked}"),
            ProposalOutcomeKind::Interrupted => format!("approved, cut short by an app restart: {asked}"),
            // A server group has no ops to tally, so a file outcome on one says only that it ran.
            ProposalOutcomeKind::Ran { .. } => format!("approved: {asked}"),
        }
    }
}

impl ProposalOutcomes {
    /// The sweep as the MODEL reads it: what the user answered, and what to do about it. Empty
    /// renders empty, never a header saying nothing happened: that would spend budget to say
    /// nothing.
    ///
    /// ⚠️ **The instruction lives in this turn's opener (`agent/prompts/followup.md`), not in
    /// the system prompt.** This block opens exactly one kind of turn, and the system prompt
    /// rides the cached prefix of every turn the rail and every wake ever run. A permanent tax
    /// on all of them, to steer one, is the wrong trade.
    ///
    /// ❌ Never render this into the UI. It is deliberately English and deliberately shaped for
    /// a prompt; the rail says the same numbers in the user's own language.
    pub fn render(&self) -> String {
        if self.decisions.is_empty() {
            return String::new();
        }
        let decisions: String = self
            .decisions
            .iter()
            .map(|decision| format!("{}\n", decision.render()))
            .collect();
        crate::agent::prompts::followup(&decisions)
    }

    /// Every path the block mentions, in order. The follow-up message's FTS text: the display
    /// names are the user's own data rather than authored copy.
    pub fn paths(&self) -> Vec<&str> {
        self.decisions.iter().map(|decision| decision.what.as_str()).collect()
    }
}

/// Who started a turn: the user, or the app on its own.
///
/// Set by each caller of `run_turn`, ❌ never read off the opener's shape: a follow-up rides as
/// `UserTurn::Outcomes` today, and the dispatch gate (`agent::tools::view`) must stay right if
/// one day it doesn't. The turn hands it to every tool call, and analytics reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOrigin {
    /// The user typed into the rail.
    Text,
    /// The proactive loop noticed something and opened a thread of its own.
    Wake,
    /// The follow-up turn the app opens after the user turned a sweep down.
    Outcomes,
    /// RESERVED, set by no production caller: a persisted thread driven again from its
    /// transcript at the user's request. A rail retry is a fresh send, so it runs as `Text`.
    /// ⚠️ A replay the app starts on its own isn't this, and needs an origin of its own.
    Resume,
}

impl TurnOrigin {
    /// The stable analytics token (`ask_cmdr_turn`'s `origin`).
    pub fn as_token(self) -> &'static str {
        match self {
            TurnOrigin::Text => "text",
            TurnOrigin::Wake => "wake",
            TurnOrigin::Outcomes => "outcomes",
            TurnOrigin::Resume => "resume",
        }
    }

    /// Whether the user started this turn. A background turn runs with nobody watching, so a
    /// tool that reaches past the app (`Access::Session`) waits for one the user did start.
    pub fn user_started(self) -> bool {
        match self {
            TurnOrigin::Text | TurnOrigin::Resume => true,
            TurnOrigin::Wake | TurnOrigin::Outcomes => false,
        }
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
            protocol: None,
        }
    }

    fn server_decision(verb: ProposalVerb, outcome: ProposalOutcomeKind) -> ProposalDecision {
        ProposalDecision {
            verb,
            what: "build-box".to_string(),
            ops: 0,
            outcome,
            protocol: Some(DraftProtocol::Sftp),
        }
    }

    /// A server group's line names the protocol and the server, and never a count: "0 items"
    /// would teach the agent nothing and read as if nothing happened.
    #[test]
    fn a_server_decision_line_names_the_server_and_no_count() {
        let saved = server_decision(ProposalVerb::AddServer, ProposalOutcomeKind::Saved { note: None }).render();
        assert_eq!(saved, "approved: saved the SFTP server build-box");

        let rejected = server_decision(ProposalVerb::AddServer, ProposalOutcomeKind::Rejected).render();
        assert_eq!(rejected, "turned down: add the SFTP server build-box");

        let edit = server_decision(ProposalVerb::EditServer, ProposalOutcomeKind::Rejected).render();
        assert_eq!(edit, "turned down: change the SFTP server build-box");
    }

    /// A save that wrote the host but not its share says so, so the agent doesn't believe the
    /// share is there.
    #[test]
    fn a_partial_server_save_says_what_it_left_undone() {
        let mut decision = server_decision(
            ProposalVerb::AddServer,
            ProposalOutcomeKind::Saved {
                note: Some(ServerSaveNote::ShareNotSaved),
            },
        );
        decision.protocol = Some(DraftProtocol::Smb);
        decision.what = "naspolya".to_string();

        assert_eq!(
            decision.render(),
            "approved: saved the SMB server naspolya, not its share"
        );
    }

    /// The protocol is optional on disk: a decision stored before servers could be proposed
    /// carries none, and must still read back.
    #[test]
    fn a_decision_stored_without_a_protocol_still_reads() {
        let stored = r#"{"verb":"trash","what":"/x","ops":1,"outcome":{"kind":"rejected"}}"#;
        let decision: ProposalDecision = serde_json::from_str(stored).expect("an older decision reads");
        assert_eq!(decision.protocol, None);
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
    /// lives here rather than in the system prompt, which every other turn would pay for.
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
