# Locale-aware formatting and message runtime

Two locale sources, each read from one place: the UI LANGUAGE for catalog text (via ICU), and the OS's FORMATTING locale
for numbers, sizes, dates.

## Module map

- `locale.ts`: `getUiLocale()` (what we speak), `getFormatLocale()` (always the OS), `setOsFormatLocale()` (the OS's tag
  arriving). `_setLocaleForTests` pins both, `_setFormatLocaleForTests` splits them; `setLocale()` is the switch.
- `os-locales.ts`: the OS's two answers. `loadSystemLocales()` fetches the Rust pair per window, `pickUiLocale(setting)`
  maps the language half (`null` = no override), `watchSystemLocales()` follows a live change.
- `language-analytics.ts`: the language events, shipped catalog tags only; they hang off the PICK, never a subscription
  (`src-tauri/src/analytics/DETAILS.md`).
- `number-format.ts`: memoized `Intl.NumberFormat` factory (`getNumberFormatter`), `formatInteger`, and
  `getGroupSeparator` (the byte-triad separator).
- `locale-inheritance.ts`: which catalog a locale may inherit from (same language AND same script), shared with the i18n
  checks and Rust.
- `messages.svelte.ts`: the runtime: `t()` (catalog + ICU), `getMessage()` (raw), `setLocale()`, `availableLocales()`,
  `resolvedCatalogLocale()`, the fallback chain, the locale-version rune. `document-language.ts`: the `<html lang>`
  write. `locale-display-names.ts`: the picker's row labels. `Trans.svelte`: inline-component sentences. `keys.gen.ts`:
  the generated `MessageKey` union (never hand-edit). `messages/`: the catalogs (`messages/CLAUDE.md`).

## Must-knows

- **`t()`/`getMessage()` MUST read the locale-version rune FIRST, before any cache lookup**, or `{t('key')}` won't
  re-run on a language change. `setLocale()` bumps it, `_setLocaleForTests` doesn't, and `state_referenced_locally` is
  suppressed: `messages.svelte.test.ts`'s reactivity test is the only guard.

- **The resolver loads ALL locale dirs (`messages/*/*.json`) by dir tag, then falls back per key** to `en`.
  `screenshots/` isn't a locale: exclude it IN THE GLOB, never only at the runtime gate, which ships and parses 280 kB
  before rejecting it. A misclassified dir also becomes a fake language.
- **❌ A fallback never crosses a SCRIPT boundary**, so `zh-Hant` skips Simplified `zh` and lands on English. Go through
  `inheritableAncestors()`; the checks and Rust obey the same rule. Regional fallback (`pt-PT` → `pt`) must keep
  working: `DETAILS.md`.
- **Picker labels take the WHOLE shipped list**: `zh` reads `简体中文` only because `zh-Hant` ships. ❌ Never decorate
  unconditionally ("Deutsch (Lateinisch)"); keep rows distinct.
- **Both locale answers come from Rust, ❌ never the webview tag**, which drops the region override. Go through
  `pickUiLocale()`; secondary windows call `initWindowLanguageSync()` (`routes/window-route-coverage.test.ts`).
  `watchSystemLocales()` takes ONE subscriber per window (the first adopts): hang new reactions off it. Depth:
  `DETAILS.md`, `apps/desktop/src-tauri/src/intl/DETAILS.md`.
- **Error copy uses `getMessage()` (raw), NOT `t()`/ICU**: the pipeline's `{system_settings}` tokens and `esc()` HTML
  entities collide with ICU grammar. Only plural/select sentences go through `t()`.
- **Catalog values double every apostrophe (`''`)** (ICU escaping): `messages/CLAUDE.md`.
- **`<Trans>` renders tag content via a zero-arg `{#snippet content()}`** (snippets aren't callable:
  `invalid_snippet_arguments`). No `{@html}`, so XSS-safe. An unmatched tag renders NOTHING (`i18n-trans-snippets`
  enforces pairing).
- **`setLocale()` alone writes `<html lang>`** (`document-language.ts`), with the RESOLVED catalog tag (`ja-JP` → `en`).
  Every window and live change funnels through it; ❌ no per-window write.
- **UI copy reads `getUiLocale()`, formatters read `getFormatLocale()`, nothing else resolves a locale.** `setLocale()`
  writes the UI half only: a `hu` pick must NOT move dates or grouping off the OS. Month NAMES are UI language, the
  first-day-of-week beside them formatting.
- **Format ONLY through this layer + `$lib/settings/format-utils`** (`formatInteger`/`formatNumber`,
  `formatSizeForDisplay`, `formatDateForDisplay`). ❌ Never hardcode a locale or build an `Intl` formatter in feature
  code (`cmdr/no-raw-locale-format`).
- **Both readers stay SSR-safe and uncached** (a cache here would hide a switch), but **keep `Intl` formatters memoized
  by (locale, options)**: they run per visible entry in render AND the column fold, so per-call construction regresses
  scroll.

Depth (runtime design, the error-pipeline boundary, the ICU split, `'system'`, language vs region, the composed
formatting tag, the grouping split between byte triads and human sizes): `DETAILS.md`.
