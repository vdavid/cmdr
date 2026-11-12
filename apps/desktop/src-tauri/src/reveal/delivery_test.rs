use super::*;

fn dirs(paths: &[&str]) -> impl Fn(&Path) -> bool + use<> {
    let dirs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    move |path: &Path| dirs.iter().any(|d| d == path)
}

fn p(path: &str) -> PathBuf {
    PathBuf::from(path)
}

fn finder(paths: &[&str]) -> RevealRequest {
    RevealRequest {
        paths: paths.iter().map(PathBuf::from).collect(),
        target: GoToTarget::FOCUSED,
        origin: RevealOrigin::Finder,
    }
}

fn terminal(paths: &[&str], pane: PaneChoice) -> RevealRequest {
    RevealRequest {
        paths: paths.iter().map(PathBuf::from).collect(),
        target: GoToTarget { pane, new_tab: false },
        origin: RevealOrigin::Terminal,
    }
}

fn urls(raw: &[&str]) -> Vec<tauri::Url> {
    raw.iter().map(|url| tauri::Url::parse(url).expect("a URL")).collect()
}

#[test]
fn a_file_opens_its_parent_with_the_cursor_on_it() {
    let plan = plan_reveal(&[p("/Users/test_user_10/notes/todo.md")], &dirs(&[])).expect("a plan");
    assert_eq!(plan.dir, p("/Users/test_user_10/notes"));
    assert_eq!(plan.entries, vec!["todo.md"]);
}

#[test]
fn a_folder_is_opened_rather_than_pointed_at() {
    // A reveal OF a folder becomes an open of it. Finder parity isn't the goal here;
    // landing inside the folder the user asked about is.
    let plan = plan_reveal(&[p("/Users/test_user_10/notes")], &dirs(&["/Users/test_user_10/notes"])).expect("a plan");
    assert_eq!(plan.dir, p("/Users/test_user_10/notes"));
    assert!(plan.entries.is_empty());
}

#[test]
fn siblings_are_all_selected() {
    let plan = plan_reveal(&[p("/tmp/one.txt"), p("/tmp/two.txt"), p("/tmp/three.txt")], &dirs(&[])).expect("a plan");
    assert_eq!(plan.dir, p("/tmp"));
    assert_eq!(plan.entries, vec!["one.txt", "two.txt", "three.txt"]);
}

#[test]
fn paths_outside_the_first_ones_directory_are_left_out() {
    // One pane shows one directory. Anything elsewhere is dropped, and `deliver`
    // logs how many.
    let plan = plan_reveal(&[p("/tmp/one.txt"), p("/var/other.txt")], &dirs(&[])).expect("a plan");
    assert_eq!(plan.entries, vec!["one.txt"]);
}

#[test]
fn a_folder_first_ignores_the_rest() {
    let plan = plan_reveal(&[p("/tmp/folder"), p("/tmp/one.txt")], &dirs(&["/tmp/folder"])).expect("a plan");
    assert_eq!(plan.dir, p("/tmp/folder"));
    assert!(plan.entries.is_empty());
}

fn request(paths: &[&str], origin: RevealOrigin) -> RevealRequest {
    RevealRequest {
        paths: paths.iter().map(PathBuf::from).collect(),
        target: GoToTarget::FOCUSED,
        origin,
    }
}

fn moves(plans: &[RevealPlan]) -> Vec<(&str, Vec<&str>, bool)> {
    plans
        .iter()
        .map(|plan| {
            (
                plan.dir.to_str().expect("a UTF-8 test path"),
                plan.entries.iter().map(String::as_str).collect(),
                plan.new_tab,
            )
        })
        .collect()
}

#[test]
fn a_revealed_file_stays_in_the_current_tab() {
    let plans = plan_moves(&request(&["/tmp/a.txt"], RevealOrigin::Finder), &dirs(&[]));
    assert_eq!(moves(&plans), vec![("/tmp", vec!["a.txt"], false)]);
}

#[test]
fn a_revealed_folder_gets_a_new_tab() {
    // A folder opened through the folder default and a reveal of a folder arrive as the
    // same event, so they can't be told apart, and both get the same answer.
    let plans = plan_moves(&request(&["/tmp/a"], RevealOrigin::Finder), &dirs(&["/tmp/a"]));
    assert_eq!(moves(&plans), vec![("/tmp/a", vec![], true)]);
}

#[test]
fn revealed_files_elsewhere_are_still_left_out() {
    let plans = plan_moves(
        &request(&["/tmp/a.txt", "/tmp/b.txt", "/var/c.txt"], RevealOrigin::Finder),
        &dirs(&[]),
    );
    assert_eq!(moves(&plans), vec![("/tmp", vec!["a.txt", "b.txt"], false)]);
}

