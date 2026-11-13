//! The card's wire: one request in, a stream of events out.

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
    /// planned from its own `lstat`.
    #[serde(default)]
    pub is_directory: bool,
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
/// one `fact` per landing in any order, then `factsSettled` once.
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
