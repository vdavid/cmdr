# What the OS says about language and region

Two answers, one seam: which catalog the UI speaks (resolved from the user's ORDERED macOS language preferences against
the catalogs we ship), and which tag the app FORMATS in (composed from the OS's language and region). Plus the strings
Rust draws.

## Module map

- `mod.rs`: `resolve_ui_locale` (the walk, the script guard, `ancestor_chain`), `OsLocales`, `get_os_locales`.
- `macos_fallback_test.rs`: pins our English answers against CFBundle's. macOS-only.
- `format_locale.rs`: `resolved_format_locale()`, composing `<language>[-Script]-REGION` from `NSLocale`.
- `live_locale.rs`: `observe_os_locale_changes`, following both answers and emitting `os-locales-changed`. macOS only.
- `native_strings.rs`: `menu_t` / `menu_t_with` and the active-locale cache the native surfaces read.
- `shipped_locales.gen.rs` / `native_strings.gen.rs`: the tables. Generated, never hand-edited.

## Must-knows

- **The LIST is the input, not its first entry.** `apple_languages()` returns the user's own ordered fallback plan;
  honoring only element zero means a `[hu-HU, sv-SE]` user never reaches Swedish. ❌ Never sort, dedupe, or truncate it.
- **A match needs the same language AND the same script.** `zh-Hant-TW` must not open the Simplified `zh` catalog. ❌
  Don't extend the guard to regional variants: `pt-PT` → `pt` and `en-CA` → `en` are WANTED (dialect friction is a
  papercut, an unreadable script a wall).
- **Then it walks CLDR's parent chain, ❌ not subtag truncation.** Truncation drops `en-NZ` on US English and a New
  Zealander reads "Trash"; CLDR parents it to `en-001`, which `en-GB` `covers`, so ~110 regional Englishes reach an
  overlay. ❌ No region table beside it, and set `covers` when an overlay speaks for a CLDR group.
  `macos_fallback_test.rs` pins us to what the reader's own Mac would do.
- **`shipped_locales.gen.rs` holds the CLDR likely-subtags and parent-locale data Rust can't compute at runtime**
  (`pnpm intl:shipped-locales` from `apps/desktop/`, guarded by `shipped-locales-fresh` plus a unit test against the
  catalog dirs). ❌ Never hand-edit it, and never hand-write a script or parent table beside it: both drift with CLDR.
- **Rust's own user-facing strings go through `menu_t("menu.…")`**, reading `native_strings.gen.rs`
  (`pnpm intl:native-strings`, guarded by `native-strings-fresh`). A RAW lookup, so `menu.*` is a raw family in
  `isRawKey` and apostrophes stay single. ❌ Never `t()` one. Missing → English; unknown → the key, never a panic.
- **`set_language_preference` stores the SETTING, not the resolved tag**, so `'system'` keeps tracking the OS. It and
  `refresh_active_locale` report whether the answer MOVED: the cue to rebuild the menu bar.
- **The `en-XA` pseudolocale is excluded by the GENERATORS, not filtered here.** Auto-selection draws only from the
  tables, so leaving it out is what makes it unreachable. Keep it that way, not a runtime check.
- **The formatting tag is COMPOSED here, ❌ never read off the webview**, which drops the `-u-rg-` region override. It
  follows the OS, never `appearance.language`. An unrecognized part composes `None`, never a half-built tag.
  `DETAILS.md`.
- **`'system'` means the language and region the user has set NOW, not at launch.** Two macOS notifications feed one
  `LocaleWatcher`, which collapses a burst into ONE re-read and emits `os-locales-changed` only when the PAIR moved.
  The emit site also rebuilds the native menu bar. Linux has no equivalent signal; the no-op says so.
- **`defaults write -g AppleLanguages` posts NO notification**, so it can't test the observer alone: write it, then post
  `AppleLanguagePreferencesChangedNotification` on the distributed centre. `DETAILS.md`.
- **`get_os_locales` returns two `None`s off macOS**: "no OS answer, the webview default stands". On macOS the language
  half always answers, falling back to `en`. The NATIVE surfaces have no webview to defer to, so `os_ui_locale()` reads
  `LC_ALL` / `LC_MESSAGES` / `LANG` on Linux. That split leaves the parent chain out of the Linux webview:
  `DETAILS.md` § Known gap.

Everything deeper: `DETAILS.md`. Frontend: `apps/desktop/src/lib/intl/`.
