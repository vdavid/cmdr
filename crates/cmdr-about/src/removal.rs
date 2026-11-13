//! What removing an item would cost, derived from the facts alone.
//!
//! The row speaks in consequences and ❌ never gives a green light: no variant
//! says or implies a deletion is safe, by construction. "Cmdr can't tell" is a
//! valid, honest answer. Several consequences can apply; they come ordered by how
//! much they'd hurt, and the card shows the top [`SHOWN`].

use serde::Serialize;

use crate::card::AboutCard;
use crate::digest::{AppData, FolderClass};
use crate::fact::{AboutTarget, Fact, Prominence, UnknownWhy};
use crate::owner::{Certainty, HomebrewKind, InstalledApp, OwnerClaim, OwnerPlace};
use crate::payload::{GitStatus, TrackedState};
use crate::provenance::{WebSite, WhereFrom};
use crate::text::ItemText;

/// How many consequences the card shows before "More".
const SHOWN: usize = 2;

/// One thing removing the item would mean.
///
/// Declared from the one that hurts most to the one that hurts least, which is
/// the order [`consequences`] returns them in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RemovalConsequence {
    /// It holds changes no commit has.
    UncommittedWork,
    /// It's in a repo and git doesn't track it, so git can't bring it back.
    UntrackedNotInGit,
    /// Git ignores it, so git can't bring it back. ❌ Never read as "a tool can
    /// recreate this": a project ignores its `data/` and its secrets as readily as
    /// its build output.
    IgnoredByGit,
    /// Nothing Cmdr knows of made it or can recreate it.
    UserDocument,
    /// It's an app; removing it leaves its data in the Library.
    RemovesApp {
        /// The app's Library folders, or why Cmdr can't tell.
        data_left: Fact<AppData>,
    },
    /// It's an installed app's data.
    AppData {
        /// The app.
        app: ItemText,
        /// How sure the match is.
        certainty: Certainty,
    },
    /// An installer put it here, and something installed may need it.
    InstalledBy {
        /// What installed it.
        installer: Installer,
    },
    /// It's data of an app that isn't installed.
    LeftoverOfUninstalledApp {
        /// The bundle id the folder is named after.
        bundle_id: ItemText,
    },
    /// The OS recorded where it was downloaded from. ❌ Never "you can get it
    /// again": Cmdr can't tell whether the site still has it.
    RecordedDownload {
        /// The site's host name.
        host: String,
    },
    /// Git tracks it and it matches the last commit, so git can restore it.
    GitCanRestore,
    /// A package manager or build tool can recreate it, which takes time and may
    /// need a network.
    RebuildableOutput {
        /// What recreates it.
        by: Rebuilder,
    },
    /// It's a cache: whatever wrote it writes it again, and is slower until it
    /// has.
    RegeneratedCache {
        /// The app whose cache it is. `None`: Cmdr knows it's a cache and not
        /// whose.
        app: Option<ItemText>,
    },
    /// Other names point at the same content, which stays reachable through
    /// them.
    OtherNamesRemain {
        /// How many other names.
        others: u64,
    },
    /// It's a link: removing it leaves what it points at.
    LinkOnly,
    /// Cmdr can't tell what depends on this.
    CantTell,
}

impl RemovalConsequence {
    /// Where the consequence falls from most to least painful: its declaration
    /// order.
    fn rank(&self) -> u8 {
        match self {
            RemovalConsequence::UncommittedWork => 0,
            RemovalConsequence::UntrackedNotInGit => 1,
            RemovalConsequence::IgnoredByGit => 2,
            RemovalConsequence::UserDocument => 3,
            RemovalConsequence::RemovesApp { .. } => 4,
            RemovalConsequence::AppData { .. } => 5,
            RemovalConsequence::InstalledBy { .. } => 6,
            RemovalConsequence::LeftoverOfUninstalledApp { .. } => 7,
            RemovalConsequence::RecordedDownload { .. } => 8,
            RemovalConsequence::GitCanRestore => 9,
            RemovalConsequence::RebuildableOutput { .. } => 10,
            RemovalConsequence::RegeneratedCache { .. } => 11,
            RemovalConsequence::OtherNamesRemain { .. } => 12,
            RemovalConsequence::LinkOnly => 13,
            RemovalConsequence::CantTell => 14,
        }
    }
}

