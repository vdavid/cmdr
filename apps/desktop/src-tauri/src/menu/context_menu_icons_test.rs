//! Tests for the file context menu's image runs and title matching (`context_menu_icons.rs`).

use super::super::file_context_menu::Slow;
use super::*;
use crate::file_system::file_provider_actions::OfferedAction;
use crate::file_system::open_with::AppCandidate;
use crate::file_system::share::ShareService;
use std::collections::HashSet;
use std::path::PathBuf;

/// Every ID here is one the file context menu actually builds.
///
/// The icons land through AppKit, which resolves a Tauri ID to the title it
/// currently carries and matches on that, so a stale ID costs an icon with no
/// crash and no log line anyone reads. Building a real menu needs AppKit on the main
/// thread, so the source is what we can check here.
#[test]
fn every_icon_names_an_item_the_context_menu_builds() {
    let source = include_str!("file_context_menu.rs");
    let ids: HashSet<&str> = FILE_CONTEXT_ICONS.iter().map(|&(id, _)| id).collect();
    for id in ids {
        // The constant's NAME, since that's what the builder call spells.
        let name = constant_named(id).expect("every context-menu icon id is a `command_map.rs` constant");
        assert!(
            source.contains(&name),
            "`file_context_menu.rs` builds no item with `{name}`, so its icon never lands"
        );
    }
}

/// The two submenus a run can sit in are built with the IDs the runs name. `Submenu::new`
/// would mint a random one, and `arm` would find no host and drop the run silently.
#[test]
fn the_submenus_holding_runs_are_built_with_their_ids() {
    assert!(include_str!("open_with.rs").contains("Submenu::with_id(app, OPEN_WITH_SUBMENU_ID"));
    assert!(include_str!("share_submenu.rs").contains("Submenu::with_id(app, SHARE_SUBMENU_ID"));
}

/// A symbol name is a string AppKit looks up at runtime, so a typo is silent. Pin
/// the ones we ship so a rename has to be deliberate. The logos are pinned in
/// `provider_logos.rs`.
#[test]
fn the_symbols_are_the_ones_we_chose() {
    assert_eq!(
        FILE_CONTEXT_ICONS,
        &[
            (DRIVE_OPEN_ID, "arrow.up.forward.app"),
            (DRIVE_COPY_LINK_ID, "link"),
            (DRIVE_ASK_GEMINI_ID, "sparkles"),
        ]
    );
}

fn offer(provider_id: &str, actions: usize) -> ProviderOffer {
    ProviderOffer {
        provider_domain_id: format!("{provider_id}/domain"),
        provider_id: provider_id.to_string(),
        item_identifiers: vec!["item".to_string()],
        actions: (0..actions)
            .map(|index| OfferedAction {
                identifier: format!("action.{index}"),
                label: format!("Action {index}"),
            })
            .collect(),
    }
}

fn app(bundle_id: &str, with_icon: bool) -> AppCandidate {
    AppCandidate {
        bundle_id: bundle_id.to_string(),
        display_name: bundle_id.to_string(),
        app_path: PathBuf::from(format!("/Applications/{bundle_id}.app")),
        icon: with_icon.then(|| AppIcon {
            rgba: vec![0; 4],
            width: 1,
            height: 1,
        }),
    }
}

fn service(title: &str) -> ShareService {
    ShareService {
        title: title.to_string(),
    }
}

/// One run as text: where it sits, then `ID = what it shows` per item.
fn described(runs: Vec<ImageRun>) -> Vec<String> {
    runs.into_iter()
        .map(|run| {
            let host = match run.host {
                RunHost::Menu => "menu".to_string(),
                RunHost::Submenu(id) => format!("submenu {id}"),
            };
            let items: Vec<String> = run
                .items
                .into_iter()
                .map(|(id, image)| {
                    let shown = match image {
                        None => "nothing".to_string(),
                        Some(ItemImage::Symbol(symbol)) => format!("symbol {symbol}"),
                        Some(ItemImage::Logo(logo)) => format!("{} logo", logo.provider),
                        Some(ItemImage::AppIcon(_)) => "app icon".to_string(),
                        Some(ItemImage::TagCircle { color, applied }) => {
                            format!("circle {color}{}", if applied { " checked" } else { "" })
                        }
                        Some(ItemImage::ShareService(index)) => format!("service {index}'s icon"),
                    };
                    format!("{id} = {shown}")
                })
                .collect();
            format!("{host}: {}", items.join(", "))
        })
        .collect()
}

/// The runs every menu carries: each Drive symbol on its own, and the seven tag circles.
fn always(applied: &[u8]) -> Vec<String> {
    let mut runs = vec![
        "menu: drive_open = symbol arrow.up.forward.app".to_string(),
        "menu: drive_copy_link = symbol link".to_string(),
        "menu: drive_ask_gemini = symbol sparkles".to_string(),
    ];
    let circles: Vec<String> = SWATCHES
        .iter()
        .map(|swatch| {
            let checked = if applied.contains(&swatch.color) {
                " checked"
            } else {
                ""
            };
            format!("tag-color:{0} = circle {0}{checked}", swatch.color)
        })
        .collect();
    runs.push(format!("menu: {}", circles.join(", ")));
    runs
}

#[test]
fn a_plain_row_carries_the_drive_symbols_and_the_tag_circles() {
    assert_eq!(described(image_runs(&FileContextInfo::default())), always(&[]));
}

/// A pending fact's items get their images when it lands, so the menu going up arms none
/// for them; the tag circles are there regardless, unchecked until the reads answer.
#[test]
fn pending_facts_arm_no_images_and_pending_tags_show_unchecked_circles() {
    let info = FileContextInfo {
        file_provider_offer: Slow::Pending,
        open_with: Slow::Pending,
        share_services: Slow::Pending,
        applied_tag_colors: Slow::Pending,
        ..FileContextInfo::default()
    };
    assert_eq!(described(image_runs(&info)), always(&[]));
}

