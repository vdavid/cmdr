//! Where each OS keeps the user's language, for the UI half of `intl`. A platform seam
//! (`docs/guides/platform-seams.md`); the resolver it feeds (`../resolve.rs`) is portable.

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

/// The POSIX locale environment, read by every arm whose OS has no preference list of its own.
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod locale_env;

const _: fn() -> Option<String> = webview_ui_locale;
const _: fn() -> String = os_ui_locale;

/// The catalog the OS asks the WEBVIEW to open, or `None` when the OS has no answer of its own
/// and the webview's default should stand. Read fresh on every call, so a live change shows.
pub(crate) use imp::webview_ui_locale;

/// The catalog the NATIVE surfaces (the menu bar, the already-running alert) open while the user
/// hasn't pinned a language. Always an answer: they have no webview to defer to.
pub(crate) use imp::os_ui_locale;