/// What installed an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Installer {
    /// An installer package, by its receipt.
    Receipt {
        /// The receipt's package id.
        pkg_id: ItemText,
        /// Whether the id is Apple's own: part of macOS.
        part_of_os: bool,
    },
    /// Homebrew.
    Homebrew {
        /// The formula's or cask's name.
        name: ItemText,
        /// Which of the two it is.
        kind: HomebrewKind,
    },
}

/// What can recreate rebuildable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Rebuilder {
    /// A package manager (installed dependencies).
    PackageManager,
    /// A build tool (build output).
    BuildTool,
}

/// The removal row: the consequences the card shows, and the rest behind "More".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    /// The most painful ones, at most [`SHOWN`]. Never empty.
    pub shown: Vec<RemovalConsequence>,
    /// The rest, most painful first.
    pub more: Vec<RemovalConsequence>,
}

/// What removing the card's item would mean, from the facts that have landed.
///
/// Callable at any moment: facts still pending simply don't speak yet, and with
/// no evidence either way the answer is [`RemovalConsequence::CantTell`].
pub fn consequences(card: &AboutCard) -> Removal {
    let mut found: Vec<RemovalConsequence> = Vec::new();

    let git_status = known(&card.git).and_then(|git| git.status.known().copied());
    match git_status {
        Some(GitStatus::Tracked(TrackedState::Modified)) => found.push(RemovalConsequence::UncommittedWork),
        Some(GitStatus::Untracked) => found.push(RemovalConsequence::UntrackedNotInGit),
        Some(GitStatus::Tracked(TrackedState::Clean)) => found.push(RemovalConsequence::GitCanRestore),
        Some(GitStatus::Ignored) | None => {}
    }

    if card.target == AboutTarget::AppBundle {
        found.push(RemovalConsequence::RemovesApp {
            data_left: card
                .app_data
                .clone()
                .unwrap_or_else(|| Fact::unknown(UnknownWhy::VolumeCantTell)),
        });
    }

    let mut cache_owner = None;
    let mut owned_cache = false;
    match known(&card.owner) {
        Some(OwnerClaim::ByBundleId { bundle_id, place, app }) => match app {
            InstalledApp::NotInstalled => found.push(RemovalConsequence::LeftoverOfUninstalledApp {
                bundle_id: bundle_id.clone(),
            }),
            InstalledApp::Installed { name } if *place == OwnerPlace::Caches => {
                cache_owner = Some(name.clone());
                owned_cache = true;
            }
            InstalledApp::Installed { name } => found.push(RemovalConsequence::AppData {
                app: name.clone(),
                certainty: Certainty::Certain,
            }),
        },
        Some(OwnerClaim::ByName { app, place }) if *place == OwnerPlace::Caches => {
            cache_owner = Some(app.clone());
            owned_cache = true;
        }
        Some(OwnerClaim::ByName { app, .. }) => found.push(RemovalConsequence::AppData {
            app: app.clone(),
            certainty: Certainty::Probable,
        }),
        Some(OwnerClaim::InsideApp { app }) => found.push(RemovalConsequence::AppData {
            app: app.clone(),
            certainty: Certainty::Certain,
        }),
        Some(OwnerClaim::InstalledBy { pkg_id, part_of_os }) => found.push(RemovalConsequence::InstalledBy {
            installer: Installer::Receipt {
                pkg_id: pkg_id.clone(),
                part_of_os: *part_of_os,
            },
        }),
        Some(OwnerClaim::Homebrew { name, kind }) => found.push(RemovalConsequence::InstalledBy {
            installer: Installer::Homebrew {
                name: name.clone(),
                kind: *kind,
            },
        }),
        None => {}
    }

    let class = known(&card.folder_digest).map(|digest| digest.class);
    match class {
        Some(FolderClass::Dependencies) => found.push(RemovalConsequence::RebuildableOutput {
            by: Rebuilder::PackageManager,
        }),
        Some(FolderClass::BuildOutput) => found.push(RemovalConsequence::RebuildableOutput {
            by: Rebuilder::BuildTool,
        }),
        Some(FolderClass::Cache | FolderClass::Plain | FolderClass::VersionControl) | None => {}
    }
    // A class Cmdr knows says more than "git ignores it", so it's the one line.
    let class_says_more = matches!(
        class,
        Some(FolderClass::Dependencies | FolderClass::BuildOutput | FolderClass::Cache)
    );
    if git_status == Some(GitStatus::Ignored) && !class_says_more {
        found.push(RemovalConsequence::IgnoredByGit);
    }
    if owned_cache || class == Some(FolderClass::Cache) {
        found.push(RemovalConsequence::RegeneratedCache { app: cache_owner });
    }

    if let Some(host) = known(&card.download).and_then(|download| recorded_host(download.sources.known()?)) {
        found.push(RemovalConsequence::RecordedDownload { host });
    }

    if card.target == AboutTarget::Symlink {
        found.push(RemovalConsequence::LinkOnly);
    } else if let Some(names) = known(&card.identity).and_then(|identity| identity.hard_links)
        && names > 1
    {
        found.push(RemovalConsequence::OtherNamesRemain { others: names - 1 });
    }

    if found.is_empty() {
        found.push(fallback(card));
    }
    found.sort_by_key(RemovalConsequence::rank);
    let more = found.split_off(found.len().min(SHOWN));
    Removal { shown: found, more }
}

