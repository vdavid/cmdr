//! The dialog's two answers. Both are thin: the gate owns every decision, and
//! either command may arrive late (or never), which the gate treats as a no-op.

/// The user pressed Quit. Stops every operation and ends the process; see
/// [`super::tear_down_and_exit`] for the order and the budget.
#[tauri::command]
#[specta::specta]
pub fn quit_confirm() {
    super::gate().confirm();
}

/// The user pressed "Keep working". Releases the gate and **removes** the
/// countdown; it is not a snooze.
#[tauri::command]
#[specta::specta]
pub fn quit_cancel() {
    super::gate().cancel();
}
