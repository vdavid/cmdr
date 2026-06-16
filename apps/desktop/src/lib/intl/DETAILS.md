# Locale-aware formatting + message runtime details

Depth behind `CLAUDE.md`. Two efforts live here: step 3 (the locale-aware FORMATTING layer, below) and step 2 (the
message RUNTIME, next). Together they make the app translation-ready: copy resolves from a JSON catalog through one
typed `t()` formatted with a real ICU engine, and numbers/sizes/dates format per the OS region the way a native macOS
app does. Step 1 (error-text-to-frontend) shipped earlier.

## The message runtime (step 2)

`messages.svelte.ts` is a thin (~180-line) runtime over `intl-messageformat` (ICU MessageFormat 1, BSD-3-Clause). It
resolves user-facing text from JSON catalogs under `messages/<locale>/`, reading the locale from `getLocale()`.

### Two accessors, two pipelines

- **`t(key, params?)`** is the path for ordinary copy: resolve the catalog string, compile it with `intl-messageformat`
  (memoized per `(locale, key)`), `.format(params)`. ONE code path for everything — plain `{name}` interpolation,
  `{count, plural, …}`, `{kind, select, …}`, and rich-text `<tag>` sentences (via `Trans.svelte`). Plurals/selects are
  resolved by the engine's `Intl.PluralRules`; we never hand-roll category selection.
- **`getMessage(key)`** returns the RAW catalog string with NO ICU parsing, for callers that do their own composition
  and must not hit ICU's brace/apostrophe grammar — specifically the error pipeline (`$lib/errors` `compose.ts` +
  `expandSystemStrings` + snarkdown). Its `{system_settings}` tokens and `esc()` HTML entities would collide with ICU
  placeholders. Same fallback chain as `t()`, just no `format()`.

This is the **error-pipeline boundary**: error literals migrate INTO the catalog as `errors.*` keys (plain strings,
possibly markdown), but they keep rendering through the existing snarkdown + `{@html}` + param-escaper pipeline via
`getMessage()`, NOT `t()`/`<Trans>`. `<Trans>` is only for the handful of UI sentences with inline INTERACTIVE
components (a `<LinkButton>` mid-sentence). Don't conflate the two.

### Fallback chain

`resolveRaw(locale, key)` tries `catalog[locale]` → `catalog[baseLanguage]` (`de-DE` → `de`) → `catalog.en` → the key
string itself. A missing key renders as its own key (visible, never a crash). Only `en` is populated today; the
base-language and exact-locale rungs exist for the future translation pipeline and are exercised by the
`_setCatalogForTests` seam (a synthetic test-only locale, never a shipped non-en catalog).

### Reactivity (load-bearing)

A module-level `localeVersion = $state(0)` rune (hence `.svelte.ts`) is a re-render SIGNAL, not a second locale source —
`getLocale()` stays the single source of truth for the VALUE. Every `t()`/`getMessage()` call reads the rune
UNCONDITIONALLY and FIRST, before any compiled-message cache lookup; otherwise Svelte doesn't track the dependency and a
markup `{t('key')}` won't re-run on a locale switch. `setLocale(locale)` writes the value into `locale.ts`'s override
(the same single source the formatters read) AND bumps the rune AND clears the compiled cache. `_setLocaleForTests`
writes the value only — use it for non-reactive snapshot tests; use `setLocale()` for reactivity tests. The pattern
mirrors `system-strings.svelte.ts`. Reactivity holds only inside a reactive context (markup / `$derived`); a `t()` in a
plain `.ts` computation is a snapshot, which is the right semantics for transient strings (toasts, error copy).

No SSR/prerender concern: the app is a pure SPA (`+layout.ts` has `ssr = false`), so route components are never
server/build-rendered; the catalog merge (a `import.meta.glob` over `messages/en/*.json`) and `getLocale()` touch no
`window`.

### The ICU-vs-`$lib/intl` formatting split

