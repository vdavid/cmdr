//! macOS: the user's ordered `AppleLanguages` list, resolved against the catalogs we ship. Both
//! halves answer the same, so the menu bar and the webview always agree.

use super::super::SHIPPED_LOCALES;
use super::super::native_strings::NativeOs;
use super::super::resolve_ui_locale;

/// Native strings read their `macos` text here.
pub(crate) const NATIVE_TEXT_OS: NativeOs = NativeOs::MacOs;

/// The resolved preference list. macOS always answers, falling back to `en`.
pub(crate) fn webview_ui_locale() -> Option<String> {
    Some(os_ui_locale())
}

/// Read fresh from the OS preference list every time. Uncached on purpose: the read is a
/// `NSUserDefaults` lookup (~65 µs), and caching it is what would make a live language change
/// invisible.
pub(crate) fn os_ui_locale() -> String {
    let preferences = crate::system_strings::apple_languages();
    resolve_ui_locale(&preferences, SHIPPED_LOCALES).unwrap_or_else(|| "en".to_string())
}
