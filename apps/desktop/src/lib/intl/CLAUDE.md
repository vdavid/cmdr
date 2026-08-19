# Locale-aware formatting and message runtime

Two locale sources, each read from one place here: the UI LANGUAGE for the message runtime (catalog text via ICU), and
the OS's FORMATTING locale for numbers, sizes, and dates.

## Module map

- `locale.ts`: `getUiLocale()` (what we speak) + `getFormatLocale()` (what we format in, always the OS).
  `_setLocaleForTests` pins both, `_setFormatLocaleForTests` splits them; `setLocale()` in `messages.svelte.ts` is the
  reactive switch.
- `ui-locale.ts`: what `'system'` resolves to. `loadSystemUiLocale()` fetches the Rust answer once per window;
  `pickUiLocale(setting)` maps it (`null` = no override).
- `number-format.ts`: memoized `Intl.NumberFormat` factory (`getNumberFormatter`), plus `formatInteger` (counts) and
  `getGroupSeparator` (byte-triad separator).
- `messages.svelte.ts`: the runtime: `t(key, params?)` (catalog + ICU), `getMessage(key)` (raw), `setLocale()`,
  `availableLocales()` (drives the Language picker), the catalog map + BCP-47 resolver, the locale-version rune, the
  compiled-message cache. `Trans.svelte`: inline-component sentences. `keys.gen.ts`: the generated `MessageKey` union
  (never hand-edit). `messages/`: the JSON catalogs (`messages/CLAUDE.md`).

## Must-knows

- **`t()`/`getMessage()` MUST read the locale-version rune FIRST, before any cache lookup**, or `{t('key')}` won't
  re-run on a language change. `setLocale()` bumps the rune; `_setLocaleForTests` doesn't. `state_referenced_locally` is
  suppressed, so the compiler won't warn; `messages.svelte.test.ts`'s reactivity test is the only guard.
- **The resolver loads ALL locale dirs (`messages/*/*.json`) by dir tag, BCP-47 fallback** (locale → base → `en` → key).
  `screenshots/` sits among them and is NOT a locale: a glob/gate change must keep excluding it or it surfaces as a fake
  locale in the picker (Settings > Appearance > Language, via `settings-applier.ts`: live, no restart, frontend-only).
- **`'system'` resolves from the OS preference LIST in Rust, ❌ never from the webview tag**, which exposes ONE tag: so
  `[hu-HU, sv-SE]` never reaches Swedish and `zh-Hant-TW` slides into Simplified `zh`. Go through `pickUiLocale()`;
  secondary windows call `initWindowLanguageSync()`. The walk and the script guard:
  `apps/desktop/src-tauri/src/intl/DETAILS.md`.
- **Error copy uses `getMessage()` (raw), NOT `t()`/ICU.** The pipeline's `{system_settings}` tokens and `esc()` HTML
  entities collide with ICU's brace/apostrophe grammar. Only real plural/select sentences go through `t()`.
- **Catalog messages double apostrophes (`''`).** ICU reads a lone `'` before `{`/`<`/`#` as an escape and swallows the
  text after it; `''` always collapses to `'`, so double it everywhere.
- **`<Trans>` renders a tag's inner content via a zero-arg `{#snippet content()}`** (a snippet isn't callable,
  `invalid_snippet_arguments`). No `{@html}` → XSS-safe by construction. An unmatched tag renders NOTHING;
  `i18n-trans-snippets` enforces the pairing.
- **UI copy reads `getUiLocale()`, formatters read `getFormatLocale()`, nothing else resolves a locale.** Language and
  region are two macOS settings, so `setLocale()` writes the UI half only: a `hu` pick must NOT move dates or number
  grouping off the OS. Classify per call site: month NAMES are UI language, the first-day-of-week beside them is
  formatting (`query-ui/filter-chips/filter-popover-helpers.ts` does both).
- **Format ONLY through this layer + `$lib/settings/format-utils`.** Don't hardcode a locale, call `toLocaleString`, or
  build an `Intl.NumberFormat`/`DateTimeFormat` in feature code (`cmdr/no-raw-locale-format`, off for `*.test.ts`).
  Counts go through `formatInteger`/`formatNumber`, sizes through `formatSizeForDisplay`, dates through
  `formatDateForDisplay`. `Intl.Segmenter`/`Intl.Locale` aren't formatters.
- **Both readers stay SSR-safe** (no `window`/DOM, never throw: the SvelteKit Node pass and the viewer window call them)
  **and uncached** (the formatters they feed are cached, so a locale cache here would hide a switch).
- **Keep `Intl` formatters memoized by (locale, options).** They run per-visible-entry in render AND in the
  column-measurement fold; per-call construction (~10× a format call) regresses scroll/measure on big directories.
- **en-US output matches the pre-locale code EXCEPT raw-byte triads** (comma-grouped now). Human-friendly sizes use
  `useGrouping: false`, so a forced `10000.00 MB` must not become `10,000.00`. Net: `en-us-parity.test.ts`.

Depth (runtime design, the error-pipeline boundary, the ICU split, what `'system'` resolves to, language vs region, the
region override WebKit drops, the en-US triad change): `DETAILS.md`.
