//! Tests for `text_editor.rs`: reading the stored choice, the launch argv, the
//! fallback when the chosen app is gone, the editor list, and pick canonicalization,
//! all against a `FakeMac`, plus one question put to the real LaunchServices.

use std::collections::HashMap;

use super::TextEditorChoice::{AppPath, BundleId, SystemDefault};
use super::*;

const SUBLIME: &str = "com.sublimetext.4";
const TEXTEDIT: &str = "com.apple.TextEdit";
const XCODE: &str = "com.apple.dt.Xcode";
const WARP: &str = "dev.warp.Warp-Stable";
const BBEDIT: &str = "com.barebones.bbedit";

/// Where macOS runs a quarantined download from: a randomized read-only mirror.
const TRANSLOCATED_SUBLIME: &str = "/private/var/folders/n9/rg0ms9/T/AppTranslocation/D00135D9-8E1B/d/Sublime Text.app";

/// A Mac described by what LaunchServices and the filesystem would answer.
#[derive(Default)]
struct FakeMac {
    /// Lowercased bundle id → where LaunchServices resolves it.
    resolves: HashMap<String, PathBuf>,
    /// Bundles on disk → the bundle id inside, if any.
    on_disk: HashMap<PathBuf, Option<String>>,
    /// A spelling → the path it canonicalizes to.
    links: HashMap<PathBuf, PathBuf>,
}

impl FakeMac {
    /// An app on disk, and the copy LaunchServices launches for its id.
    fn with_app(self, id: &str, path: &str) -> Self {
        self.with_copy(id, path).with_resolution(id, path)
    }
    /// A copy on disk that LaunchServices doesn't pick for its id.
    fn with_copy(mut self, id: &str, path: &str) -> Self {
        self.on_disk.insert(path.into(), Some(id.into()));
        self
    }
    /// What LaunchServices answers for an id, whether or not that bundle exists.
    fn with_resolution(mut self, id: &str, path: &str) -> Self {
        self.resolves.insert(id.to_ascii_lowercase(), path.into());
        self
    }
    fn with_unidentified_app(mut self, path: &str) -> Self {
        self.on_disk.insert(path.into(), None);
        self
    }
    fn with_link(mut self, from: &str, to: &str) -> Self {
        self.links.insert(from.into(), to.into());
        self
    }
}

impl AppLookup for FakeMac {
    fn installed_bundle(&self, bundle_id: &str) -> Option<PathBuf> {
        let path = self.resolves.get(&bundle_id.to_ascii_lowercase())?;
        self.on_disk.contains_key(path).then(|| path.clone())
    }
    fn bundle_id_at(&self, app_path: &Path) -> Option<String> {
        self.on_disk.get(&self.canonical_path(app_path)).cloned().flatten()
    }
    fn app_exists(&self, app_path: &Path) -> bool {
        self.on_disk.contains_key(&self.canonical_path(app_path))
    }
    fn canonical_path(&self, path: &Path) -> PathBuf {
        self.links.get(path).cloned().unwrap_or_else(|| path.to_path_buf())
    }
}

/// Xcode is the plain-text default (as on the machine this was measured on),
/// and TextEdit, Sublime Text, and Warp are installed.
fn a_mac() -> FakeMac {
    FakeMac::default()
        .with_app(XCODE, "/Applications/Xcode.app")
        .with_app(TEXTEDIT, "/System/Applications/TextEdit.app")
        .with_app(SUBLIME, "/Applications/Sublime Text.app")
        .with_app(WARP, "/Applications/Warp.app")
}

