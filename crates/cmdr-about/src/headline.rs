//! The one deterministic sentence at the top of the card.
//!
//! It names what the item is, its size, and the single strongest origin or use
//! fact, from whatever has landed. It returns parts plus a template key, never
//! English: the frontend words it through the message catalog. It never names a
//! removal consequence (that row has its own place), and every self-reported
//! part is worded as recorded.

use serde::Serialize;

use crate::card::AboutCard;
use crate::fact::{AboutTarget, Fact};
use crate::owner::{Certainty, InstalledApp, OwnerClaim, OwnerPlace};
use crate::payload::{SizeCoverage, SnapshotKind};
use crate::provenance::{LandedBy, WebSite, WhereFrom};
use crate::sniff::{SniffedKind, TypeAgreement};
use crate::text::ItemText;

/// The card's headline, as parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Headline {
    /// What the item is.
    pub what: HeadlineWhat,
    /// Its size. `None`: not known yet, or not known at all.
    pub size: Option<HeadlineSize>,
    /// The strongest origin or use fact, whose variant is the template key.
    pub lead: HeadlineLead,
}

/// What the item is, for the headline's first words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HeadlineWhat {
    /// A file.
    File {
        /// Its family: what the bytes are when Cmdr recognizes them, else what
        /// the extension claims. `None`: neither says.
        kind: Option<SniffedKind>,
        /// Its extension, without the dot. `None`: the name has none, or it
        /// isn't known yet.
        extension: Option<ItemText>,
    },
    /// A folder.
    Folder,
    /// An app.
    App {
        /// Its version, as the bundle states it. `None`: not stated, or not
        /// known yet.
        version: Option<ItemText>,
    },
    /// A package.
    Package,
    /// A symbolic link.
    Symlink,
    /// An entry inside an archive.
    ArchiveEntry,
}

/// A size for the headline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HeadlineSize {
    /// The bytes.
    pub bytes: u64,
    /// Whether the real size may be larger (a folder the index only partly
    /// covers).
    pub at_least: bool,
}

/// The strongest origin or use fact. The variant is the template key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "template", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HeadlineLead {
    /// It sits inside an archive: for an entry, where it comes from.
    InsideArchive {
        /// The archive's name.
        archive: ItemText,
    },
    /// It's git's record of an item at a point in the repo's history.
    FromGitHistory {
        /// Which part of the history.
        kind: SnapshotKind,
        /// The branch, tag, or stash entry's name, or a commit's full id.
        name: ItemText,
    },
    /// The OS recorded it as downloaded from a site.
    DownloadedFrom {
        /// The site's host name.
        host: String,
        /// The app that downloaded it. `None`: the record names none.
        by: Option<ItemText>,
        /// When. `None`: the record has no time.
        at: Option<i64>,
    },
    /// A Cmdr operation put it here.
    PutHereByCmdr {
        /// What the operation did.
        kind: LandedBy,
        /// When.
        at: i64,
        /// The volume it came from. `None`: the same volume, or made new.
        source_volume: Option<ItemText>,
    },
    /// The OS recorded it as a Mail attachment.
    ReceivedByMail {
        /// When. `None`: the record has no time.
        at: Option<i64>,
    },
    /// The OS recorded it as received over AirDrop.
    ReceivedByAirDrop {
        /// When. `None`: the record has no time.
        at: Option<i64>,
    },
    /// It's an installed app's data.
    DataOfApp {
        /// The app.
        app: ItemText,
        /// How sure the match is.
        certainty: Certainty,
    },
    /// It's an installed app's cache (`Library/Caches`, `~/.cache`): said as a
    /// cache, so the headline agrees with the removal row below it.
    CacheOfApp {
        /// The app.
        app: ItemText,
        /// How sure the match is.
        certainty: Certainty,
    },
    /// It's named after an app that isn't installed.
    DataOfMissingApp {
        /// The bundle id it's named after.
        bundle_id: ItemText,
    },
    /// The OS recorded which app downloaded it, and not from where.
    DownloadedBy {
        /// The app.
        app: ItemText,
        /// When. `None`: the record has no time.
        at: Option<i64>,
    },
    /// Spotlight recorded when it was last opened.
    LastOpened {
        /// When.
        at: i64,
    },
    /// Nothing beyond what it is and its size.
    Plain,
}

/// The headline for what has landed on `card` so far. It may gain a size and a
/// lead as facts land.
pub fn headline(card: &AboutCard) -> Headline {
    Headline {
        what: what(card),
        size: size(card),
        lead: lead(card),
    }
}

fn known<T>(slot: &Option<Fact<T>>) -> Option<&T> {
    slot.as_ref().and_then(Fact::known)
}

