//! Who an item belongs to, and how Cmdr knows.
//!
//! Each claim carries its evidence (a bundle id in the path, a name match, an
//! installer receipt, a Homebrew path), so the card and a model can word certainty
//! honestly and the removal row can lean on it.
//!
//! Two steps, both pure: [`OwnerQuestion::of`] reads the path alone (which
//! Library folder, which id or name it names), and [`OwnerQuestion::answer`]
//! settles it from what the host looked up ([`OwnerLookups`]). ❌ Nothing here
//! reads inside an app's container: the folder's NAME is the whole answer.

use serde::Serialize;

use crate::fact::{Fact, UnknownWhy};
use crate::text::ItemText;

/// The app or installer an item belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
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
        /// Where the app is, for the card's reveal action: raw, ❌ never shown.
        /// `None`: the lookup didn't say.
        app_path: Option<String>,
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
        /// The app bundle's path, for the card's reveal action: raw, ❌ never
        /// shown.
        app_path: String,
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
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum InstalledApp {
    /// It is.
    Installed {
        /// The app's name.
        name: ItemText,
        /// Where the app is, for the card's reveal action: raw, ❌ never shown.
        /// `None`: the lookup didn't say.
        path: Option<String>,
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

/// What an item's path says about its owner, before anything is looked up.
///
/// Built only by [`OwnerQuestion::of`]. The host reads the variant to know which
/// lookups it will be asked for; [`OwnerQuestion::answer`] does the deciding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerQuestion {
    /// The path names an app's id: `~/Library/Containers/<id>`,
    /// `~/Library/Preferences/<id>.plist`, `~/.var/app/<id>`.
    BundleId {
        /// The id, as the folder or file names it.
        id: String,
        /// Which app-data folder it sits in.
        place: OwnerPlace,
    },
    /// A folder named like an app: `Application Support/Slack`, `~/.config/<name>`.
    Name {
        /// The folder's name.
        name: String,
        /// Which app-data folder it sits in.
        place: OwnerPlace,
        /// Whether it sits in `~/Library`, where a folder no app answers to is
        /// worth a sentence; elsewhere that's an ordinary absence.
        in_library: bool,
    },
    /// Outside the home folder, where installers write (`/Library`, `/usr/local`,
    /// `/opt`): the installer receipts know.
    Receipt,
    /// The path alone answers: a Homebrew cellar or caskroom, or the inside of an
    /// app bundle.
    Answered(OwnerClaim),
    /// Nothing about the path names an owner, so the card has no owner row.
    Nothing,
}

/// An app the host found installed, for matching a folder's id or name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownApp {
    /// The name the OS shows for it (Finder's, or a desktop entry's `Name=`).
    pub name: String,
    /// Its bundle id, or desktop-file id on Linux. `None`: it names none.
    pub id: Option<String>,
    /// Other names it answers to: `CFBundleName`, the bundle folder's name.
    pub other_names: Vec<String>,
    /// Where it is (an app bundle's path), for the card's reveal action. `None`:
    /// the lookup didn't say, or there's no bundle to show (a Linux desktop entry).
    pub path: Option<String>,
}

/// What the host can look up for [`OwnerQuestion::answer`]. Each is asked only
/// when the question needs it, so a Homebrew path never costs an app scan.
pub struct OwnerLookups<'a> {
    /// The apps in the OS's app folders (`/Applications`, `/System/Applications`,
    /// `~/Applications`, their `Utilities`; the desktop entries on Linux). ❌ Never
    /// one on another volume: a backup disk's or a mounted disk image's app isn't
    /// installed.
    pub installed_apps: &'a dyn Fn() -> Vec<KnownApp>,
    /// The app with this exact id somewhere else on this computer's own disk (a
    /// helper under `/Library`, a macOS component), or `None`. ❌ Never one on
    /// another volume or in the Trash.
    pub app_with_id: &'a dyn Fn(&str) -> Option<KnownApp>,
    /// The ids of the installer receipts that list the path. Empty: none does.
    pub receipts: &'a dyn Fn() -> Result<Vec<String>, UnknownWhy>,
}

/// The `~/Library` folders whose children are named after the app that owns them.
/// `Preferences` is apart: its children are files named by an id alone.
const LIBRARY_PLACES: [(&str, OwnerPlace); 10] = [
    ("Containers", OwnerPlace::Containers),
    ("Group Containers", OwnerPlace::GroupContainers),
    ("Application Support", OwnerPlace::ApplicationSupport),
    ("Application Scripts", OwnerPlace::ApplicationScripts),
    ("Caches", OwnerPlace::Caches),
    ("HTTPStorages", OwnerPlace::WebData),
    ("WebKit", OwnerPlace::WebData),
    ("Saved Application State", OwnerPlace::SavedState),
    ("Logs", OwnerPlace::Logs),
    ("Preferences", OwnerPlace::Preferences),
];

