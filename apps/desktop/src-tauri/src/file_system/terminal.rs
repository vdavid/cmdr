//! "Open terminal here": which terminal apps Cmdr knows, how each one takes a
//! folder, and what actually happens when the action fires.
//!
//! macOS has no system-wide "default terminal", so Cmdr carries its own table the
//! way every other app offering this does. Three parts:
//!
//! - [`KNOWN_TERMINALS`], the table: bundle id, display name, and the recipe that
//!   says how that app takes a folder.
//! - [`launch_argv`], a pure choice + folder → argv function, so every recipe is
//!   unit-testable without launching anything.
//! - [`open_terminal_here`] and [`list_terminal_apps`], the two things the IPC
//!   commands in `commands/file_actions.rs` pass through to.
//!
//! Installed-ness is a millisecond `NSWorkspace` question, asked per call. ❌ Never
//! scan `/Applications`.
//!
//! macOS only, table and all: bundle ids are a macOS vocabulary, and Linux has no
//! default terminal either (`x-terminal-emulator` on Debian, the emerging
//! `xdg-terminal-exec`). A Linux build gets its own module registered under the
//! same command names, the way `permissions` and `permissions_linux` pair up.
//!
//! Rationale, and why there's no window-vs-tab control: `DETAILS.md` § "Open
//! terminal here".

use std::path::{Path, PathBuf};

use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::file_system::open_with::{load_app_icon, read_app_display_name, read_bundle_identifier};

/// Terminal.app: always present on macOS, so it's both the default choice and the
/// fallback when the chosen app has been uninstalled.
pub const TERMINAL_APP_BUNDLE_ID: &str = "com.apple.Terminal";

/// How one terminal app takes the folder it should start in.
///
/// Each app is launched the way it natively accepts a folder, and whether that
/// lands in a window or a tab is left to the app's own preferences. There is no
/// portable way to ask for one or the other, and the user already configured that
/// in their terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchRecipe {
    /// `open -b <bundle id> <dir>`: the app registers as a folder handler and
    /// opens there. Covers most terminals.
    FolderAsDocument,
    /// `open -n -b <bundle id> --args --working-directory <dir>`: the app won't
    /// take a folder as a document, so the directory goes in as a CLI flag.
    /// `-n` is load-bearing: without it an already-running instance is merely
    /// activated and the args are dropped.
    WorkingDirectoryFlag,
    /// `open warp://action/new_window?path=<percent-encoded dir>`: Warp's own
    /// documented URI scheme.
    WarpUri,
}

/// One terminal app Cmdr knows how to launch a folder in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KnownTerminal {
    pub bundle_id: &'static str,
    /// The app's own name, shown in the settings dropdown. Product names, so they
    /// keep their vendor casing (`iTerm2`, `kitty`, `WezTerm`).
    pub display_name: &'static str,
    pub recipe: LaunchRecipe,
}

