//! The one projection of a card for a model: everything about an item that may
//! leave the machine, and nothing else.
//!
//! Both doors a card's facts leave by (the verdict's prompt and the agent tool's
//! result) go through [`for_model`]. ❌ No second serializer of facts
//! for a model.
//!
//! How a silent widening is made impossible:
//!
//! - **Every source payload is destructured with no `..`**, so a field added to a
//!   fact fails to compile here until someone decides to keep, redact, or drop it.
//! - **The output is this module's own types**, never a payload type reused: a
//!   new field on a payload can't ride along. The only shared types are tokens
//!   (enums with no data, held so by [`tokens_are_fieldless`]) and
//!   [`EgressSource`], which IS the redaction.
//! - **Every output field names its disclosure item** ([`AboutDisclosure`]) in
//!   `model_tests.rs`, by destructuring [`ModelCard`] with no `..`; the frontend
//!   renders the consent copy from a `Record` over that enum, so a new item fails
//!   the typecheck until it has words.

use serde::Serialize;

use crate::card::AboutCard;
use crate::digest::{
    AppData, ByteShare, ChildKind, Children, FolderClass, FolderDigest, KindCount, LibraryFolder, SizedChild,
};
use crate::fact::{AboutTarget, Fact, NotApplicableWhy, UnknownWhy};
use crate::owner::{Certainty, HomebrewKind, InstalledApp, OwnerClaim, OwnerPlace};
use crate::payload::{
    AppRef, Bundle, ExecutableCheck, GitContext, GitSnapshot, GitStatus, Identity, Importance, ImportanceBand,
    NotCheckedWhy, RecursiveSize, Signature, Signer, SizeCoverage, SnapshotKind, StapledTicket, TrackedState, Usage,
};
use crate::provenance::{CmdrOperation, Download, EgressSource, LandedBy, StartedBy, redact_for_egress};
use crate::removal::{Installer, Rebuilder, Removal, RemovalConsequence, consequences};
use crate::sniff::{ExtensionClaim, RealType, Sniffed, SniffedKind, TypeAgreement};
use crate::text::{HiddenInName, ItemText};

/// Why an item's contents stay home: the app's sensitive-content gate, in this
/// crate's words. For such a path the projection keeps name-level facts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProtectedReason {
    /// SSH, GPG, cloud, and tool credentials, OS credential stores, password
    /// managers.
    Credentials,
    /// A browser's profile.
    BrowserData,
    /// Mail, messages, and notes.
    MessageStore,
    /// Cmdr's own data folder.
    CmdrData,
    /// The file's text holds a private key.
    PrivateKey,
    /// The host's gate couldn't follow the path's links in time: it fails closed.
    Unresolved,
}

/// A size for a model to read out, with what makes it uncertain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpokenSize {
    /// The bytes.
    pub bytes: u64,
    /// The real size can only be higher.
    pub lower_bound: bool,
    /// The indexer is still changing it.
    pub updating: bool,
}

/// How the host words a number for a model: its own size formatter and its own
/// calendar, so a size or a date reads the same in every tool result. The crate
/// knows no zone and no unit table.
pub trait Speak {
    /// A size, with any uncertainty inside the string (`≥ 1.8 TB`).
    fn size(&self, size: SpokenSize) -> String;
    /// A date (seconds since the Unix epoch) on this machine's calendar.
    fn date(&self, secs: i64) -> String;
}

