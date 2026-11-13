//! The card's wire: one request in, a stream of events out.

use cmdr_about::actions::Actions;
use cmdr_about::card::{AboutFact, AboutPlan};
use cmdr_about::headline::Headline;
use cmdr_about::removal::Removal;
use serde::{Deserialize, Serialize};

/// What the card asks about.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AboutRequest {
    /// Minted by the frontend (a UUID), so a card closed before `about_start`
    /// resolves can still cancel it.
    pub request_id: String,
    /// The item's path, as the pane holds it.
    pub path: String,
    /// The pane's volume. `None`: the resolver places the bare path.
    pub volume_id: Option<String>,
    /// Whether the pane listed it as a folder. ❗ Only the plan of an item Cmdr
    /// reaches through its volume reads this (a share, a server, a phone, an
    /// archive, git's history): that plan goes out before the volume answers, so a
    /// volume that stopped answering still gets a card at once. A local item is
    /// planned from its own `lstat`. `None` (the agent's lookup, which has no
    /// pane): the drive index's answer.
    #[serde(default)]
    pub is_directory: Option<bool>,
}

/// One message on a card's channel. Every one carries its request's id, so the
/// card drops whatever isn't for the item it shows now.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AboutEvent {
    /// The request this answers.
    pub request_id: String,
    /// What happened.
    pub update: AboutUpdate,
}

/// What happened on a card's request.
///
/// `planned` always comes first (or `unavailable`, instead of everything), then
/// one `fact` per landing in any order, then `factsSettled` once. The verdict's
/// events interleave with the facts after `planned`: `verdictPlanned` or
/// `verdictUnavailable` first, right after `planned`; then `verdictGiven`,
/// `verdictDelta`s, and one `verdictEnd` (or one `verdictFromEarlier` instead of
/// those three).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AboutUpdate {
    /// Which rows the card has, before any job runs, with the headline and removal
    /// row as they stand with every row still pending.
    Planned {
        /// The sections and rows, in display order.
        plan: AboutPlan,
        /// What the headline and the removal row say now.
        derived: Derived,
    },
    /// One row's answer, with the headline and removal row it leaves.
    Fact {
        /// The answer.
        fact: AboutFact,
        /// What the headline and the removal row say now that it landed.
        derived: Derived,
    },
    /// The item's volume stopped answering: the card's volume note says so from
    /// now on, and every row still waiting lands `volumeStoppedAnswering` right
    /// after. At most once, before `factsSettled`.
    VolumeStoppedAnswering,
    /// Every planned row has its answer. Nothing more about facts follows.
    FactsSettled,
    /// The card can't be gathered at all.
    Unavailable {
        /// Why.
        why: AboutUnavailable,
    },
    /// The card gets a verdict: reserve its block. Right after `planned`.
    VerdictPlanned,
    /// The card gets no verdict, and why. Right after `planned`.
    VerdictUnavailable {
        /// Why.
        why: VerdictUnavailable,
    },
    /// The verdict's call is about to go out with this look inside the item, so the
    /// card can say what's leaving while it leaves. Before the first delta.
    VerdictGiven {
        /// What of the item's contents rides with the facts.
        contents: VerdictContents,
    },
    /// The next piece of the verdict's text. Plain text, ❌ never Markdown or markup:
    /// render it as a text node.
    VerdictDelta {
        /// The text to append.
        text: String,
    },
    /// The verdict ended. Nothing more about it follows.
    VerdictEnd {
        /// How it ended.
        ending: VerdictEnding,
        /// What the call cost, when the provider reported it. A stopped or failed
        /// call reports nothing, so there is nothing to show.
        cost: Option<VerdictCost>,
    },
    /// A verdict this item already got in this session, unchanged since (same path,
    /// node, size, and modification time): no call was made. Instead of
    /// `verdictGiven`, the deltas, and `verdictEnd`.
    VerdictFromEarlier {
        /// What of the item's contents that verdict was given.
        contents: VerdictContents,
        /// The whole text. Plain text, like a delta.
        text: String,
    },
}

