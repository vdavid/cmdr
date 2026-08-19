# Locale-aware formatting and message runtime

Two locale sources, each read from one place: the UI LANGUAGE for catalog text (via ICU), and the OS's FORMATTING locale
for numbers, sizes, and dates.

## Module map

- `locale.ts`: `getUiLocale()` (what we speak), `getFormatLocale()` (always the OS), `setOsFormatLocale()` (the OS's tag
  arriving). `_setLocaleForTests` pins both, `_setFormatLocaleForTests` splits them; `setLocale()`
  (`messages.svelte.ts`) is the reactive switch.
- `os-locales.ts`: the OS's two answers. `loadSystemLocales()` fetches the Rust pair per window, `pickUiLocale(setting)`
  maps the language half (`null` = no override), `watchSystemLocales()` follows a live change. The formatting half goes
  straight to `locale.ts`.
- `language-analytics.ts`: the language events, base subtags only; they hang off the PICK, never a settings subscription
  (`src-tauri/src/analytics/DETAILS.md`).
- `number-format.ts`: memoized `Intl.NumberFormat` factory (`getNumberFormatter`), plus `formatInteger` (counts) and
  `getGroupSeparator` (byte-triad separator).
- `messages.svelte.ts`: the runtime: `t()` (catalog + ICU), `getMessage()` (raw), `setLocale()`, `availableLocales()`
  (drives the Language picker), the catalog map, BCP-47 resolver, locale-version rune, and compiled-message cache.
  `Trans.svelte`: inline-component sentences. `keys.gen.ts`: the generated `MessageKey` union (never hand-edit).
  `messages/`: the catalogs (`messages/CLAUDE.md`).

## Must-knows

- **`t()`/`getMessage()` MUST read the locale-version rune FIRST, before any cache lookup**, or `{t('key')}` won't
  re-run on a language change. `setLocale()` bumps the rune; `_setLocaleForTests` doesn't. `state_referenced_locally` is
  suppressed, so the compiler won't warn; `messages.svelte.test.ts`'s reactivity test is the only guard.
- **The resolver loads ALL locale dirs (`messages/*/*.json`) by dir tag, BCP-47 fallback** (locale → base → `en` → key).
  `screenshots/` sits among them and is NOT a locale: a glob/gate change must keep excluding it or it shows up as a fake
  language in the Language picker.
- **Both locale answers come from Rust, ❌ never from the webview tag**, which exposes ONE language tag and drops the
  region override. Go through `pickUiLocale()`; secondary windows call `initWindowLanguageSync()`. Both track the OS
  live through `watchSystemLocales()`. What each mistake costs, the walk, the guard, and the composition: `DETAILS.md`
  and `apps/desktop/src-tauri/src/intl/DETAILS.md`.
- **Error copy uses `getMessage()` (raw), NOT `t()`/ICU.** The pipeline's `{system_settings}` tokens and `esc()` HTML
  entities collide with ICU's brace/apostrophe grammar. Only real plural/select sentences go through `t()`.
- **Catalog messages double apostrophes (`''`).** ICU reads a lone `'` before `{`/`<`/`#` as an escape and swallows the
  text after it; `''` always collapses to `'`, so double it everywhere.
- **`<Trans>` renders a tag's inner content via a zero-arg `{#snippet content()}`** (a snippet isn't callable,
  `invalid_snippet_arguments`). No `{@html}` → XSS-safe by construction. An unmatched tag renders NOTHING;
  `i18n-trans-snippets` enforces the pairing.
- **UI copy reads `getUiLocale()`, formatters read `getFormatLocale()`, nothing else resolves a locale.** `setLocale()`
  writes the UI half only: a `hu` pick must NOT move dates or grouping off the OS. Classify per call site: month NAMES
  are UI language, the first-day-of-week beside them is formatting (`query-ui/filter-chips/filter-popover-helpers.ts`
  does both).
- **Format ONLY through this layer + `$lib/settings/format-utils`**: `formatInteger`/`formatNumber` for counts,
  `formatSizeForDisplay` for sizes, `formatDateForDisplay` for dates. Don't hardcode a locale or build an `Intl`
  formatter in feature code (`cmdr/no-raw-locale-format`, off for tests); `Intl.Segmenter`/`Intl.Locale` aren't
  formatters.
- **Both readers stay SSR-safe** (no `window`/DOM, never throw: the Node pass and viewer window call them) **and
  uncached** (the formatters they feed are cached, so a locale cache here would hide a switch).
- **Keep `Intl` formatters memoized by (locale, options).** They run per-visible-entry in render AND in the
  column-measurement fold; per-call construction (~10× a format call) regresses scroll/measure on big directories.
- **Raw-byte triads are comma-grouped; human-friendly sizes are NOT** (`useGrouping: false`), so a forced `10000.00 MB`
  must not become `10,000.00`. Net: `en-us-parity.test.ts`.

Depth (runtime design, the error-pipeline boundary, the ICU split, what `'system'` resolves to, language vs region, the
composed formatting tag, the en-US triad change): `DETAILS.md`.
