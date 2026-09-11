//! The macOS half of `text_editor.rs`: asking LaunchServices for the text editors,
//! canonicalizing a pick, and handing a file to the chosen one with `open`.

use std::path::{Path, PathBuf};

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_services::{
    LSCopyAllRoleHandlersForContentType, LSCopyDefaultRoleHandlerForContentType, kLSRolesAll, kLSRolesEditor,
};

use super::{EditorOpenOutcome, EditorOpenReport, OpenInEditorError};
use crate::file_system::open_with::{
    app_icon_data_url, installed_app_path, read_app_display_name, read_bundle_identifier,
};

/// The stored value meaning "whatever macOS opens plain text in". The frontend
/// mirrors it as `SYSTEM_DEFAULT_EDITOR_CHOICE`.
const SYSTEM_DEFAULT_CHOICE: &str = "system";

/// The content type whose editors F4 offers. VS Code claims text by extension and
/// OSType only, which LaunchServices files under plain text and none of its
/// parents, so this is the one type that lists it.
const PLAIN_TEXT: &str = "public.plain-text";

/// Which app the stored setting names. One string holds all three, told apart by
/// shape: the sentinel, an absolute path, or anything else (a bundle id never
/// starts with `/`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum TextEditorChoice {
    /// `open -t`: the plain-text default, exactly what F4 always did.
    SystemDefault,
    /// `open -b <id>`: a listed editor. LaunchServices picks the copy, so the
    /// choice survives an update that moves or renames the bundle.
    BundleId(String),
    /// `open -a <path>`: a "Choose an app…" pick that has no bundle id, or is a
    /// different copy than the one LaunchServices would launch.
    AppPath(PathBuf),
}

impl TextEditorChoice {
    /// The value that goes back into the setting, and the id the frontend tells
    /// rows apart by.
    fn id(&self) -> String {
        match self {
            Self::SystemDefault => SYSTEM_DEFAULT_CHOICE.to_string(),
            Self::BundleId(id) => id.clone(),
            Self::AppPath(path) => path.to_string_lossy().into_owned(),
        }
    }
}

/// One text editor, as the settings row needs it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TextEditorApp {
    /// Exactly what goes into the setting: a bundle id, or an absolute `.app` path.
    pub id: String,
    /// The app's name the way Finder shows it.
    pub display_name: String,
    /// The app's icon as a base64 WebP data URL, read from its bundle. Absent when
    /// the bundle carries no readable icon.
    pub icon: Option<String>,
}

/// The text editors on this Mac, plus the system default and which one is chosen.
// DEFAULT-OK: an empty list with no default and nothing chosen is what the command's
// deadline has to answer with, and it claims nothing about which editors exist. The
// `TimedOut` flag beside it says which case it was.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TextEditorList {
    /// What the system default is called, for the "System default (…)" row. `None`
    /// when nothing on this Mac claims plain text.
    pub default_app_name: Option<String>,
    pub default_app_icon: Option<String>,
    /// Every other listed editor, plus the chosen app when macOS doesn't list it.
    /// ❗ In no particular order: LaunchServices' own order shifts between calls,
    /// so the frontend sorts by name.
    pub apps: Vec<TextEditorApp>,
    /// The stored choice in canonical form: `system`, or the `id` of one of `apps`.
    /// `None` once the chosen app is gone, which the row displays as the default
    /// without writing anything.
    pub chosen_id: Option<String>,
}

/// One listed app before its name and icon are read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorRow {
    id: String,
    app_path: PathBuf,
}

/// What the list rules ask of the Mac, behind a seam so they're testable without
/// LaunchServices or real bundles.
trait AppLookup {
    /// Where LaunchServices would launch the app with this bundle id from, when
    /// that bundle is on disk right now.
    fn installed_bundle(&self, bundle_id: &str) -> Option<PathBuf>;
    /// The bundle id inside the `.app` at this path, if it has one.
    fn bundle_id_at(&self, app_path: &Path) -> Option<String>;
    /// Whether an `.app` bundle sits at this path right now.
    fn app_exists(&self, app_path: &Path) -> bool;
    /// The path with symlinks resolved, or the path as given when it can't be.
    fn canonical_path(&self, path: &Path) -> PathBuf;
}

/// The real Mac: LaunchServices for bundle ids, the filesystem for paths.
struct ThisMac;

impl AppLookup for ThisMac {
    fn installed_bundle(&self, bundle_id: &str) -> Option<PathBuf> {
        // LaunchServices keeps answering for a bundle that's been deleted until it
        // notices, so the answer only counts once the bundle is really there.
        installed_app_path(bundle_id).filter(|path| path.is_dir())
    }

