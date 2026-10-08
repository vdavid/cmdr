//! "Open with" submenu builder.
//!
//! Builds a native submenu from a list of candidate apps. Each item uses a stable ID
//! of the form `open-with:<bundle-id>`; `lib.rs::on_menu_event` prefix-matches that
//! and routes the click to `file_system::open_with::open_paths_with`. App URLs are
//! cached in `MenuState.open_with_apps` keyed by bundle ID so we can resolve the
//! click target without encoding paths into menu IDs.
//!
//! Each candidate is a plain item. Its app icon (loaded inside
//! `file_system::open_with::compute_open_with_choices` as it builds the candidate list)
//! lands when the menu starts tracking, through `context_menu_icons.rs`, which finds this
//! submenu by `OPEN_WITH_SUBMENU_ID`.
//!
//! The candidates come from LaunchServices off the main thread (`context_menu_facts.rs`).
//! When they haven't answered by the time the menu goes up, the submenu opens with a
//! disabled "Finding apps…" line in their place ([`build_pending_open_with_submenu`]), and
//! [`fill_open_with_submenu`] swaps the real list in while the menu is open.
//!
//! The first candidate (the OS default for the right-clicked file) reads "{app} (default)".
//! [`default_label`] splits that label into parts, and `context_menu_icons.rs` draws the words
//! around the app's name in `secondaryLabelColor` on the live item, matching Finder's "Open
//! with" submenu. The Tauri item itself carries the plain text.

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::{
    AppHandle, Runtime,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::context_menu_live::Placeheld;
use crate::file_system::open_with::AppCandidate;
use crate::intl::menu_t;

/// Menu item ID prefix for "Open with" candidate apps. Followed by the app's bundle ID.
pub const OPEN_WITH_ID_PREFIX: &str = "open-with:";

/// The "Open with" submenu's own ID, which `context_menu_icons.rs` finds it by to put the
/// app icons on its items. Outside the `open-with:` family, which `handle_menu_event`
/// prefix-routes as launch targets.
pub const OPEN_WITH_SUBMENU_ID: &str = "open-with-submenu";

/// Menu item ID for "Open with → Other…" (NSOpenPanel picker).
pub const OPEN_WITH_OTHER_ID: &str = "open-with-other";

/// Builds the "Open with" submenu and returns it alongside a `bundle_id → app_path`
/// map that the caller stores in `MenuState` so click events can resolve the launch
/// target.
pub fn build_open_with_submenu<R: Runtime>(
    app: &AppHandle<R>,
    candidates: &[AppCandidate],
) -> tauri::Result<(Submenu<R>, HashMap<String, PathBuf>)> {
    let submenu = Submenu::with_id(app, OPEN_WITH_SUBMENU_ID, menu_t("menu.context.openWith"), true)?;
    // An empty intersection (a mixed selection, or no apps registered) still gets "Other…",
    // like Finder, so the user can pick an app by hand.
    let (items, bundle_to_path) = candidate_items(app, candidates)?;
    for item in &items {
        submenu.append(item)?;
    }
    if !items.is_empty() {
        submenu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    submenu.append(&other_item(app)?)?;
    Ok((submenu, bundle_to_path))
}

/// The "Open with" submenu for a menu that went up before LaunchServices answered: a
/// disabled "Finding apps…" line, a separator, and "Other…", which works without the list.
pub fn build_pending_open_with_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Placeheld<R>> {
    let submenu = Submenu::with_id(app, OPEN_WITH_SUBMENU_ID, menu_t("menu.context.openWith"), true)?;
    let placeholder = MenuItem::new(app, menu_t("menu.context.openWithLoading"), false, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    submenu.append(&placeholder)?;
    submenu.append(&separator)?;
    submenu.append(&other_item(app)?)?;
    Ok(Placeheld {
        submenu,
        placeholder,
        separator,
    })
}

/// Swaps the late candidate list into a pending submenu, which may be open on screen, leaving
/// the shape [`build_open_with_submenu`] would have built. Answers the `bundle_id → app_path`
/// map for `MenuState`.
///
/// ❗ Runs while the menu tracks, so it touches only the submenu and its items, never the
/// context menu itself: muda holds that one borrowed for the whole popup.
pub fn fill_open_with_submenu<R: Runtime>(
    app: &AppHandle<R>,
    pending: &Placeheld<R>,
    candidates: &[AppCandidate],
) -> tauri::Result<HashMap<String, PathBuf>> {
    let (items, bundle_to_path) = candidate_items(app, candidates)?;
    pending.submenu.remove(&pending.placeholder)?;
    if items.is_empty() {
        pending.submenu.remove(&pending.separator)?;
    }
    for (position, item) in items.iter().enumerate() {
        pending.submenu.insert(item, position)?;
    }
    Ok(bundle_to_path)
}

/// The candidates' items, and the `bundle_id → app_path` map a click resolves its app through.
type CandidateItems<R> = (Vec<MenuItem<R>>, HashMap<String, PathBuf>);

/// One item per candidate, the OS default first and marked.
fn candidate_items<R: Runtime>(app: &AppHandle<R>, candidates: &[AppCandidate]) -> tauri::Result<CandidateItems<R>> {
    let mut items = Vec::with_capacity(candidates.len());
    let mut bundle_to_path: HashMap<String, PathBuf> = HashMap::new();
    for (idx, candidate) in candidates.iter().enumerate() {
        let label = if idx == 0 {
            default_label(&candidate.display_name)
                .into_iter()
                .map(|part| part.text)
                .collect()
        } else {
            candidate.display_name.clone()
        };
        let id = format!("{OPEN_WITH_ID_PREFIX}{}", candidate.bundle_id);
        items.push(MenuItem::with_id(app, &id, &label, true, None::<&str>)?);
        bundle_to_path.insert(candidate.bundle_id.clone(), candidate.app_path.clone());
    }
    Ok((items, bundle_to_path))
}

/// One stretch of the OS default's label: the app's name, or the words around it that say it's
/// the default, which draw dimmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LabelPart {
    pub text: String,
    pub dim: bool,
}

/// The OS default's label for `app` in the active language, in parts.
pub(super) fn default_label(app: &str) -> Vec<LabelPart> {
    default_label_parts(&menu_t("menu.context.openWithDefault"), app)
}

/// `template` with each `{app}` as an undimmed part holding `app`, and the words between them
/// dimmed. The parts join into exactly what `menu_t_with` would make of it. A template with no
/// `{app}` (a translation that dropped it) comes back whole and undimmed.
fn default_label_parts(template: &str, app: &str) -> Vec<LabelPart> {
    const TOKEN: &str = "{app}";
    if !template.contains(TOKEN) {
        return vec![LabelPart {
            text: template.to_string(),
            dim: false,
        }];
    }
    let mut parts = Vec::new();
    for (index, words) in template.split(TOKEN).enumerate() {
        if index > 0 && !app.is_empty() {
            parts.push(LabelPart {
                text: app.to_string(),
                dim: false,
            });
        }
        if !words.is_empty() {
            parts.push(LabelPart {
                text: words.to_string(),
                dim: true,
            });
        }
    }
    parts
}

fn other_item<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        app,
        OPEN_WITH_OTHER_ID,
        menu_t("menu.context.openWithOther"),
        true,
        None::<&str>,
    )
}

