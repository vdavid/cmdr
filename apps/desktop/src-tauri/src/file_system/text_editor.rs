//! Which app F4 opens a file in: the text editors macOS lists, the one the user
//! chose, and what happened when the file was handed over.
//!
//! The list is LaunchServices' own answer to "which apps EDIT plain text", so it
//! carries whatever macOS carries, oddities included. ❌ No editor table, no
//! `/Applications` scan. The stored choice arrives as an argument, because the
//! frontend owns the settings store.
//!
//! The wire types compile on every platform: Linux's `xdg-open` arm and the
//! off-macOS `playwright-e2e` arm of `commands/file_actions.rs::open_in_editor`
//! answer with them too. Everything else is macOS only.
//!
//! Evidence, the pick canonicalization, and what's verified about `open -t`:
//! `DETAILS.md` § "Text editor".

/// What `open_in_editor` did with the file, so the frontend acts on a variant rather
/// than reading a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EditorOpenOutcome {
    /// The chosen app got the file (the system default, when that's the choice).
    /// ❗ It means `open` took the request, not that a window appeared.
    Opened,
    /// The chosen app isn't on this Mac right now, so the system default got the file.
    /// The frontend resets the setting and says so.
    ChosenAppMissingOpenedDefaultInstead,
}

/// The answer to one F4.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditorOpenReport {
    pub outcome: EditorOpenOutcome,
    /// The app that got the file, named the way Finder shows it. `None` when there's
    /// nothing to read a name from (no system default resolves, or off macOS).
    pub opened_in_name: Option<String>,
    /// `None` unless the caller asked. When asked: whether macOS lists any installed
    /// text editor besides the system default, which is what the one-time hint needs.
    pub other_editors_installed: Option<bool>,
}

/// Why `open_in_editor` couldn't answer at all. Distinct from [`EditorOpenOutcome`],
/// which reports things that DID happen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OpenInEditorError {
    /// The launcher couldn't be spawned. Carries the OS errno where there is one, so
    /// nothing has to read a message.
    LaunchRefused { errno: Option<i32> },
    /// The launch didn't finish inside the command's deadline.
    TimedOut,
}

impl std::fmt::Display for OpenInEditorError {
    /// ❗ For logs only; the frontend words the variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LaunchRefused { errno } => write!(f, "launch refused (errno {errno:?})"),
            Self::TimedOut => f.write_str("timed out"),
        }
    }
}

impl std::error::Error for OpenInEditorError {}

#[cfg(target_os = "macos")]
mod imp {
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
        let array_ref =
            unsafe { LSCopyAllRoleHandlersForContentType(content_type.as_concrete_TypeRef(), kLSRolesEditor) };
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
    /// once `open` has the request, so none of it delays the editor.
    pub fn open_in_editor(
        file: &Path,
        setting: &str,
        ask_about_other_editors: bool,
    ) -> Result<EditorOpenReport, OpenInEditorError> {
        let mac = ThisMac;
        let (choice, outcome) = resolve_choice(parse_choice(setting), |choice| choice_is_installed(choice, &mac));
        launch(&launch_argv(&choice, file), file)?;
        log::info!(target: "text_editor", "opened {file:?} in {} ({outcome:?})", choice.id());

        let default_id = plain_text_default_id();
        let opened_in = match &choice {
            TextEditorChoice::SystemDefault => default_id.as_deref().and_then(|id| mac.installed_bundle(id)),
            TextEditorChoice::BundleId(id) => mac.installed_bundle(id),
            TextEditorChoice::AppPath(path) => Some(path.clone()),
        };
        let other_editors_installed = ask_about_other_editors
            .then(|| other_editors_installed(&plain_text_editor_ids(), default_id.as_deref(), &mac));
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
    mod tests {
        use std::collections::HashMap;

        use super::TextEditorChoice::{AppPath, BundleId, SystemDefault};
        use super::*;

        const SUBLIME: &str = "com.sublimetext.4";
        const TEXTEDIT: &str = "com.apple.TextEdit";
        const XCODE: &str = "com.apple.dt.Xcode";
        const WARP: &str = "dev.warp.Warp-Stable";
        const BBEDIT: &str = "com.barebones.bbedit";

        /// Where macOS runs a quarantined download from: a randomized read-only mirror.
        const TRANSLOCATED_SUBLIME: &str =
            "/private/var/folders/n9/rg0ms9/T/AppTranslocation/D00135D9-8E1B/d/Sublime Text.app";

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

        // LaunchServices itself.

        /// TextEdit ships with macOS, so something always claims plain text.
        #[test]
        fn macos_names_a_plain_text_default() {
            assert!(plain_text_default_id().is_some());
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::{TextEditorList, list_text_editors, open_in_editor};