/// Every terminal app Cmdr knows, Terminal first (it's the default) and the rest
/// alphabetically, which is the order the settings dropdown renders.
///
/// Bundle ids verified 2026-09-04, one source each:
/// - Terminal, Ghostty, Warp: `mdls -name kMDItemCFBundleIdentifier` against the
///   copy installed on the machine.
/// - Alacritty: `extra/osx/Alacritty.app/Contents/Info.plist` (`alacritty/alacritty`, `master`).
/// - Hyper: `appId` in `electron-builder.json` (`vercel/hyper`, `canary`).
/// - iTerm2: `PRODUCT_BUNDLE_IDENTIFIER` in `iTerm2.xcodeproj/project.pbxproj`
///   (`gnachman/iTerm2`, `master`). The `com.iterm2.*` ids beside it belong to
///   helper targets (pidinfo, the proxy, the sandboxed worker), not the app.
/// - kitty: `CFBundleIdentifier=f'net.kovidgoyal.{appname}'` in `setup.py`
///   (`kovidgoyal/kitty`, `master`).
/// - WezTerm: `assets/macos/WezTerm.app/Contents/Info.plist` (`wezterm/wezterm`, `main`).
pub const KNOWN_TERMINALS: &[KnownTerminal] = &[
    KnownTerminal {
        bundle_id: TERMINAL_APP_BUNDLE_ID,
        display_name: "Terminal",
        recipe: LaunchRecipe::FolderAsDocument,
    },
    KnownTerminal {
        bundle_id: "org.alacritty",
        display_name: "Alacritty",
        recipe: LaunchRecipe::WorkingDirectoryFlag,
    },
    KnownTerminal {
        bundle_id: "com.mitchellh.ghostty",
        display_name: "Ghostty",
        recipe: LaunchRecipe::FolderAsDocument,
    },
    KnownTerminal {
        bundle_id: "co.zeit.hyper",
        display_name: "Hyper",
        recipe: LaunchRecipe::FolderAsDocument,
    },
    KnownTerminal {
        bundle_id: "com.googlecode.iterm2",
        display_name: "iTerm2",
        recipe: LaunchRecipe::FolderAsDocument,
    },
    KnownTerminal {
        bundle_id: "net.kovidgoyal.kitty",
        display_name: "kitty",
        recipe: LaunchRecipe::FolderAsDocument,
    },
    KnownTerminal {
        bundle_id: "dev.warp.Warp-Stable",
        display_name: "Warp",
        recipe: LaunchRecipe::WarpUri,
    },
    KnownTerminal {
        bundle_id: "com.github.wez.wezterm",
        display_name: "WezTerm",
        recipe: LaunchRecipe::FolderAsDocument,
    },
];

/// The known terminal with this bundle id, if Cmdr carries a recipe for it.
pub fn known_terminal(bundle_id: &str) -> Option<&'static KnownTerminal> {
    KNOWN_TERMINALS.iter().find(|t| t.bundle_id == bundle_id)
}

/// Which app the "open terminal here" setting names.
///
/// The stored setting is one string holding either kind, so the two are told apart
/// structurally: a custom pick is an absolute path (`/Applications/Foo.app`), and a
/// bundle id never is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalChoice {
    /// One of [`KNOWN_TERMINALS`], with its recipe.
    Known(&'static KnownTerminal),
    /// An app the user picked by hand through "Choose an app…", held as the
    /// absolute path to its `.app` bundle. Launched as a folder handler, which is
    /// what nearly every terminal registers as.
    CustomApp(PathBuf),
}

impl TerminalChoice {
    /// The value that goes back into the setting, and the id the frontend uses to
    /// tell one dropdown option from another.
    pub fn id(&self) -> String {
        match self {
            Self::Known(t) => t.bundle_id.to_string(),
            Self::CustomApp(path) => path.to_string_lossy().into_owned(),
        }
    }
}

/// Reads a stored setting value into a choice.
///
/// `None` means the setting names something Cmdr can't launch: a bundle id that
/// left the table, or a value from a build that knew more apps than this one. The
/// caller falls back to Terminal and says so.
pub fn parse_choice(setting: &str) -> Option<TerminalChoice> {
    let setting = setting.trim();
    if setting.is_empty() {
        // No choice recorded yet, which is the same thing as the default.
        return known_terminal(TERMINAL_APP_BUNDLE_ID).map(TerminalChoice::Known);
    }
    let path = Path::new(setting);
    if path.is_absolute() {
        return Some(TerminalChoice::CustomApp(path.to_path_buf()));
    }
    known_terminal(setting).map(TerminalChoice::Known)
}

/// The bytes left literal inside Warp's `path=` query value: the RFC 3986
/// unreserved set plus `/`, which is legal in a query and is the shape Warp's own
/// docs show. Everything else, `&`, `?`, `#`, `%`, `+`, space, quotes, and every
/// non-ASCII byte, is percent-encoded, so a folder name can't rewrite the URI.
const WARP_PATH_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'/');

/// Warp's documented URI scheme for opening a folder in a new window.
/// <https://docs.warp.dev/terminal/more-features/uri-scheme/>
fn warp_new_window_uri(dir: &Path) -> String {
    let dir = dir.to_string_lossy();
    let encoded = utf8_percent_encode(&dir, WARP_PATH_SET);
    format!("warp://action/new_window?path={encoded}")
}

