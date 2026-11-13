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
/// [`Signature::checked`] is [`SignatureCheck::ExecutableMatches`], the card says
/// "carries a signature from".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    /// Who the signature names.
    pub signer: Signer,
    /// Whether a notarization ticket is stapled to the item.
    pub ticket: StapledTicket,
    /// Whether the main executable was compared against the signature.
    pub checked: SignatureCheck,
}

/// Who a signature names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
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
}

/// Whether the main executable was compared against the signature it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", content = "why", rename_all = "camelCase")]
pub enum SignatureCheck {
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
    /// The strongest signals behind the score, as the scorer's own words
    /// (`projectRoot`), strongest first.
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

/// The git repo around an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GitContext {
    /// The repo's root folder.
    pub repo_root: ItemText,
    /// The checked-out branch. `None`: the head is detached.
    pub branch: Option<ItemText>,
    /// The item's status in the repo. Unknown on its own, since reading it can
    /// mean walking the whole worktree while the root and branch are cheap.
    pub status: Fact<GitStatus>,
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