/// One kind of thing the projection carries, as the cloud AI disclosure names
/// it. Each has its own line in the consent copy, keyed by this token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AboutDisclosure {
    /// What it is, its name, path, sizes, and dates, a folder's file and folder
    /// counts, other names, a link's target, and whether its name hid a
    /// reordering character.
    Identity,
    /// What its first bytes say it is.
    RealType,
    /// The site it was recorded as downloaded from (host only), when, and the
    /// downloading app.
    Download,
    /// An app's name, id, and version, and for it or a disk image who signed
    /// it, whether its executable matches, and its notarization ticket.
    AppAndSigner,
    /// When it was last opened, how often, and which app opens it.
    Use,
    /// The Cmdr operation that put it here: when, its source path and the drive
    /// or server name, and who started it.
    CmdrHistory,
    /// The git repo, branch, and status, or the commit or branch a snapshot is
    /// from and when it last changed there.
    Git,
    /// The app or installer it belongs to, and how Cmdr knows.
    Owner,
    /// How much a folder matters by Cmdr's ranking, and how often Cmdr visited
    /// it.
    Importance,
    /// A folder's class and child count, its biggest children and largest files
    /// by name and size, its mix of kinds, and how much sat unchanged for a year.
    Inside,
    /// An app's folders in the Library, by name and size.
    AppData,
    /// What removing it would mean, worked out from the facts above.
    Removal,
    /// That its contents stay home, and so only name-level facts leave.
    ContentsStayHome,
}

/// What a model gets about one item. Serialize it; nothing else reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCard {
    target: AboutTarget,
    path: ItemText,
    #[serde(skip_serializing_if = "Option::is_none")]
    contents_stay_home: Option<ProtectedReason>,
    /// What removing it would mean, as Cmdr works it out from the rows below.
    /// First among the facts on purpose: a model reading "what is this?" past a
    /// folder's long child lists answered without it (a real-model try,
    /// 2026-11-13), and it's the row a person most needs passed on.
    removal: Vec<ModelRemoval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<ModelFact<ModelIdentity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    real_type: Option<ModelFact<ModelRealType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle: Option<ModelFact<ModelBundle>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<ModelFact<ModelSignature>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    download: Option<ModelFact<ModelDownload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cmdr_operation: Option<ModelFact<ModelOperation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_opened: Option<ModelFact<ModelUsage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_app: Option<ModelFact<ModelApp>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    importance: Option<ModelFact<ModelImportance>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<ModelFact<ModelOwner>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git: Option<ModelFact<ModelGit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recursive_size: Option<ModelFact<ModelSize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    folder_digest: Option<ModelFact<ModelDigest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_data: Option<ModelFact<ModelAppData>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_snapshot: Option<ModelFact<ModelSnapshot>>,
}

/// The card as a model may read it.
///
/// `path` is where the item is; `protected` is the host's sensitive-content
/// gate's answer for it. A protected path keeps identity, the owner claim,
/// and a folder's whole size (all derived from the path or the volume's own
/// metadata); every other row it had answers `Unknown(Protected)`, and the
/// removal row is worked out from what's left. Download sources leave as
/// their host or kind ([`redact_for_egress`]); a raw path used for
/// navigating (an app's bundle, a child, a Library folder) never leaves.
///
/// ❗ A free function, not a method on [`AboutCard`]: an `impl AboutCard` here
/// would make `card` depend on this module while this module reads `card`, a
/// module cycle (`module-cycles`).
pub fn for_model(about: &AboutCard, path: &str, protected: Option<ProtectedReason>, speak: &dyn Speak) -> ModelCard {
    let card = match protected {
        Some(_) => name_level_only(about),
        None => about.clone(),
    };
    let Removal { shown, more } = consequences(&card);
    let AboutCard {
        target,
        identity,
        real_type,
        bundle,
        signature,
        download,
        cmdr_operation,
        last_opened,
        default_app,
        importance,
        owner,
        git,
        recursive_size,
        folder_digest,
        app_data,
        git_snapshot,
        // ❌ Not sent: what the image index recognized has no disclosure line yet.
        image_contents: _,
    } = &card;
    ModelCard {
        target: *target,
        path: ItemText::new(path),
        contents_stay_home: protected,
        identity: row(identity, |identity| ModelIdentity::of(identity, speak)),
        real_type: row(real_type, ModelRealType::of),
        bundle: row(bundle, ModelBundle::of),
        signature: row(signature, ModelSignature::of),
        download: row(download, |download| ModelDownload::of(download, speak)),
        cmdr_operation: row(cmdr_operation, |operation| ModelOperation::of(operation, speak)),
        last_opened: row(last_opened, |usage| ModelUsage::of(usage, speak)),
        default_app: row(default_app, ModelApp::of),
        importance: row(importance, ModelImportance::of),
        owner: row(owner, ModelOwner::of),
        git: row(git, ModelGit::of),
        recursive_size: row(recursive_size, |size| ModelSize::of(size, speak)),
        folder_digest: row(folder_digest, |digest| ModelDigest::of(digest, speak)),
        app_data: row(app_data, |data| ModelAppData::of(data, speak)),
        git_snapshot: row(git_snapshot, |snapshot| ModelSnapshot::of(snapshot, speak)),
        removal: shown
            .iter()
            .chain(&more)
            .map(|consequence| ModelRemoval::of(consequence, speak))
            .collect(),
    }
}

