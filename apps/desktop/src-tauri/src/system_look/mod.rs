//! What the OS says about how things should look: the accent color, Accessibility > Display
//! options, the Liquid Glass slider, and Text Size. One command each, answered per OS by
//! `platform/`, plus the observers that push a change as it happens.

mod platform;

use tauri::AppHandle;

use crate::system_events::DisplayAccessibility;

/// Starts following every system-look signal this OS posts. Call once, from the setup hook.
pub(crate) use platform::observe_changes;

/// Tauri command: returns the system accent color as a hex string (for example, `#007aff`), or the
/// brand gold when the OS doesn't say.
#[tauri::command]
#[specta::specta]
pub async fn get_accent_color(app: AppHandle) -> String {
    platform::accent_color(app).await
}

/// Tauri command: returns the Accessibility > Display options the UI mirrors, all off where the
/// OS has none.
#[tauri::command]
#[specta::specta]
pub async fn get_display_accessibility(app: AppHandle) -> DisplayAccessibility {
    platform::display_accessibility(app).await
}

/// Tauri command: the Liquid Glass slider value in `0.0..=1.0`, or `None` when the OS doesn't
/// report one.
#[tauri::command]
#[specta::specta]
pub async fn get_glass_tint_amount() -> Option<f32> {
    platform::glass_tint_amount().await
}

/// Tauri command: returns the current system text-size multiplier.
#[tauri::command]
#[specta::specta]
pub fn get_system_text_size_multiplier() -> f32 {
    platform::text_size_multiplier()
}
