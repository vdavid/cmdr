//! How each OS previews a file in a Quick Look panel. A platform seam
//! (`docs/guides/platform-seams.md`).

use std::future::Future;

use tauri::AppHandle;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod unsupported;

const _: fn() -> QuickLookState = init_state;
// An `async fn` doesn't coerce to a fn pointer, so these pins name each one's arguments and the
// output its future resolves to.
const _: fn(AppHandle) = |app| {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<Result<bool, String>, _>(open(app.clone(), String::new(), String::new()));
    resolves_to::<Result<(), String>, _>(set_path(app.clone(), String::new(), String::new()));
    resolves_to::<Result<(), String>, _>(close(app));
};

/// The state the panel keeps between commands, managed once at setup: the controller on macOS,
/// `()` where there's no panel, so the same `app.manage(...)` call site compiles everywhere.
pub(crate) use imp::QuickLookState;

/// The empty state.
pub(crate) use imp::init_state;

/// Opens (or re-opens) the panel on `path`. Answers whether it opened: `false` for a volume
/// whose paths the OS can't preview, or where there's no panel.
pub(crate) use imp::open;

/// Points the open panel at `path`.
pub(crate) use imp::set_path;

/// Closes the panel.
pub(crate) use imp::close;