#[test]
fn an_applied_tag_color_gets_the_checked_circle() {
    let mut applied_tag_colors = [false; 8];
    applied_tag_colors[2] = true;
    applied_tag_colors[6] = true;
    let info = FileContextInfo {
        applied_tag_colors: Slow::Ready(applied_tag_colors),
        ..FileContextInfo::default()
    };
    assert_eq!(described(image_runs(&info)), always(&[2, 6]));
}

/// Google Drive's own lines get Drive's logo, while Cmdr's three Drive items beside
/// them keep their symbols.
#[test]
fn every_line_of_a_known_providers_offer_carries_its_logo() {
    let info = FileContextInfo {
        file_provider_offer: Slow::Ready(Some(offer("com.google.drivefs.fpext", 2))),
        ..FileContextInfo::default()
    };
    let mut expected = always(&[]);
    expected.push("menu: fp-action:0 = Google Drive logo, fp-action:1 = Google Drive logo".to_string());
    assert_eq!(described(image_runs(&info)), expected);
}

#[test]
fn an_unknown_providers_lines_or_an_empty_offer_add_no_run() {
    for offer in [
        offer("com.example.fileprovider", 3),
        offer("com.getdropbox.dropbox.fileprovider", 0),
    ] {
        let info = FileContextInfo {
            file_provider_offer: Slow::Ready(Some(offer)),
            ..FileContextInfo::default()
        };
        assert_eq!(described(image_runs(&info)), always(&[]));
    }
}

/// Every candidate stays in the run, iconless ones too, since the run is matched as
/// one contiguous stretch of the submenu.
#[test]
fn open_with_is_one_run_in_its_submenu_with_a_gap_where_an_app_has_no_icon() {
    let info = FileContextInfo {
        open_with: Slow::Ready(OpenWithChoices {
            candidates: vec![
                app("com.apple.Preview", true),
                app("com.example.NoIcon", false),
                app("com.apple.Safari", true),
            ],
        }),
        ..FileContextInfo::default()
    };
    let mut expected = always(&[]);
    expected.push(
        "submenu open-with-submenu: open-with:com.apple.Preview = app icon, \
         open-with:com.example.NoIcon = nothing, open-with:com.apple.Safari = app icon"
            .to_string(),
    );
    assert_eq!(described(image_runs(&info)), expected);
}

#[test]
fn share_is_one_run_in_its_submenu_pointing_at_each_offered_service() {
    let info = FileContextInfo {
        share_services: Slow::Ready(vec![service("AirDrop"), service("Mail")]),
        ..FileContextInfo::default()
    };
    let mut expected = always(&[]);
    expected.push(
        "submenu share-submenu: share-service:0 = service 0's icon, share-service:1 = service 1's icon".to_string(),
    );
    assert_eq!(described(image_runs(&info)), expected);
}

fn titles(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_run_is_found_where_it_starts() {
    let menu = ["Open with", "Photos", "Preview", "", "Other…"];
    assert_eq!(find_title_run(&menu, &titles(&["Photos", "Preview"])), Some(1));
    assert_eq!(find_title_run(&menu, &titles(&["Other…"])), Some(4));
}

/// The header line carries the bare filename, so a folder named `Mail` sits above the
/// real `Mail` item. A run of two can't start on it.
#[test]
fn a_lookalike_title_outside_the_run_doesnt_take_it() {
    let menu = ["Mail", "", "AirDrop", "Mail", "Messages"];
    assert_eq!(find_title_run(&menu, &titles(&["Mail", "Messages"])), Some(3));
}

/// Two share extensions may carry one name; inside the run they pair up by position.
#[test]
fn duplicate_titles_inside_a_run_match_by_position() {
    let menu = ["Notes", "Notes", "Reminders"];
    assert_eq!(
        find_title_run(&menu, &titles(&["Notes", "Notes", "Reminders"])),
        Some(0)
    );
}

#[test]
fn a_partial_or_absent_run_matches_nothing() {
    let menu = ["AirDrop", "", "Mail"];
    assert_eq!(find_title_run(&menu, &titles(&["AirDrop", "Mail"])), None);
    assert_eq!(find_title_run(&menu, &titles(&["Messages"])), None);
    assert_eq!(find_title_run(&menu[..1], &titles(&["AirDrop", "Mail"])), None);
}

/// `display_accelerators.rs` puts the glyph after a TAB in the title AppKit reports.
#[test]
fn a_title_matches_without_its_displayed_shortcut() {
    let menu = ["Invert selection\t⇧8", "Select files…\t+"];
    assert_eq!(
        find_title_run(&menu, &titles(&["Invert selection", "Select files…"])),
        Some(0)
    );
}

/// A separator's title is empty, so an empty title in a run would match one.
#[test]
fn an_empty_run_or_an_empty_title_matches_nothing() {
    let menu = ["AirDrop", "", "Mail"];
    assert_eq!(find_title_run(&menu, &[]), None);
    assert_eq!(find_title_run(&menu, &titles(&[""])), None);
    assert_eq!(find_title_run(&menu, &titles(&["AirDrop", ""])), None);
}

/// The `command_map.rs` constant whose value is `value`.
fn constant_named(value: &str) -> Option<String> {
    include_str!("command_map.rs").lines().find_map(|line| {
        let rest = line.trim().strip_prefix("pub const ")?;
        let (name, rest) = rest.split_once(": &str = \"")?;
        (rest.strip_suffix("\";")? == value).then(|| name.to_string())
    })
}
