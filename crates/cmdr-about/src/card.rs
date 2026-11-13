//! The card: which rows it has ([`plan`]) and what each one holds
//! ([`AboutCard`]).
//!
//! The plan is decided before any job runs, from what the host already knows, so
//! the card draws only rows that can have an answer: a row that can't apply is
//! left out, never drawn as "not applicable". A boring card is short.

use serde::Serialize;

use crate::digest::{AppData, FolderDigest};
use crate::fact::{AboutTarget, Fact, FactKind, UnknownWhy};
use crate::owner::OwnerClaim;
use crate::payload::{AppRef, Bundle, GitContext, Identity, Importance, RecursiveSize, Signature, Usage};
use crate::provenance::{CmdrOperation, Download};
use crate::sniff::RealType;
use crate::text::ItemText;

/// One answered fact: what a gathering job returns and what the card receives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", content = "fact", rename_all = "camelCase")]
pub enum AboutFact {
    /// Name, sizes, dates.
    Identity(Fact<Identity>),
    /// The bytes against the extension.
    RealType(Fact<RealType>),
    /// The bundle's own description.
    Bundle(Fact<Bundle>),
    /// The code signature it carries.
    Signature(Fact<Signature>),
    /// The OS's download record.
    Download(Fact<Download>),
    /// The Cmdr operation that put it here.
    CmdrOperation(Fact<CmdrOperation>),
    /// Spotlight's record of its use.
    LastOpened(Fact<Usage>),
    /// The app that opens it.
    DefaultApp(Fact<AppRef>),
    /// How much the folder matters.
    Importance(Fact<Importance>),
    /// Who it belongs to.
    Owner(Fact<OwnerClaim>),
    /// The repo around it.
    Git(Fact<GitContext>),
    /// A folder's whole size.
    RecursiveSize(Fact<RecursiveSize>),
    /// What a folder holds.
    FolderDigest(Fact<FolderDigest>),
    /// What an app leaves in the Library.
    AppData(Fact<AppData>),
}

impl AboutFact {
    /// Which fact this is.
    pub fn kind(&self) -> FactKind {
        match self {
            AboutFact::Identity(_) => FactKind::Identity,
            AboutFact::RealType(_) => FactKind::RealType,
            AboutFact::Bundle(_) => FactKind::Bundle,
            AboutFact::Signature(_) => FactKind::Signature,
            AboutFact::Download(_) => FactKind::Download,
            AboutFact::CmdrOperation(_) => FactKind::CmdrOperation,
            AboutFact::LastOpened(_) => FactKind::LastOpened,
            AboutFact::DefaultApp(_) => FactKind::DefaultApp,
            AboutFact::Importance(_) => FactKind::Importance,
            AboutFact::Owner(_) => FactKind::Owner,
            AboutFact::Git(_) => FactKind::Git,
            AboutFact::RecursiveSize(_) => FactKind::RecursiveSize,
            AboutFact::FolderDigest(_) => FactKind::FolderDigest,
            AboutFact::AppData(_) => FactKind::AppData,
        }
    }

    /// The fact of `kind` that's unknown for `why`: what a job past its budget or
    /// a volume that stopped answering leaves in its row.
    pub fn unknown(kind: FactKind, why: UnknownWhy) -> Self {
        match kind {
            FactKind::Identity => AboutFact::Identity(Fact::unknown(why)),
            FactKind::RealType => AboutFact::RealType(Fact::unknown(why)),
            FactKind::Bundle => AboutFact::Bundle(Fact::unknown(why)),
            FactKind::Signature => AboutFact::Signature(Fact::unknown(why)),
            FactKind::Download => AboutFact::Download(Fact::unknown(why)),
            FactKind::CmdrOperation => AboutFact::CmdrOperation(Fact::unknown(why)),
            FactKind::LastOpened => AboutFact::LastOpened(Fact::unknown(why)),
            FactKind::DefaultApp => AboutFact::DefaultApp(Fact::unknown(why)),
            FactKind::Importance => AboutFact::Importance(Fact::unknown(why)),
            FactKind::Owner => AboutFact::Owner(Fact::unknown(why)),
            FactKind::Git => AboutFact::Git(Fact::unknown(why)),
            FactKind::RecursiveSize => AboutFact::RecursiveSize(Fact::unknown(why)),
            FactKind::FolderDigest => AboutFact::FolderDigest(Fact::unknown(why)),
            FactKind::AppData => AboutFact::AppData(Fact::unknown(why)),
        }
    }
}

