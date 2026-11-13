//! The card's next actions, derived from the facts alone.
//!
//! Which actions show, in what order, and which sit behind "More" is decided
//! here, so the footer is the same for the same facts and ❌ never depends on
//! anything a model wrote. The host says what the item's volume lets an action
//! do ([`ActionCaps`]); the facts say whether there's somewhere to go.
//!
//! Two rules hold by construction:
//!
//! - **Removing is never first.** It's always the last action, and when nothing
//!   else comes before it, it sits behind "More", so it's never the button a
//!   press lands on by accident. It goes through the app's own delete flow and
//!   its confirmation; nothing here deletes.
//! - **The only address an action opens is a parsed `http` or `https` one**,
//!   written back out by the URL parser, with its host shown before the press.
//!   An item's download record is attacker-writable: a `file:`, `smb:`, or an
//!   app's own scheme would launch whatever registered it.

use serde::Serialize;
use url::{Host, Url};

use crate::card::AboutCard;
use crate::fact::{AboutTarget, Fact};
use crate::owner::{InstalledApp, OwnerClaim};
use crate::provenance::WhereFrom;
use crate::text::ItemText;

/// How many actions show before "More". Three, plus "More" and Close, keep the
/// footer on one line at the card's default width ("Open", "Show in Finder",
/// "Show TextEdit"); four pushed it past it.
pub(crate) const SHOWN_ACTIONS: usize = 3;

/// What the card's actions can do: what the item's volume allows, filled by the
/// host per way of reaching the item before any job runs, and whether the
/// person has Ask Cmdr on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionCaps {
    /// The OS's file manager can show the item: its path is one the OS sees,
    /// and it's a real item there (❌ not an entry inside an archive or git's
    /// history).
    pub file_manager: bool,
    /// How the volume removes the item. `None`: the card offers no removal (a
    /// read-only volume, a view into an archive or git's history).
    pub removal: Option<RemoveBy>,
    /// The app-data folder the item sits inside
    /// ([`crate::owner::OwnerQuestion::app_folder_above`]), raw. `None`: none
    /// does.
    pub app_folder: Option<String>,
    /// Ask Cmdr is on, so the card can hand the item to it
    /// ([`AboutAction::ContinueInAskCmdr`]).
    pub ask_cmdr: bool,
}

impl ActionCaps {
    /// A volume where no action can work: nothing shows.
    pub fn none() -> Self {
        Self {
            file_manager: false,
            removal: None,
            app_folder: None,
            ask_cmdr: false,
        }
    }
}

/// How a volume removes an item, which names the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveBy {
    /// To the OS's Trash.
    Trash,
    /// For good: the volume has no Trash. The delete dialog still asks first.
    Delete,
}

/// One thing the card's footer can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AboutAction {
    /// Opens the item the way Enter in the pane does (its default app), so
    /// whatever the pane would ask or refuse, the card does too.
    OpenWithDefaultApp {
        /// The app that opens it.
        app: ItemText,
    },
    /// Shows the item in the OS's file manager.
    ShowInFileManager,
    /// Opens the page the item was recorded as downloaded from.
    OpenDownloadPage {
        /// The site's host, lowercased, punycode when not plain ASCII: what the
        /// button's tooltip names before the press.
        host: String,
        /// The whole address, parsed and written back out, `http` or `https`
        /// only.
        url: String,
    },
    /// Shows the app the item belongs to, in the pane.
    RevealOwner {
        /// The app's name.
        app: ItemText,
        /// The app bundle's path: raw, ❌ never shown.
        path: String,
    },
    /// Opens the app-data folder the item sits inside, where the app keeps the
    /// rest of its data.
    GoToAppFolder {
        /// The app, when Cmdr knows which one is installed. `None`: the folder
        /// is an app's, by its place, and Cmdr can't name an installed one.
        app: Option<ItemText>,
        /// The folder: raw, ❌ never shown.
        path: String,
    },
    /// Opens Ask Cmdr with the item attached and a fixed, unsent question in
    /// the composer, so the person can ask more. ❌ The draft never quotes the
    /// card's facts or its AI take: a user turn is the most trusted text the
    /// agent reads, and a take steered by a hostile name must not arrive there
    /// as the person's own words. The agent reads the facts itself, as a tool
    /// result (`about_item`).
    ContinueInAskCmdr,
    /// Moves the item to the Trash, through the delete dialog.
    MoveToTrash,
    /// Deletes the item for good on a volume with no Trash, through the delete
    /// dialog.
    Delete,
}

