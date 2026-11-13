//! The coverage vocabulary: a fact and how much of it Cmdr knows.
//!
//! Every answer on the card is a [`Fact`]: known, unknown for a typed reason, not
//! applicable for a typed reason, or still out. ❌ No empty string, zero, or `false`
//! stands in for "don't know": "macOS has no record of opening this"
//! ([`UnknownWhy::NoRecord`]) is not "never opened".

use serde::Serialize;

/// One answer, with its coverage.
///
/// The wire is one-way (Cmdr to the card and to a model), so nothing here
/// deserializes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "state", content = "value", rename_all = "camelCase")]
pub enum Fact<T> {
    /// Cmdr has the answer.
    Known(T),
    /// There may be an answer, and Cmdr doesn't have it.
    Unknown(Unknown),
    /// The question has no answer for this item.
    NotApplicable(NotApplicableWhy),
    /// Asked, and not answered yet.
    Pending,
}

impl<T> Fact<T> {
    /// An unknown answer, with the prominence its reason carries.
    pub fn unknown(why: UnknownWhy) -> Self {
        Fact::Unknown(Unknown::from(why))
    }

    /// The answer, when Cmdr has it.
    pub fn known(&self) -> Option<&T> {
        match self {
            Fact::Known(value) => Some(value),
            Fact::Unknown(_) | Fact::NotApplicable(_) | Fact::Pending => None,
        }
    }

    /// Why the answer is missing, when it's unknown.
    pub fn unknown_why(&self) -> Option<UnknownWhy> {
        match self {
            Fact::Unknown(unknown) => Some(unknown.why()),
            Fact::Known(_) | Fact::NotApplicable(_) | Fact::Pending => None,
        }
    }

    /// Whether the answer is still out.
    pub fn is_pending(&self) -> bool {
        matches!(self, Fact::Pending)
    }
}

/// An unknown answer: why, and how loudly the card says so.
///
/// Built only from an [`UnknownWhy`], so the prominence on the wire is always the
/// one the rule gives and the card never decides what's worth worrying about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Unknown {
    why: UnknownWhy,
    prominence: Prominence,
}

impl Unknown {
    /// Why Cmdr doesn't have the answer.
    pub fn why(&self) -> UnknownWhy {
        self.why
    }

    /// How loudly the card says so.
    pub fn prominence(&self) -> Prominence {
        self.prominence
    }
}

impl From<UnknownWhy> for Unknown {
    fn from(why: UnknownWhy) -> Self {
        Self {
            why,
            prominence: why.prominence(),
        }
    }
}

/// How loudly the card says a fact is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Prominence {
    /// An ordinary absence: one short muted line ("No download record").
    Routine,
    /// Something got in the way: a full sentence saying what.
    Notable,
}

/// Why Cmdr doesn't have an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UnknownWhy {
    /// Nothing recorded it: no download record on a file you made, no last-opened
    /// date in Spotlight, no Cmdr operation that landed this node. ❌ Never "never".
    NoRecord,
    /// The repo's status doesn't list the path, which a clean file, an ignored one,
    /// and one outside the repo all look like.
    NotInStatus,
    /// No installed app answers to the folder's name, somewhere nobody expects an
    /// owner.
    NoInstalledAppMatches,
    /// Cmdr answers what this volume answers, and it doesn't answer this. The card
    /// says so once, in its volume note.
    VolumeCantTell,
    /// A Library folder named like an app, and no installed app has that name.
    LibraryNameMatchesNoApp,
    /// The drive index doesn't cover the path: indexing is off, or it hasn't
    /// reached the place yet. Routine: nothing went wrong, and on a folder it's
    /// the same cause behind the size, what's inside, and the importance, so a
    /// warning per row would be a wall of the same sentence.
    NotIndexed,
    /// The answer didn't arrive inside its budget.
    TimedOut,
    /// The volume stopped answering.
    Unreachable,
    /// The item's volume stopped answering while the card was gathering. Routine:
    /// the card's volume note says so once, at the top, so a sentence per row
    /// would repeat it.
    VolumeStoppedAnswering,
    /// Nothing is at the path any more: it moved or went since the pane listed it.
    NotFound,
    /// The OS refused the read.
    PermissionDenied,
    /// The path is one Cmdr doesn't read into.
    Protected,
    /// A record exists, for a node that has been replaced since.
    NodeChangedSince,
    /// Several candidates fit and nothing picks between them (`Google`,
    /// `Microsoft`).
    Ambiguous,
    /// The record is there and Cmdr can't read it.
    Unparseable,
    /// A turn Cmdr started on its own doesn't touch a network or device volume.
    RefusedInBackground,
}

impl UnknownWhy {
    /// How loudly the card says this reason.
    pub fn prominence(self) -> Prominence {
        match self {
            UnknownWhy::NoRecord
            | UnknownWhy::NotInStatus
            | UnknownWhy::NoInstalledAppMatches
            | UnknownWhy::VolumeCantTell
            | UnknownWhy::VolumeStoppedAnswering
            | UnknownWhy::NotIndexed => Prominence::Routine,
            UnknownWhy::LibraryNameMatchesNoApp
            | UnknownWhy::TimedOut
            | UnknownWhy::Unreachable
            | UnknownWhy::NotFound
            | UnknownWhy::PermissionDenied
            | UnknownWhy::Protected
            | UnknownWhy::NodeChangedSince
            | UnknownWhy::Ambiguous
            | UnknownWhy::Unparseable
            | UnknownWhy::RefusedInBackground => Prominence::Notable,
        }
    }
}