/// What this volume, on this OS, can answer about an item. The host fills it per
/// volume kind before any job runs; [`plan`] reads nothing else about the volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeCaps {
    /// The volume's name, for the note.
    pub name: ItemText,
    /// The file's first and last bytes can be read cheaply (the real-type row).
    pub content: bool,
    /// The OS's download record can be read (the xattrs).
    pub download_record: bool,
    /// The item is a path the OS itself can ask about: Spotlight, the default
    /// app, bundles, code signatures, and the owner mapping.
    pub system: bool,
    /// The drive index and the importance scores may cover the path.
    pub index: bool,
    /// Cmdr's operation log may hold what landed here.
    pub operation_log: bool,
    /// A git repo can sit around the path.
    pub git: bool,
}

impl VolumeCaps {
    /// A volume that answers everything: the boot disk and other local disks.
    pub fn everything(name: ItemText) -> Self {
        Self {
            name,
            content: true,
            download_record: true,
            system: true,
            index: true,
            operation_log: true,
            git: true,
        }
    }

    /// A volume that answers an item's name, size, and dates and nothing more: a
    /// phone, or any volume before its own row of capabilities is filled in.
    pub fn identity_only(name: ItemText) -> Self {
        Self {
            name,
            content: false,
            download_record: false,
            system: false,
            index: false,
            operation_log: false,
            git: false,
        }
    }

    fn answers_everything(&self) -> bool {
        self.content && self.download_record && self.system && self.index && self.operation_log && self.git
    }

    fn answers_identity_only(&self) -> bool {
        !(self.content || self.download_record || self.system || self.index || self.operation_log || self.git)
    }
}

/// What the host knows about an item before any job runs.
#[derive(Debug, Clone, Copy)]
pub struct PlanInput<'a> {
    /// What the card is about.
    pub target: AboutTarget,
    /// The name's extension, without its dot. `None`: the name has none.
    pub extension: Option<&'a str>,
    /// Whether the item's executable bit is set.
    pub is_executable: bool,
    /// What its volume can answer.
    pub caps: &'a VolumeCaps,
}

/// A section of the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    /// Where it came from.
    Origin,
    /// What it is.
    Nature,
    /// How it's used.
    Use,
    /// Where it sits: its owner, its repo, an app's data in the Library.
    Context,
    /// What's inside (folders).
    Contents,
    /// What removing it means. Derived from the other facts, so it plans no rows
    /// of its own, and it's always last, right above the actions.
    Removal,
}

/// One section with its rows, in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSection {
    /// Which section.
    pub section: Section,
    /// Its rows, top to bottom.
    pub kinds: Vec<FactKind>,
}

/// Which rows the card has, decided before any job runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AboutPlan {
    /// What the card is about.
    pub target: AboutTarget,
    /// The sections in display order, each with its rows. A section with no row
    /// that can answer is left out, [`Section::Removal`] excepted.
    pub sections: Vec<PlannedSection>,
    /// What the volume can't answer, said once at the top. `None`: it answers
    /// everything.
    pub volume_note: Option<VolumeNote>,
}

impl AboutPlan {
    /// Every planned row, in display order.
    pub fn kinds(&self) -> Vec<FactKind> {
        self.sections
            .iter()
            .flat_map(|section| section.kinds.iter().copied())
            .collect()
    }

    /// Whether `kind` has a row.
    pub fn has(&self, kind: FactKind) -> bool {
        self.sections.iter().any(|section| section.kinds.contains(&kind))
    }
}

/// What a volume can't answer, as parts the frontend words once at the top of the
/// card ("On Naspolya, Cmdr can see the name, size, and dates").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeNote {
    /// The volume's name.
    pub volume: ItemText,
    /// How much it answers.
    pub limit: VolumeLimit,
}

/// How much of the card a volume answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VolumeLimit {
    /// The name, size, and dates.
    IdentityOnly,
    /// More than identity, less than a local disk.
    SomeFacts,
    /// Nothing right now: the volume stopped answering. The host sets this when
    /// its health check runs out, never [`plan`].
    Unreachable,
}

/// Extensions whose files carry a code signature worth reading, beyond anything
/// with its executable bit set.
const SIGNED_EXTENSIONS: [&str; 4] = ["dmg", "pkg", "mpkg", "dylib"];