/// The home folders outside `~/Library` whose children are named after the app
/// that owns them: the XDG layout, and Linux's sandboxed app homes.
const HOME_PLACES: [(&str, HomePlace); 5] = [
    (".config", HomePlace::Named(OwnerPlace::Preferences)),
    (".local/share", HomePlace::Named(OwnerPlace::ApplicationSupport)),
    (".cache", HomePlace::Named(OwnerPlace::Caches)),
    (".var/app", HomePlace::ById),
    ("snap", HomePlace::ById),
];

#[derive(Clone, Copy)]
enum HomePlace {
    /// Children are named after the app, matched by name: probable.
    Named(OwnerPlace),
    /// Children are named by the app's id (a flatpak id, a snap's name): certain.
    ById,
}

/// Where installers write outside the home folder.
const RECEIPT_ROOTS: [&str; 3] = ["/Library", "/usr/local", "/opt"];

/// Homebrew's prefixes: Apple silicon, Intel, Linux.
const HOMEBREW_PREFIXES: [&str; 3] = ["/opt/homebrew", "/usr/local", "/home/linuxbrew/.linuxbrew"];

/// Suffixes a Library child carries after the id it's named by.
const ID_SUFFIXES: [&str; 3] = [".plist", ".savedState", ".binarycookies"];

impl OwnerQuestion {
    /// What `path` (absolute, `/`-separated) says about its owner, with `home` the
    /// person's home folder (a parameter, so a test or a fixture home never
    /// touches the real one).
    pub fn of(path: &str, home: &str) -> OwnerQuestion {
        let path = path.trim_end_matches('/');
        if let Some((app, app_path)) = inside_an_app(path) {
            return OwnerQuestion::Answered(OwnerClaim::InsideApp {
                app: ItemText::new(app),
                app_path: app_path.to_string(),
            });
        }
        if let Some(claim) = homebrew(path) {
            return OwnerQuestion::Answered(claim);
        }
        if let Some(in_home) = under(path, home.trim_end_matches('/')) {
            return home_question(in_home);
        }
        if RECEIPT_ROOTS.iter().any(|root| under(path, root).is_some()) {
            return OwnerQuestion::Receipt;
        }
        OwnerQuestion::Nothing
    }

    /// The app-data folder `path` sits strictly INSIDE (`~/Library/Application
    /// Support/Slack` for a file deep in it), with `home` as in [`Self::of`]: the
    /// place the card's "go to the app's data" action opens. `None` for the
    /// folder itself, and for a path no app-data folder holds.
    pub fn app_folder_above(path: &str, home: &str) -> Option<String> {
        let path = path.trim_end_matches('/');
        let home = home.trim_end_matches('/');
        let in_home = under(path, home)?;
        let (base, folders): (String, Vec<&str>) = match under(in_home, "Library") {
            Some(_) => (
                format!("{home}/Library"),
                LIBRARY_PLACES.iter().map(|(folder, _)| *folder).collect(),
            ),
            None => (
                home.to_string(),
                HOME_PLACES.iter().map(|(folder, _)| *folder).collect(),
            ),
        };
        let relative = under(path, &base)?;
        folders.into_iter().find_map(|folder| {
            let name = child_of(relative, folder)?;
            let owned = format!("{base}/{folder}/{name}");
            (path.len() > owned.len()).then_some(owned)
        })
    }

    /// The owner, settled from what `lookups` answers.
    pub fn answer(&self, lookups: &OwnerLookups<'_>) -> Fact<OwnerClaim> {
        match self {
            OwnerQuestion::BundleId { id, place } => by_id(id, *place, lookups),
            OwnerQuestion::Name {
                name,
                place,
                in_library,
            } => by_name(name, *place, *in_library, &(lookups.installed_apps)()),
            OwnerQuestion::Receipt => match (lookups.receipts)() {
                Ok(ids) => from_receipts(&ids),
                Err(why) => Fact::unknown(why),
            },
            OwnerQuestion::Answered(claim) => Fact::Known(claim.clone()),
            // The plan leaves the row out, so nothing asks; "no record" if it did.
            OwnerQuestion::Nothing => Fact::unknown(UnknownWhy::NoRecord),
        }
    }
}

/// `path` relative to `root` (empty when it IS the root), or `None` outside it.
fn under<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    if root.is_empty() {
        return None;
    }
    let rest = path.strip_prefix(root)?;
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix('/')
}

