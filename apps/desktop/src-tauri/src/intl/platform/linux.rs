//! Linux: no preference list, only the session's locale environment, which WebKit reads too.

use super::super::native_strings::NativeOs;

/// Native strings read their `other` text here, the branch Linux owns.
pub(crate) const NATIVE_TEXT_OS: NativeOs = NativeOs::Linux;

/// `None`: WebKit reads the same session environment the desktop does, so its default is right.
pub(crate) fn webview_ui_locale() -> Option<String> {
    None
}

/// The session's locale environment (`LC_ALL` / `LC_MESSAGES` / `LANG`).
pub(crate) fn os_ui_locale() -> String {
    super::locale_env::posix_ui_locale()
}