/// The rows this target can answer on this volume, in display order.
pub fn plan(input: PlanInput<'_>) -> AboutPlan {
    use AboutTarget::{AppBundle, ArchiveEntry, File, Folder, Package, Symlink};
    let PlanInput {
        target,
        extension,
        is_executable,
        caps,
    } = input;
    let is_bundle = matches!(target, AppBundle | Package);
    let is_folder_like = matches!(target, Folder | AppBundle | Package);
    let extension = extension.map(str::to_ascii_lowercase);
    let signable = is_bundle
        || (target == File
            && (is_executable
                || extension
                    .as_deref()
                    .is_some_and(|extension| SIGNED_EXTENSIONS.contains(&extension))));

    let applies = |kind: FactKind| match kind {
        FactKind::Identity => true,
        FactKind::RealType => target == File && caps.content,
        FactKind::Bundle => is_bundle && caps.system,
        FactKind::Signature => signable && caps.system,
        // A symlink carries no record of its own, and an archive's entry answers with
        // the archive's.
        FactKind::Download => target != Symlink && caps.download_record,
        FactKind::CmdrOperation => target != ArchiveEntry && caps.operation_log,
        FactKind::LastOpened => matches!(target, File | AppBundle | Package) && caps.system,
        FactKind::DefaultApp => matches!(target, File | Package) && caps.system,
        FactKind::Importance => target == Folder && caps.index,
        FactKind::Owner => target != ArchiveEntry && caps.system,
        FactKind::Git => target != ArchiveEntry && caps.git,
        FactKind::RecursiveSize => is_folder_like && caps.index,
        FactKind::FolderDigest => target == Folder && caps.index,
        FactKind::AppData => target == AppBundle && caps.index && caps.system,
    };

    // A folder leads with what's inside, an app with what it is, a file with where it
    // came from. What removing it means is always last.
    let order = match target {
        Folder => [
            Section::Contents,
            Section::Nature,
            Section::Context,
            Section::Use,
            Section::Origin,
            Section::Removal,
        ],
        AppBundle | Package => [
            Section::Nature,
            Section::Context,
            Section::Use,
            Section::Origin,
            Section::Contents,
            Section::Removal,
        ],
        File | Symlink | ArchiveEntry => [
            Section::Origin,
            Section::Nature,
            Section::Use,
            Section::Context,
            Section::Contents,
            Section::Removal,
        ],
    };
    let sections = order
        .into_iter()
        .map(|section| PlannedSection {
            section,
            kinds: rows_of(section).iter().copied().filter(|kind| applies(*kind)).collect(),
        })
        .filter(|planned| planned.section == Section::Removal || !planned.kinds.is_empty())
        .collect();

    let volume_note = if caps.answers_everything() {
        None
    } else {
        Some(VolumeNote {
            volume: caps.name.clone(),
            limit: if caps.answers_identity_only() {
                VolumeLimit::IdentityOnly
            } else {
                VolumeLimit::SomeFacts
            },
        })
    };
    AboutPlan {
        target,
        sections,
        volume_note,
    }
}

/// A section's rows, top to bottom. Every kind sits in exactly one section.
fn rows_of(section: Section) -> &'static [FactKind] {
    match section {
        Section::Origin => &[FactKind::Download, FactKind::CmdrOperation],
        Section::Nature => &[
            FactKind::Identity,
            FactKind::RealType,
            FactKind::Bundle,
            FactKind::Signature,
            FactKind::RecursiveSize,
        ],
        Section::Use => &[FactKind::LastOpened, FactKind::DefaultApp, FactKind::Importance],
        Section::Context => &[FactKind::Owner, FactKind::Git, FactKind::AppData],
        Section::Contents => &[FactKind::FolderDigest],
        Section::Removal => &[],
    }
}

/// The card's facts: one slot per kind, `None` for a row the plan left out.
///
/// The host keeps one per open card, applies each answer as it lands, and derives
/// the headline and the removal row from it ([`crate::headline::headline`],
/// [`crate::removal::consequences`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutCard {
    /// What the card is about.
    pub target: AboutTarget,
    /// Name, sizes, dates.
    pub identity: Option<Fact<Identity>>,
    /// The bytes against the extension.
    pub real_type: Option<Fact<RealType>>,
    /// The bundle's own description.
    pub bundle: Option<Fact<Bundle>>,
    /// The code signature it carries.
    pub signature: Option<Fact<Signature>>,
    /// The OS's download record.
    pub download: Option<Fact<Download>>,
    /// The Cmdr operation that put it here.
    pub cmdr_operation: Option<Fact<CmdrOperation>>,
    /// Spotlight's record of its use.
    pub last_opened: Option<Fact<Usage>>,
    /// The app that opens it.
    pub default_app: Option<Fact<AppRef>>,
    /// How much the folder matters.
    pub importance: Option<Fact<Importance>>,
    /// Who it belongs to.
    pub owner: Option<Fact<OwnerClaim>>,
    /// The repo around it.
    pub git: Option<Fact<GitContext>>,
    /// A folder's whole size.
    pub recursive_size: Option<Fact<RecursiveSize>>,
    /// What a folder holds.
    pub folder_digest: Option<Fact<FolderDigest>>,
    /// What an app leaves in the Library.
    pub app_data: Option<Fact<AppData>>,
}