/// Builds the argv that opens `dir` in the chosen terminal, with `open` itself as
/// the first element.
///
/// Pure: no filesystem, no `NSWorkspace`, no spawning, so every recipe is
/// unit-tested directly. Nothing here goes through a shell, so a folder name with
/// spaces, quotes, or `$` is an ordinary argument.
pub fn launch_argv(choice: &TerminalChoice, dir: &Path) -> Vec<String> {
    let dir_arg = dir.to_string_lossy().into_owned();
    match choice {
        TerminalChoice::Known(terminal) => match terminal.recipe {
            LaunchRecipe::FolderAsDocument => {
                vec!["open".into(), "-b".into(), terminal.bundle_id.into(), dir_arg]
            }
            LaunchRecipe::WorkingDirectoryFlag => vec![
                "open".into(),
                "-n".into(),
                "-b".into(),
                terminal.bundle_id.into(),
                "--args".into(),
                "--working-directory".into(),
                dir_arg,
            ],
            LaunchRecipe::WarpUri => vec!["open".into(), warp_new_window_uri(dir)],
        },
        TerminalChoice::CustomApp(app_path) => vec![
            "open".into(),
            "-a".into(),
            app_path.to_string_lossy().into_owned(),
            dir_arg,
        ],
    }
}

/// What `open_terminal_here` did, so the frontend acts on a variant rather than
/// reading a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OpenTerminalOutcome {
    /// The chosen terminal was launched at the folder.
    Opened,
    /// The chosen app isn't installed anymore, so Terminal opened instead. The
    /// frontend says so and resets the setting.
    AppMissingOpenedTerminalInstead,
    /// The pane isn't on a path a shell can `cd` into (MTP, ADB, a share whose
    /// mount went away), so nothing was launched.
    NotALocalPath,
}

/// Why `open_terminal_here` couldn't answer at all. Distinct from
/// [`OpenTerminalOutcome`], which reports things that DID happen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OpenTerminalError {
    /// `open` couldn't be spawned. Carries the OS errno where there is one, so
    /// nothing has to read the message.
    LaunchRefused { errno: Option<i32> },
    /// The launch didn't finish inside the command's deadline.
    TimedOut,
}

impl std::fmt::Display for OpenTerminalError {
    /// ❗ For logs only; the frontend words the variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LaunchRefused { errno } => write!(f, "launch refused (errno {errno:?})"),
            Self::TimedOut => f.write_str("timed out"),
        }
    }
}

impl std::error::Error for OpenTerminalError {}

/// One installed terminal, as the settings dropdown needs it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalApp {
    /// Exactly what goes into the setting: a bundle id for a known terminal, an
    /// absolute `.app` path for a custom pick.
    pub id: String,
    pub display_name: String,
    /// The app's icon as a base64 WebP data URL, read from its bundle. Absent
    /// when the bundle carries no readable icon.
    pub icon: Option<String>,
    /// Whether the app is running right now. The first-use picker prefers a
    /// running terminal when exactly one is.
    pub is_running: bool,
}

/// The terminal apps installed on this machine, plus which one is chosen.
// DEFAULT-OK: an empty list with nothing chosen is exactly what a non-macOS build
// (and a machine mid-query) has to report, and it renders as "Terminal only".
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAppList {
    /// Installed apps in table order, plus the custom pick last when there is one.
    pub apps: Vec<TerminalApp>,
    /// The id of the currently chosen app, present in `apps`. Absent when the
    /// chosen app has been uninstalled, which is the frontend's cue to reset.
    pub chosen_id: Option<String>,
}