    fn bundle_id_at(&self, app_path: &Path) -> Option<String> {
        read_bundle_identifier(app_path)
    }

    fn app_exists(&self, app_path: &Path) -> bool {
        app_path.is_dir()
    }

    fn canonical_path(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Bundle ids compare case-insensitively everywhere here: Finder stores the default
/// it wrote lowercased (`com.apple.dt.xcode`), while LaunchServices answers
/// `com.apple.dt.Xcode`.
fn same_bundle_id(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Reads a stored setting value into a choice. Any value names something; whether
/// that something is on this Mac is `choice_is_installed`'s question.
fn parse_choice(setting: &str) -> TextEditorChoice {
    let setting = setting.trim();
    if setting.is_empty() || setting == SYSTEM_DEFAULT_CHOICE {
        return TextEditorChoice::SystemDefault;
    }
    let path = Path::new(setting);
    if path.is_absolute() {
        TextEditorChoice::AppPath(path.to_path_buf())
    } else {
        TextEditorChoice::BundleId(setting.to_string())
    }
}

/// The argv that opens `file` in `choice`, with `open` itself first. Pure, so every
/// shape is unit-tested without launching anything. Nothing goes through a shell,
/// so an awkward file name is an ordinary argument.
fn launch_argv(choice: &TextEditorChoice, file: &Path) -> Vec<String> {
    let file_arg = file.to_string_lossy().into_owned();
    match choice {
        TextEditorChoice::SystemDefault => vec!["open".into(), "-t".into(), file_arg],
        TextEditorChoice::BundleId(id) => vec!["open".into(), "-b".into(), id.clone(), file_arg],
        TextEditorChoice::AppPath(app) => {
            vec!["open".into(), "-a".into(), app.to_string_lossy().into_owned(), file_arg]
        }
    }
}

/// The app that will actually get the file. Pure given the installed-ness answer:
/// an installed choice is used as is, a missing one falls back to the system
/// default and says so, and the system default itself is never checked.
fn resolve_choice(
    choice: TextEditorChoice,
    is_installed: impl Fn(&TextEditorChoice) -> bool,
) -> (TextEditorChoice, EditorOpenOutcome) {
    match choice {
        TextEditorChoice::SystemDefault => (TextEditorChoice::SystemDefault, EditorOpenOutcome::Opened),
        choice if is_installed(&choice) => (choice, EditorOpenOutcome::Opened),
        _ => (
            TextEditorChoice::SystemDefault,
            EditorOpenOutcome::ChosenAppMissingOpenedDefaultInstead,
        ),
    }
}

/// Whether the app a choice names is on this Mac right now: a bundle id through
/// LaunchServices AND a bundle on disk where it points, a picked path through the
/// path itself.
fn choice_is_installed(choice: &TextEditorChoice, mac: &impl AppLookup) -> bool {
    match choice {
        TextEditorChoice::SystemDefault => true,
        TextEditorChoice::BundleId(id) => mac.installed_bundle(id).is_some(),
        TextEditorChoice::AppPath(path) => mac.app_exists(path),
    }
}

/// The editors LaunchServices listed that are on this Mac, minus the system default,
/// which has a row of its own.
fn installed_editors(handler_ids: &[String], default_id: Option<&str>, mac: &impl AppLookup) -> Vec<EditorRow> {
    let mut rows: Vec<EditorRow> = Vec::new();
    for id in handler_ids {
        let is_default = default_id.is_some_and(|default| same_bundle_id(default, id));
        if is_default || rows.iter().any(|row| same_bundle_id(&row.id, id)) {
            continue;
        }
        if let Some(app_path) = mac.installed_bundle(id) {
            rows.push(EditorRow {
                id: id.clone(),
                app_path,
            });
        }
    }
    rows
}

/// A stored choice in the form the list compares by. Only a picked path can change:
/// it becomes its bundle id when that id launches the very bundle picked
/// (`is_same_bundle`), so the choice survives the app moving, and stays a path
/// otherwise (no bundle id, or a second copy chosen on purpose).
fn canonical_choice(choice: TextEditorChoice, mac: &impl AppLookup) -> TextEditorChoice {
    let TextEditorChoice::AppPath(picked) = &choice else {
        return choice;
    };
    let Some(bundle_id) = mac.bundle_id_at(picked) else {
        return choice;
    };
    match mac.installed_bundle(&bundle_id) {
        Some(resolved) if is_same_bundle(picked, &resolved, mac) => TextEditorChoice::BundleId(bundle_id),
        _ => choice,
    }
}

/// Whether `picked` and the bundle LaunchServices `resolved` for the same bundle id
/// are one copy. Compared after resolving symlinks, since the dialog's spelling and
/// LaunchServices' can differ by a symlink or a trailing slash.
///
/// A translocated side can't be compared by path: macOS runs a quarantined download
/// from a randomized read-only mirror under `AppTranslocation/`, whose path says
/// nothing about where the original sits. There the bundle folder's name decides.
/// The bundle id already matched, so only a copy renamed on purpose counts as a
/// different one.
fn is_same_bundle(picked: &Path, resolved: &Path, mac: &impl AppLookup) -> bool {
    let picked = mac.canonical_path(picked);
    let resolved = mac.canonical_path(resolved);
    if picked == resolved {
        return true;
    }
    (is_translocated(&picked) || is_translocated(&resolved)) && picked.file_name() == resolved.file_name()
}

fn is_translocated(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "AppTranslocation")
}

/// The rows the dropdown shows below "System default", and the stored choice's
/// `chosen_id`. The chosen app gets a row of its own when macOS doesn't list it (a
/// pick of an app that doesn't claim plain text, or the default pinned by id), and
/// `chosen_id` is `None` once it's gone.
fn assemble_list(
    handler_ids: &[String],
    default_id: Option<&str>,
    choice: &TextEditorChoice,
    mac: &impl AppLookup,
) -> (Vec<EditorRow>, Option<String>) {
    let mut rows = installed_editors(handler_ids, default_id, mac);
    let chosen_id = match canonical_choice(choice.clone(), mac) {
        TextEditorChoice::SystemDefault => Some(SYSTEM_DEFAULT_CHOICE.to_string()),
        TextEditorChoice::BundleId(id) => {
            if let Some(row) = rows.iter().find(|row| same_bundle_id(&row.id, &id)) {
                // The listed spelling, so the frontend finds the row by plain equality.
                Some(row.id.clone())
            } else if let Some(app_path) = mac.installed_bundle(&id) {
                rows.push(EditorRow {
                    id: id.clone(),
                    app_path,
                });
                Some(id)
            } else {
                None
            }
        }
        TextEditorChoice::AppPath(app_path) => mac.app_exists(&app_path).then(|| {
            let id = app_path.to_string_lossy().into_owned();
            rows.push(EditorRow {
                id: id.clone(),
                app_path,
            });
            id
        }),
    };
    (rows, chosen_id)
}

/// The hint's question: does macOS list any installed text editor besides the
/// system default? Ids and installed-ness only, ❌ no names or icons: it runs inside
/// the launch's deadline, and a deadline that expires after `open` spawned would
/// word an editor that did open as a timeout.
fn other_editors_installed(handler_ids: &[String], default_id: Option<&str>, mac: &impl AppLookup) -> bool {
    !installed_editors(handler_ids, default_id, mac).is_empty()
}

/// Whether the report has to name the app that got the file. Two things word that
/// name: the missing-app toast, and the one-time hint, which can only be due when the
/// caller asked about other editors. A plain open of a stored choice reads nothing,
/// so it skips the default-id lookup and the name read.
fn report_needs_app_name(outcome: &EditorOpenOutcome, ask_about_other_editors: bool) -> bool {
    *outcome != EditorOpenOutcome::Opened || ask_about_other_editors
}

/// Every app LaunchServices lists as a plain-text EDITOR, by bundle id, in its own
/// order (which shifts between calls). The editor role keeps browsers and viewers
/// out. Empty when nothing claims plain text.
///
/// Deprecated since macOS 12 and still in the macOS 26 SDK; a C function, available
/// from macOS 10.4, so the 10.15 floor needs no gate.
fn plain_text_editor_ids() -> Vec<String> {
    let content_type = CFString::from_static_string(PLAIN_TEXT);
    // SAFETY: `content_type` is a live CFString for the whole call, passed by its concrete ref,
    // and `kLSRolesEditor` is the framework's role-mask constant. The function follows the Copy
    // rule: it returns a +1 CFArray, or NULL when nothing claims the type, which is checked next.
    let array_ref = unsafe { LSCopyAllRoleHandlersForContentType(content_type.as_concrete_TypeRef(), kLSRolesEditor) };
    if array_ref.is_null() {
        return Vec::new();
    }
    // SAFETY: `array_ref` is non-null and holds the single +1 reference from the Copy call above,
    // so `wrap_under_create_rule` takes it over and releases it once on drop. Its elements are
    // the bundle-id CFStrings LaunchServices documents, borrowed under the Get rule by `iter`.
    let ids: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(array_ref) };
    ids.iter().map(|id| id.to_string()).collect()
}

/// The bundle id of the app macOS opens plain text in, for all roles. `None` when
/// nothing claims plain text.
fn plain_text_default_id() -> Option<String> {
    let content_type = CFString::from_static_string(PLAIN_TEXT);
    // SAFETY: `content_type` is a live CFString for the whole call, passed by its concrete ref,
    // and `kLSRolesAll` is the framework's role-mask constant. The function follows the Copy
    // rule: it returns a +1 CFString, or NULL when nothing claims the type, which is checked next.
    let id_ref = unsafe { LSCopyDefaultRoleHandlerForContentType(content_type.as_concrete_TypeRef(), kLSRolesAll) };
    if id_ref.is_null() {
        return None;
    }
    // SAFETY: `id_ref` is non-null and holds the single +1 reference from the Copy call above, so
    // `wrap_under_create_rule` takes it over and releases it once on drop.
    let id = unsafe { CFString::wrap_under_create_rule(id_ref) };
    Some(id.to_string())
}

/// The text editors on this Mac, the system default, and which one the stored
/// `setting` names.
///
/// Asked fresh every time the settings row renders, and to canonicalize a
/// "Choose an app…" pick (pass the picked path, read back `chosen_id`). Nothing is
/// cached and there's no "Refresh" button.
pub fn list_text_editors(setting: &str) -> TextEditorList {
    let mac = ThisMac;
    let default_id = plain_text_default_id();
    let default_app = default_id.as_deref().and_then(|id| mac.installed_bundle(id));
    let (rows, chosen_id) = assemble_list(
        &plain_text_editor_ids(),
        default_id.as_deref(),
        &parse_choice(setting),
        &mac,
    );
    TextEditorList {
        default_app_name: default_app.as_deref().map(read_app_display_name),
        default_app_icon: default_app.as_deref().and_then(app_icon_data_url),
        apps: rows
            .into_iter()
            .map(|row| TextEditorApp {
                display_name: read_app_display_name(&row.app_path),
                icon: app_icon_data_url(&row.app_path),
                id: row.id,
            })
            .collect(),
        chosen_id,
    }
}

/// Opens `file` in the app the stored `setting` names.
///
/// Reports what happened rather than whether it worked: a chosen app that's gone
/// falls back to the system default and says so. Everything after the launch (the
/// app's name, and the other-editors query when `ask_about_other_editors`) runs
/// once `open` has the request, so none of it delays the editor, and only what a
/// reader will word runs at all (`report_needs_app_name`): it shares the launch's
/// deadline, and a lookup nobody reads could turn an editor that did open into a
/// `timedOut`.
pub fn open_in_editor(
    file: &Path,
    setting: &str,
    ask_about_other_editors: bool,
) -> Result<EditorOpenReport, OpenInEditorError> {
    let mac = ThisMac;
    let (choice, outcome) = resolve_choice(parse_choice(setting), |choice| choice_is_installed(choice, &mac));
    launch(&launch_argv(&choice, file), file)?;
    log::info!(target: "text_editor", "opened {file:?} in {} ({outcome:?})", choice.id());

    let needs_name = report_needs_app_name(&outcome, ask_about_other_editors);
    // Both answers below can need the system default's id, so it's asked at most once,
    // and only when one of them does.
    let default_id = (ask_about_other_editors || (needs_name && choice == TextEditorChoice::SystemDefault))
        .then(plain_text_default_id)
        .flatten();
    let opened_in = needs_name
        .then(|| match &choice {
            TextEditorChoice::SystemDefault => default_id.as_deref().and_then(|id| mac.installed_bundle(id)),
            TextEditorChoice::BundleId(id) => mac.installed_bundle(id),
            TextEditorChoice::AppPath(path) => Some(path.clone()),
        })
        .flatten();
    let other_editors_installed =
        ask_about_other_editors.then(|| other_editors_installed(&plain_text_editor_ids(), default_id.as_deref(), &mac));
    Ok(EditorOpenReport {
        outcome,
        opened_in_name: opened_in.as_deref().map(read_app_display_name),
        other_editors_installed,
    })
}

/// Spawns the built argv. Fire and forget: `open` returns as soon as
/// LaunchServices has the request.
#[cfg(not(feature = "playwright-e2e"))]
fn launch(argv: &[String], _file: &Path) -> Result<(), OpenInEditorError> {
    let (program, args) = argv.split_first().expect("every argv puts `open` first");
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| OpenInEditorError::LaunchRefused {
            errno: e.raw_os_error(),
        })
}

/// E2E variant: record the FILE instead of launching an editor, so a suite run
/// doesn't pile up windows nothing can close. Never the argv or the app: every
/// `e2e_opened_paths` consumer reads file paths.
#[cfg(feature = "playwright-e2e")]
fn launch(_argv: &[String], file: &Path) -> Result<(), OpenInEditorError> {
    crate::open_mock::record(file.to_string_lossy().into_owned());
    Ok(())
}

#[cfg(test)]
#[path = "text_editor_test.rs"]
mod tests;
