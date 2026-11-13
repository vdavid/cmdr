//! Who an item belongs to, and how Cmdr knows.
//!
//! Each claim carries its evidence (a bundle id in the path, a name match, an
//! installer receipt, a Homebrew path), so the card and a model can word certainty
//! honestly and the removal row can lean on it.

use serde::Serialize;

use crate::text::ItemText;

/// The app or installer an item belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OwnerClaim {
    /// Certain: the path names a bundle id (`~/Library/Containers/<id>`).
    ByBundleId {
        /// The id in the path.
        bundle_id: ItemText,
        /// Which app-data folder the path sits in.
        place: OwnerPlace,
        /// The app that id belongs to, when one is installed.
        app: InstalledApp,
    },
    /// Probable: exactly one installed app answers to the folder's name
    /// (`Application Support/Slack`). Worded "probably".
    ByName {
        /// The app's name.
        app: ItemText,
        /// Which app-data folder the path sits in.
        place: OwnerPlace,
    },
    /// Certain: an installer receipt lists the path.
    InstalledBy {
        /// The receipt's package id.
        pkg_id: ItemText,
        /// Whether the id is Apple's own (`com.apple.*`: part of macOS).
        part_of_os: bool,
    },
    /// Certain: the path sits in a Homebrew cellar or caskroom.
    Homebrew {
        /// The formula's or cask's name.
        name: ItemText,
        /// Which of the two it is.
        kind: HomebrewKind,
    },
    /// Certain: the path sits inside an app bundle.
    InsideApp {
        /// The app's name.
        app: ItemText,
    },
}

/// How sure an ownership match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Certainty {
    /// The path names the app's id, or the item sits inside the app.
    Certain,
    /// One installed app answers to the folder's name. Worded "probably".
    Probable,
}

/// Whether the app a bundle id names is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InstalledApp {
    /// It is.
    Installed {
        /// The app's name.
        name: ItemText,
    },
    /// No app with that id sits in the app folders: the data is left over.
    NotInstalled,
}

/// Which app-data folder an owned path sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OwnerPlace {
    /// `Library/Caches`, `~/.cache`: the app recreates it.
    Caches,
    /// `Library/Containers`: a sandboxed app's whole home.
    Containers,
    /// `Library/Group Containers`: data several apps of one team share.
    GroupContainers,
    /// `Library/Application Support`, `~/.local/share`: the app's own data.
    ApplicationSupport,
    /// `Library/Application Scripts`.
    ApplicationScripts,
    /// `Library/Preferences`, `~/.config`: settings.
    Preferences,
    /// `Library/Saved Application State`.
    SavedState,
    /// `Library/Logs`.
    Logs,
    /// `Library/HTTPStorages` and `Library/WebKit`: web data.
    WebData,
    /// `~/.var/app` or `~/snap`: a sandboxed Linux app's home.
    SandboxHome,
}

/// Which kind of Homebrew package a path belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HomebrewKind {
    /// A formula, under `Cellar`.
    Formula,
    /// A cask, under `Caskroom`.
    Cask,
}