/// The footer's actions: the ones that show, and the rest behind "More", each
/// in the target's fixed order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Actions {
    /// At most three, never starting with a removal.
    pub shown: Vec<AboutAction>,
    /// The rest.
    pub more: Vec<AboutAction>,
}

/// What the card's footer offers, from the facts that have landed and what the
/// volume allows. Callable at any moment: an action whose fact is still pending
/// isn't there yet.
pub fn actions(card: &AboutCard, caps: &ActionCaps) -> Actions {
    let open = open_with(card);
    let download = download_page(card);
    let owner = reveal_owner(card);
    let folder = caps.app_folder.clone().map(|path| AboutAction::GoToAppFolder {
        app: owner_name(card),
        path,
    });
    let reveal = caps.file_manager.then_some(AboutAction::ShowInFileManager);
    let ask = caps.ask_cmdr.then_some(AboutAction::ContinueInAskCmdr);
    let removal = caps.removal.map(|by| match by {
        RemoveBy::Trash => AboutAction::MoveToTrash,
        RemoveBy::Delete => AboutAction::Delete,
    });

    // Per target, most likely next step first, then asking Ask Cmdr about it.
    // Removal is always last.
    let order = match card.target {
        AboutTarget::File | AboutTarget::Package => [open, reveal, download, owner, folder, ask],
        AboutTarget::Folder => [reveal, owner, folder, ask, None, None],
        AboutTarget::AppBundle => [reveal, download, ask, None, None, None],
        AboutTarget::Symlink => [reveal, owner, folder, ask, None, None],
        AboutTarget::ArchiveEntry => [download, ask, None, None, None, None],
    };
    let mut all: Vec<AboutAction> = order.into_iter().flatten().collect();
    let before_removal = all.len();
    all.extend(removal);

    // A removal with nothing before it goes behind "More": it's never the first
    // button.
    let shown_count = if before_removal == 0 {
        0
    } else {
        all.len().min(SHOWN_ACTIONS)
    };
    let more = all.split_off(shown_count);
    Actions { shown: all, more }
}

/// Open, when the OS named the app that opens it.
fn open_with(card: &AboutCard) -> Option<AboutAction> {
    let app = known(&card.default_app)?;
    Some(AboutAction::OpenWithDefaultApp { app: app.name.clone() })
}

/// The LAST recorded web source that parses as an `http(s)` address with a
/// plain host. Browsers record the file's own address first and the page it
/// was downloaded from after it (Chrome and Safari both), so the last is the
/// page: opening the file's address would only download it again.
fn download_page(card: &AboutCard) -> Option<AboutAction> {
    let download = known(&card.download)?;
    let sources = download.sources.known()?;
    sources.iter().rev().find_map(|source| match source {
        WhereFrom::Web { url: Some(url), .. } => web_page(url),
        _ => None,
    })
}

/// `raw` as an address the card may open: parsed, `http` or `https` with a
/// domain host, written back out by the parser. ❌ Anything else is no action:
/// another scheme launches whatever app registered it, and `smb://` mounts a
/// stranger's share.
pub(crate) fn web_page(raw: &str) -> Option<AboutAction> {
    let parsed = Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    // A numeric address names no site the person could recognize on the button.
    let Some(Host::Domain(host)) = parsed.host() else {
        return None;
    };
    let host = host.trim_end_matches('.');
    let plain = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.'));
    // User info in a link to click is a disguise (`https://apple.com@evil.example`).
    if !plain || !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    Some(AboutAction::OpenDownloadPage {
        host: host.to_string(),
        url: parsed.to_string(),
    })
}

/// The installed app the item belongs to, where Cmdr knows its path.
fn reveal_owner(card: &AboutCard) -> Option<AboutAction> {
    let (app, path) = match known(&card.owner)? {
        OwnerClaim::ByBundleId {
            app: InstalledApp::Installed { name, path: Some(path) },
            ..
        } => (name, path),
        OwnerClaim::ByName {
            app,
            app_path: Some(path),
            ..
        } => (app, path),
        OwnerClaim::InsideApp { app, app_path } => (app, app_path),
        _ => return None,
    };
    Some(AboutAction::RevealOwner {
        app: app.clone(),
        path: path.clone(),
    })
}

/// The installed app's name, for the app-folder action's words.
fn owner_name(card: &AboutCard) -> Option<ItemText> {
    match known(&card.owner)? {
        OwnerClaim::ByBundleId {
            app: InstalledApp::Installed { name, .. },
            ..
        } => Some(name.clone()),
        OwnerClaim::ByName { app, .. } => Some(app.clone()),
        _ => None,
    }
}

fn known<T>(slot: &Option<Fact<T>>) -> Option<&T> {
    slot.as_ref().and_then(Fact::known)
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
