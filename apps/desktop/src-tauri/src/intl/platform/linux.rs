//! Linux: no preference list, only the session's locale environment, which WebKit reads too.

/// `None`: WebKit reads the same session environment the desktop does, so its default is right.
pub(crate) fn webview_ui_locale() -> Option<String> {
    None
}

/// The session's locale environment (`LC_ALL` / `LC_MESSAGES` / `LANG`).
pub(crate) fn os_ui_locale() -> String {
    super::locale_env::posix_ui_locale()
}
