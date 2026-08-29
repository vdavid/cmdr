# Locale-aware formatting and message runtime

Two locale sources, each read from one place: the UI LANGUAGE for catalog text (via ICU), and the OS's FORMATTING locale
for numbers, sizes, dates.

## Module map

- `locale.ts`: `getUiLocale()` (what we speak), `getFormatLocale()` (always the OS), `setOsFormatLocale()` (the OS's tag
  arriving). `_setLocaleForTests` pins both, `_setFormatLocaleForTests` splits them; `setLocale()`
  (`messages.svelte.ts`) is the reactive switch.
- `os-locales.ts`: the OS's two answers. `loadSystemLocales()` fetches the Rust pair per window, `pickUiLocale(setting)`
  maps the language half (`null` = no override), `watchSystemLocales()` follows a live change.
- `language-analytics.ts`: the language events, shipped catalog tags only; they hang off the PICK, never a subscription
  (`src-tauri/src/analytics/DETAILS.md`).
- `number-format.ts`: memoized `Intl.NumberFormat` factory (`getNumberFormatter`), plus `formatInteger` and
  `getGroupSeparator` (the byte-triad separator).
- `locale-inheritance.ts`: which catalog a locale may inherit from (same language AND same script), shared with the i18n
  checks and Rust.
- `messages.svelte.ts`: the runtime: `t()` (catalog + ICU), `getMessage()` (raw), `setLocale()`, `availableLocales()`
  the catalog map, the fallback chain, `resolvedCatalogLocale()`, the locale-version rune, the compiled-message cache.
  `document-language.ts`: the `<html lang>` write. `Trans.svelte`: inline-component sentences. `keys.gen.ts`: the
  generated `MessageKey` union (never hand-edit). `messages/`: the catalogs (`messages/CLAUDE.md`).

## Must-knows

- **`t()`/`getMessage()` MUST read the locale-version rune FIRST, before any cache lookup**, or `{t('key')}` won't
  re-run on a language change. `setLocale()` bumps it; `_setLocaleForTests` doesn't. `state_referenced_locally` is
  suppressed, so the compiler won't warn: `messages.svelte.test.ts`'s reactivity test is the guard.
- **The resolver loads ALL locale dirs (`messages/*/*.json`) by dir tag, then falls back per key** to `en`.
  `screenshots/` sits among them and is NOT a locale: keep every glob/gate excluding it, or it becomes a fake language.
- **❌ A fallback never crosses a SCRIPT boundary**, so `zh-Hant` skips Simplified `zh` and lands on English. Go through
  `inheritableAncestors()`; the checks and Rust obey the same rule. Regional fallback (`pt-PT` → `pt`) must keep
  working: `DETAILS.md`.
- **Both locale answers come from Rust, ❌ never from the webview tag**, which drops the region override. Go through
  `pickUiLocale()`; secondary windows call `initWindowLanguageSync()` (guarded by
  `routes/window-route-coverage.test.ts`). `watchSystemLocales()` takes ONE subscriber per window (the first adopts, so
  a second never fires): hang new reactions off it. Depth: `DETAILS.md`, `apps/desktop/src-tauri/src/intl/DETAILS.md`.
- **Error copy uses `getMessage()` (raw), NOT `t()`/ICU**: the pipeline's `{system_settings}` tokens and `esc()` HTML
  entities collide with ICU's grammar. Only plural/select sentences go through `t()`.
- **Catalog values double every apostrophe (`''`)**, an ICU escaping rule: `messages/CLAUDE.md`.
- **`<Trans>` renders tag content via a zero-arg `{#snippet content()}`** (snippets aren't callable:
  `invalid_snippet_arguments`). No `{@html}`, so XSS-safe by construction. An unmatched tag renders NOTHING
  (`i18n-trans-snippets` enforces pairing).
- **`setLocale()` is the ONLY writer of `<html lang>`** (via `document-language.ts`), and it writes the RESOLVED catalog
  tag, so `ja-JP` with no Japanese catalog announces `en`. Every window and every live change funnels through it; ❌
  don't add a per-window write.
- **UI copy reads `getUiLocale()`, formatters read `getFormatLocale()`, nothing else resolves a locale.** `setLocale()`
  writes the UI half only: a `hu` pick must NOT move dates or grouping off the OS. Month NAMES are UI language, the
  first-day-of-week beside them is formatting.
- **Format ONLY through this layer + `$lib/settings/format-utils`** (`formatInteger`/`formatNumber`,
  `formatSizeForDisplay`, `formatDateForDisplay`). ❌ Never hardcode a locale or build an `Intl` formatter in feature
  code (`cmdr/no-raw-locale-format`, off for tests).
- **Both readers stay SSR-safe and uncached**: their formatters are cached, but a cache here would hide a switch.
- **Keep `Intl` formatters memoized by (locale, options)**: they run per visible entry in render AND in the column fold,
  so per-call construction regresses scroll.

Depth (runtime design, the error-pipeline boundary, the ICU split, `'system'`, language vs region, the composed
formatting tag, the grouping split between raw-byte triads and human-friendly sizes): `DETAILS.md`.