/// Why a card gets no verdict. The card words each one; with `aiOff` and
/// `turnedOff` it draws no block at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum VerdictUnavailable {
    /// `ai.provider` is off.
    AiOff,
    /// The person turned the verdict off (`aboutThis.aiVerdict`).
    TurnedOff,
    /// The local model writes no verdict until its answers are measured good.
    LocalOff,
    /// The configured cloud model is of a class whose takes failed the verdict eval's
    /// bar (`gate::writes_verdicts`), so it writes none. Draws nothing, like `localOff`.
    ModelOff,
    /// Cloud AI is picked and not allowed (or allowed under older copy).
    NoCloudConsent,
    /// A cloud provider is picked but has no key or endpoint.
    NotConfigured,
    /// The local model's server isn't running. A verdict never starts it.
    NotReady,
    /// The organization's policy refuses it.
    Managed {
        /// The policy's reason.
        refusal: crate::managed_policy::ManagedAiRefusal,
    },
    /// Too many verdicts started in the last minute: try again in a moment.
    TooManyRequests,
}

/// What of the item's contents rides with the facts to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VerdictContents {
    /// The facts only: a folder (its digest is the look inside), an item on a
    /// volume Cmdr doesn't read bytes from, or a file whose contents gave nothing.
    None,
    /// The first lines of a text file.
    FirstLines,
    /// A PDF's title and first page.
    FirstPage,
    /// The names of an archive's first entries.
    EntryNames,
    /// A photo's camera details, ❌ never its location.
    CameraDetails,
    /// The item is somewhere Cmdr keeps contents home (credentials, a browser
    /// profile, mail): name-level facts only, and nothing from inside.
    Protected,
}

/// How a verdict ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum VerdictEnding {
    /// The model finished.
    Done,
    /// Stopped before the end (Stop, cloud AI turned off, a policy change); the
    /// text so far stands.
    Stopped,
    /// The provider couldn't answer.
    Failed {
        /// What went wrong, by kind.
        problem: VerdictProblem,
    },
}

/// What kept the provider from answering, classified by its typed error, ❌ never
/// by its wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VerdictProblem {
    /// The key was refused, or none is set.
    AuthFailed,
    /// Rate-limited, out of quota, or the plan lacks the model.
    RateLimited,
    /// Couldn't reach the provider, or it timed out.
    Unreachable,
    /// The provider is overloaded right now.
    Overloaded,
    /// The organization's policy refused the request on its way out.
    Managed,
    /// The call ended with no text (a reasoning model that spent its whole reply
    /// thinking). Its input was still billed: `verdictEnd` carries the cost.
    NoAnswer,
    /// Anything else the provider answered.
    Other,
}

/// What one verdict cost, as the cost meter recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VerdictCost {
    /// Input tokens, the cached share included.
    pub prompt_tokens: u32,
    /// Output tokens.
    pub completion_tokens: u32,
    /// Micro-USD; 0 on the local model. Only meaningful when `priced`.
    pub cost_micros: i64,
    /// False when the model isn't in the price table: show "unknown", ❌ never $0.
    pub priced: bool,
}

/// The parts of the card derived from its facts, recomputed with every landing
/// so the card never words a stale one.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Derived {
    /// The one-line summary at the top.
    pub headline: Headline,
    /// What removing it would mean.
    pub removal: Removal,
    /// What the footer offers next.
    pub actions: Actions,
}

/// Why a card can't be gathered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AboutUnavailable {
    /// The named volume isn't one the app knows.
    UnknownVolume,
    /// The path isn't under the named volume's root.
    PathOutsideVolume,
    /// No connected volume holds the path (a dead mount point).
    NoVolumeAtPath,
    /// Nothing is at the path.
    NotFound,
    /// The OS refused to describe it.
    PermissionDenied,
    /// The volume didn't answer in time.
    Unreachable,
    /// The OS answered with something else that stops the read.
    Unreadable,
}
