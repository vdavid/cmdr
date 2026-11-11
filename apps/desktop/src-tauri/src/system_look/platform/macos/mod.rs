//! macOS: the accent color (`NSColor`), Accessibility > Display (`NSWorkspace`), the macOS 27
//! Liquid Glass slider (CFPreferences), and Text Size (`NSUserDefaults`), each with a live
//! observer.

mod accent_color;
mod display_accessibility;
mod glass_tint;
mod text_size;

use tauri::AppHandle;

pub(crate) use accent_color::accent_color;
pub(crate) use display_accessibility::display_accessibility;
pub(crate) use glass_tint::glass_tint_amount;
pub(crate) use text_size::text_size_multiplier;

/// All four observers.
pub(crate) fn observe_changes(app: AppHandle) {
    accent_color::observe_accent_color_changes(app.clone());
    display_accessibility::observe_display_accessibility_changes(app.clone());
    glass_tint::observe_glass_tint_changes(app.clone());
    text_size::observe_system_text_size_changes(app);
}
