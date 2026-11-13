//! What each fact carries when it's known: identity, bundle, signature, use, git,
//! importance, and a folder's size. The provenance, real-type, owner, and digest
//! payloads sit with their rules, in their own modules.
//!
//! Dates are seconds since the Unix epoch. An `Option` field is a definite "the
//! record has none" (each says which); where Cmdr might not know, the field is a
//! [`Fact`].

use serde::Serialize;

use crate::fact::Fact;
use crate::text::ItemText;

/// The item itself, as its volume describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    /// Its name.
    pub name: ItemText,
    /// What the files claim to be, in bytes. `None`: the volume reports no size
    /// for it (a folder's own size is [`RecursiveSize`]).
    pub logical_size: Option<u64>,
    /// What it occupies on disk, in bytes. `None`: the volume doesn't say.
    pub physical_size: Option<u64>,
    /// When it was created. `None`: the volume keeps no creation date.
    pub created: Option<i64>,
    /// When it last changed. `None`: the volume keeps no such date.
    pub modified: Option<i64>,
    /// How many names point at this node. Above one, removing this name leaves the
    /// content reachable through the others. `None`: the volume has no hard links
    /// to count.
    pub hard_links: Option<u64>,
    /// Where a symlink points, as written. `None`: it isn't a symlink.
    pub symlink_target: Option<ItemText>,
}

/// What a bundle's `Info.plist` says about it. Every string is the bundle's own
/// claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    /// `CFBundleIdentifier`. `None`: the plist names none.
    pub bundle_id: Option<ItemText>,
    /// The display name, falling back to the bundle name. `None`: neither is set.
    pub display_name: Option<ItemText>,
    /// The short version string. `None`: the plist names none.
    pub version: Option<ItemText>,
}

/// The code signature an item carries.
///
/// ❌ Never worded "verified" or "trusted": a signature blob can be copied onto a
/// tampered binary, and the bundle's resources are never hashed. Unless
/// [`Signature::checked`] is [`ExecutableCheck::ExecutableMatches`], the card says
/// "carries a signature from".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    /// Who the signature names.
    pub signer: Signer,
    /// Whether a notarization ticket is stapled to the item.
    pub ticket: StapledTicket,
    /// Whether the main executable was compared against the signature.
    pub checked: ExecutableCheck,
}

impl Signature {
    /// The signature as the host read it, with what a ticket can say settled
    /// here: only a Developer ID (or an unrecognized) signature is the kind Apple
    /// notarizes. Apple's own software and the Mac App Store's aren't notarized,
    /// and unsigned or ad hoc code can't be, so for those a missing ticket is no
    /// news and the ticket reads [`StapledTicket::DoesNotApply`].
    pub fn read(signer: Signer, ticket: StapledTicket, checked: ExecutableCheck) -> Self {
        let ticket = match signer {
            Signer::DeveloperId { .. } | Signer::Unrecognized => ticket,
            Signer::Unsigned | Signer::AdHoc | Signer::AppStore | Signer::Apple => StapledTicket::DoesNotApply,
        };
        Self {
            signer,
            ticket,
            checked,
        }
    }
}

/// Who a signature names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Signer {
    /// No signature at all.
    Unsigned,
    /// Signed with no identity (a local build).
    AdHoc,
    /// A Developer ID certificate.
    DeveloperId {
        /// The team id in the certificate.
        team: ItemText,
        /// The organization name in the certificate.
        org: ItemText,
    },
    /// The Mac App Store's signature.
    AppStore,
    /// Apple's own software signing.
    Apple,
    /// A signature whose authority Cmdr doesn't recognize.
    Unrecognized,
}

/// Whether a notarization ticket is stapled to the item. An absent ticket doesn't
/// say the item was never notarized: macOS can also ask Apple online.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StapledTicket {
    /// A ticket is stapled.
    Present,
    /// No ticket is stapled.
    Absent,
    /// Cmdr didn't look: a disk image keeps its ticket inside its signature,
    /// where Cmdr doesn't read. ❌ Never worded as either answer.
    NotRead,
    /// No ticket belongs on this kind of signature: Apple's own software and the
    /// Mac App Store's aren't notarized, and unsigned or ad hoc code can't be.
    /// Nothing to say ([`Signature::read`] decides it).
    DoesNotApply,
}

/// Whether the main executable was compared against the signature it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", content = "why", rename_all = "camelCase")]
pub enum ExecutableCheck {
    /// The executable matches its signature. Resources weren't hashed.
    ExecutableMatches,
    /// The executable doesn't match the signature it carries.
    ExecutableDiffers,
    /// Nothing was compared.
    NotChecked(NotCheckedWhy),
}

