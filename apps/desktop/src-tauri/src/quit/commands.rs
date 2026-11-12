//! The dialog's two answers. Both are thin: the gate owns every decision, and
//! either command may arrive late (or never).
//!
//! Both return the gate's [`QuitAnswer`] rather than dropping it. The dialog has
//! nothing left to do with a [`QuitAnswer::NoQuitPending`] (the deadline claimed
//! the decision, or the quit was called off elsewhere), but a `()` here is how a
//! surface further up invents a success it never got.

use super::QuitAnswer;

/// The user pressed Quit. Stops every operation and ends the process; see
/// [`super::tear_down_and_exit`] for the order and the budget.
#[tauri::command]
#[specta::specta]
pub fn quit_confirm() -> QuitAnswer {
    super::gate().confirm()
}

/// Quits Cmdr and opens it again: the update toast's Restart and onboarding's "Restart Cmdr".
/// The quit goes through the gate like ⌘Q. ❌ Never Tauri's `relaunch()` on macOS: its `open -n`
/// doubles the Dock icon. See [`super::relaunch`].
#[tauri::command]
#[specta::specta]
pub fn relaunch_app(app: tauri::AppHandle) {
    super::relaunch::relaunch(&app);
}

/// Cmdr's own Quit command (the palette, the File menu, a key the user bound), where no app menu
/// owns Quit (Windows and Linux). The same path closing the main window takes
/// (`app_lifecycle::quit_through_gate`): with work in flight the gate asks, else the app exits.
/// macOS never calls it: AppKit's Quit item reaches the gate through `ExitRequested`.
#[tauri::command]
#[specta::specta]
pub fn quit_app(app: tauri::AppHandle) {
    // The outcome isn't the caller's to act on: a held quit is the gate's to finish (it shows the
    // prompt and exits on the answer or its deadline), and a proceeding one ends the process.
    let _ = crate::app_lifecycle::quit_through_gate(&app);
}

/// The user pressed "Keep working". Releases the gate and **removes** the
/// countdown; it is not a snooze.
#[tauri::command]
#[specta::specta]
pub fn quit_cancel() -> QuitAnswer {
    super::gate().cancel()
}
