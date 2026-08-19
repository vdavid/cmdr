//! The conventions the app formats in: dates, number grouping, first day of week.
//!
//! macOS models language and region as two settings, and a person can
//! legitimately read US English while living in Sweden. Foundation carries that
//! second setting as a region override on the locale
//! (`AppleLocale = en_US@rg=sezzzz`), and it formats accordingly: `2026-08-19`
//! and `1 234 567,89`.
//!
//! WebKit doesn't. The webview resolves that same machine to plain `en-US` and
//! writes `08/19/2026` and `1,234,567.89`, and handing the extension back
//! explicitly doesn't help (`en-US-u-rg-sezzzz` resolves straight to `en-US`).
//! A real region SUBTAG does: `en-SE` reproduces Foundation's output exactly.
//! So we compose that tag here rather than asking the webview.

/// The tag whose conventions the user formats in, or `None` when the OS has no
/// usable answer and the webview's own default should stand.
///
/// Reads the AUTOUPDATING current locale, the one Foundation documents as
/// tracking the user's preferences. `+[NSLocale currentLocale]` is a snapshot,
/// and this is read from a live-change path, so the autoupdating one removes
/// any question of whether a cached snapshot outlived the change that triggered
/// the read.
#[cfg(target_os = "macos")]
pub(crate) fn resolved_format_locale() -> Option<String> {
    use objc2_foundation::NSLocale;

    let locale = NSLocale::autoupdatingCurrentLocale();
    compose_format_locale(
        &locale.languageCode().to_string(),
        locale.scriptCode().map(|script| script.to_string()).as_deref(),
        locale.regionCode().map(|region| region.to_string()).as_deref()?,
    )
}

/// `None` off macOS: the webview's default is the right answer on Linux, where
/// the desktop's formatting conventions come from the same environment WebKit
/// already reads.
#[cfg(not(target_os = "macos"))]
pub(crate) fn resolved_format_locale() -> Option<String> {
    None
}

/// Composes `<language>[-Script]-REGION` from Foundation's three parts, or
/// `None` when any part is missing or malformed.
///
/// `None` is load-bearing: the caller falls back to the webview's own locale,
/// which is a working answer. A malformed tag is not — `Intl` would either
/// throw or quietly resolve to something nobody chose, so a part we don't
/// recognize means we don't answer.
///
/// The script rides along only when Foundation names one, which is exactly when
/// dropping it would change the answer (`zh-Hans` and `zh-Hant` format dates
/// differently). Foundation leaves it `nil` for every locale whose script is
/// implied by its language, so the common tag stays the short one.
fn compose_format_locale(language: &str, script: Option<&str>, region: &str) -> Option<String> {
    let language = language.trim();
    let region = region.trim();
    if !is_language_subtag(language) || !is_region_subtag(region) {
        return None;
    }
    let mut tag = language.to_ascii_lowercase();
    if let Some(script) = script.map(str::trim).filter(|script| !script.is_empty()) {
        if !is_script_subtag(script) {
            return None;
        }
        tag.push('-');
        tag.push_str(&titlecase(script));
    }
    tag.push('-');
    tag.push_str(&region.to_ascii_uppercase());
    Some(tag)
}

/// Two or three letters, and not the `und` that means "no language here".
/// Composing `und-SE` would be a tag we can't defend; no answer is better.
fn is_language_subtag(part: &str) -> bool {
    matches!(part.len(), 2 | 3) && part.chars().all(|c| c.is_ascii_alphabetic()) && !part.eq_ignore_ascii_case("und")
}

/// Four letters (`Hans`, `Latn`).
fn is_script_subtag(part: &str) -> bool {
    part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic())
}

/// Two letters (`SE`) or three digits (the UN M49 codes, `419`).
fn is_region_subtag(part: &str) -> bool {
    (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
        || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
}

/// `hans` → `Hans`, the casing BCP-47 writes scripts in.
fn titlecase(part: &str) -> String {
    let mut chars = part.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_the_region_override_this_machine_carries() {
        // The motivating case: US English, Swedish region. `en-SE` is the tag
        // that reproduces Foundation's `2026-08-19` and `1 234 567,89`; plain
        // `en-US` is what the webview would have said on its own.
        assert_eq!(compose_format_locale("en", None, "SE"), Some("en-SE".to_string()));
    }

    #[test]
    fn a_machine_with_no_override_composes_the_tag_it_already_had() {
        // Nothing changes for the common case: an en-US Mac still formats en-US.
        assert_eq!(compose_format_locale("en", None, "US"), Some("en-US".to_string()));
        assert_eq!(compose_format_locale("hu", None, "HU"), Some("hu-HU".to_string()));
    }

    #[test]
    fn a_named_script_rides_along() {
        // Dropping it would hand `zh-Hant` readers Simplified date and number
        // conventions.
        assert_eq!(
            compose_format_locale("zh", Some("Hant"), "TW"),
            Some("zh-Hant-TW".to_string())
        );
    }

    #[test]
    fn casing_comes_out_canonical() {
        assert_eq!(
            compose_format_locale("ZH", Some("hant"), "tw"),
            Some("zh-Hant-TW".to_string())
        );
    }

    #[test]
    fn a_numeric_region_is_a_region() {
        // UN M49 codes are legal region subtags and Foundation does emit them.
        assert_eq!(compose_format_locale("es", None, "419"), Some("es-419".to_string()));
    }

    #[test]
    fn a_missing_or_malformed_part_answers_nothing() {
        // Every one of these would compose a tag `Intl` either rejects or
        // resolves to something the user never chose. The caller falls back to
        // the webview's own locale instead, which at least works.
        assert_eq!(compose_format_locale("", None, "SE"), None);
        assert_eq!(compose_format_locale("en", None, ""), None);
        assert_eq!(compose_format_locale("english", None, "SE"), None);
        assert_eq!(compose_format_locale("en", None, "SWE"), None);
        assert_eq!(compose_format_locale("en", None, "S"), None);
        assert_eq!(compose_format_locale("en", Some("Han"), "TW"), None);
        assert_eq!(compose_format_locale("e n", None, "SE"), None);
    }

    #[test]
    fn an_unknown_language_answers_nothing() {
        // `und` is Foundation saying it doesn't know. `und-SE` is a tag we
        // can't defend, so we'd rather say nothing and let the webview answer.
        assert_eq!(compose_format_locale("und", None, "SE"), None);
    }

    #[test]
    fn an_empty_script_is_the_same_as_no_script() {
        assert_eq!(compose_format_locale("en", Some(""), "SE"), Some("en-SE".to_string()));
    }

    /// The real machine, not a table: whatever this Mac is set to, the answer
    /// has to be a tag we'd hand `Intl` without flinching.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_live_answer_is_a_well_formed_tag() {
        let Some(tag) = resolved_format_locale() else {
            return; // A Mac with no region set at all: the webview default stands.
        };
        let mut parts = tag.split('-');
        assert!(
            is_language_subtag(parts.next().unwrap_or_default()),
            "bad language in {tag}"
        );
        let last = parts.next_back().unwrap_or_default();
        assert!(is_region_subtag(last), "bad region in {tag}");
    }
}
