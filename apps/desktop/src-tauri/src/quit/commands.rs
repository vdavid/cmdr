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

/// The user pressed "Keep working". Releases the gate and **removes** the
/// countdown; it is not a snooze.
#[tauri::command]
#[specta::specta]
pub fn quit_cancel() -> QuitAnswer {
    super::gate().cancel()
}
