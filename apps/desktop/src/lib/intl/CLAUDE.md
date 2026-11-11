# Locale-aware formatting and message runtime

Two locale sources: the UI LANGUAGE for catalog text and numbers, the OS REGION for dates.

## Module map

- `locale.ts`: `getUiLocale()` (what we speak), `getCatalogLocale()` (its resolved catalog tag: numbers, joins),
  `getFormatLocale()` (always the OS), `setOsFormatLocale()` (the OS's tag arriving). Tests: `_setLocaleForTests` pins
  both, `_setFormatLocaleForTests` splits them.
- `os-locales.ts`: the OS's two answers. `loadSystemLocales()` fetches the Rust pair per window, `pickUiLocale(setting)`
  maps the language half (`null` = no override), `watchSystemLocales()` follows a live change.
- `language-analytics.ts`: language events, fired off the PICK, never a subscription
  (`src-tauri/src/analytics/DETAILS.md`).
- `number-format.ts`: the memoized `getNumberFormatter`, `formatInteger`, `formatCompactCount`, `formatMoney`,
  `getGroupSeparator`. `list-format.ts`: joins (and, or, unit); `createListFormat`, the sole `Intl.ListFormat` door
  (Catalina lacks it). `sentence-join.ts`: `joinSentences()` (❌ never `.join(' ')`: no space in Chinese).
  `duration-format.ts`: narrow durations, no `Intl.DurationFormat`. `relative-time-format.ts`: "2 days ago".
- `locale-inheritance.ts`: which catalog a locale may inherit from (same language AND same script), shared with the i18n
  checks and Rust.
- `messages.svelte.ts`: the runtime: `t()` (catalog + ICU), `getMessage()` (raw), `setLocale()`, `availableLocales()`,
  `resolvedCatalogLocale()`, `manifestMessage()`, the fallback chain, catalog loading, the locale-version rune.
  `startup-catalog.ts`: the catalog a new window starts in. `window-title.svelte.ts`: live window titles.
  `document-language.ts`: the `<html lang>` write. `locale-display-names.ts`: picker row labels. `Trans.svelte`:
  inline-component sentences. `keys.gen.ts`: the generated `MessageKey` union (never hand-edit). `messages/`: the
  catalogs (`messages/CLAUDE.md`).

## Must-knows

- **`t()`/`getMessage()` MUST read the locale-version rune FIRST, before any cache lookup**, or `{t('key')}` won't
  re-run on a language change. `state_referenced_locally` is suppressed: `messages.svelte.test.ts`'s reactivity test is
  the only guard.
- **A window holds `en` + its language's catalogs; the rest load on demand** (`scripts/vite-locale-catalogs.ts`).
  `setLocale()` loads, THEN switches in one step; latest call wins. `test-setup.ts` preloads all; a `vi.resetModules()`
  test must await. Text from EVERY language: the manifest (`manifestMessage`).
- **❌ The runtime never imports the logger or `#lib/settings`**: boot hands them in (`DETAILS.md` § Tests).
- **Locale dirs come from the plugin's `listLocales()` alone** (keeps `screenshots/` out).
- **❌ A fallback never crosses a SCRIPT boundary**, so `zh-Hant` skips Simplified `zh` and lands on English. Go through
  `inheritableAncestors()`, as the checks and Rust do. Regional fallback (`pt-PT` → `pt`) must keep working:
  `DETAILS.md`.
- **Picker labels take the WHOLE shipped list** (`zh` is `简体中文` only beside `zh-Hant`).
- **Both locale answers come from Rust, ❌ never the webview tag**, which drops the region override. Go through
  `pickUiLocale()`; secondary windows call `initWindowLanguageSync()`. `watchSystemLocales()` takes ONE subscriber per
  window: hang new reactions off it. Depth: `apps/desktop/src-tauri/src/intl/DETAILS.md`.
- **Error copy uses `getMessage()` (raw), NOT `t()`/ICU**: its `{system_settings}` tokens and `esc()` entities collide
  with ICU. Only plural/select sentences use `t()`.
- **Per-OS copy is one whole-value `{platform, select, …}` block**: `t()` injects `platform`, `getMessage()` splits
  (`platform-select.ts`). Vitest runs as Linux. `DETAILS.md` § Platform variants.
- **`<Trans>` renders tag content via a zero-arg `{#snippet content()}`** (snippets aren't callable:
  `invalid_snippet_arguments`). No `{@html}`, so XSS-safe. An unmatched tag renders NOTHING (`i18n-trans-snippets`).
- **Only the runtime writes `<html lang>`** (`setLocale()` and the startup catalog), with the RESOLVED catalog tag
  (`ja-JP` → `en`). ❌ No per-window write.
- **Copy reads `getUiLocale()`, numbers and joins `getCatalogLocale()`, dates `getFormatLocale()`; nothing else resolves
  a locale.** Numbers follow the language (English UI, Swedish Mac: `612.00 MB`); dates keep the region. Month NAMES are
  language, the week start beside them region.
- **Format ONLY through this layer + `#lib/settings/format-utils`** (`formatInteger`/`formatNumber`,
  `formatSizeForDisplay`, `formatDateForDisplay`). ❌ Never hardcode a locale or build an `Intl` formatter in feature
  code (`cmdr/no-raw-locale-format`).
- **Both readers stay SSR-safe and uncached** (a cache would hide a switch), but **memoize `Intl` formatters by (locale,
  options)**: they run per visible row.

Depth (runtime, catalog loading, ICU split, `'system'`, language vs region, grouping, titles): `DETAILS.md`.