/// Launches `app_path` on the rows an "Open with" click acts on, for both the listed apps
/// and "Other…".
///
/// A row only a route serves (a file inside an archive) has no file an app can open, so
/// it's pulled into a fresh read-only copy first (`file_viewer/open_with_extract.rs`). That
/// pull streams the file, so it runs off the main thread, and the launch hops back on.
/// Every other launch stays on the main thread, synchronous, as it always was.
///
/// A pull that can't finish (too big, a locked or damaged archive) launches nothing and
/// tells the main window why (`OpenWithCopyRefused`), which shows it as a toast.
#[cfg(target_os = "macos")]
pub(super) fn launch_with<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>, app_path: PathBuf) {
    use crate::file_system::open_with::open_paths_with;
    use crate::file_viewer::open_with_extract;

    if !open_with_extract::any_needs_extraction(&paths) {
        if let Err(e) = open_paths_with(&paths, &app_path) {
            log::warn!("Open with failed for {}: {e}", app_path.display());
        }
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let copies = match open_with_extract::launch_paths(&paths) {
            Ok(copies) => copies,
            Err(refused) => {
                log::warn!(
                    "Open with: couldn't copy {} out for {}: {}",
                    refused.path.display(),
                    app_path.display(),
                    refused.error
                );
                announce_refused_copy(&app, &refused, &app_path);
                return;
            }
        };
        let hopped = app.run_on_main_thread(move || {
            if let Err(e) = open_paths_with(&copies, &app_path) {
                log::warn!("Open with failed for {}: {e}", app_path.display());
            }
        });
        if let Err(e) = hopped {
            log::warn!("Open with: couldn't reach the main thread to launch: {e}");
        }
    });
}

/// Tells the main window an "Open with" launch didn't happen, and why.
#[cfg(target_os = "macos")]
fn announce_refused_copy<R: Runtime>(
    app: &AppHandle<R>,
    refused: &crate::file_viewer::open_with_extract::RefusedCopy,
    app_path: &std::path::Path,
) {
    use crate::file_viewer::open_with_extract::OpenWithCopyRefused;
    use tauri_specta::Event;

    if let Err(e) = OpenWithCopyRefused::new(refused, app_path).emit(app) {
        log::warn!("Open with: couldn't tell the main window the copy was refused: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(text: &str, dim: bool) -> LabelPart {
        LabelPart {
            text: text.to_string(),
            dim,
        }
    }

    #[test]
    fn the_words_around_the_app_name_dim_and_the_name_does_not() {
        assert_eq!(
            default_label_parts("{app} (default)", "Preview"),
            vec![part("Preview", false), part(" (default)", true)]
        );
    }

    /// A language may put its words before the name, or on both sides.
    #[test]
    fn words_on_either_side_of_the_name_dim() {
        assert_eq!(
            default_label_parts("Standard: {app} ✓", "Vorschau"),
            vec![part("Standard: ", true), part("Vorschau", false), part(" ✓", true)]
        );
    }

    /// A translation that dropped `{app}` is a bug, but its label still shows as it's written,
    /// undimmed, rather than as one gray line.
    #[test]
    fn a_template_without_the_name_shows_plain() {
        assert_eq!(default_label_parts("Default", "Preview"), vec![part("Default", false)]);
    }

    /// The parts always join into exactly the label `menu_t_with` makes (a plain `{app}`
    /// replace), which is the title the icon pass matches the item by.
    #[test]
    fn the_parts_join_into_the_plain_label() {
        for template in ["{app} (default)", "{app}（默认）", "Default", "{app} and {app}"] {
            let joined: String = default_label_parts(template, "Preview")
                .into_iter()
                .map(|part| part.text)
                .collect();
            assert_eq!(joined, template.replace("{app}", "Preview"));
        }
    }
}
