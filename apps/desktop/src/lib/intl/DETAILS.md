# Locale-aware formatting layer details

Depth behind `CLAUDE.md`. This is step 3 of the "i18n-ready" effort: route every user-facing number, file size, and date
through one locale-aware layer with one locale source, so the app formats per the user's OS region the way a native
macOS app does. Step 1 (error-text-to-frontend) shipped; step 2 (an i18n catalog library) is separate and will own
locale switching.

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

A plain function call returning the live runtime default keeps a future locale-switching layer (step 2) able to change
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

## Step-2 hand-off

`getLocale()` is intentionally a single function, not a locale-management system. Step 2 (catalog tool) replaces its
internals to make the locale catalog-driven; callers won't change. Keep the seam minimal so nothing here gets
reimplemented later.