fn handlers(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// Row ids, sorted: the order rows come in is unspecified.
fn ids(rows: &[EditorRow]) -> Vec<String> {
    let mut ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    ids.sort();
    ids
}

fn sorted(ids: &[&str]) -> Vec<String> {
    let mut ids = handlers(ids);
    ids.sort();
    ids
}

// Reading the stored value.

#[test]
fn the_sentinel_and_an_empty_value_read_as_the_system_default() {
    assert_eq!(parse_choice("system"), SystemDefault);
    assert_eq!(parse_choice(""), SystemDefault);
}

#[test]
fn an_absolute_path_is_a_picked_app() {
    assert_eq!(
        parse_choice("/Applications/Sublime Text.app"),
        AppPath(PathBuf::from("/Applications/Sublime Text.app"))
    );
}

#[test]
fn anything_else_is_a_bundle_id() {
    assert_eq!(parse_choice(SUBLIME), BundleId(SUBLIME.into()));
}

// The launch.

#[test]
fn the_system_default_opens_with_open_t_exactly_as_before() {
    assert_eq!(
        launch_argv(&SystemDefault, Path::new("/Users/dave/notes.txt")),
        vec!["open", "-t", "/Users/dave/notes.txt"]
    );
}

#[test]
fn a_listed_editor_opens_by_bundle_id() {
    assert_eq!(
        launch_argv(&BundleId(SUBLIME.into()), Path::new("/Users/dave/notes.txt")),
        vec!["open", "-b", SUBLIME, "/Users/dave/notes.txt"]
    );
}

#[test]
fn a_picked_app_opens_by_path() {
    assert_eq!(
        launch_argv(
            &AppPath("/Users/dave/Apps/Homemade.app".into()),
            Path::new("/Users/dave/notes.txt")
        ),
        vec!["open", "-a", "/Users/dave/Apps/Homemade.app", "/Users/dave/notes.txt"]
    );
}

/// No shell sees these argvs, so awkward names are ordinary arguments.
#[test]
fn awkward_file_names_travel_verbatim_as_the_last_argument() {
    let file = Path::new("/Users/dave/Ünnepi \"terv\" & co's árvíztűrő.txt");
    for choice in [
        SystemDefault,
        BundleId(SUBLIME.into()),
        AppPath("/Applications/Sublime Text.app".into()),
    ] {
        let argv = launch_argv(&choice, file);
        assert_eq!(
            argv.last().map(String::as_str),
            Some("/Users/dave/Ünnepi \"terv\" & co's árvíztűrő.txt"),
            "{choice:?} should take the file verbatim"
        );
    }
}

// Falling back when the chosen app is gone.

#[test]
fn an_installed_choice_is_launched_as_is() {
    let (choice, outcome) = resolve_choice(BundleId(SUBLIME.into()), |_| true);
    assert_eq!(choice, BundleId(SUBLIME.into()));
    assert_eq!(outcome, EditorOpenOutcome::Opened);
}

#[test]
fn a_missing_bundle_id_falls_back_to_the_system_default_and_says_so() {
    let (choice, outcome) = resolve_choice(BundleId(SUBLIME.into()), |_| false);
    assert_eq!(choice, SystemDefault);
    assert_eq!(outcome, EditorOpenOutcome::ChosenAppMissingOpenedDefaultInstead);
}

#[test]
fn a_missing_picked_app_falls_back_to_the_system_default_and_says_so() {
    let (choice, outcome) = resolve_choice(AppPath("/Volumes/Stick/Editor.app".into()), |_| false);
    assert_eq!(choice, SystemDefault);
    assert_eq!(outcome, EditorOpenOutcome::ChosenAppMissingOpenedDefaultInstead);
}

#[test]
fn the_system_default_is_never_checked_so_never_missing() {
    let (choice, outcome) = resolve_choice(SystemDefault, |_: &TextEditorChoice| -> bool {
        panic!("the system default must not be checked for installed-ness")
    });
    assert_eq!(choice, SystemDefault);
    assert_eq!(outcome, EditorOpenOutcome::Opened);
}

#[test]
fn a_bundle_id_launchservices_still_remembers_is_missing_once_its_bundle_is_gone() {
    let mac = a_mac().with_resolution("org.example.Deleted", "/Applications/Deleted.app");
    assert!(!choice_is_installed(&BundleId("org.example.Deleted".into()), &mac));
    assert!(choice_is_installed(&BundleId(SUBLIME.into()), &mac));
    assert!(!choice_is_installed(&AppPath("/Applications/Gone.app".into()), &mac));
    assert!(choice_is_installed(&AppPath("/Applications/Warp.app".into()), &mac));
}

// The list.

#[test]
fn listed_editors_that_are_not_on_disk_are_dropped() {
    let mac = a_mac().with_resolution("org.example.Deleted", "/Applications/Deleted.app");
    let listed = handlers(&[TEXTEDIT, SUBLIME, "org.example.Deleted", "org.example.NeverResolves"]);
    let rows = installed_editors(&listed, Some(XCODE), &mac);
    assert_eq!(ids(&rows), sorted(&[SUBLIME, TEXTEDIT]));
}

/// Finder stores the id it wrote as `com.apple.dt.xcode`, while LaunchServices
/// lists `com.apple.dt.Xcode`.
#[test]
fn the_system_default_is_left_out_whatever_its_case() {
    let rows = installed_editors(&handlers(&[XCODE, SUBLIME]), Some("com.apple.dt.xcode"), &a_mac());
    assert_eq!(ids(&rows), sorted(&[SUBLIME]));
}

#[test]
fn the_system_default_choice_answers_the_sentinel() {
    let (rows, chosen) = assemble_list(&handlers(&[XCODE, SUBLIME]), Some(XCODE), &SystemDefault, &a_mac());
    assert_eq!(ids(&rows), sorted(&[SUBLIME]));
    assert_eq!(chosen.as_deref(), Some("system"));
}

#[test]
fn a_listed_chosen_editor_is_not_repeated_and_answers_in_the_listed_spelling() {
    let choice = BundleId("com.SublimeText.4".into());
    let (rows, chosen) = assemble_list(&handlers(&[TEXTEDIT, SUBLIME]), Some(XCODE), &choice, &a_mac());
    assert_eq!(ids(&rows), sorted(&[SUBLIME, TEXTEDIT]));
    assert_eq!(chosen.as_deref(), Some(SUBLIME));
}

#[test]
fn a_chosen_editor_macos_does_not_list_is_added() {
    let mac = a_mac().with_app(BBEDIT, "/Applications/BBEdit.app");
    let (rows, chosen) = assemble_list(&handlers(&[TEXTEDIT]), Some(XCODE), &BundleId(BBEDIT.into()), &mac);
    assert_eq!(ids(&rows), sorted(&[BBEDIT, TEXTEDIT]));
    assert_eq!(chosen.as_deref(), Some(BBEDIT));
}

#[test]
fn the_system_default_pinned_by_id_gets_a_row_of_its_own() {
    let (rows, chosen) = assemble_list(
        &handlers(&[XCODE, SUBLIME]),
        Some(XCODE),
        &BundleId(XCODE.into()),
        &a_mac(),
    );
    assert_eq!(ids(&rows), sorted(&[SUBLIME, XCODE]));
    assert_eq!(chosen.as_deref(), Some(XCODE));
}

#[test]
fn a_picked_app_without_a_bundle_id_is_added_by_path() {
    let homemade = "/Users/dave/Apps/Homemade.app";
    let mac = a_mac().with_unidentified_app(homemade);
    let (rows, chosen) = assemble_list(&handlers(&[SUBLIME]), Some(XCODE), &AppPath(homemade.into()), &mac);
    assert_eq!(ids(&rows), sorted(&[homemade, SUBLIME]));
    assert_eq!(chosen.as_deref(), Some(homemade));
}

#[test]
fn a_stored_path_to_a_listed_editor_is_not_repeated() {
    let choice = AppPath("/Applications/Sublime Text.app".into());
    let (rows, chosen) = assemble_list(&handlers(&[SUBLIME]), Some(XCODE), &choice, &a_mac());
    assert_eq!(ids(&rows), sorted(&[SUBLIME]));
    assert_eq!(chosen.as_deref(), Some(SUBLIME));
}

#[test]
fn a_chosen_app_that_is_gone_answers_no_chosen_id() {
    let mac = a_mac().with_resolution("org.example.Deleted", "/Applications/Deleted.app");
    for choice in [
        BundleId("org.example.Deleted".into()),
        AppPath("/Applications/Gone.app".into()),
    ] {
        let (rows, chosen) = assemble_list(&handlers(&[SUBLIME]), Some(XCODE), &choice, &mac);
        assert_eq!(ids(&rows), sorted(&[SUBLIME]), "{choice:?} should add no row");
        assert_eq!(chosen, None, "{choice:?} should answer no chosen id");
    }
}

// Canonicalizing a "Choose an app…" pick.

#[test]
fn a_pick_of_the_copy_launchservices_launches_is_stored_as_its_bundle_id() {
    let choice = canonical_choice(AppPath("/Applications/Sublime Text.app".into()), &a_mac());
    assert_eq!(choice, BundleId(SUBLIME.into()));
}

#[test]
fn a_pick_spelled_through_a_symlink_or_a_trailing_slash_is_still_that_copy() {
    let mac = a_mac().with_link(
        "/Users/dave/Applications/Sublime Text.app",
        "/Applications/Sublime Text.app",
    );
    for picked in [
        "/Users/dave/Applications/Sublime Text.app",
        "/Applications/Sublime Text.app/",
    ] {
        assert_eq!(
            canonical_choice(AppPath(picked.into()), &mac),
            BundleId(SUBLIME.into()),
            "{picked} should store the bundle id"
        );
    }
}

#[test]
fn a_second_copy_picked_on_purpose_is_stored_as_its_path() {
    let mac = a_mac().with_copy(WARP, "/Users/dave/Downloads/Warp.app");
    let choice = canonical_choice(AppPath("/Users/dave/Downloads/Warp.app".into()), &mac);
    assert_eq!(choice, AppPath("/Users/dave/Downloads/Warp.app".into()));
}

#[test]
fn a_pick_without_a_bundle_id_is_stored_as_its_path() {
    let mac = a_mac().with_unidentified_app("/Users/dave/Apps/Homemade.app");
    let choice = canonical_choice(AppPath("/Users/dave/Apps/Homemade.app".into()), &mac);
    assert_eq!(choice, AppPath("/Users/dave/Apps/Homemade.app".into()));
}

/// A browser download keeps its quarantine flag, and macOS may launch it from a
/// translocated mirror while the user picked the real bundle.
#[test]
fn a_pick_that_launchservices_resolves_to_its_translocated_mirror_is_stored_as_its_bundle_id() {
    let mac = FakeMac::default()
        .with_app(SUBLIME, TRANSLOCATED_SUBLIME)
        .with_copy(SUBLIME, "/Applications/Sublime Text.app");
    let choice = canonical_choice(AppPath("/Applications/Sublime Text.app".into()), &mac);
    assert_eq!(choice, BundleId(SUBLIME.into()));
}

#[test]
fn a_translocated_pick_is_stored_as_its_bundle_id() {
    let mac = a_mac().with_copy(SUBLIME, TRANSLOCATED_SUBLIME);
    assert_eq!(
        canonical_choice(AppPath(TRANSLOCATED_SUBLIME.into()), &mac),
        BundleId(SUBLIME.into())
    );
}

#[test]
fn a_translocated_copy_under_another_name_is_a_different_copy() {
    let renamed = "/private/var/folders/n9/rg0ms9/T/AppTranslocation/D00135D9-8E1B/d/Sublime Text Dev.app";
    let mac = a_mac().with_copy(SUBLIME, renamed);
    assert_eq!(canonical_choice(AppPath(renamed.into()), &mac), AppPath(renamed.into()));
}

// The hint's question.

#[test]
fn no_listed_editors_means_no_others() {
    assert!(!other_editors_installed(&[], Some(XCODE), &a_mac()));
}

#[test]
fn the_default_alone_means_no_others() {
    assert!(!other_editors_installed(
        &handlers(&[XCODE]),
        Some("com.apple.dt.xcode"),
        &a_mac()
    ));
}

#[test]
fn one_more_installed_editor_means_others() {
    assert!(other_editors_installed(
        &handlers(&[XCODE, SUBLIME]),
        Some(XCODE),
        &a_mac()
    ));
}

#[test]
fn a_listed_editor_whose_bundle_is_gone_does_not_count() {
    let mac = a_mac().with_resolution("org.example.Deleted", "/Applications/Deleted.app");
    assert!(!other_editors_installed(
        &handlers(&[XCODE, "org.example.Deleted"]),
        Some(XCODE),
        &mac
    ));
}

// Which reports name the app.

#[test]
fn a_plain_open_nobody_asked_about_names_nothing() {
    assert!(!report_needs_app_name(&EditorOpenOutcome::Opened, false));
}

#[test]
fn a_fallback_names_the_app_its_toast_words() {
    assert!(report_needs_app_name(
        &EditorOpenOutcome::ChosenAppMissingOpenedDefaultInstead,
        false
    ));
    assert!(report_needs_app_name(
        &EditorOpenOutcome::ChosenAppMissingOpenedDefaultInstead,
        true
    ));
}

#[test]
fn a_press_that_may_show_the_hint_names_the_app() {
    assert!(report_needs_app_name(&EditorOpenOutcome::Opened, true));
}

// LaunchServices itself.

/// TextEdit ships with macOS, so something always claims plain text.
#[test]
fn macos_names_a_plain_text_default() {
    assert!(plain_text_default_id().is_some());
}
