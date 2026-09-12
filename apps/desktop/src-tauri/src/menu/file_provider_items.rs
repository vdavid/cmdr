//! The file context menu's File Provider group (macOS): one flat line per action the
//! right-clicked rows' provider offers, below Cmdr's own cloud items.
//!
//! The offer itself (which actions apply, their labels, and the click) lives in
//! `file_system/file_provider_actions/`. This file only draws it and names the IDs.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

use crate::file_system::file_provider_actions::ProviderOffer;

/// Menu item ID prefix for one offered action. Followed by its index in the offer,
/// because identifiers repeat (Drive declares `ACTION_SHARE` three times).
pub const FILE_PROVIDER_ACTION_ID_PREFIX: &str = "fp-action:";

/// The ID for the offered action at `index`.
pub fn file_provider_action_id(index: usize) -> String {
    format!("{FILE_PROVIDER_ACTION_ID_PREFIX}{index}")
}

/// The offer index a clicked ID names, or `None` when it isn't one of this family's.
///
/// Answers rather than trusts: the ID space is shared with every other menu item, and a
/// click that resolved to a wrong index would run the wrong action.
pub fn file_provider_action_index(id: &str) -> Option<usize> {
    id.strip_prefix(FILE_PROVIDER_ACTION_ID_PREFIX)?.parse().ok()
}

/// `(id, label)` for each item the group draws, in the provider's own order.
pub fn group_entries(offer: &ProviderOffer) -> Vec<(String, String)> {
    offer
        .actions
        .iter()
        .enumerate()
        .map(|(index, action)| (file_provider_action_id(index), action.label.clone()))
        .collect()
}

/// Appends a separator and the offer's actions to `menu`. Nothing for an empty offer.
pub fn append_file_provider_group<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    offer: &ProviderOffer,
) -> tauri::Result<()> {
    let entries = group_entries(offer);
    if entries.is_empty() {
        return Ok(());
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for (id, label) in entries {
        menu.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::file_provider_actions::OfferedAction;

    fn offer(labels: &[&str]) -> ProviderOffer {
        ProviderOffer {
            provider_domain_id: "com.getdropbox.dropbox.fileprovider/abc".to_string(),
            item_identifiers: vec!["item-1".to_string()],
            actions: labels
                .iter()
                .map(|label| OfferedAction {
                    identifier: "com.getdropbox.dropbox.fileprovider.action.x".to_string(),
                    label: label.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn every_index_round_trips_through_its_id() {
        for index in [0, 1, 7, 24, 1000] {
            assert_eq!(file_provider_action_index(&file_provider_action_id(index)), Some(index));
        }
        assert_eq!(file_provider_action_id(3), "fp-action:3");
    }

    #[test]
    fn only_this_familys_ids_resolve_to_an_index() {
        for id in [
            "share-service:1",
            "fp-action:",
            "fp-action:x",
            "fp-action:-1",
            "drive_open",
            "3",
        ] {
            assert_eq!(
                file_provider_action_index(id),
                None,
                "`{id}` must not resolve to an index"
            );
        }
    }

    /// The group is flat and in the provider's own order, and an identifier that repeats
    /// still gets a distinct ID per line.
    #[test]
    fn the_group_lists_the_offer_in_order_with_positional_ids() {
        let entries = group_entries(&offer(&["Copy Dropbox link", "Share...", "Right-click actions..."]));
        assert_eq!(
            entries,
            [
                ("fp-action:0".to_string(), "Copy Dropbox link".to_string()),
                ("fp-action:1".to_string(), "Share...".to_string()),
                ("fp-action:2".to_string(), "Right-click actions...".to_string()),
            ]
        );
        assert!(group_entries(&offer(&[])).is_empty());
    }
}