/// The card a protected path may show a model: identity, the owner claim, and a
/// folder's whole size stay; every other planned row answers `Protected`.
/// Destructured with no `..`, so a new row is a decision here.
fn name_level_only(card: &AboutCard) -> AboutCard {
    fn protected<T>(slot: &Option<Fact<T>>) -> Option<Fact<T>> {
        slot.as_ref().map(|_| Fact::unknown(UnknownWhy::Protected))
    }
    let AboutCard {
        target,
        identity,
        real_type,
        bundle,
        signature,
        download,
        cmdr_operation,
        last_opened,
        default_app,
        importance,
        owner,
        git,
        recursive_size,
        folder_digest,
        app_data,
        git_snapshot,
        image_contents,
    } = card;
    AboutCard {
        target: *target,
        // Kept: the name, path, sizes, and dates are what the standing promise
        // says may leave ("their names can still reach your provider"), the owner
        // is read from the path, and a folder's total is the index's metadata.
        identity: identity.clone(),
        owner: owner.clone(),
        recursive_size: recursive_size.clone(),
        // Dropped: what the item or the OS recorded about it (a browser profile's
        // download hosts, a mail folder's contents list, a key file's history).
        real_type: protected(real_type),
        bundle: protected(bundle),
        signature: protected(signature),
        download: protected(download),
        cmdr_operation: protected(cmdr_operation),
        last_opened: protected(last_opened),
        default_app: protected(default_app),
        importance: protected(importance),
        git: protected(git),
        folder_digest: protected(folder_digest),
        app_data: protected(app_data),
        git_snapshot: protected(git_snapshot),
        image_contents: protected(image_contents),
    }
}

/// A fact as a model reads it: an unknown keeps its reason, and the card's
/// prominence (how loudly a row says it) stays home.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "value", rename_all = "camelCase")]
pub(crate) enum ModelFact<T> {
    Known(T),
    Unknown(UnknownWhy),
    NotApplicable(NotApplicableWhy),
    Pending,
}

impl<T> ModelFact<T> {
    fn of<S>(fact: &Fact<S>, project: impl FnOnce(&S) -> T) -> Self {
        match fact {
            Fact::Known(value) => ModelFact::Known(project(value)),
            Fact::Unknown(unknown) => ModelFact::Unknown(unknown.why()),
            Fact::NotApplicable(why) => ModelFact::NotApplicable(*why),
            Fact::Pending => ModelFact::Pending,
        }
    }
}

fn row<S, T>(slot: &Option<Fact<S>>, project: impl FnOnce(&S) -> T) -> Option<ModelFact<T>> {
    slot.as_ref().map(|fact| ModelFact::of(fact, project))
}

fn spoken_date(secs: Option<i64>, speak: &dyn Speak) -> Option<String> {
    secs.map(|secs| speak.date(secs))
}