/// Resolves a stored setting to the app that will actually be launched.
///
/// Pure given the "is this installed?" answer, so the fallback rule is testable
/// without an `NSWorkspace`: an installed choice is used as is, anything else
/// falls back to Terminal and reports that it did.
fn resolve_choice(
    setting: &str,
    is_installed: impl Fn(&TerminalChoice) -> bool,
) -> (TerminalChoice, OpenTerminalOutcome) {
    let fallback = || {
        known_terminal(TERMINAL_APP_BUNDLE_ID)
            .map(TerminalChoice::Known)
            .expect("the known-terminals table always carries Terminal.app")
    };
    match parse_choice(setting) {
        Some(choice) if is_installed(&choice) => (choice, OpenTerminalOutcome::Opened),
        // Either the setting named something this build can't launch, or the app
        // it named is gone. Both land on Terminal, and both owe the user a word.
        _ => (fallback(), OpenTerminalOutcome::AppMissingOpenedTerminalInstead),
    }
}

/// Where the app with this bundle id lives, or `None` if it isn't installed.
/// One LaunchServices lookup, which is why the settings row can ask on every
/// render and the action can ask again at launch time. ❌ Never a
/// `/Applications` scan.
fn installed_app_path(bundle_id: &str) -> Option<PathBuf> {
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let url = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
        url.path().map(|p| PathBuf::from(p.to_string()))
    })
}

/// Whether an app with this bundle id has a running instance right now.
fn is_running(bundle_id: &str) -> bool {
    autoreleasepool(|_| {
        !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id)).is_empty()
    })
}

/// Where a choice's `.app` bundle sits, or `None` once it's been uninstalled.
fn choice_app_path(choice: &TerminalChoice) -> Option<PathBuf> {
    match choice {
        TerminalChoice::Known(terminal) => installed_app_path(terminal.bundle_id),
        // A custom pick is an absolute path the user handed us, so its own
        // existence is the whole question.
        TerminalChoice::CustomApp(path) => path.is_dir().then(|| path.clone()),
    }
}

/// The dropdown entry for an app known to be installed at `app_path`.
///
/// The icon comes from the bundle's own `.icns` rather than `NSWorkspace`:
/// it's a plain file read, so it needs no TCC permission and can't descend
/// into a FileProvider XPC chain deep enough to blow a pool thread's stack.
fn app_entry(id: String, display_name: String, app_path: &Path, bundle_id: &str) -> TerminalApp {
    let icon =
        load_app_icon(app_path).and_then(|icon| crate::icons::rgba_to_data_url(&icon.rgba, icon.width, icon.height));
    TerminalApp {
        id,
        display_name,
        icon,
        is_running: is_running(bundle_id),
    }
}

/// The terminal apps installed on this machine, in dropdown order, plus which one
/// the stored `setting` names.
///
/// Asked fresh every time the settings row renders: installed-ness is one
/// LaunchServices lookup per app, so there's nothing to cache and no "Refresh"
/// button to offer.
pub fn list_terminal_apps(setting: &str) -> TerminalAppList {
    let mut apps: Vec<TerminalApp> = KNOWN_TERMINALS
        .iter()
        .filter_map(|terminal| {
            let app_path = installed_app_path(terminal.bundle_id)?;
            Some(app_entry(
                terminal.bundle_id.to_string(),
                terminal.display_name.to_string(),
                &app_path,
                terminal.bundle_id,
            ))
        })
        .collect();

    let choice = parse_choice(setting);

    // A custom pick isn't in the table, so it's appended as its own option;
    // otherwise the dropdown couldn't show what's currently selected.
    if let Some(TerminalChoice::CustomApp(path)) = &choice
        && path.is_dir()
    {
        let bundle_id = read_bundle_identifier(path).unwrap_or_default();
        apps.push(app_entry(
            path.to_string_lossy().into_owned(),
            read_app_display_name(path),
            path,
            &bundle_id,
        ));
    }

    let chosen_id = choice
        .map(|choice| choice.id())
        .filter(|id| apps.iter().any(|app| &app.id == id));
    TerminalAppList { apps, chosen_id }
}