/// Why a signature went unchecked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NotCheckedWhy {
    /// The check didn't finish inside its budget. ❌ Never a pass.
    TimedOut,
    /// There's no signature to check against.
    NothingToCheck,
    /// Cmdr doesn't check this kind of item (an installer package's signature
    /// covers its archive, not an executable).
    NotSupported,
    /// The check couldn't run.
    CheckUnavailable,
}

/// What Spotlight recorded about the item's use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// When it was last opened.
    pub last_opened: i64,
    /// How many times it was opened. `None`: Spotlight recorded no count.
    pub use_count: Option<u64>,
}

/// How far apart Spotlight's last-used date and the last change may sit and still
/// be one event: the save that wrote the file.
const SAVE_IS_THE_USE_SECS: i64 = 2;

/// The last-opened fact from what Spotlight keeps: `kMDItemLastUsedDate`,
/// `kMDItemUseCount`, and the item's modification date.
///
/// Spotlight stamps a last-used date on a file an app WRITES, opened or not (a
/// file `echo` wrote a moment ago has one equal to its creation; verified on
/// macOS 27.0.1 with `mdls`, 2026-11-13). So a date that sits on the last change
/// with no use count is the save, ❌ never "last opened": `NoRecord`. With a use
/// count, an open was counted, and the date stands.
pub fn usage_fact(last_used: Option<i64>, use_count: Option<u64>, modified: Option<i64>) -> Fact<Usage> {
    let Some(last_used) = last_used else {
        return Fact::unknown(crate::fact::UnknownWhy::NoRecord);
    };
    let is_the_save =
        use_count.is_none() && modified.is_some_and(|modified| (last_used - modified).abs() <= SAVE_IS_THE_USE_SECS);
    if is_the_save {
        return Fact::unknown(crate::fact::UnknownWhy::NoRecord);
    }
    Fact::Known(Usage {
        last_opened: last_used,
        use_count,
    })
}

/// An app, as the OS names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppRef {
    /// Its display name.
    pub name: ItemText,
    /// Its bundle id, or desktop-file id on Linux. `None`: the OS gave none.
    pub id: Option<ItemText>,
}

/// How much a folder matters, by Cmdr's own scoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Importance {
    /// Where the folder's score falls.
    pub band: ImportanceBand,
    /// Whether the score predates the volume's latest recompute.
    pub stale: bool,
    /// The strongest signals behind the score, strongest first, as the scorer's
    /// own camelCase tokens for the frontend to word: a signal (`projectMarker`,
    /// `mtimeRecency`, `visitActivity`, `lastUsed`, `extensionDiversity`), a path
    /// class (`projectRoot`, `userContent`), or for a floored folder why it floors
    /// (`nameDenylisted`, `hiddenOrSystem`, `underFlooredAncestor`).
    pub reasons: Vec<String>,
    /// How many times a Cmdr pane visited the folder. `None`: the score keeps no
    /// visit count.
    pub visits: Option<u64>,
}

/// Where a folder's importance score falls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportanceBand {
    /// Among the folders that matter most.
    High,
    /// In the middle.
    Medium,
    /// Scored, and low.
    Low,
    /// Scores zero by design (a cache, build output, a hidden system folder).
    Floored,
}

impl ImportanceBand {
    /// The band a stored `0.0..=1.0` score falls in. `High` starts where Ask Cmdr
    /// calls a folder hot (0.7), so the card and the agent agree on what matters
    /// most; `Medium` covers the next 0.3. A floored folder has no score and is
    /// [`ImportanceBand::Floored`] by its own rule, never by this.
    pub fn of_score(score: f64) -> ImportanceBand {
        if score >= 0.7 {
            ImportanceBand::High
        } else if score >= 0.4 {
            ImportanceBand::Medium
        } else {
            ImportanceBand::Low
        }
    }
}

/// The git repo around an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitContext {
    /// The repo's root folder.
    pub repo_root: ItemText,
    /// The checked-out branch. `None`: the head is detached, or the repo has no
    /// commit yet.
    pub branch: Option<ItemText>,
    /// The item's status in the repo. Unknown on its own, since reading it can
    /// mean walking the whole worktree while the root and branch are cheap.
    pub status: Fact<GitStatus>,
}

/// Where in git's history a `.git` portal entry is a snapshot from: what the path
/// names (a branch, tag, commit, or stash entry) and when a commit last changed
/// the entry there.
///
/// No author: naming one needs a per-entry read `cmdr-git` doesn't offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitSnapshot {
    /// The repo whose history holds it.
    pub repo_root: ItemText,
    /// Which part of the history the path names.
    pub kind: SnapshotKind,
    /// The branch, tag, or stash entry's name, or a commit's full id (the card
    /// shortens it).
    pub name: ItemText,
    /// When a commit last changed it, by the portal's date. `None`: the portal
    /// gave no date.
    pub last_changed: Option<i64>,
}