fn spoken_bytes(bytes: Option<u64>, speak: &dyn Speak) -> Option<String> {
    bytes.map(|bytes| {
        speak.size(SpokenSize {
            bytes,
            lower_bound: false,
            updating: false,
        })
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelIdentity {
    name: ItemText,
    /// Only when true: the name hid a character that reorders text, so it may
    /// read differently elsewhere (part of the name's disclosure line).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    name_hid_reordering: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    logical_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logical_size_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_size_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hard_links: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symlink_target: Option<ItemText>,
}

impl ModelIdentity {
    fn of(identity: &Identity, speak: &dyn Speak) -> Self {
        let Identity {
            name,
            name_hid,
            logical_size,
            physical_size,
            created,
            modified,
            hard_links,
            symlink_target,
        } = identity;
        Self {
            name: name.clone(),
            // An invisible character stays the card's to say: the model never sees
            // it, and telling it one was there would widen the egress line.
            name_hid_reordering: *name_hid == Some(HiddenInName::Reordering),
            logical_size: *logical_size,
            logical_size_human: spoken_bytes(*logical_size, speak),
            physical_size: *physical_size,
            physical_size_human: spoken_bytes(*physical_size, speak),
            created: *created,
            created_human: spoken_date(*created, speak),
            modified: *modified,
            modified_human: spoken_date(*modified, speak),
            hard_links: *hard_links,
            symlink_target: symlink_target.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelRealType {
    bytes_are: SniffedKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extension: Option<ItemText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extension_claims: Option<SniffedKind>,
    agreement: TypeAgreement,
}

impl ModelRealType {
    fn of(real_type: &RealType) -> Self {
        let RealType {
            sniffed: Sniffed { kind, format },
            claimed: ExtensionClaim {
                extension,
                kind: claimed_kind,
            },
            agreement,
        } = real_type;
        Self {
            bytes_are: *kind,
            // A token from Cmdr's own format table (`png`, `mach`), never the
            // item's text.
            format: format.clone(),
            extension: extension.clone(),
            extension_claims: *claimed_kind,
            agreement: *agreement,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelBundle {
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle_id: Option<ItemText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<ItemText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<ItemText>,
}

impl ModelBundle {
    fn of(bundle: &Bundle) -> Self {
        let Bundle {
            bundle_id,
            display_name,
            version,
        } = bundle;
        Self {
            bundle_id: bundle_id.clone(),
            display_name: display_name.clone(),
            version: version.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelSignature {
    signer: ModelSigner,
    ticket: StapledTicket,
    /// Whether the executable was compared: a signer is only "checked" with
    /// `executableMatches`, and is otherwise a claim the item carries.
    checked: ModelCheck,
}

impl ModelSignature {
    fn of(signature: &Signature) -> Self {
        let Signature {
            signer,
            ticket,
            checked,
        } = signature;
        Self {
            signer: ModelSigner::of(signer),
            ticket: *ticket,
            checked: ModelCheck::of(checked),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ModelSigner {
    Unsigned,
    AdHoc,
    DeveloperId { team: ItemText, org: ItemText },
    AppStore,
    Apple,
    Unrecognized,
}

impl ModelSigner {
    fn of(signer: &Signer) -> Self {
        match signer {
            Signer::Unsigned => ModelSigner::Unsigned,
            Signer::AdHoc => ModelSigner::AdHoc,
            Signer::DeveloperId { team, org } => ModelSigner::DeveloperId {
                team: team.clone(),
                org: org.clone(),
            },
            Signer::AppStore => ModelSigner::AppStore,
            Signer::Apple => ModelSigner::Apple,
            Signer::Unrecognized => ModelSigner::Unrecognized,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "why", rename_all = "camelCase")]
pub(crate) enum ModelCheck {
    ExecutableMatches,
    ExecutableDiffers,
    NotChecked(NotCheckedWhy),
}

impl ModelCheck {
    fn of(check: &ExecutableCheck) -> Self {
        match check {
            ExecutableCheck::ExecutableMatches => ModelCheck::ExecutableMatches,
            ExecutableCheck::ExecutableDiffers => ModelCheck::ExecutableDiffers,
            ExecutableCheck::NotChecked(why) => ModelCheck::NotChecked(*why),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelDownload {
    /// Each source by its host or its kind: [`redact_for_egress`], the one
    /// redaction.
    sources: ModelFact<Vec<EgressSource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloaded_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloaded_at_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloaded_by: Option<ItemText>,
    /// The archive whose record this is, for an entry inside one.
    #[serde(skip_serializing_if = "Option::is_none")]
    archive: Option<ItemText>,
}

impl ModelDownload {
    fn of(download: &Download, speak: &dyn Speak) -> Self {
        let Download {
            sources,
            downloaded_at,
            downloaded_by,
            archive,
        } = download;
        Self {
            sources: ModelFact::of(sources, |sources| sources.iter().map(redact_for_egress).collect()),
            downloaded_at: *downloaded_at,
            downloaded_at_human: spoken_date(*downloaded_at, speak),
            downloaded_by: downloaded_by.clone(),
            archive: archive.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelOperation {
    kind: LandedBy,
    at: i64,
    at_human: String,
    initiator: StartedBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<ItemText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_volume: Option<ItemText>,
}

impl ModelOperation {
    fn of(operation: &CmdrOperation, speak: &dyn Speak) -> Self {
        let CmdrOperation {
            kind,
            at,
            initiator,
            source_path,
            source_volume,
            // The card's caveat about the item's own dates, ❌ not sent: the model
            // reads the item's dates on the identity row.
            dates_changed: _,
        } = operation;
        Self {
            kind: *kind,
            at: *at,
            at_human: speak.date(*at),
            initiator: *initiator,
            source_path: source_path.clone(),
            source_volume: source_volume.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelUsage {
    last_opened: i64,
    last_opened_human: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_count: Option<u64>,
}

impl ModelUsage {
    fn of(usage: &Usage, speak: &dyn Speak) -> Self {
        let Usage { last_opened, use_count } = usage;
        Self {
            last_opened: *last_opened,
            last_opened_human: speak.date(*last_opened),
            use_count: *use_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelApp {
    name: ItemText,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<ItemText>,
}

impl ModelApp {
    fn of(app: &AppRef) -> Self {
        let AppRef { name, id } = app;
        Self {
            name: name.clone(),
            id: id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelImportance {
    band: ImportanceBand,
    stale: bool,
    /// The scorer's own tokens, never the item's text.
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visits: Option<u64>,
}

impl ModelImportance {
    fn of(importance: &Importance) -> Self {
        let Importance {
            band,
            stale,
            reasons,
            visits,
        } = importance;
        Self {
            band: *band,
            stale: *stale,
            reasons: reasons.clone(),
            visits: *visits,
        }
    }
}

/// Who an item belongs to and how Cmdr knows (the variant). ❌ No app bundle
/// path: the card keeps those for its reveal action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ModelOwner {
    ByBundleId {
        bundle_id: ItemText,
        place: OwnerPlace,
        app: ModelInstalled,
    },
    ByName {
        app: ItemText,
        place: OwnerPlace,
    },
    InstalledBy {
        pkg_id: ItemText,
        part_of_os: bool,
    },
    Homebrew {
        name: ItemText,
        kind: HomebrewKind,
    },
    InsideApp {
        app: ItemText,
    },
}

impl ModelOwner {
    fn of(claim: &OwnerClaim) -> Self {
        match claim {
            OwnerClaim::ByBundleId { bundle_id, place, app } => ModelOwner::ByBundleId {
                bundle_id: bundle_id.clone(),
                place: *place,
                app: ModelInstalled::of(app),
            },
            OwnerClaim::ByName {
                app,
                place,
                app_path: _,
            } => ModelOwner::ByName {
                app: app.clone(),
                place: *place,
            },
            OwnerClaim::InstalledBy { pkg_id, part_of_os } => ModelOwner::InstalledBy {
                pkg_id: pkg_id.clone(),
                part_of_os: *part_of_os,
            },
            OwnerClaim::Homebrew { name, kind } => ModelOwner::Homebrew {
                name: name.clone(),
                kind: *kind,
            },
            OwnerClaim::InsideApp { app, app_path: _ } => ModelOwner::InsideApp { app: app.clone() },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ModelInstalled {
    Installed { name: ItemText },
    NotInstalled,
}

impl ModelInstalled {
    fn of(app: &InstalledApp) -> Self {
        match app {
            InstalledApp::Installed { name, path: _ } => ModelInstalled::Installed { name: name.clone() },
            InstalledApp::NotInstalled => ModelInstalled::NotInstalled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelGit {
    repo_root: ItemText,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<ItemText>,
    status: ModelFact<ModelGitStatus>,
}

impl ModelGit {
    fn of(git: &GitContext) -> Self {
        let GitContext {
            repo_root,
            branch,
            status,
        } = git;
        Self {
            repo_root: repo_root.clone(),
            branch: branch.clone(),
            status: ModelFact::of(status, ModelGitStatus::of),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "state", rename_all = "camelCase")]
pub(crate) enum ModelGitStatus {
    Tracked(TrackedState),
    Untracked,
    Ignored,
}

impl ModelGitStatus {
    fn of(status: &GitStatus) -> Self {
        match status {
            GitStatus::Tracked(state) => ModelGitStatus::Tracked(*state),
            GitStatus::Untracked => ModelGitStatus::Untracked,
            GitStatus::Ignored => ModelGitStatus::Ignored,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelSize {
    logical: u64,
    /// With its uncertainty inside the string, so a model that drops the flags
    /// can't state a lower bound as exact.
    logical_human: String,
    physical: u64,
    physical_human: String,
    files: u64,
    folders: u64,
    coverage: SizeCoverage,
    updating: bool,
    has_symlinks: bool,
}

impl ModelSize {
    fn of(size: &RecursiveSize, speak: &dyn Speak) -> Self {
        let RecursiveSize {
            logical,
            physical,
            files,
            folders,
            coverage,
            updating,
            has_symlinks,
        } = size;
        let spoken = |bytes| {
            speak.size(SpokenSize {
                bytes,
                lower_bound: *coverage == SizeCoverage::LowerBound,
                updating: *updating,
            })
        };
        Self {
            logical: *logical,
            logical_human: spoken(*logical),
            physical: *physical,
            physical_human: spoken(*physical),
            files: *files,
            folders: *folders,
            coverage: *coverage,
            updating: *updating,
            has_symlinks: *has_symlinks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelDigest {
    class: FolderClass,
    children: ModelFact<ModelChildren>,
    largest_files: ModelFact<Vec<ModelChild>>,
    unchanged_for_a_year: ModelFact<ModelShare>,
}

impl ModelDigest {
    fn of(digest: &FolderDigest, speak: &dyn Speak) -> Self {
        let FolderDigest {
            class,
            children,
            largest_files,
            unchanged_for_a_year,
        } = digest;
        Self {
            class: *class,
            children: ModelFact::of(children, |children| ModelChildren::of(children, speak)),
            largest_files: ModelFact::of(largest_files, |files| {
                files.iter().map(|file| ModelChild::of(file, speak)).collect()
            }),
            unchanged_for_a_year: ModelFact::of(unchanged_for_a_year, |share| ModelShare::of(share, speak)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelChildren {
    count: u64,
    biggest: Vec<ModelChild>,
    kind_mix: Vec<ModelKindCount>,
}

impl ModelChildren {
    fn of(children: &Children, speak: &dyn Speak) -> Self {
        let Children {
            count,
            biggest,
            kind_mix,
        } = children;
        Self {
            count: *count,
            biggest: biggest.iter().map(|child| ModelChild::of(child, speak)).collect(),
            kind_mix: kind_mix.iter().map(ModelKindCount::of).collect(),
        }
    }
}

/// A child by name and size. ❌ No path: the largest files sit anywhere under
/// the folder, and their paths would map its whole subtree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelChild {
    name: ItemText,
    is_folder: bool,
    size: ModelFact<ModelSize>,
}

impl ModelChild {
    fn of(child: &SizedChild, speak: &dyn Speak) -> Self {
        let SizedChild {
            name,
            path: _,
            is_folder,
            size,
        } = child;
        Self {
            name: name.clone(),
            is_folder: *is_folder,
            size: ModelFact::of(size, |size| ModelSize::of(size, speak)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelKindCount {
    kind: ChildKind,
    count: u64,
}

impl ModelKindCount {
    fn of(kind_count: &KindCount) -> Self {
        let KindCount { kind, count } = kind_count;
        Self {
            kind: *kind,
            count: *count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelShare {
    bytes: u64,
    bytes_human: String,
    of: u64,
    of_human: String,
    at_least: bool,
}

impl ModelShare {
    fn of(share: &ByteShare, speak: &dyn Speak) -> Self {
        let ByteShare { bytes, of, at_least } = share;
        Self {
            bytes: *bytes,
            bytes_human: speak.size(SpokenSize {
                bytes: *bytes,
                lower_bound: *at_least,
                updating: false,
            }),
            of: *of,
            of_human: speak.size(SpokenSize {
                bytes: *of,
                lower_bound: false,
                updating: false,
            }),
            at_least: *at_least,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelAppData {
    folders: Vec<ModelLibraryFolder>,
    not_looked_in: Vec<OwnerPlace>,
    total_bytes: u64,
    total_human: String,
    total_is_lower_bound: bool,
}

impl ModelAppData {
    fn of(data: &AppData, speak: &dyn Speak) -> Self {
        let AppData {
            folders,
            not_looked_in,
            total_bytes,
            total_is_lower_bound,
        } = data;
        Self {
            folders: folders
                .iter()
                .map(|folder| ModelLibraryFolder::of(folder, speak))
                .collect(),
            not_looked_in: not_looked_in.clone(),
            total_bytes: *total_bytes,
            total_human: speak.size(SpokenSize {
                bytes: *total_bytes,
                lower_bound: *total_is_lower_bound,
                updating: false,
            }),
            total_is_lower_bound: *total_is_lower_bound,
        }
    }
}

/// A Library folder by name, place, and size. ❌ No path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelLibraryFolder {
    name: ItemText,
    place: OwnerPlace,
    size: ModelFact<ModelSize>,
}

impl ModelLibraryFolder {
    fn of(folder: &LibraryFolder, speak: &dyn Speak) -> Self {
        let LibraryFolder {
            name,
            path: _,
            place,
            size,
        } = folder;
        Self {
            name: name.clone(),
            place: *place,
            size: ModelFact::of(size, |size| ModelSize::of(size, speak)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelSnapshot {
    repo_root: ItemText,
    kind: SnapshotKind,
    name: ItemText,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_changed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_changed_human: Option<String>,
}

impl ModelSnapshot {
    fn of(snapshot: &GitSnapshot, speak: &dyn Speak) -> Self {
        let GitSnapshot {
            repo_root,
            kind,
            name,
            last_changed,
        } = snapshot;
        Self {
            repo_root: repo_root.clone(),
            kind: *kind,
            name: name.clone(),
            last_changed: *last_changed,
            last_changed_human: spoken_date(*last_changed, speak),
        }
    }
}

/// What removing the item would mean, most painful first. ❌ No variant says or
/// implies a deletion is safe, as in [`RemovalConsequence`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ModelRemoval {
    UncommittedWork,
    HoldsRepoHistory,
    UntrackedNotInGit,
    IgnoredByGit,
    UserDocument,
    RemovesApp { data_left: ModelFact<ModelAppData> },
    AppData { app: ItemText, certainty: Certainty },
    InstalledBy { installer: ModelInstaller },
    LeftoverOfUninstalledApp { bundle_id: ItemText },
    RecordedDownload { host: String },
    GitCanRestore,
    RebuildableOutput { by: Rebuilder },
    RegeneratedCache { app: Option<ItemText> },
    OtherNamesRemain { others: u64 },
    LinkOnly,
    InGitHistory,
    CantTell,
}

impl ModelRemoval {
    fn of(consequence: &RemovalConsequence, speak: &dyn Speak) -> Self {
        use RemovalConsequence as R;
        match consequence {
            R::UncommittedWork => ModelRemoval::UncommittedWork,
            R::HoldsRepoHistory => ModelRemoval::HoldsRepoHistory,
            R::UntrackedNotInGit => ModelRemoval::UntrackedNotInGit,
            R::IgnoredByGit => ModelRemoval::IgnoredByGit,
            R::UserDocument => ModelRemoval::UserDocument,
            R::RemovesApp { data_left } => ModelRemoval::RemovesApp {
                data_left: ModelFact::of(data_left, |data| ModelAppData::of(data, speak)),
            },
            R::AppData { app, certainty } => ModelRemoval::AppData {
                app: app.clone(),
                certainty: *certainty,
            },
            R::InstalledBy { installer } => ModelRemoval::InstalledBy {
                installer: ModelInstaller::of(installer),
            },
            R::LeftoverOfUninstalledApp { bundle_id } => ModelRemoval::LeftoverOfUninstalledApp {
                bundle_id: bundle_id.clone(),
            },
            // A site's host alone: `removal` takes it from a source's parsed,
            // plain-name site, the same host `redact_for_egress` lets out.
            R::RecordedDownload { host } => ModelRemoval::RecordedDownload {
                host: crate::provenance::main_address(host).to_owned(),
            },
            R::GitCanRestore => ModelRemoval::GitCanRestore,
            R::RebuildableOutput { by } => ModelRemoval::RebuildableOutput { by: *by },
            R::RegeneratedCache { app } => ModelRemoval::RegeneratedCache { app: app.clone() },
            R::OtherNamesRemain { others } => ModelRemoval::OtherNamesRemain { others: *others },
            R::LinkOnly => ModelRemoval::LinkOnly,
            R::InGitHistory => ModelRemoval::InGitHistory,
            R::CantTell => ModelRemoval::CantTell,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ModelInstaller {
    Receipt { pkg_id: ItemText, part_of_os: bool },
    Homebrew { name: ItemText, kind: HomebrewKind },
}

impl ModelInstaller {
    fn of(installer: &Installer) -> Self {
        match installer {
            Installer::Receipt { pkg_id, part_of_os } => ModelInstaller::Receipt {
                pkg_id: pkg_id.clone(),
                part_of_os: *part_of_os,
            },
            Installer::Homebrew { name, kind } => ModelInstaller::Homebrew {
                name: name.clone(),
                kind: *kind,
            },
        }
    }
}

/// The payload types the projection shares with the card are tokens: enums with
/// no data, so a variant added later can't carry text out. A cast to `u8`
/// compiles only for a fieldless enum, so a data-carrying variant added to any of
/// these fails the build here, and that type then gets a projection of its own.
#[expect(dead_code, reason = "a compile-time guard: it only has to type-check")]
type Tokens = (
    AboutTarget,
    UnknownWhy,
    NotApplicableWhy,
    SniffedKind,
    TypeAgreement,
    StapledTicket,
    NotCheckedWhy,
    LandedBy,
    StartedBy,
    ImportanceBand,
    SizeCoverage,
    OwnerPlace,
    HomebrewKind,
    Certainty,
    FolderClass,
    ChildKind,
    SnapshotKind,
    TrackedState,
    Rebuilder,
);

#[expect(dead_code, reason = "a compile-time guard: it only has to type-check")]
fn tokens_are_fieldless(tokens: Tokens) -> [u8; 19] {
    let (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s) = tokens;
    [
        a as u8, b as u8, c as u8, d as u8, e as u8, f as u8, g as u8, h as u8, i as u8, j as u8, k as u8, l as u8,
        m as u8, n as u8, o as u8, p as u8, q as u8, r as u8, s as u8,
    ]
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