/// Opens `dir` in the terminal the stored `setting` names.
///
/// Reports what happened rather than whether it worked: a pane on a volume with no
/// OS-visible paths launches nothing, and a setting naming an app that's been
/// uninstalled falls back to Terminal and says so.
pub fn open_terminal_here(
    volume_id: &str,
    dir: &Path,
    setting: &str,
) -> Result<OpenTerminalOutcome, OpenTerminalError> {
    if !volume_paths_are_os_visible(volume_id) {
        return Ok(OpenTerminalOutcome::NotALocalPath);
    }
    let (choice, outcome) = resolve_choice(setting, |choice| choice_app_path(choice).is_some());
    launch(&launch_argv(&choice, dir))?;
    log::info!(
        target: "terminal",
        "opened {dir:?} in {} ({outcome:?})",
        choice.id()
    );
    Ok(outcome)
}

/// Whether the pane's volume hands out paths the OS (and so a shell) can
/// reach: local POSIX, OS-mounted shares, direct SMB while its share is
/// mounted. False for MTP, ADB, and a share whose mount went away.
///
/// ❌ Not a test on the path string, and ❌ not `supports_local_fs_access()`:
/// that asks whether Cmdr reads through `std::fs`, and direct SMB answers
/// `false` while its `/Volumes/…` paths are exactly what a shell can `cd`
/// into. Same reading Quick Look and the drag commands take.
fn volume_paths_are_os_visible(volume_id: &str) -> bool {
    match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(volume) => volume.paths_are_os_visible(),
        None => {
            // An id the registry doesn't know is an unmount race, not a
            // verdict. The frontend gates on the pane's volume kind before
            // it ever gets here, so assume yes rather than silently refusing.
            log::debug!(target: "terminal", "volume {volume_id} not found; assuming its paths are OS-visible");
            true
        }
    }
}

/// Spawns the built argv. Fire and forget: `open` returns as soon as
/// LaunchServices has the request.
#[cfg(not(feature = "playwright-e2e"))]
fn launch(argv: &[String]) -> Result<(), OpenTerminalError> {
    let (program, args) = argv.split_first().expect("every recipe puts `open` at argv[0]");
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| OpenTerminalError::LaunchRefused {
            errno: e.raw_os_error(),
        })
}