#[test]
fn a_revealed_package_is_selected_in_its_folder_like_a_file() {
    // Finder selects a revealed `Foo.app` in its folder, and so do we.
    let plans = plan_moves(
        &request(&["/Users/test_user_10/Downloads/Foo.app"], RevealOrigin::Finder),
        &dirs(&["/Users/test_user_10/Downloads/Foo.app"]),
    );
    assert_eq!(
        moves(&plans),
        vec![("/Users/test_user_10/Downloads", vec!["Foo.app"], false)]
    );
}

#[test]
fn open_in_cmdr_still_opens_a_package_it_was_pointed_at() {
    // Somebody who picked a package from the right-click menu asked for it by name.
    let plans = plan_moves(
        &request(&["/tmp/Foo.app"], RevealOrigin::OpenInCmdr),
        &dirs(&["/tmp/Foo.app"]),
    );
    assert_eq!(moves(&plans), vec![("/tmp/Foo.app", vec![], true)]);
}

fn plan(dir: &str, entries: &[&str]) -> RevealPlan {
    RevealPlan {
        dir: p(dir),
        entries: entries.iter().map(|entry| (*entry).to_string()).collect(),
        new_tab: false,
    }
}

fn not_shown(name: &str, folder: &str, reason: RevealNotShownReason) -> RevealNotShown {
    RevealNotShown {
        name: name.to_string(),
        folder: folder.to_string(),
        reason,
    }
}

#[test]
fn a_move_that_missed_the_file_names_it_and_its_folder() {
    let missed = GoToFailure::NotPointed(crate::tools::ToolError::internal("not found"));
    assert_eq!(
        not_shown_for_plan(&plan("/Users/test_user_10/Downloads", &["a.zip"]), &missed),
        not_shown("a.zip", "Downloads", RevealNotShownReason::NotInFolder)
    );
}

#[test]
fn a_move_that_never_opened_the_folder_says_so() {
    let refused = GoToFailure::NotOpened(crate::tools::ToolError::internal("refused"));
    assert_eq!(
        not_shown_for_plan(&plan("/Users/test_user_10/Downloads", &["a.zip"]), &refused),
        not_shown("a.zip", "Downloads", RevealNotShownReason::FolderNotOpened)
    );
    // A folder move names the folder as the item too.
    let refused = GoToFailure::NotOpened(crate::tools::ToolError::internal("refused"));
    assert_eq!(
        not_shown_for_plan(&plan("/Volumes/nas/photos", &[]), &refused),
        not_shown("photos", "photos", RevealNotShownReason::FolderNotOpened)
    );
}

#[test]
fn a_folder_that_stopped_answering_reads_as_not_answering() {
    let stalled = GoToFailure::NotOpened(
        crate::tools::ToolError::internal("stalled")
            .with_data(serde_json::json!({ "reason": "folderStalled", "path": "/Volumes/nas" })),
    );
    assert_eq!(
        not_shown_for_plan(&plan("/Volumes/nas", &["a.txt"]), &stalled).reason,
        RevealNotShownReason::NotAnswering
    );
}

#[test]
fn an_unplanned_reveal_names_its_first_path() {
    assert_eq!(
        not_shown_for_paths(
            &[p("/Volumes/nas/a.txt"), p("/tmp/b.txt")],
            RevealNotShownReason::NotAnswering
        ),
        Some(not_shown("a.txt", "nas", RevealNotShownReason::NotAnswering))
    );
    assert_eq!(
        not_shown_for_paths(&[p("/")], RevealNotShownReason::FolderNotOpened),
        Some(not_shown("/", "/", RevealNotShownReason::FolderNotOpened))
    );
    assert_eq!(not_shown_for_paths(&[], RevealNotShownReason::FolderNotOpened), None);
}

#[test]
fn open_in_cmdr_gives_each_folder_its_own_tab() {
    let plans = plan_moves(
        &request(&["/tmp/a", "/tmp/b"], RevealOrigin::OpenInCmdr),
        &dirs(&["/tmp/a", "/tmp/b"]),
    );
    assert_eq!(moves(&plans), vec![("/tmp/a", vec![], true), ("/tmp/b", vec![], true)]);
}

#[test]
fn open_in_cmdr_opens_a_files_folder_in_a_new_tab_with_the_file_selected() {
    let plans = plan_moves(&request(&["/tmp/a.txt"], RevealOrigin::OpenInCmdr), &dirs(&[]));
    assert_eq!(moves(&plans), vec![("/tmp", vec!["a.txt"], true)]);
}

