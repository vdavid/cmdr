//! Whether the agent may watch, and whether it may think.
//!
//! Four gaps can stop a wake, and the ORDER matters because each one either asks the user for
//! something or records that they already answered. Consent comes first: asking somebody to grant
//! Full Disk Access, or to paste an API key, for a feature they have not opted into is asking them
//! to widen access for something they may not want at all. AI being off comes next, because there
//! is nothing to ask somebody who turned the feature off, and both remaining gaps would be noise
//! about a feature they are not using. Disk access follows, because it decides whether the agent
//! can SEE anything, and the key is last, because it only decides whether the agent can THINK
//! about what it saw.
//!
//! ⚠️ **Consent outranks [`WakeReadiness::Off`] even though both render as silence**, so the
//! ordering between them is invisible to the user and decided entirely by what each state DOES.
//! `NeedsConsent` is the one state that takes the stored backlog away
//! ([`WakeReadiness::permits_stored_signal`]); ordering `Off` first would let a toggle mask a
//! withdrawn consent and leave that record sitting on disk.
//!
//! Every state is a value with an action to take, and the status corner renders two of the four: a
//! user who declined disk access and a user with a tidy Downloads folder would otherwise see the
//! identical nothing, and only one of those is the feature working.
//!
//! ⚠️ **[`WakeReadiness::NeedsConsent`] and [`WakeReadiness::Off`] are the exceptions, and render
//! as SILENCE.** They are the states of a user who never wanted AI and a user who turned it off,
//! and an always-present nag in front of either is exactly the noise `SuggestedOpsIndicator` hides
//! at zero to avoid. `askCmdr.proactive` being off is the same case: nothing is watching, so the
//! corner has nothing to report. All of those gates are the frontend's, in the status corner's wake
//! indicator; the values here stay complete.

/// What the configured AI provider would do with a send right now.
///
/// A tri-state rather than a `has_api_key: bool`, because that bool collapses "the user turned AI
/// off" and "the user picked a cloud provider and left the key blank" into one `false`, and those
/// two deserve opposite answers: one is a gap to close, the other is an answer already given.
/// `crate::ai::manager::BackendResolution` models the difference already, so this mirrors it rather
/// than re-deciding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderGate {
    /// `ai.provider = "off"`: the user turned AI off.
    Off,
    /// A provider is picked but would refuse a send: a blank key, a local server that is not up, an
    /// unrecognized provider id.
    NotConfigured,
    /// A provider is picked and would answer a send right now.
    Ready,
}

/// What the app knows about the gates, in the polarity each source reports.
///
/// `fda_pending` keeps the name and the sense of `fda_gate::is_fda_pending_runtime()` rather
/// than being normalised to match its neighbours: a field whose meaning is inverted relative
/// to its source is the kind of thing that gets read wrongly once and stays wrong. `provider`
/// follows the same rule against `BackendResolution`, which is why it is not a bool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentGates {
    /// The user has accepted the CURRENT consent copy (`consent::has_current_consent`).
    pub consented: bool,
    /// The Full Disk Access decision is still outstanding.
    pub fda_pending: bool,
    /// What the resolved provider for the interactive slot would do with a send.
    pub provider: ProviderGate,
}

/// What the agent may do right now, and what to ask the user for if the answer is not much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReadiness {
    Ready,
    /// Nothing may be stored and nothing may run. The consent screen is the answer.
    NeedsConsent,
    /// The user turned AI off. Nothing new may be stored and nothing may run, and there is nothing
    /// to ask them for: they already answered. What was stored while AI was on stays.
    Off,
    /// Signal may accumulate, but a digest built now would describe a fraction of the truth:
    /// the flagship scenario reads TCC-protected ground the indexer is not walking yet.
    NeedsFullDiskAccess,
    /// Signal may accumulate; nothing may reach a provider.
    NeedsApiKey,
}

impl WakeReadiness {
    /// Whether the pipeline may ADD to what it stores.
    ///
    /// Consent and the off switch gate this. Admitting rows without consent means keeping a record
    /// of what the user has been doing with their files, for a purpose they have not agreed to; it
    /// would also mean that consenting on a Tuesday hands somebody a backlog of everything they did
    /// since installing. Admitting rows with AI off means growing a pile for a feature that is
    /// switched off, which nothing may ever read. A missing key is different in kind: the user
    /// opted in and left AI on, the gap is one they can close, and the backlog waiting for them is
    /// theirs.
    pub fn admits_to_inbox(self) -> bool {
        !matches!(self, WakeReadiness::NeedsConsent | WakeReadiness::Off)
    }

    /// Whether what is ALREADY stored may be kept.
    ///
    /// ⚠️ **Narrower than [`admits_to_inbox`](Self::admits_to_inbox), and the two must not be
    /// merged.** Only a withdrawn consent takes the backlog away, because consent is the purpose
    /// those rows were kept for. Turning AI off stops the pile growing, but the rows were gathered
    /// under a permission the user gave and has not taken back; deleting them the moment somebody
    /// flips a toggle they can flip straight back would throw away their own signal for nothing.
    pub fn permits_stored_signal(self) -> bool {
        self != WakeReadiness::NeedsConsent
    }

    /// Whether a wake may run a turn. Only a fully ready agent may.
    pub fn may_wake(self) -> bool {
        self == WakeReadiness::Ready
    }
}

/// Which gap to report, in precedence order.
pub fn readiness(gates: AgentGates) -> WakeReadiness {
    if !gates.consented {
        WakeReadiness::NeedsConsent
    } else if gates.provider == ProviderGate::Off {
        WakeReadiness::Off
    } else if gates.fda_pending {
        WakeReadiness::NeedsFullDiskAccess
    } else if gates.provider != ProviderGate::Ready {
        WakeReadiness::NeedsApiKey
    } else {
        WakeReadiness::Ready
    }
}
