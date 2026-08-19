# Locale-aware formatting and message runtime

One locale source feeding two consumers, both reading it from exactly one place here: the formatting layer (numbers,
sizes, dates) and the message runtime (catalog text via ICU).

## Module map

- `locale.ts`: `getLocale()`, the single locale source (OS runtime default today). `_setLocaleForTests` injects a value
  only (no rune bump); `setLocale()` in `messages.svelte.ts` is the reactive switch.
- `ui-locale.ts`: what `'system'` resolves to. `loadSystemUiLocale()` fetches the Rust answer once per window;
  `pickUiLocale(setting)` maps it (`null` = no override).
- `number-format.ts`: memoized `Intl.NumberFormat` factory (`getNumberFormatter`), plus `formatInteger` (counts) and
  `getGroupSeparator` (byte-triad separator).
- `messages.svelte.ts`: the runtime: `t(key, params?)` (catalog + ICU), `getMessage(key)` (raw), `setLocale()`,
  `availableLocales()` (drives the Language picker), the catalog map + BCP-47 resolver, the locale-version rune, the
  compiled-message cache. `Trans.svelte`: inline-component sentences. `keys.gen.ts`: the generated `MessageKey` union
  (never hand-edit). `messages/`: the JSON catalogs (`messages/CLAUDE.md`).

## Must-knows

- **`t()`/`getMessage()` MUST read the locale-version rune FIRST, before any cache lookup.** Read it after the
  compiled-message cache and `{t('key')}` won't re-run on a locale change. `setLocale()` bumps the rune;
  `_setLocaleForTests` doesn't. `state_referenced_locally` is suppressed, so the compiler won't warn on a wrong read;
  the `messages.svelte.test.ts` reactivity test is the only guard.
- **The resolver loads ALL locale dirs (`messages/*/*.json`), keyed by the dir tag, with BCP-47 fallback** (locale →
  base → `en` → key). A non-BCP-47 dir is NOT a locale: `screenshots/` sits alongside the locale dirs, so a glob/gate
  change must keep excluding it or it surfaces as a fake locale in the picker. The live picker is Settings > Appearance
  > Language, applied by `settings-applier.ts`: immediate, no restart, frontend-only.
- **`'system'` resolves from the OS preference LIST in Rust, ❌ never from the webview tag.** `getLocale()` exposes ONE
  tag, so `[hu-HU, sv-SE]` never reaches Swedish and `zh-Hant-TW` slides into Simplified `zh`. Go through
  `pickUiLocale()`; secondary windows call `initWindowLanguageSync()`. The walk and the script guard:
  `apps/desktop/src-tauri/src/intl/DETAILS.md`.
- **Error copy uses `getMessage()` (raw lookup), NOT `t()`/ICU.** The error pipeline's `{system_settings}` tokens and
  `esc()` HTML entities collide with ICU's brace/apostrophe grammar. Only genuine multi-variable plural/select sentences
  go through `t()`. Don't route error strings through `format()`.
- **Catalog messages double apostrophes (`''`).** ICU treats `'` as an escape char: a lone `'` before `{`/`<`/`#` opens
  a quoted section and swallows text. `''` always collapses to `'`, so it's the rule even where a lone one would render
  fine.
- **`<Trans>` renders a tag's inner content via a zero-arg `{#snippet content()}`**, since you can't call a snippet as a
  function to produce a value (`invalid_snippet_arguments`). No `{@html}` → XSS-safe by construction. An unmatched tag
  renders NOTHING; `i18n-trans-snippets` enforces the pairing.
- **Read the locale ONLY via `getLocale()`; format ONLY through this layer + `$lib/settings/format-utils`.** Don't
  hardcode a locale, call `toLocaleString`, or build an `Intl.NumberFormat`/`DateTimeFormat` in feature code
  (`cmdr/no-raw-locale-format`, off for `*.test.ts`). Counts go through `formatInteger`/`formatNumber`, sizes through
  `formatSizeForDisplay`, dates through `formatDateForDisplay`. `Intl.Segmenter`/`Intl.Locale` aren't formatters.
- **`getLocale()` must stay SSR-safe**: no `window`/DOM, never throws (the SvelteKit Node pass and the
  capability-restricted viewer both call it). It returns the live default per call, uncached by design; the formatters
  it feeds are cached, so don't add a locale cache that would hide a locale change.
- **Keep `Intl` formatters memoized by (locale, options).** They run per-visible-entry in render AND in the
  column-measurement fold; per-call construction (~10× a format call) regresses scroll/measure on large directories.
- **en-US output is byte-identical to the pre-locale code EXCEPT raw-byte triads** (comma-grouped now). Human-friendly
  sizes use `useGrouping: false`, so a forced `10000.00 MB` must not become `10,000.00`. Net: `en-us-parity.test.ts`.

Depth (runtime design, the error-pipeline boundary, the ICU split, what `'system'` resolves to, the en-US triad change):
`DETAILS.md`.