/// The app bundle the path sits INSIDE (never the bundle itself): its name (the
/// folder's, without `.app`) and the bundle's path.
fn inside_an_app(path: &str) -> Option<(&str, &str)> {
    let components: Vec<&str> = path.split('/').collect();
    let (_item, parents) = components.split_last()?;
    let mut end = 0;
    for component in parents {
        end += component.len();
        let stem = component
            .len()
            .checked_sub(4)
            .and_then(|stem_len| Some((component.get(..stem_len)?, component.get(stem_len..)?)))
            .filter(|(stem, extension)| !stem.is_empty() && extension.eq_ignore_ascii_case(".app"))
            .map(|(stem, _)| stem);
        if let Some(stem) = stem {
            return path.get(..end).map(|bundle| (stem, bundle));
        }
        // The separator after this component.
        end += 1;
    }
    None
}

fn homebrew(path: &str) -> Option<OwnerClaim> {
    HOMEBREW_PREFIXES.iter().find_map(|prefix| {
        let (shelf, rest) = under(path, prefix)?.split_once('/')?;
        let kind = match shelf {
            "Cellar" => HomebrewKind::Formula,
            "Caskroom" => HomebrewKind::Cask,
            _ => return None,
        };
        let name = rest.split('/').next().filter(|name| !name.is_empty())?;
        Some(OwnerClaim::Homebrew {
            name: ItemText::new(name),
            kind,
        })
    })
}

/// The question for a path inside the home folder, `in_home` relative to it.
fn home_question(in_home: &str) -> OwnerQuestion {
    if let Some(in_library) = under(in_home, "Library") {
        for (folder, place) in LIBRARY_PLACES {
            let Some(name) = child_of(in_library, folder) else {
                continue;
            };
            let id = match place {
                OwnerPlace::GroupContainers => group_id_of(name),
                _ => id_of(name),
            };
            return match (id, place) {
                (Some(id), _) => OwnerQuestion::BundleId { id, place },
                // Only `Application Support` names its folders after an app's name;
                // a plain name elsewhere is a tool's or the system's.
                (None, OwnerPlace::ApplicationSupport) => OwnerQuestion::Name {
                    name: name.to_string(),
                    place,
                    in_library: true,
                },
                (None, _) => OwnerQuestion::Nothing,
            };
        }
        return OwnerQuestion::Nothing;
    }
    for (folder, place) in HOME_PLACES {
        let Some(name) = child_of(in_home, folder) else {
            continue;
        };
        return match place {
            HomePlace::Named(place) => OwnerQuestion::Name {
                name: name.to_string(),
                place,
                in_library: false,
            },
            HomePlace::ById => OwnerQuestion::BundleId {
                id: name.to_string(),
                place: OwnerPlace::SandboxHome,
            },
        };
    }
    OwnerQuestion::Nothing
}

/// The child of `folder` that `path` is or sits under. `None` for `folder`
/// itself, which no one app owns.
fn child_of<'a>(path: &'a str, folder: &str) -> Option<&'a str> {
    under(path, folder)?.split('/').next().filter(|name| !name.is_empty())
}

/// The reverse-DNS id a Library child is named by (`com.eltima.cmd1`, or
/// `com.apple.finder.plist`), or `None` for a plain name.
fn id_of(name: &str) -> Option<String> {
    let id = ID_SUFFIXES
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name);
    is_reverse_dns(id).then(|| id.to_string())
}

/// The id a group container is named by: `group.<id>`, `<TEAM ID>.<id>`, or an
/// id alone.
fn group_id_of(name: &str) -> Option<String> {
    if let Some(id) = name.strip_prefix("group.")
        && is_reverse_dns(id)
    {
        return Some(id.to_string());
    }
    if let Some((team, id)) = name.split_once('.')
        && is_team_id(team)
        && !id.is_empty()
    {
        return Some(id.to_string());
    }
    id_of(name)
}