Numbers/sizes/dates format through `$lib/intl` + `format-utils` (step 3, below), NOT through ICU `number`/`date`
skeletons. `t()` embeds ALREADY-formatted count STRINGS as `*Text` params (e.g. `transfer.movedPhrase`'s `filesText`),
keeping formatting single-sourced. The raw integer is passed alongside ONLY to drive ICU `plural` selection (noun +
was/were agreement), never for display. Don't reformat inside messages with ICU `{n, number}`.

### Generated keys, codegen, checks

`scripts/gen-message-keys.js` (pure logic in `gen-message-keys-lib.js`, run via `pnpm intl:keys`) reads
`messages/en/*.json`, strips `@key` metadata, and emits the `keys.gen.ts` `MessageKey` union, so a wrong/missing key is
a typecheck error. It also reports keys used-in-code-but-missing (exit 1, a build failure) and catalog-keys-never-used
(a warning; the scan only sees STATIC keys, so a dynamically-built key reads as dead — verify before deleting). Two Go
checks guard the rest: `desktop-message-keys-fresh` (regenerate-and-diff `keys.gen.ts`, fail if stale) and
`desktop-message-key-naming` (the `area.feature.leaf` shape + a known first-segment area).
`cmdr/no-raw-user-facing-string` (ESLint) stops new hardcoded copy in migrated areas (a closed sink set: `addToast`
content, `title`/`label`/`placeholder`/ `aria-label` props, `.svelte` text nodes; an area allowlist widened per M2
tranche).

## The locale-aware formatting layer (step 3)

## What this layer owns vs. doesn't

Owns: the locale decision (`getLocale`), and number/size grouping + decimals (`number-format.ts`). The DATE formatter
lives in `$lib/settings/format-utils.ts` (`formatDateForDisplay` + the cached `getSystemLocaleFormatter`) because dates
carry per-component age-tier coloring that belongs with the date-color settings; it reads `getLocale()` from here, so
the locale source is still single.

Doesn't own (deliberately out of scope for step 3):

- Pluralization and sentence assembly (`pluralize.ts`, `${n} ${pluralize(...)}` sites, the fragment-concatenated
  transfer toasts). Locale-correct plurals (`Intl.PluralRules`, 6 categories) and whole-template messages belong to step
  2, where a catalog can hold the variants.
- Locale switching / live reactivity. The locale is read from the OS at runtime; there's no in-app picker and no
  requirement that a locale change re-renders open views without reload. Don't build a reactive locale store.
- The deliberately-fixed date formats: the `iso`/`short`/`custom` modes (`format-utils.ts::applyTokens`) and the ISO
  `formatDate` helper in `selection-info-utils.ts` (`YYYY-MM-DD hh:mm:ss`). These are user-chosen fixed formats,
  locale-independent by design. Only the `'system'` date mode is locale-driven.
- The backend. Rust emits raw numbers, byte counts, and Unix timestamps; formatting is and stays a frontend concern.

## Why `getLocale()` is uncached

A plain function call returning the live runtime default keeps the locale-switching seam (`setLocale()`) able to change
the answer observably. Caching the resolved locale here would freeze it for the page's life and make a switch invisible.
The cost is one cheap `Intl.NumberFormat().resolvedOptions().locale` resolve per formatter construction, and the
formatters themselves are memoized (keyed on the returned locale), so the hot paths don't pay it per format call.

## Memoization shape

`getNumberFormatter(options)` caches by `${locale} ${JSON.stringify(options)}` and rebuilds only when `getLocale()`
changes. `getGroupSeparator()` caches the group character per locale (derived from
`Intl.NumberFormat(locale).formatToParts(11111)`). Both mirror the lazy-singleton `getSystemLocaleFormatter()` in
`format-utils.ts`, which now also keys its single cached `Intl.DateTimeFormat` on the active locale.

## The en-US triad change (Decision 4, reviewable)

Raw-byte triads (`formatSizeTriads`) now group with the locale's separator instead of the hardcoded U+2009 thin space,
so byte sizes agree with the localized counts from `formatNumber`. en-US's `Intl` group separator is the comma, so for
an en-US user the byte readout changes from `1 234` (thin space) to `1,234`. This is the one place en-US output is NOT
byte-identical to the pre-change code. The alternative was to keep the thin space always (locale-independent); that
would have preserved the en-US look but left counts (comma) and byte sizes (thin space) incoherent within a locale, the
same incoherence German users would get. The commit is isolated and revertible on its own if the always-thin-space look
is preferred (in which case `formatNumber` arguably should match it).

Human-friendly sizes (`formatFileSizeWithFormat`) use `useGrouping: false`, so en-US stays byte-identical there: the old
`toFixed(2)`/`String(value)` never grouped, and a forced-unit `10000.00 MB` must not become `10,000.00 MB`. Only the
decimal separator localizes (`1.02 MB` → `1,02 MB`).

## Value↔unit spacing invariant

Human-friendly sizes compose as `` `${value} ${unitLabel}` `` with an explicit ASCII space; we never adopt `Intl`'s
`style: 'unit'`, which injects a narrow no-break space. `colorizeSizeString`/`tierClassForUnit` recover the unit via
`lastIndexOf(' ')`, so a non-ASCII space there would break tier coloring.

## Column measurement

`views/measure-column-widths.ts` shrink-wraps the Size/Modified columns and calls `formatSizeForDisplay` per visible
entry (render path) AND in `foldEntries` over the prefetch buffer. Because render and measure share that one function,
they read the same locale, so a localized separator is produced identically in both. `tabularize` substitutes only
digits (modeling `font-variant-numeric: tabular-nums`), so a localized separator is measured at its real width, which is
correct. Never add a second formatting path for measurement.

## The locale source seam

`getLocale()` is intentionally a single function, not a locale-management system: `setLocale()` (in
`messages.svelte.ts`) writes its override and bumps the message rune, so a locale switch is observable everywhere
without a store. Keep the seam minimal. No in-app locale picker ships today; the seam supports one later.