impl AboutCard {
    /// A card with every planned row pending and every other row absent.
    pub fn new(plan: &AboutPlan) -> Self {
        fn slot<T>(plan: &AboutPlan, kind: FactKind) -> Option<Fact<T>> {
            plan.has(kind).then_some(Fact::Pending)
        }
        Self {
            target: plan.target,
            identity: slot(plan, FactKind::Identity),
            real_type: slot(plan, FactKind::RealType),
            bundle: slot(plan, FactKind::Bundle),
            signature: slot(plan, FactKind::Signature),
            download: slot(plan, FactKind::Download),
            cmdr_operation: slot(plan, FactKind::CmdrOperation),
            last_opened: slot(plan, FactKind::LastOpened),
            default_app: slot(plan, FactKind::DefaultApp),
            importance: slot(plan, FactKind::Importance),
            owner: slot(plan, FactKind::Owner),
            git: slot(plan, FactKind::Git),
            recursive_size: slot(plan, FactKind::RecursiveSize),
            folder_digest: slot(plan, FactKind::FolderDigest),
            app_data: slot(plan, FactKind::AppData),
        }
    }

    /// Puts an answer in its row. An answer for a row the plan left out is
    /// refused, so a kind outside the plan never reaches the card.
    pub fn apply(&mut self, fact: AboutFact) -> Applied {
        fn put<T>(slot: &mut Option<Fact<T>>, fact: Fact<T>) -> Applied {
            match slot {
                Some(current) => {
                    *current = fact;
                    Applied::Yes
                }
                None => Applied::NotPlanned,
            }
        }
        match fact {
            AboutFact::Identity(fact) => put(&mut self.identity, fact),
            AboutFact::RealType(fact) => put(&mut self.real_type, fact),
            AboutFact::Bundle(fact) => put(&mut self.bundle, fact),
            AboutFact::Signature(fact) => put(&mut self.signature, fact),
            AboutFact::Download(fact) => put(&mut self.download, fact),
            AboutFact::CmdrOperation(fact) => put(&mut self.cmdr_operation, fact),
            AboutFact::LastOpened(fact) => put(&mut self.last_opened, fact),
            AboutFact::DefaultApp(fact) => put(&mut self.default_app, fact),
            AboutFact::Importance(fact) => put(&mut self.importance, fact),
            AboutFact::Owner(fact) => put(&mut self.owner, fact),
            AboutFact::Git(fact) => put(&mut self.git, fact),
            AboutFact::RecursiveSize(fact) => put(&mut self.recursive_size, fact),
            AboutFact::FolderDigest(fact) => put(&mut self.folder_digest, fact),
            AboutFact::AppData(fact) => put(&mut self.app_data, fact),
        }
    }

    /// The rows still waiting for their answer.
    pub fn pending(&self) -> Vec<FactKind> {
        fn waits<T>(slot: &Option<Fact<T>>) -> bool {
            slot.as_ref().is_some_and(Fact::is_pending)
        }
        let waiting = [
            (FactKind::Identity, waits(&self.identity)),
            (FactKind::RealType, waits(&self.real_type)),
            (FactKind::Bundle, waits(&self.bundle)),
            (FactKind::Signature, waits(&self.signature)),
            (FactKind::Download, waits(&self.download)),
            (FactKind::CmdrOperation, waits(&self.cmdr_operation)),
            (FactKind::LastOpened, waits(&self.last_opened)),
            (FactKind::DefaultApp, waits(&self.default_app)),
            (FactKind::Importance, waits(&self.importance)),
            (FactKind::Owner, waits(&self.owner)),
            (FactKind::Git, waits(&self.git)),
            (FactKind::RecursiveSize, waits(&self.recursive_size)),
            (FactKind::FolderDigest, waits(&self.folder_digest)),
            (FactKind::AppData, waits(&self.app_data)),
        ];
        waiting
            .into_iter()
            .filter_map(|(kind, waits)| waits.then_some(kind))
            .collect()
    }

    /// Gives every row still waiting the answer "unknown, for `why`", and returns
    /// those answers for the host to send. For a volume that stopped answering or
    /// a gathering whose time ran out.
    pub fn settle_pending(&mut self, why: UnknownWhy) -> Vec<AboutFact> {
        let settled: Vec<AboutFact> = self
            .pending()
            .into_iter()
            .map(|kind| AboutFact::unknown(kind, why))
            .collect();
        for fact in &settled {
            self.apply(fact.clone());
        }
        settled
    }
}

/// Whether an answer found its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The row holds the answer now.
    Yes,
    /// The plan has no such row, so the answer was dropped.
    NotPlanned,
}

#[cfg(test)]
#[path = "card_tests.rs"]
mod tests;