/// Which part of git's history a portal path names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotKind {
    /// A branch's latest commit.
    Branch,
    /// A tag's commit.
    Tag,
    /// One commit.
    Commit,
    /// A stash entry.
    Stash,
}

/// An item's status in its repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", content = "state", rename_all = "camelCase")]
pub enum GitStatus {
    /// Git tracks it.
    Tracked(TrackedState),
    /// Git doesn't track it, and no ignore rule covers it.
    Untracked,
    /// An ignore rule covers it.
    Ignored,
}

/// A tracked item's state against the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TrackedState {
    /// Identical to the last commit.
    Clean,
    /// Holds changes no commit has.
    Modified,
}

/// A folder's whole size from the drive index, with the index's own coverage
/// flags verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecursiveSize {
    /// Logical bytes over the whole subtree.
    pub logical: u64,
    /// Physical bytes over the whole subtree.
    pub physical: u64,
    /// Files anywhere under the folder.
    pub files: u64,
    /// Folders anywhere under it, itself excluded.
    pub folders: u64,
    /// How much of the subtree the total covers.
    pub coverage: SizeCoverage,
    /// Whether the indexer is behind on this subtree right now.
    pub updating: bool,
    /// Whether the subtree holds symlinks, whose targets the total leaves out.
    pub has_symlinks: bool,
}

/// How much of a subtree a size covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SizeCoverage {
    /// Every folder in the subtree was listed, at the volume's current epoch.
    Exact,
    /// Every folder was listed, some at an older epoch: accurate then, maybe not
    /// now.
    ExactButStale,
    /// Some folder was never listed, so the real size is at least this.
    LowerBound,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fact::UnknownWhy;

    #[test]
    fn a_score_falls_in_its_band_at_the_edges() {
        assert_eq!(ImportanceBand::of_score(1.0), ImportanceBand::High);
        assert_eq!(ImportanceBand::of_score(0.7), ImportanceBand::High);
        assert_eq!(ImportanceBand::of_score(0.699), ImportanceBand::Medium);
        assert_eq!(ImportanceBand::of_score(0.4), ImportanceBand::Medium);
        assert_eq!(ImportanceBand::of_score(0.399), ImportanceBand::Low);
        assert_eq!(ImportanceBand::of_score(0.0), ImportanceBand::Low);
    }

    #[test]
    fn a_ticket_is_only_news_on_a_signature_apple_notarizes() {
        let checked = ExecutableCheck::ExecutableMatches;
        let developer = Signer::DeveloperId {
            team: ItemText::new("T3SS3RA001"),
            org: ItemText::new("Tessera Labs AB"),
        };
        assert_eq!(
            Signature::read(developer, StapledTicket::Absent, checked).ticket,
            StapledTicket::Absent
        );
        assert_eq!(
            Signature::read(Signer::Unrecognized, StapledTicket::Present, checked).ticket,
            StapledTicket::Present
        );
        for signer in [Signer::Apple, Signer::AppStore, Signer::AdHoc, Signer::Unsigned] {
            assert_eq!(
                Signature::read(signer, StapledTicket::Absent, checked).ticket,
                StapledTicket::DoesNotApply
            );
        }
    }

    #[test]
    fn no_last_used_date_is_no_record_never_never() {
        assert_eq!(usage_fact(None, None, Some(100)), Fact::unknown(UnknownWhy::NoRecord));
    }

    /// A file an app just wrote carries a last-used date equal to its change: the
    /// save, not an open.
    #[test]
    fn a_last_used_date_on_the_last_save_with_no_count_is_no_record() {
        assert_eq!(
            usage_fact(Some(1_000), None, Some(1_001)),
            Fact::unknown(UnknownWhy::NoRecord)
        );
        assert_eq!(
            usage_fact(Some(1_000), None, Some(998)),
            Fact::unknown(UnknownWhy::NoRecord)
        );
    }

    #[test]
    fn a_counted_open_or_a_date_apart_from_the_save_stands() {
        let known = |last_opened, use_count| Fact::Known(Usage { last_opened, use_count });
        assert_eq!(usage_fact(Some(1_000), Some(2), Some(1_000)), known(1_000, Some(2)));
        assert_eq!(usage_fact(Some(5_000), None, Some(1_000)), known(5_000, None));
        assert_eq!(usage_fact(Some(5_000), None, None), known(5_000, None));
    }
}
