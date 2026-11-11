//! What the OS says about language and region, answered for the whole app.
//!
//! Two questions, because macOS asks the user two: which LANGUAGE should the app
//! speak, and whose CONVENTIONS should it format dates and numbers in? A person
//! can read US English and live in Sweden, and System Settings keeps those apart
//! on purpose.
//!
//! The language half is this file: macOS hands us an ORDERED list of languages
//! the user reads (`system_strings::apple_languages`, macOS-only), and given the
//! catalogs we actually ship, one of them is the answer. The region half is
//! `format_locale.rs`, which composes the tag the webview can't work out for
//! itself. [`get_os_locales`] hands both over at once.
//!
//! The answers live in Rust rather than in the frontend because two of the
//! language half's three consumers run before the webview exists: the native
//! menu bar (built during `setup`) and the already-running-instance alert (fires
//! before any window). A second resolver in Rust for those, with the real one in
//! TypeScript, would be two implementations of one rule, drifting apart.

use serde::{Deserialize, Serialize};

// The catalog table is generated from the message-catalog directories. The
// `#[path]` keeps the `.gen.rs` spelling the repo already uses to mark a
// generated artifact (`keys.gen.ts`, `bindings.ts`).
#[path = "shipped_locales.gen.rs"]
mod shipped_locales;

use shipped_locales::SHIPPED_LOCALES;

mod format_locale;
mod live_locale;
mod platform;
mod resolve;

#[cfg(test)]
pub(crate) use resolve::overlay_base;
pub(crate) use resolve::{inheritance_chain, resolve_ui_locale};
// The resolver's answers, checked against what macOS itself would do with the
// same question. macOS-only because CFBundle is.
#[cfg(all(test, target_os = "macos"))]
mod macos_fallback_test;
// `pub(crate)` only so a test in another module can take the locale lock; the
// module's real surface is the re-export below.
#[cfg(test)]
pub(crate) mod native_strings;
#[cfg(not(test))]
mod native_strings;

pub use live_locale::observe_os_locale_changes;
/// The catalog the native surfaces open while the user hasn't pinned a language. Distinct from
/// [`get_os_locales`], which answers `None` off macOS: the menu bar has no webview to defer to.
pub(crate) use platform::os_ui_locale;
// `refresh_active_locale` is deliberately NOT re-exported: its only caller is
// `live_locale.rs`, one module down, and re-exporting it would leave a dead
// public name on every platform that has no live-locale observer.
pub use native_strings::{active_locale, menu_t, menu_t_with, set_language_preference};

/// One catalog we ship, plus the CLDR facts the resolver needs.
///
/// The scripts come from CLDR's likely-subtags data, which Rust has no runtime
/// access to; `apps/desktop/scripts/gen-shipped-locales.ts` asks Node's `Intl`
/// for them at build time and emits [`SHIPPED_LOCALES`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShippedLocale {
    /// The catalog directory name, verbatim (`zh`, `en`). Compared
    /// case-insensitively; handed back to the caller spelled as it is here,
    /// because the frontend keys its catalog map on the directory name.
    pub(crate) tag: &'static str,
    /// The likely script of `tag` itself, lowercased: what a reader of this
    /// catalog reads. Our `zh` catalog is Simplified, so `"hans"`.
    pub(crate) script: &'static str,
    /// The likely script of the bare language subtag, lowercased. Differs from
    /// [`Self::script`] only when `tag` names a script itself (`zh-Hant`).
    pub(crate) default_script: &'static str,
    /// Regions of that language whose likely script differs from
    /// [`Self::default_script`], lowercased: `zh` carries `("tw", "hant")` and
    /// friends. Empty for every Latin-script language.
    pub(crate) region_scripts: &'static [(&'static str, &'static str)],
    /// CLDR nodes this catalog answers for besides its own [`Self::tag`],
    /// lowercased. Only `en-GB` carries one today: it answers for `en-001`,
    /// CLDR's World English, which is where [`shipped_locales::PARENT_LOCALES`] sends `en-NZ`,
    /// `en-IE`, `en-ZA`, and ~110 other regions nobody ships a catalog for.
    /// Declared in the generator's `CATALOG_COVERS`, with the reasoning.
    pub(crate) covers: &'static [&'static str],
}

/// Whether `tag` names a catalog this build ships, compared case-insensitively. A pinned
/// `appearance.language` is always one; anything else is a hand-edited or newer build's value.
pub(crate) fn is_shipped_locale(tag: &str) -> bool {
    SHIPPED_LOCALES
        .iter()
        .any(|locale| locale.tag.eq_ignore_ascii_case(tag))
}

impl ShippedLocale {
    /// Whether this catalog is the answer for an already-[`resolve::normalize`]d CLDR
    /// node: it either IS that node, or stands in for it.
    fn answers_for(&self, node: &str) -> bool {
        self.tag.eq_ignore_ascii_case(node) || self.covers.iter().any(|covered| covered.eq_ignore_ascii_case(node))
    }
}

/// Everything the OS has to say about locale, in the two halves the app keeps
/// apart.
///
/// `None` on either half means "no OS answer here, use the webview's own
/// default" — the right behavior off macOS, and on macOS the honest answer when
/// the region is missing or unreadable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OsLocales {
    /// The catalog the UI should open while `appearance.language` is `'system'`
    /// (`hu`, `en`, `en-GB`). Always a tag we ship a catalog for, so it carries a
    /// region only when a regional catalog exists. ❌ Never a formatting tag:
    /// that's `format`, which is composed from the OS region instead.
    pub ui: Option<String>,
    /// The tag whose conventions dates, numbers, and calendars follow (`en-SE`).
    /// Follows the OS and ❌ never the `appearance.language` setting: a
    /// Hungarian UI on a US-English/Swedish-region Mac still formats `en-SE`.
    pub format: Option<String>,
}

/// Tauri command: both locale answers, read fresh from the OS.
///
/// One command rather than two, because the frontend wants both at the same
/// moment (startup, and again whenever the OS moves) and a second round-trip on
/// the startup path would buy nothing.
#[tauri::command]
#[specta::specta]
pub fn get_os_locales() -> OsLocales {
    resolved_os_locales()
}

/// The OS's answers, both halves read fresh on every call, which is what makes a live change
/// visible. Where the OS has no preference list and no region override (Linux), both are `None`,
/// so the webview's own default stands: WebKit reads the same session environment the desktop
/// does.
///
/// The format half goes through `format_locale` rather than being answered here, so that module
/// keeps ONE answer per platform.
pub(crate) fn resolved_os_locales() -> OsLocales {
    OsLocales {
        ui: platform::webview_ui_locale(),
        format: format_locale::resolved_format_locale(),
    }
}
