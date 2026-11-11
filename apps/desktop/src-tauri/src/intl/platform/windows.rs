//! Windows: no reader of the OS language yet (`GetUserPreferredUILanguages` is the one to add),
//! so both halves answer as an OS with no preference list does.

use super::super::native_strings::NativeOs;

/// Native strings read their `windows` text here. Never Linux's: `cmdr-menu`'s `Platform`
/// folds Windows into Linux, which is why text doesn't ask it.
pub(crate) const NATIVE_TEXT_OS: NativeOs = NativeOs::Windows;

/// `None`: the webview's own default stands.
pub(crate) fn webview_ui_locale() -> Option<String> {
    None
}

/// The POSIX locale environment, which a Windows session normally doesn't set, so this is
/// usually `en`.
pub(crate) fn os_ui_locale() -> String {
    super::locale_env::posix_ui_locale()
}