fn what(card: &AboutCard) -> HeadlineWhat {
    match card.target {
        AboutTarget::File => {
            let real_type = known(&card.real_type);
            // The bytes win over the name: a program named like a picture is a program.
            let kind = real_type.and_then(|found| match found.agreement {
                TypeAgreement::Matches | TypeAgreement::CantTell => {
                    found.claimed.kind.or(found.sniffed.known().map(|sniffed| sniffed.kind))
                }
                TypeAgreement::Mismatch => found.sniffed.known().map(|sniffed| sniffed.kind),
            });
            HeadlineWhat::File {
                kind: kind.filter(|kind| *kind != SniffedKind::Unrecognized),
                extension: real_type.and_then(|found| found.claimed.extension.clone()),
            }
        }
        AboutTarget::Folder => HeadlineWhat::Folder,
        AboutTarget::AppBundle => HeadlineWhat::App {
            version: known(&card.bundle).and_then(|bundle| bundle.version.clone()),
        },
        AboutTarget::Package => HeadlineWhat::Package,
        AboutTarget::Symlink => HeadlineWhat::Symlink,
        AboutTarget::ArchiveEntry => HeadlineWhat::ArchiveEntry,
    }
}

fn size(card: &AboutCard) -> Option<HeadlineSize> {
    if let Some(total) = known(&card.recursive_size) {
        return Some(HeadlineSize {
            bytes: total.logical,
            at_least: total.coverage == SizeCoverage::LowerBound,
        });
    }
    // A folder's own size is its directory entry, which says nothing about it.
    if matches!(
        card.target,
        AboutTarget::Folder | AboutTarget::AppBundle | AboutTarget::Package
    ) {
        return None;
    }
    known(&card.identity)?
        .logical_size
        .map(|bytes| HeadlineSize { bytes, at_least: false })
}

fn lead(card: &AboutCard) -> HeadlineLead {
    if let Some(snapshot) = known(&card.git_snapshot) {
        return HeadlineLead::FromGitHistory {
            kind: snapshot.kind,
            name: snapshot.name.clone(),
        };
    }
    // An entry's download record is its archive's: the archive is the origin to
    // name, and the site it came from is the row's to tell.
    if let Some(archive) = known(&card.download).and_then(|download| download.archive.clone()) {
        return HeadlineLead::InsideArchive { archive };
    }
    let download = known(&card.download);
    let sources = download.and_then(|download| download.sources.known());
    let at = download.and_then(|download| download.downloaded_at);
    let by = download.and_then(|download| download.downloaded_by.clone());

    let site = sources.and_then(|sources| {
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
    });
    if let Some(host) = site {
        return HeadlineLead::DownloadedFrom { host, by, at };
    }
    if let Some(operation) = known(&card.cmdr_operation) {
        return HeadlineLead::PutHereByCmdr {
            kind: operation.kind,
            at: operation.at,
            source_volume: operation.source_volume.clone(),
        };
    }
    match sources.and_then(|sources| sources.first()) {
        Some(WhereFrom::MailAttachment { .. }) => return HeadlineLead::ReceivedByMail { at },
        Some(WhereFrom::AirDrop { .. }) => return HeadlineLead::ReceivedByAirDrop { at },
        Some(WhereFrom::Web { .. } | WhereFrom::Other { .. }) | None => {}
    }
    let of_app = |app: &ItemText, place: OwnerPlace, certainty| {
        if place == OwnerPlace::Caches {
            HeadlineLead::CacheOfApp {
                app: app.clone(),
                certainty,
            }
        } else {
            HeadlineLead::DataOfApp {
                app: app.clone(),
                certainty,
            }
        }
    };
    match known(&card.owner) {
        Some(OwnerClaim::ByBundleId {
            app: InstalledApp::Installed { name, .. },
            place,
            ..
        }) => {
            return of_app(name, *place, Certainty::Certain);
        }
        Some(OwnerClaim::ByBundleId {
            bundle_id,
            app: InstalledApp::NotInstalled,
            ..
        }) => {
            return HeadlineLead::DataOfMissingApp {
                bundle_id: bundle_id.clone(),
            };
        }
        Some(OwnerClaim::ByName { app, place, .. }) => {
            return of_app(app, *place, Certainty::Probable);
        }
        // An installer's or Homebrew's claim, a file inside an app, and a standard
        // folder are context for their own row: they don't say why the item is
        // here in a few words.
        Some(
            OwnerClaim::InstalledBy { .. }
            | OwnerClaim::Homebrew { .. }
            | OwnerClaim::InsideApp { .. }
            | OwnerClaim::StandardFolder { .. },
        )
        | None => {}
    }
    if let Some(app) = by {
        return HeadlineLead::DownloadedBy { app, at };
    }
    if let Some(usage) = known(&card.last_opened) {
        return HeadlineLead::LastOpened { at: usage.last_opened };
    }
    HeadlineLead::Plain
}

#[cfg(test)]
#[path = "headline_tests.rs"]
mod tests;
