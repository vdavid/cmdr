//! The menu's shape, its dedup rules, and its refusal to fall over on hostile input.

use super::*;

/// A home directory to expand `~` against. Never touched on disk: nothing here
/// stats anything, which is the property half these tests exist to hold.
fn home() -> &'static Path {
    Path::new("/Users/dave")
}

/// The location rows, in order, as `(rendered-ish label, path, kind)`.
fn locations(rows: &[DockRow]) -> Vec<(DockLabel, String, LocationKind)> {
    rows.iter()
        .filter_map(|row| match row {
            DockRow::Location(location) => Some((location.label.clone(), location.path.clone(), location.kind)),
            _ => None,
        })
        .collect()
}

/// Just the names, for the many tests that only care which rows survived.
fn names(rows: &[DockRow]) -> Vec<String> {
    locations(rows)
        .into_iter()
        .map(|(label, _, _)| label.name().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// The menu David specified
// ---------------------------------------------------------------------------

#[test]
fn the_four_commands_come_first_split_by_a_separator() {
    let rows = menu_rows(&[], &[], Some(home()));

    assert_eq!(
        rows,
        vec![
            DockRow::Command(DockCommand::OpenCmdr),
            DockRow::Command(DockCommand::SearchFiles),
            DockRow::Separator,
            DockRow::Command(DockCommand::GoToFolder),
            DockRow::Command(DockCommand::ConnectToServer),
        ],
        "with no bookmarks and no tabs the menu is exactly the four commands"
    );
}

#[test]
fn bookmarks_and_tabs_each_get_their_own_separated_group() {
    let rows = menu_rows(
        &[Candidate::bookmark("Desktop", "/Users/dave/Desktop")],
        &[Candidate::tab("/Users/dave/code")],
        Some(home()),
    );

    assert_eq!(
        rows,
        vec![
            DockRow::Command(DockCommand::OpenCmdr),
            DockRow::Command(DockCommand::SearchFiles),
            DockRow::Separator,
            DockRow::Command(DockCommand::GoToFolder),
            DockRow::Command(DockCommand::ConnectToServer),
            DockRow::Separator,
            DockRow::Location(DockLocation {
                label: DockLabel::Plain("Desktop".to_string()),
                path: "/Users/dave/Desktop".to_string(),
                kind: LocationKind::Bookmark,
            }),
            DockRow::Separator,
            DockRow::Location(DockLocation {
                label: DockLabel::Plain("code".to_string()),
                path: "/Users/dave/code".to_string(),
                kind: LocationKind::Tab,
            }),
        ]
    );
}

#[test]
fn an_empty_group_takes_its_separator_with_it() {
    let rows = menu_rows(&[], &[Candidate::tab("/Users/dave/code")], Some(home()));

    let separators = rows.iter().filter(|row| **row == DockRow::Separator).count();
    assert_eq!(separators, 2, "one between the command pairs, one before the tabs");
    assert_ne!(
        rows.last(),
        Some(&DockRow::Separator),
        "a menu must never end on a dividing line"
    );
}

// ---------------------------------------------------------------------------
// Deduplication
// ---------------------------------------------------------------------------

#[test]
fn a_bookmark_listed_twice_is_drawn_once() {
    let rows = menu_rows(
        &[
            Candidate::bookmark("Desktop", "/Users/dave/Desktop"),
            Candidate::bookmark("Desktop again", "/Users/dave/Desktop/"),
        ],
        &[],
        Some(home()),
    );

    assert_eq!(names(&rows), vec!["Desktop"], "the trailing slash is the same folder");
}

#[test]
fn a_tab_that_repeats_a_bookmark_is_dropped() {
    let rows = menu_rows(
        &[Candidate::bookmark("Applications", "/Applications")],
        &[Candidate::tab("/Applications"), Candidate::tab("/Users/dave/code")],
        Some(home()),
    );

    assert_eq!(
        names(&rows),
        vec!["Applications", "code"],
        "Finder lists Applications twice with nothing to tell them apart; we don't"
    );
}

#[test]
fn the_root_folder_keeps_its_slash_through_dedup() {
    let rows = menu_rows(
        &[Candidate::bookmark("Macintosh HD", "/")],
        &[Candidate::tab("/")],
        Some(home()),
    );

    let offered = locations(&rows);
    assert_eq!(offered.len(), 1, "`/` and `/` are the same place");
    assert_eq!(offered[0].1, "/");
}

// ---------------------------------------------------------------------------
// Disambiguation
// ---------------------------------------------------------------------------

#[test]
fn two_rows_sharing_a_name_are_told_apart_by_their_parent() {
    let rows = menu_rows(
        &[],
        &[
            Candidate::tab("/Users/dave/Documents/Projects"),
            Candidate::tab("/Users/dave/Work/Projects"),
        ],
        Some(home()),
    );

    assert_eq!(
        locations(&rows)
            .into_iter()
            .map(|(label, _, _)| label)
            .collect::<Vec<_>>(),
        vec![
            DockLabel::InParent {
                name: "Projects".to_string(),
                parent: "Documents".to_string()
            },
            DockLabel::InParent {
                name: "Projects".to_string(),
                parent: "Work".to_string()
            },
        ]
    );
}

#[test]
fn a_name_nothing_else_carries_is_left_alone() {
    let rows = menu_rows(
        &[],
        &[
            Candidate::tab("/Users/dave/Documents/Projects"),
            Candidate::tab("/Users/dave/Work/Notes"),
        ],
        Some(home()),
    );

    assert_eq!(
        locations(&rows)
            .into_iter()
            .map(|(label, _, _)| label)
            .collect::<Vec<_>>(),
        vec![
            DockLabel::Plain("Projects".to_string()),
            DockLabel::Plain("Notes".to_string()),
        ],
        "qualifying a name nothing collides with would be noise"
    );
}

#[test]
fn rows_whose_parents_also_match_fall_back_to_the_whole_path() {
    let rows = menu_rows(
        &[],
        &[Candidate::tab("/a/shared/thing"), Candidate::tab("/b/shared/thing")],
        Some(home()),
    );

    assert_eq!(
        locations(&rows)
            .into_iter()
            .map(|(label, _, _)| label)
            .collect::<Vec<_>>(),
        vec![
            DockLabel::Path("/a/shared/thing".to_string()),
            DockLabel::Path("/b/shared/thing".to_string()),
        ],
        "the parent didn't separate them, so the path has to"
    );
}

#[test]
fn a_user_named_favorite_collides_on_its_own_label_not_its_folder() {
    let rows = menu_rows(
        &[
            Candidate::bookmark("Work", "/Users/dave/Documents/clients"),
            Candidate::bookmark("Work", "/Users/dave/Archive/old-clients"),
        ],
        &[],
        Some(home()),
    );

    assert_eq!(
        locations(&rows)
            .into_iter()
            .map(|(label, _, _)| label)
            .collect::<Vec<_>>(),
        vec![
            DockLabel::InParent {
                name: "Work".to_string(),
                parent: "Documents".to_string()
            },
            DockLabel::InParent {
                name: "Work".to_string(),
                parent: "Archive".to_string()
            },
        ],
        "the row is called what the user called it, so that is what collides"
    );
}

// ---------------------------------------------------------------------------
// Which paths the menu is willing to offer
// ---------------------------------------------------------------------------

#[test]
fn a_tilde_path_is_expanded_against_home() {
    let rows = menu_rows(&[], &[Candidate::tab("~/Downloads"), Candidate::tab("~")], Some(home()));

    let offered: Vec<String> = locations(&rows).into_iter().map(|(_, path, _)| path).collect();
    assert_eq!(offered, vec!["/Users/dave/Downloads", "/Users/dave"]);
}

#[test]
fn a_virtual_location_never_reaches_the_menu() {
    let rows = menu_rows(
        &[],
        &[
            Candidate::tab("search-results"),
            Candidate::tab("mtp://Pixel/DCIM"),
            Candidate::tab(""),
            Candidate::tab("relative/path"),
            Candidate::tab("/Users/dave/real"),
        ],
        Some(home()),
    );

    assert_eq!(
        names(&rows),
        vec!["real"],
        "a Dock click can only navigate to a real absolute path"
    );
}

#[test]
fn with_no_home_directory_the_tilde_rows_simply_go_away() {
    let rows = menu_rows(&[], &[Candidate::tab("~/Downloads"), Candidate::tab("/tmp")], None);

    assert_eq!(names(&rows), vec!["tmp"]);
}

// ---------------------------------------------------------------------------
// Hostile input: the menu builder must answer, never unwind
// ---------------------------------------------------------------------------

#[test]
fn a_group_stops_at_the_cap() {
    let many: Vec<Candidate> = (0..500).map(|i| Candidate::tab(format!("/tmp/dir{i}"))).collect();

    let rows = menu_rows(&many, &[], Some(home()));

    assert_eq!(
        locations(&rows).len(),
        MAX_LOCATIONS_PER_GROUP,
        "500 favorites must not run the Dock menu off the screen"
    );
}

#[test]
fn a_tab_group_that_only_repeats_the_bookmarks_contributes_nothing() {
    let shared: Vec<Candidate> = (0..MAX_LOCATIONS_PER_GROUP)
        .map(|i| Candidate::tab(format!("/tmp/dir{i}")))
        .collect();

    let rows = menu_rows(&shared, &shared, Some(home()));

    assert_eq!(
        locations(&rows).len(),
        MAX_LOCATIONS_PER_GROUP,
        "every tab names a folder a bookmark already offers"
    );
    let separators = rows.iter().filter(|row| **row == DockRow::Separator).count();
    assert_eq!(separators, 2, "an emptied tab group takes its separator with it");
}

#[test]
fn both_groups_reach_the_cap_when_they_do_not_overlap() {
    let bookmarks: Vec<Candidate> = (0..50).map(|i| Candidate::tab(format!("/tmp/a{i}"))).collect();
    let tabs: Vec<Candidate> = (0..50).map(|i| Candidate::tab(format!("/tmp/b{i}"))).collect();

    let rows = menu_rows(&bookmarks, &tabs, Some(home()));

    assert_eq!(locations(&rows).len(), MAX_LOCATIONS_PER_GROUP * 2);
}

#[test]
fn absurd_names_and_paths_produce_a_menu_rather_than_a_panic() {
    let hostile = vec![
        Candidate::bookmark("x".repeat(100_000), "/tmp/huge"),
        Candidate::bookmark("", "/tmp/unnamed"),
        Candidate::bookmark("newline\nand\ttab", "/tmp/control"),
        Candidate::bookmark("🇭🇺🧑‍🚀 combining ñ", "/tmp/emoji"),
        // A lone surrogate can't reach a Rust `String`, but a lossy-decoded name can
        // carry the replacement character, and the label logic must not care.
        Candidate::bookmark("\u{FFFD}", "/tmp/replacement"),
        Candidate::tab("/"),
        Candidate::tab("//"),
        Candidate::tab("/tmp/trailing/"),
        Candidate::tab(format!("/{}", "deep/".repeat(2_000))),
        Candidate::tab("~"),
        Candidate::tab("~/"),
    ];

    let rows = menu_rows(&hostile, &hostile, Some(home()));

    assert!(
        rows.len() >= DockCommand::ALL.len(),
        "the four commands are there whatever the locations did"
    );
    for row in &rows {
        if let DockRow::Location(location) = row {
            assert!(location.path.starts_with('/'), "every offered path is absolute");
            assert!(!location.label.name().is_empty(), "no row is drawn nameless");
        }
    }
}

#[test]
fn an_empty_name_falls_back_to_the_folder_it_points_at() {
    let rows = menu_rows(&[Candidate::bookmark("", "/tmp/unnamed")], &[], Some(home()));

    assert_eq!(names(&rows), vec!["unnamed"]);
}

#[test]
fn the_root_folder_is_named_after_itself_when_it_has_no_leaf() {
    let rows = menu_rows(&[Candidate::tab("/")], &[], Some(home()));

    assert_eq!(names(&rows), vec!["/"], "`/` has no last component to borrow");
}

// ---------------------------------------------------------------------------
// The static tables
// ---------------------------------------------------------------------------

#[test]
fn every_command_carries_a_menu_key_and_a_symbol() {
    for command in DockCommand::ALL {
        // Segment by segment, because a literal spelling the family prefix would be
        // read as a key of its own by `native_strings`' key-literal scanner, which
        // would then fail to find it in the catalog. That scanner reads comments too.
        let mut segments = command.label_key().split('.');
        assert_eq!(segments.next(), Some("menu"), "{command:?} isn't a native-menu key");
        assert_eq!(segments.next(), Some("dock"), "{command:?} belongs to the Dock family");
        assert!(
            segments.next().is_some_and(|leaf| !leaf.is_empty()),
            "{command:?} has no leaf segment"
        );
        assert!(!command.symbol().is_empty(), "{command:?} has no glyph");
    }
}

#[test]
fn the_two_location_kinds_do_not_share_a_glyph() {
    assert_ne!(
        LocationKind::Bookmark.symbol(),
        LocationKind::Tab.symbol(),
        "a bookmark and a tab have to be distinguishable at a glance"
    );
}
