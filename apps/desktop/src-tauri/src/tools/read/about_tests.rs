//! `about_item`'s shaping: who counts as starting a lookup, and the biggest card
//! Cmdr can draw still fits one tool result.

use cmdr_about::card::AboutCard;
use cmdr_about::digest::{AppData, SHOWN_CHILDREN};
use cmdr_about::digest::{ByteShare, FolderClass, FolderDigest, LibraryFolder, SizedChild};
use cmdr_about::fact::{AboutTarget, Fact, FactKind};
use cmdr_about::owner::{OwnerClaim, OwnerPlace};
use cmdr_about::payload::{GitContext, GitStatus, Identity, Importance, ImportanceBand, RecursiveSize, SizeCoverage};
use cmdr_about::text::{ItemText, MAX_ITEM_TEXT_CHARS};

use super::*;
use crate::agent::chat::budget::{MAX_TOOL_RESULT_TOKENS, estimate_serialized_tokens};
use crate::agent::types::TurnOrigin;

#[test]
fn only_a_turn_cmdr_started_counts_as_cmdrs() {
    assert_eq!(
        started(Caller::Agent {
            origin: TurnOrigin::Text
        }),
        Started::ByPerson
    );
    assert_eq!(
        started(Caller::Agent {
            origin: TurnOrigin::Resume
        }),
        Started::ByPerson
    );
    assert_eq!(
        started(Caller::Agent {
            origin: TurnOrigin::Wake
        }),
        Started::ByCmdr
    );
    assert_eq!(
        started(Caller::Agent {
            origin: TurnOrigin::Outcomes
        }),
        Started::ByCmdr
    );
    assert_eq!(started(Caller::External { direct: true }), Started::ByPerson);
}

/// The longest name an item's own text can carry.
fn long(prefix: &str) -> ItemText {
    ItemText::new(&format!("{prefix}{}", "x".repeat(MAX_ITEM_TEXT_CHARS)))
}

fn size(bytes: u64) -> RecursiveSize {
    RecursiveSize {
        logical: bytes,
        physical: bytes,
        files: 1_000,
        folders: 100,
        coverage: SizeCoverage::LowerBound,
        updating: true,
        has_symlinks: true,
    }
}

/// A card with every list at its cap and every name at its longest. Not a card
/// any item gets (a folder with an app's Library data), which is the point.
fn biggest_card() -> AboutCard {
    let child = |i: usize| SizedChild {
        name: long(&format!("child{i}")),
        path: format!("/Users/jane/{i}"),
        is_folder: i.is_multiple_of(2),
        size: Fact::Known(size(1_000_000 * i as u64)),
    };
    let children: Vec<SizedChild> = (0..SHOWN_CHILDREN).map(child).collect();
    let folders: Vec<LibraryFolder> = cmdr_about::digest::APP_DATA_PLACES
        .iter()
        .map(|(_, place)| LibraryFolder {
            name: long("com.example."),
            path: "/Users/jane/Library/x".to_owned(),
            place: *place,
            size: Fact::Known(size(2_000_000_000)),
        })
        .collect();
    AboutCard {
        target: AboutTarget::Folder,
        identity: Some(Fact::Known(Identity {
            name: long("name"),
            name_hid: None,
            logical_size: None,
            physical_size: None,
            created: Some(1_772_400_000),
            modified: Some(1_772_400_100),
            hard_links: Some(3),
            symlink_target: Some(long("target")),
        })),
        real_type: None,
        bundle: None,
        signature: None,
        download: None,
        cmdr_operation: None,
        last_opened: None,
        default_app: None,
        importance: Some(Fact::Known(Importance {
            band: ImportanceBand::High,
            stale: true,
            reasons: vec!["projectMarker".to_owned(), "recentVisits".to_owned(), "size".to_owned()],
            visits: Some(41),
        })),
        owner: Some(Fact::Known(OwnerClaim::ByName {
            app: long("App"),
            place: OwnerPlace::ApplicationSupport,
            app_path: Some("/Applications/App.app".to_owned()),
        })),
        git: Some(Fact::Known(GitContext {
            repo_root: long("/Users/jane/code/"),
            branch: Some(long("feature/")),
            status: Fact::Known(GitStatus::Untracked),
        })),
        recursive_size: Some(Fact::Known(size(5_000_000_000))),
        folder_digest: Some(Fact::Known(FolderDigest::of(
            Fact::Known(children.clone()),
            Fact::Known(children),
            ByteShare::unchanged(1_000_000, false, &Fact::Known(size(5_000_000_000))),
            FolderClass::Plain,
        ))),
        app_data: Some(AppData::of(folders, vec![OwnerPlace::Logs])),
        git_snapshot: None,
    }
}

/// The result has no list a model could page, so its biggest shape must fit
/// whole.
#[test]
fn the_biggest_card_fits_one_tool_result() {
    let card = biggest_card();
    let path = format!("/Users/jane/{}", "y".repeat(MAX_ITEM_TEXT_CHARS));
    let rows: Vec<FactKind> = FactKind::ALL.to_vec();
    let result = AboutItemResult {
        location: Location {
            volume_id: "root".to_owned(),
            path: path.clone(),
        },
        about: crate::about::model_for_test(&card, &path),
        how_cmdr_knows: rows.iter().map(|kind| (*kind, how_cmdr_knows(*kind))).collect(),
        still_gathering: rows,
        notes: vec![STILL_GATHERING, REFUSED_IN_BACKGROUND],
    };
    let tokens = estimate_serialized_tokens(&result);
    assert!(tokens <= MAX_TOOL_RESULT_TOKENS, "estimated at {tokens}");
}