/// E2E variant: record the folder instead of launching a terminal, so a suite
/// run doesn't pile up windows nothing can close. Same store as `open_path`
/// and `open_in_editor`, read back through `e2e_opened_paths`.
#[cfg(feature = "playwright-e2e")]
fn launch(argv: &[String]) -> Result<(), OpenTerminalError> {
    crate::open_mock::record(
        argv.last()
            .cloned()
            .expect("every recipe ends with the folder or the URI naming it"),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(bundle_id: &str) -> TerminalChoice {
        parse_choice(bundle_id).expect("known bundle id")
    }

    #[test]
    fn terminal_opens_the_folder_as_a_document() {
        assert_eq!(
            launch_argv(&choice(TERMINAL_APP_BUNDLE_ID), Path::new("/Users/dave/code")),
            vec!["open", "-b", "com.apple.Terminal", "/Users/dave/code"]
        );
    }

    #[test]
    fn alacritty_takes_the_folder_as_a_flag_on_a_new_instance() {
        assert_eq!(
            launch_argv(&choice("org.alacritty"), Path::new("/Users/dave/code")),
            vec![
                "open",
                "-n",
                "-b",
                "org.alacritty",
                "--args",
                "--working-directory",
                "/Users/dave/code"
            ]
        );
    }

    #[test]
    fn warp_takes_its_own_uri() {
        assert_eq!(
            launch_argv(&choice("dev.warp.Warp-Stable"), Path::new("/Users/dave/code")),
            vec!["open", "warp://action/new_window?path=/Users/dave/code"]
        );
    }

    #[test]
    fn a_custom_app_is_launched_by_path() {
        let choice = parse_choice("/Applications/Terminus.app").expect("an absolute path is a custom pick");
        assert_eq!(
            launch_argv(&choice, Path::new("/Users/dave/code")),
            vec!["open", "-a", "/Applications/Terminus.app", "/Users/dave/code"]
        );
    }

    /// A shell never sees these argvs, so the awkward characters travel literally
    /// everywhere except Warp's URI, where they're percent-encoded.
    #[test]
    fn awkward_folder_names_travel_literally_as_arguments() {
        let dir = Path::new("/Users/dave/Ünnepi \"terv\" & co");
        for terminal in KNOWN_TERMINALS {
            if terminal.recipe == LaunchRecipe::WarpUri {
                continue;
            }
            let argv = launch_argv(&TerminalChoice::Known(terminal), dir);
            assert_eq!(
                argv.last().map(String::as_str),
                Some("/Users/dave/Ünnepi \"terv\" & co"),
                "{} should take the folder verbatim",
                terminal.display_name
            );
        }
    }

    #[test]
    fn warp_percent_encodes_spaces_quotes_ampersands_and_non_ascii() {
        let argv = launch_argv(
            &choice("dev.warp.Warp-Stable"),
            Path::new("/Users/dave/Ünnepi \"terv\" & co"),
        );
        assert_eq!(
            argv,
            vec![
                "open",
                "warp://action/new_window?path=/Users/dave/%C3%9Cnnepi%20%22terv%22%20%26%20co"
            ]
        );
    }

    /// A folder named like a query parameter must not be able to add one.
    #[test]
    fn warp_encoding_cannot_be_escaped_by_a_folder_name() {
        let argv = launch_argv(
            &choice("dev.warp.Warp-Stable"),
            Path::new("/tmp/x?path=/etc&mode=evil#frag%2F+plus"),
        );
        assert_eq!(
            argv[1],
            "warp://action/new_window?path=/tmp/x%3Fpath%3D/etc%26mode%3Devil%23frag%252F%2Bplus"
        );
    }

    #[test]
    fn every_known_terminal_builds_an_argv_that_names_it() {
        for terminal in KNOWN_TERMINALS {
            let argv = launch_argv(&TerminalChoice::Known(terminal), Path::new("/tmp"));
            assert_eq!(argv.first().map(String::as_str), Some("open"));
            assert!(
                argv.iter()
                    .any(|a| a.contains(terminal.bundle_id) || a.starts_with("warp://")),
                "{} should be named in its own argv",
                terminal.display_name
            );
        }
    }

    #[test]
    fn bundle_ids_are_unique() {
        let mut ids: Vec<&str> = KNOWN_TERMINALS.iter().map(|t| t.bundle_id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two entries share a bundle id");
    }

    #[test]
    fn an_empty_setting_reads_as_terminal() {
        assert_eq!(
            parse_choice(""),
            known_terminal(TERMINAL_APP_BUNDLE_ID).map(TerminalChoice::Known)
        );
    }

    #[test]
    fn an_unknown_bundle_id_names_nothing() {
        assert_eq!(parse_choice("com.example.NotATerminal"), None);
    }

    #[test]
    fn an_absolute_path_is_a_custom_pick() {
        assert_eq!(
            parse_choice("/Applications/Terminus.app"),
            Some(TerminalChoice::CustomApp(PathBuf::from("/Applications/Terminus.app")))
        );
    }

    #[test]
    fn an_installed_choice_is_used_as_is() {
        let (choice, outcome) = resolve_choice("dev.warp.Warp-Stable", |_| true);
        assert_eq!(choice.id(), "dev.warp.Warp-Stable");
        assert_eq!(outcome, OpenTerminalOutcome::Opened);
    }

    #[test]
    fn an_uninstalled_choice_falls_back_to_terminal_and_says_so() {
        let (choice, outcome) = resolve_choice("dev.warp.Warp-Stable", |_| false);
        assert_eq!(choice.id(), TERMINAL_APP_BUNDLE_ID);
        assert_eq!(outcome, OpenTerminalOutcome::AppMissingOpenedTerminalInstead);
    }

    #[test]
    fn a_setting_this_build_does_not_know_falls_back_to_terminal() {
        let (choice, outcome) = resolve_choice("com.example.NotATerminal", |_| true);
        assert_eq!(choice.id(), TERMINAL_APP_BUNDLE_ID);
        assert_eq!(outcome, OpenTerminalOutcome::AppMissingOpenedTerminalInstead);
    }
}