/// What the row says when no fact names a consequence: that this looks like the
/// person's own item when the evidence is in and says nothing else, and that
/// Cmdr can't tell when it isn't.
fn fallback(card: &AboutCard) -> RemovalConsequence {
    let evidence = [
        card.owner.as_ref().map(settled_quietly),
        card.git.as_ref().map(settled_quietly),
        card.download.as_ref().map(settled_quietly),
    ];
    let planned: Vec<bool> = evidence.into_iter().flatten().collect();
    let evidence_is_in = !planned.is_empty() && planned.into_iter().all(|settled| settled);
    if evidence_is_in && card.target != AboutTarget::ArchiveEntry {
        RemovalConsequence::UserDocument
    } else {
        RemovalConsequence::CantTell
    }
}

/// Whether a fact has landed with an answer or an ordinary absence. Pending, or
/// unknown because something got in the way, is no evidence.
fn settled_quietly<T>(fact: &Fact<T>) -> bool {
    match fact {
        Fact::Known(_) | Fact::NotApplicable(_) => true,
        Fact::Unknown(unknown) => {
            unknown.prominence() == Prominence::Routine && unknown.why() != UnknownWhy::VolumeCantTell
        }
        Fact::Pending => false,
    }
}

fn known<T>(slot: &Option<Fact<T>>) -> Option<&T> {
    slot.as_ref().and_then(Fact::known)
}

/// The first recorded source that names a site.
fn recorded_host(sources: &[WhereFrom]) -> Option<String> {
    sources.iter().find_map(|source| match source {
        WhereFrom::Web {
            site: WebSite::Domain { host },
            ..
        } => Some(host.clone()),
        WhereFrom::Web { .. }
        | WhereFrom::MailAttachment { .. }
        | WhereFrom::AirDrop { .. }
        | WhereFrom::Other { .. } => None,
    })
}

#[cfg(test)]
#[path = "removal_tests.rs"]
mod tests;
