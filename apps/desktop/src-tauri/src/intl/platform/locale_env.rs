use super::super::SHIPPED_LOCALES;
use super::super::resolve_ui_locale;

/// The catalog a POSIX session asks for, read from the locale environment in the order POSIX
/// itself resolves messages: `LC_ALL` beats `LC_MESSAGES` beats `LANG`.
///
/// The values are POSIX locale names (`hu_HU.UTF-8`, `C`), so the codeset suffix is dropped and
/// `resolve::normalize` folds the rest into a comparable tag. There's no ordered preference LIST
/// here the way macOS has one: the session exposes one answer, and it's the same one WebKit hands
/// the webview, so the two halves of the app agree.
pub(crate) fn posix_ui_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.split('.').next().unwrap_or_default().to_string())
        .filter(|value| !value.is_empty())
        .find_map(|value| resolve_ui_locale(&[value], SHIPPED_LOCALES))
        .unwrap_or_else(|| "en".to_string())
}