/// An Apple team id: ten uppercase letters and digits.
fn is_team_id(text: &str) -> bool {
    text.len() == 10
        && text
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

/// `com.example.tool`: two or more dot-separated parts of ASCII letters, digits,
/// `-`, and `_`, the first starting with a letter.
fn is_reverse_dns(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        && parts[0].bytes().next().is_some_and(|byte| byte.is_ascii_alphabetic())
}

/// Whether `id` is `app_id` or one of its own (`<app_id>.<more>`: an extension, a
/// helper), ignoring ASCII case as LaunchServices does.
pub(crate) fn belongs_to(id: &str, app_id: &str) -> bool {
    let Some(head) = id.get(..app_id.len()) else {
        return false;
    };
    head.eq_ignore_ascii_case(app_id) && (id.len() == app_id.len() || id.as_bytes()[app_id.len()] == b'.')
}

fn installed(app: &KnownApp) -> InstalledApp {
    InstalledApp::Installed {
        name: ItemText::new(&app.name),
        path: app.path.clone(),
    }
}

fn claim_by_id(id: &str, place: OwnerPlace, app: InstalledApp) -> Fact<OwnerClaim> {
    Fact::Known(OwnerClaim::ByBundleId {
        bundle_id: ItemText::new(id),
        place,
        app,
    })
}

/// The owner of a folder named by `id`.
///
/// - An installed app whose id is `id` or a prefix of it (`ai.perplexity.mac` owns
///   `ai.perplexity.mac.PerplexityRichNotification`): that app, the most specific.
/// - A group container's id names a team's shared data, not an app, so it also
///   matches apps whose ids extend it, and several of those are ambiguous.
/// - Else the app with that id elsewhere on this disk (a helper under `/Library`).
/// - Else "left over from an app that isn't installed", the most useful fact the
///   row has. ❌ Never said of a group id or of Apple's own (`com.apple.*`): macOS
///   keeps its services outside the app folders, so a miss there says nothing
///   about what's installed.
fn by_id(id: &str, place: OwnerPlace, lookups: &OwnerLookups<'_>) -> Fact<OwnerClaim> {
    let apps = (lookups.installed_apps)();
    let owner = apps
        .iter()
        .filter_map(|app| Some((app, app.id.as_deref()?)))
        .filter(|(_, app_id)| belongs_to(id, app_id))
        .max_by_key(|(_, app_id)| app_id.len());
    if let Some((app, _)) = owner {
        return claim_by_id(id, place, installed(app));
    }
    if place == OwnerPlace::GroupContainers {
        let mut sharing = apps
            .iter()
            .filter(|app| app.id.as_deref().is_some_and(|app_id| belongs_to(app_id, id)));
        match (sharing.next(), sharing.next()) {
            (Some(app), None) => return claim_by_id(id, place, installed(app)),
            (Some(_), Some(_)) => return Fact::unknown(UnknownWhy::Ambiguous),
            (None, _) => {}
        }
    }
    if let Some(app) = (lookups.app_with_id)(id) {
        return claim_by_id(id, place, installed(&app));
    }
    let apples_own = id.get(..10).is_some_and(|head| head.eq_ignore_ascii_case("com.apple."));
    if place == OwnerPlace::GroupContainers || apples_own {
        return Fact::unknown(UnknownWhy::LibraryNameMatchesNoApp);
    }
    claim_by_id(id, place, InstalledApp::NotInstalled)
}

/// The owner of a folder named `name`: probable when exactly one installed app
/// answers to it.
///
/// An app's own name wins (`Slack` → Slack). Failing that, a vendor's name in
/// front of an app's (`Google` → Google Chrome); a vendor with several apps
/// installed (`Google`, `Microsoft`, `Adobe`) is ambiguous, ❌ never a pick.
fn by_name(name: &str, place: OwnerPlace, in_library: bool, apps: &[KnownApp]) -> Fact<OwnerClaim> {
    let folded = name.to_lowercase();
    let names = |app: &KnownApp| -> Vec<String> {
        std::iter::once(&app.name)
            .chain(&app.other_names)
            .map(|candidate| candidate.to_lowercase())
            .collect()
    };
    let exact: Vec<&KnownApp> = apps.iter().filter(|app| names(app).contains(&folded)).collect();
    let vendor = format!("{folded} ");
    let matches = if exact.is_empty() {
        apps.iter()
            .filter(|app| names(app).iter().any(|candidate| candidate.starts_with(&vendor)))
            .collect()
    } else {
        exact
    };
    match matches.as_slice() {
        [app] => Fact::Known(OwnerClaim::ByName {
            app: ItemText::new(&app.name),
            place,
            app_path: app.path.clone(),
        }),
        [] if in_library => Fact::unknown(UnknownWhy::LibraryNameMatchesNoApp),
        [] => Fact::unknown(UnknownWhy::NoInstalledAppMatches),
        _ => Fact::unknown(UnknownWhy::Ambiguous),
    }
}

/// The owner by installer receipt. Several receipts listing one path is common
/// for a folder macOS's own packages share: all Apple's is part of macOS, and two
/// different installers' is ambiguous.
fn from_receipts(ids: &[String]) -> Fact<OwnerClaim> {
    let mut distinct: Vec<&str> = Vec::new();
    for id in ids {
        if !distinct.contains(&id.as_str()) {
            distinct.push(id);
        }
    }
    let apples = |id: &str| id.starts_with("com.apple.");
    match distinct.as_slice() {
        [] => Fact::unknown(UnknownWhy::NoRecord),
        [first, rest @ ..] if rest.is_empty() || distinct.iter().all(|id| apples(id)) => {
            Fact::Known(OwnerClaim::InstalledBy {
                pkg_id: ItemText::new(first),
                part_of_os: apples(first),
            })
        }
        _ => Fact::unknown(UnknownWhy::Ambiguous),
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
