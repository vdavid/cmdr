//! The `favorites:` section of `cmdr://state`.
//!
//! Each favorite as the app lists it: its id (what the `favorites` tool's rename / remove /
//! reorder take), name, path, the volume it lives on, and whether a pick gets there (`reach`,
//! decided by `favorites/reach.rs`). Read off the SAME completed listing the app's volume list
//! comes from, ❌ never a second assembly. When that listing's discovery timed out (its local
//! half, favorites included, is missing), the store's own list stands in, without volume or
//! reach.
//!
//! Paths are user-chosen navigation targets shown in the favorites menu, so, like the
//! `listings:` section, they render unredacted.

use crate::favorites::target::{FavoriteReach, FavoriteTarget};
use crate::volume_listing::{LocationCategory, LocationInfo};

/// One `favorites:` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FavoriteSummary {
    /// The bare store id, without the `fav-` prefix of its volume row.
    pub id: String,
    pub name: String,
    pub path: String,
    pub volume_id: Option<String>,
    pub volume_name: Option<String>,
    /// `None` when the store stood in for the listing.
    pub reach: Option<&'static str>,
}

/// The favorite rows of a completed listing, in the store's order (which the listing keeps).
pub(crate) fn from_listing(rows: &[LocationInfo]) -> Vec<FavoriteSummary> {
    rows.iter()
        .filter(|row| row.category == LocationCategory::Favorite)
        .map(|row| summary_of_row(&row.id, &row.name, &row.path, row.favorite_target.as_ref()))
        .collect()
}

/// The store's own list: what the section says when the listing came up short.
pub(crate) fn from_store() -> Vec<FavoriteSummary> {
    crate::favorites::store::list()
        .into_iter()
        .map(|favorite| FavoriteSummary {
            id: favorite.id,
            name: favorite.name,
            path: favorite.path,
            volume_id: favorite.volume.as_ref().map(|volume| volume.id.clone()),
            volume_name: favorite.volume.map(|volume| volume.name),
            reach: None,
        })
        .collect()
}

fn summary_of_row(row_id: &str, name: &str, path: &str, target: Option<&FavoriteTarget>) -> FavoriteSummary {
    FavoriteSummary {
        id: row_id.strip_prefix("fav-").unwrap_or(row_id).to_string(),
        name: name.to_string(),
        path: path.to_string(),
        volume_id: target.and_then(|target| target.volume_id.clone()),
        volume_name: target.and_then(|target| target.volume_name.clone()),
        reach: target.map(|target| reach_token(&target.reach)),
    }
}

/// The reach's `kind`, as the frontend and analytics spell it.
fn reach_token(reach: &FavoriteReach) -> &'static str {
    match reach {
        FavoriteReach::Ready => "ready",
        FavoriteReach::Connects => "connects",
        FavoriteReach::Unplugged { .. } => "unplugged",
        FavoriteReach::AccessOff { .. } => "access_off",
        FavoriteReach::Forgotten => "forgotten",
        FavoriteReach::NotFound => "not_found",
    }
}

/// The section, `favorites:` heading included.
pub(crate) fn build_favorites_yaml(favorites: &[FavoriteSummary]) -> String {
    if favorites.is_empty() {
        return "favorites: []\n".to_string();
    }
    let mut yaml = String::from("favorites:\n");
    for favorite in favorites {
        yaml.push_str(&format!(
            "  - id: {}\n    name: {:?}\n    path: {:?}\n",
            favorite.id, favorite.name, favorite.path
        ));
        if let Some(volume_id) = &favorite.volume_id {
            yaml.push_str(&format!("    volume: {volume_id}\n"));
        }
        if let Some(volume_name) = &favorite.volume_name {
            yaml.push_str(&format!("    volumeName: {volume_name:?}\n"));
        }
        if let Some(reach) = favorite.reach {
            yaml.push_str(&format!("    reach: {reach}\n"));
        }
    }
    yaml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorites::target::UnpluggedKind;

    fn target(volume_id: &str, reach: FavoriteReach) -> FavoriteTarget {
        FavoriteTarget {
            volume_id: Some(volume_id.to_string()),
            volume_name: Some("naspi on nas.local".to_string()),
            volume_root: None,
            reach,
            discovered: Default::default(),
        }
    }

    #[test]
    fn a_listed_favorite_says_where_it_lives_and_whether_a_pick_gets_there() {
        let summary = summary_of_row(
            "fav-9f1c",
            "docs",
            "/Volumes/naspi/docs",
            Some(&target("smb-naspi", FavoriteReach::Connects)),
        );

        assert_eq!(summary.id, "9f1c", "the id the favorites tool takes is the bare one");
        assert_eq!(
            build_favorites_yaml(&[summary]),
            "favorites:\n  - id: 9f1c\n    name: \"docs\"\n    path: \"/Volumes/naspi/docs\"\n    \
             volume: smb-naspi\n    volumeName: \"naspi on nas.local\"\n    reach: connects\n"
        );
    }

    #[test]
    fn every_reach_has_the_token_the_frontend_uses() {
        let cells = [
            (FavoriteReach::Ready, "ready"),
            (FavoriteReach::Connects, "connects"),
            (
                FavoriteReach::Unplugged {
                    device: UnpluggedKind::Phone,
                    reason: None,
                },
                "unplugged",
            ),
            (
                FavoriteReach::AccessOff {
                    backend: crate::favorites::target::DeviceBackend::Adb,
                },
                "access_off",
            ),
            (FavoriteReach::Forgotten, "forgotten"),
            (FavoriteReach::NotFound, "not_found"),
        ];
        for (reach, token) in cells {
            assert_eq!(reach_token(&reach), token);
        }
    }

    #[test]
    fn a_store_stand_in_leaves_out_what_only_the_listing_knows() {
        let summary = FavoriteSummary {
            id: "1".to_string(),
            name: "Home".to_string(),
            path: "/Users/me".to_string(),
            volume_id: None,
            volume_name: None,
            reach: None,
        };
        assert_eq!(
            build_favorites_yaml(&[summary]),
            "favorites:\n  - id: 1\n    name: \"Home\"\n    path: \"/Users/me\"\n"
        );
    }

    #[test]
    fn no_favorites_is_an_empty_list_not_a_missing_section() {
        assert_eq!(build_favorites_yaml(&[]), "favorites: []\n");
    }
}