/// Why a question has no answer for this item.
///
/// Rare on the card: [`crate::card::plan`] leaves out a row that can't apply, so
/// this is for what only the job can find out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NotApplicableWhy {
    /// This OS has no code signing to read.
    NoCodeSigningOnThisOs,
    /// The path sits in no git repo.
    NotInARepo,
    /// The folder is no bundle, so it has no bundle id or version.
    NotABundle,
    /// The volume keeps no such thing (a phone has no download record).
    VolumeHasNone,
}

/// Which fact an answer is: one per card row, and one per gathering job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FactKind {
    /// Name, sizes, dates, other names, a symlink's target.
    Identity,
    /// What the bytes are, against what the extension claims.
    RealType,
    /// Bundle id, display name, version.
    Bundle,
    /// The code signature the item carries.
    Signature,
    /// Where the OS recorded it as downloaded from, when, and by which app.
    Download,
    /// The Cmdr copy, move, or extract that put it here.
    CmdrOperation,
    /// When it was last opened, and how often.
    LastOpened,
    /// The app that opens it.
    DefaultApp,
    /// How much the folder matters, and Cmdr's visits to it.
    Importance,
    /// The app or installer the item belongs to.
    Owner,
    /// The git repo around it and its status there.
    Git,
    /// A folder's whole size.
    RecursiveSize,
    /// What a folder holds: its biggest children, largest files, kind mix.
    FolderDigest,
    /// An app's folders in the Library, which removing the app leaves behind.
    AppData,
    /// The git history a `.git` portal entry is a snapshot from.
    GitSnapshot,
}

impl FactKind {
    /// Every kind, in declaration order.
    pub const ALL: [FactKind; 15] = [
        FactKind::Identity,
        FactKind::RealType,
        FactKind::Bundle,
        FactKind::Signature,
        FactKind::Download,
        FactKind::CmdrOperation,
        FactKind::LastOpened,
        FactKind::DefaultApp,
        FactKind::Importance,
        FactKind::Owner,
        FactKind::Git,
        FactKind::RecursiveSize,
        FactKind::FolderDigest,
        FactKind::AppData,
        FactKind::GitSnapshot,
    ];
}

/// What the card is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AboutTarget {
    /// A regular file.
    File,
    /// A plain folder.
    Folder,
    /// An application bundle.
    AppBundle,
    /// A folder the OS shows as one item (a document package, a framework).
    Package,
    /// A symbolic link; the card is about the link, not what it points at.
    Symlink,
    /// An entry inside an archive.
    ArchiveEntry,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_WHYS: [UnknownWhy; 16] = [
        UnknownWhy::NoRecord,
        UnknownWhy::NotInStatus,
        UnknownWhy::NoInstalledAppMatches,
        UnknownWhy::VolumeCantTell,
        UnknownWhy::LibraryNameMatchesNoApp,
        UnknownWhy::NotIndexed,
        UnknownWhy::TimedOut,
        UnknownWhy::Unreachable,
        UnknownWhy::VolumeStoppedAnswering,
        UnknownWhy::NotFound,
        UnknownWhy::PermissionDenied,
        UnknownWhy::Protected,
        UnknownWhy::NodeChangedSince,
        UnknownWhy::Ambiguous,
        UnknownWhy::Unparseable,
        UnknownWhy::RefusedInBackground,
    ];

    /// The rule the card's calm rests on: an ordinary absence is one muted line, and
    /// only something that got in the way earns a sentence.
    #[test]
    fn only_ordinary_absences_are_routine() {
        let routine: Vec<UnknownWhy> = ALL_WHYS
            .into_iter()
            .filter(|why| why.prominence() == Prominence::Routine)
            .collect();
        assert_eq!(
            routine,
            [
                UnknownWhy::NoRecord,
                UnknownWhy::NotInStatus,
                UnknownWhy::NoInstalledAppMatches,
                UnknownWhy::VolumeCantTell,
                UnknownWhy::NotIndexed,
                UnknownWhy::VolumeStoppedAnswering,
            ]
        );
    }

    #[test]
    fn an_unknown_carries_its_prominence_on_the_wire() {
        let routine: Fact<u64> = Fact::unknown(UnknownWhy::NoRecord);
        assert_eq!(
            serde_json::to_value(&routine).expect("a fact serializes"),
            serde_json::json!({ "state": "unknown", "value": { "why": "noRecord", "prominence": "routine" } })
        );
        let notable: Fact<u64> = Fact::unknown(UnknownWhy::TimedOut);
        assert_eq!(
            serde_json::to_value(&notable).expect("a fact serializes"),
            serde_json::json!({ "state": "unknown", "value": { "why": "timedOut", "prominence": "notable" } })
        );
    }

    #[test]
    fn the_four_states_have_their_wire_words() {
        let known: Fact<u64> = Fact::Known(7);
        let not_applicable: Fact<u64> = Fact::NotApplicable(NotApplicableWhy::NoCodeSigningOnThisOs);
        let pending: Fact<u64> = Fact::Pending;
        assert_eq!(
            serde_json::to_value(&known).expect("a fact serializes"),
            serde_json::json!({ "state": "known", "value": 7 })
        );
        assert_eq!(
            serde_json::to_value(&not_applicable).expect("a fact serializes"),
            serde_json::json!({ "state": "notApplicable", "value": "noCodeSigningOnThisOs" })
        );
        assert_eq!(
            serde_json::to_value(&pending).expect("a fact serializes"),
            serde_json::json!({ "state": "pending" })
        );
        assert!(pending.is_pending());
        assert_eq!(known.known(), Some(&7));
        assert_eq!(not_applicable.known(), None);
    }

    #[test]
    fn every_kind_is_listed_once() {
        let mut sorted = FactKind::ALL.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), FactKind::ALL.len());
    }
}