#[test]
fn open_in_cmdr_groups_files_by_folder_around_folders_in_the_selection() {
    // A Finder search window can select across directories.
    let plans = plan_moves(
        &request(
            &["/tmp/a.txt", "/tmp/folder", "/var/c.txt", "/tmp/b.txt"],
            RevealOrigin::OpenInCmdr,
        ),
        &dirs(&["/tmp/folder"]),
    );
    assert_eq!(
        moves(&plans),
        vec![
            ("/tmp", vec!["a.txt", "b.txt"], true),
            ("/tmp/folder", vec![], true),
            ("/var", vec!["c.txt"], true),
        ]
    );
}

#[test]
fn a_terminal_request_is_one_move_in_the_tab_it_asked_for() {
    let mut opened = request(&["/tmp/a", "/tmp/b"], RevealOrigin::Terminal);
    opened.target.new_tab = true;
    let plans = plan_moves(&opened, &dirs(&["/tmp/a", "/tmp/b"]));
    assert_eq!(moves(&plans), vec![("/tmp/a", vec![], true)]);

    let plans = plan_moves(&request(&["/tmp/a"], RevealOrigin::Terminal), &dirs(&["/tmp/a"]));
    assert_eq!(moves(&plans), vec![("/tmp/a", vec![], false)]);
}

#[test]
fn only_the_first_move_may_overwrite_the_current_tab_at_the_limit() {
    assert_eq!(at_tab_limit(0), AtTabLimit::UseCurrentTabThenStop);
    assert_eq!(at_tab_limit(1), AtTabLimit::Stop);
    assert_eq!(at_tab_limit(8), AtTabLimit::Stop);
}

#[test]
fn nothing_to_show_yields_no_plan() {
    assert!(plan_reveal(&[], &dirs(&[])).is_none());
    // A parentless path (the root itself, reported as a file) can't be revealed.
    assert!(plan_reveal(&[p("/")], &dirs(&[])).is_none());
}

#[test]
fn a_cold_launch_parks_reveals_until_the_frontend_drains_them() {
    let pending = PendingReveals::default();
    assert!(
        pending.accept(finder(&["/tmp/one.txt"])).is_empty(),
        "nothing may be delivered before the frontend is listening"
    );
    assert!(pending.accept(finder(&["/tmp/two.txt"])).is_empty());
    // Back-to-back Finder reveals become one move, so both end up selected.
    assert_eq!(pending.drain(), vec![finder(&["/tmp/one.txt", "/tmp/two.txt"])]);
}

#[test]
fn a_second_reveal_never_erases_the_first() {
    let pending = PendingReveals::default();
    pending.accept(finder(&["/tmp/one.txt"]));
    pending.accept(terminal(&["/tmp/two.txt"], PaneChoice::Right));
    assert_eq!(pending.drain().len(), 2);
}

#[test]
fn terminal_requests_keep_their_own_pane_and_never_merge() {
    let pending = PendingReveals::default();
    pending.accept(terminal(&["/tmp/a"], PaneChoice::Left));
    pending.accept(terminal(&["/tmp/b"], PaneChoice::Right));
    pending.accept(finder(&["/tmp/c"]));
    assert_eq!(
        pending.drain(),
        vec![
            terminal(&["/tmp/a"], PaneChoice::Left),
            terminal(&["/tmp/b"], PaneChoice::Right),
            finder(&["/tmp/c"]),
        ],
        "arrival order, each with its own target"
    );
}

#[test]
fn once_drained_a_reveal_is_delivered_straight_away() {
    let pending = PendingReveals::default();
    pending.drain();
    assert_eq!(
        pending.accept(finder(&["/tmp/one.txt"])),
        vec![finder(&["/tmp/one.txt"])]
    );
    assert!(
        pending.drain().is_empty(),
        "an accepted request must not be queued twice"
    );
}

#[test]
fn file_urls_become_one_finder_reveal() {
    let requests = requests_from_urls(&urls(&["file:///tmp/a.txt", "file:///tmp/b%20c.txt"]));
    assert_eq!(requests, vec![finder(&["/tmp/a.txt", "/tmp/b c.txt"])]);
}

#[test]
fn a_cli_url_becomes_a_terminal_request_with_its_target() {
    let requests = requests_from_urls(&urls(&["cmdr-cli://open?path=%2Ftmp%2Fa&pane=right&tab=new"]));
    assert_eq!(
        requests,
        vec![RevealRequest {
            paths: vec![p("/tmp/a")],
            target: GoToTarget {
                pane: PaneChoice::Right,
                new_tab: true
            },
            origin: RevealOrigin::Terminal,
        }]
    );
}

#[test]
fn invalid_cli_urls_and_other_schemes_are_dropped() {
    let requests = requests_from_urls(&urls(&[
        "cmdr-cli://open?path=relative",
        "cmdr-cli://delete?path=%2Ftmp",
        "https://example.com/",
    ]));
    assert!(requests.is_empty());
}
