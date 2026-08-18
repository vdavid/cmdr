//! Whether the agent may watch, and whether it may think.
//!
//! Three gaps can stop a wake, and the ORDER matters because each one asks the user for
//! something. Consent comes first: asking somebody to grant Full Disk Access, or to paste an
//! API key, for a feature they have not opted into is asking them to widen access for
//! something they may not want at all. Disk access comes next, because it decides whether the
//! agent can SEE anything, and the key last, because it only decides whether the agent can
//! THINK about what it saw.
//!
//! Every state is a value the indicator renders with an action to take. ❌ None of them is
//! silence: a user who declined disk access and a user with a tidy Downloads folder would
//! otherwise see the identical nothing, and only one of those is the feature working.

/// What the app knows about the three gates, in the polarity each source reports.
///
/// `fda_pending` keeps the name and the sense of `fda_gate::is_fda_pending_runtime()` rather
/// than being normalised to match its neighbours: a field whose meaning is inverted relative
/// to its source is the kind of thing that gets read wrongly once and stays wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentGates {
    /// The user has accepted the CURRENT consent copy (`consent::has_current_consent`).
    pub consented: bool,
    /// The Full Disk Access decision is still outstanding.
    pub fda_pending: bool,
    /// A usable API key is configured for the resolved provider.
    pub has_api_key: bool,
}

/// What the agent may do right now, and what to ask the user for if the answer is not much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReadiness {
    Ready,
    /// Nothing may be stored and nothing may run. The consent screen is the answer.
    NeedsConsent,
    /// Signal may accumulate, but a digest built now would describe a fraction of the truth:
    /// the flagship scenario reads TCC-protected ground the indexer is not walking yet.
    NeedsFullDiskAccess,
    /// Signal may accumulate; nothing may reach a provider.
    NeedsApiKey,
}

impl WakeReadiness {
    /// Whether the pipeline may store what it sees.
    ///
    /// Only consent gates this. Admitting rows means keeping a record of what the user has
    /// been doing with their files, for a purpose they have not agreed to; it would also mean
    /// that consenting on a Tuesday hands somebody a backlog of everything they did since
    /// installing. A missing key is different in kind: the user opted in, the gap is one they
    /// can close, and the backlog waiting for them is theirs.
    pub fn admits_to_inbox(self) -> bool {
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
    } else if gates.fda_pending {
        WakeReadiness::NeedsFullDiskAccess
    } else if !gates.has_api_key {
        WakeReadiness::NeedsApiKey
    } else {
        WakeReadiness::Ready
    }
}
