//! How each OS answers what it says about how things should look. A platform seam
//! (`docs/guides/platform-seams.md`).

use std::future::Future;

use tauri::AppHandle;

use crate::system_events::DisplayAccessibility;

#[cfg_attr(target_os = "macos", path = "macos/mod.rs")]
#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> f32 = text_size_multiplier;
const _: fn(AppHandle) = observe_changes;
// An `async fn` doesn't coerce to a fn pointer, so these pins name each one's arguments and the
// output its future resolves to.
const _: fn(AppHandle) = |app| {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<String, _>(accent_color(app.clone()));
    resolves_to::<DisplayAccessibility, _>(display_accessibility(app));
    resolves_to::<Option<f32>, _>(glass_tint_amount());
};

/// The system accent color as a `#rrggbb` hex string, or the brand gold when the OS has none.
pub(super) use imp::accent_color;

/// The Accessibility > Display options the UI mirrors, all off where the OS has none.
pub(super) use imp::display_accessibility;

/// The Liquid Glass slider in `0.0..=1.0`, or `None` where there's no slider.
pub(super) use imp::glass_tint_amount;

/// The system text-size multiplier, `1.0` where the OS has none.
pub(super) use imp::text_size_multiplier;

/// Starts following every signal this OS posts when any of the above changes, emitting the
/// matching event each time. Call once, from the setup hook.
pub(crate) use imp::observe_changes;
