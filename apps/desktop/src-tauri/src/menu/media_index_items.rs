//! The image-search group of the folder context menu: which items it shows, with what
//! label, and whether each is clickable.
//!
//! Two independent per-folder facts meet here, so the group carries two items:
//!
//! - **The chosen-folder list** (`mediaIndex.alwaysIndexFolders`): membership, added or
//!   removed. Object-framed labels ("Add to / Remove from indexed folders") keep it
//!   readable next to the exclusion, which is verb-framed.
//! - **The exclusion** (`mediaIndex.excludedFolders`): the privacy veto.
//!
//! The veto BEATS membership backend-side, so an add on an excluded folder would persist
//! a list entry that indexes nothing. Rather than let that click look like it worked, the
//! add is disabled and its label names the blocker; the un-exclude item sits right below
//! it as the way out. Same for a folder an ancestor entry already covers: adding it would
//! write a redundant entry and change nothing.
//!
//! Pure, so the decision is unit-tested without an `AppHandle`; `menu_structure` renders
//! whatever this returns.

use super::{
    MEDIA_INDEX_ADD_FOLDER_ID, MEDIA_INDEX_EXCLUDE_FOLDER_ID, MEDIA_INDEX_INCLUDE_FOLDER_ID,
    MEDIA_INDEX_REMOVE_FOLDER_ID,
};

/// What the image-search group knows about the right-clicked folder, read from the live
/// `media_index` gate and config in `show_file_context_menu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageIndexMenuState {
    /// The master toggle. With it off the whole group is hidden: none of these items
    /// would do anything visible.
    pub enabled: bool,
    /// This folder (or an ancestor) is excluded from image search: the hard veto.
    pub excluded: bool,
    /// This EXACT path is on the chosen-folder list, so removing it is meaningful.
    pub chosen: bool,
    /// An ANCESTOR folder is on the chosen list, so this folder is already covered and
    /// adding it would be a redundant no-op entry.
    pub covered_by_parent: bool,
}

/// One item of the image-search group: a menu id, its label, and whether it's clickable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageIndexMenuItem {
    pub id: &'static str,
    pub label: &'static str,
    pub enabled: bool,
}

/// The image-search items to append to a folder's context menu, in order. Empty when
/// image indexing is off.
pub fn image_index_menu_items(state: ImageIndexMenuState) -> Vec<ImageIndexMenuItem> {
    if !state.enabled {
        return Vec::new();
    }

    // Membership first (the common action), then the privacy veto.
    let membership = if state.chosen {
        // Removing works even under the veto: it takes a real entry off the list.
        ImageIndexMenuItem {
            id: MEDIA_INDEX_REMOVE_FOLDER_ID,
            label: "Remove from indexed folders",
            enabled: true,
        }
    } else if state.excluded {
        ImageIndexMenuItem {
            id: MEDIA_INDEX_ADD_FOLDER_ID,
            label: "Add to indexed folders (excluded)",
            enabled: false,
        }
    } else if state.covered_by_parent {
        ImageIndexMenuItem {
            id: MEDIA_INDEX_ADD_FOLDER_ID,
            label: "Indexed through a parent folder",
            enabled: false,
        }
    } else {
        ImageIndexMenuItem {
            id: MEDIA_INDEX_ADD_FOLDER_ID,
            label: "Add to indexed folders",
            enabled: true,
        }
    };

    let exclusion = if state.excluded {
        ImageIndexMenuItem {
            id: MEDIA_INDEX_INCLUDE_FOLDER_ID,
            label: "Index images here again",
            enabled: true,
        }
    } else {
        ImageIndexMenuItem {
            id: MEDIA_INDEX_EXCLUDE_FOLDER_ID,
            label: "Don't index images in this folder",
            enabled: true,
        }
    };

    vec![membership, exclusion]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ImageIndexMenuState {
        ImageIndexMenuState {
            enabled: true,
            excluded: false,
            chosen: false,
            covered_by_parent: false,
        }
    }

    #[test]
    fn disabled_feature_shows_nothing() {
        let items = image_index_menu_items(ImageIndexMenuState {
            enabled: false,
            ..state()
        });
        assert!(items.is_empty());
    }

    #[test]
    fn plain_folder_offers_add_and_exclude() {
        let items = image_index_menu_items(state());
        assert_eq!(items[0].id, MEDIA_INDEX_ADD_FOLDER_ID);
        assert!(items[0].enabled);
        assert_eq!(items[1].id, MEDIA_INDEX_EXCLUDE_FOLDER_ID);
    }

    #[test]
    fn chosen_folder_offers_the_inverse() {
        let items = image_index_menu_items(ImageIndexMenuState {
            chosen: true,
            ..state()
        });
        assert_eq!(items[0].id, MEDIA_INDEX_REMOVE_FOLDER_ID);
        assert!(items[0].enabled);
    }

    #[test]
    fn excluded_folder_cannot_be_added_and_says_why() {
        let items = image_index_menu_items(ImageIndexMenuState {
            excluded: true,
            ..state()
        });
        assert_eq!(items[0].id, MEDIA_INDEX_ADD_FOLDER_ID);
        assert!(
            !items[0].enabled,
            "the veto beats the list, so the add must not look live"
        );
        assert!(items[0].label.contains("excluded"));
        // The way out sits right below it.
        assert_eq!(items[1].id, MEDIA_INDEX_INCLUDE_FOLDER_ID);
        assert!(items[1].enabled);
    }

    #[test]
    fn excluded_and_chosen_still_offers_removal() {
        let items = image_index_menu_items(ImageIndexMenuState {
            excluded: true,
            chosen: true,
            ..state()
        });
        assert_eq!(items[0].id, MEDIA_INDEX_REMOVE_FOLDER_ID);
        assert!(items[0].enabled);
    }

    #[test]
    fn parent_covered_folder_cannot_be_added_twice() {
        let items = image_index_menu_items(ImageIndexMenuState {
            covered_by_parent: true,
            ..state()
        });
        assert_eq!(items[0].id, MEDIA_INDEX_ADD_FOLDER_ID);
        assert!(!items[0].enabled);
    }

    #[test]
    fn an_exact_entry_beats_the_parent_coverage_hint() {
        let items = image_index_menu_items(ImageIndexMenuState {
            chosen: true,
            covered_by_parent: true,
            ..state()
        });
        assert_eq!(items[0].id, MEDIA_INDEX_REMOVE_FOLDER_ID);
        assert!(items[0].enabled);
    }
}
