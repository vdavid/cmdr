# Locale-aware formatter layer (i18n-readiness, step 3)

Route every user-facing NUMBER, FILE SIZE, and DATE through one locale-aware formatting layer with a single locale
source, so the app formats per the user's OS region the way a native macOS app does. This is step 3 of the "i18n-ready"
effort (step 1, error-text-to-frontend, shipped; step 2 is adopting an i18n catalog library, NOT this).

This stands on its own as correctness: macOS apps format numbers and dates by the user's Region independent of UI
language, and Cmdr's dates already do (`'system'` format uses `Intl.DateTimeFormat(undefined, …)`), but counts and file
sizes are hardcoded to `en-US` / `.`-decimal / thin-space grouping. This change makes numbers and sizes consistent with
dates and with the OS, behind one seam a catalog tool can later own.

## Goal

After this change:

- There is ONE locale source (a tiny `getLocale()` chokepoint). Every numeric/size/date formatter reads it. No formatter
  hardcodes a locale (`'en-US'` disappears) and no user-facing formatting calls `toLocaleString` / constructs an
  `Intl.*` formatter outside the central utils.
- Counts, file sizes (the decimal in `"1.02 MB"`, the grouping in raw-byte triads), and the `'system'` date format all
  follow the active locale. An `en-US`-region runtime renders byte-identical to today; other regions now get their
  native separators (the intended improvement).
- `Intl.*` formatter instances are memoized (locale + options keyed), so the per-entry render and the column-measurement
  fold don't pay construction cost per call.

## Non-goals (hold the line on scope)

- **No pluralization or sentence-assembly changes.** `pluralize.ts`, the `${n} ${pluralize(n,'file')}` sites, and the
  fragment-concatenated toast strings (`transfer-complete-toast.ts`) are NOT touched. Locale-correct plurals
  (`Intl.PluralRules`, 6 categories) and whole-template messages belong to step 2, where a catalog can hold the
  variants. Building plural infra now means redoing it. Leave it.
- **No i18n library, no catalog, no string extraction.** `getLocale()` is a single function returning a locale string,
  not a locale-management system. Step 2 (catalog tool, undecided) will own locale switching and replace the
  chokepoint's internals. Keep the seam minimal so nothing here gets reimplemented later.
- **No locale-switching UI and no live-locale reactivity.** The locale is read from the OS at runtime. There is no
  in-app locale picker and no requirement that a locale change re-renders open views without reload. Don't build a
  reactive locale store.
- **No backend changes.** Rust already emits raw numbers, byte counts, and Unix timestamps; formatting is and stays a
  frontend concern. Nothing in `src-tauri/` is in scope.
- **Don't localize the deliberately-fixed formats.** The `iso` / `short` / `custom` date modes
  (`format-utils.ts::applyTokens`) and the ISO `formatDate` helper (`selection-info-utils.ts`, `YYYY-MM-DD hh:mm:ss`)
  are user-chosen fixed formats, locale-independent BY DESIGN. Do NOT route them through the locale. Only the `'system'`
  date mode is locale-driven.

## The one principle this encodes

One locale source feeds one formatting layer. Today the layer is partly there (dates) and partly bypassed (numbers,
sizes hardcode `en-US`/`.`/thin-space). This unifies the bypassed paths onto the same seam, so "what locale is active"
has exactly one answer and one place to change.

## Background: current state (verified inventory)

- **Counts.** `selection-info-utils.ts::formatNumber` is `n.toLocaleString('en-US')` (hardcoded). ~48 call sites, all
  through this one function. `LoadingIcon.svelte:15` calls `n.toLocaleString()` directly (runtime default, inconsistent
  with `formatNumber`).
- **File sizes.** Two display paths, both in `format-utils.ts` / `selection-info-utils.ts`:
  - Human-friendly (`"1.02 MB"`): `formatFileSizeWithFormat` uses `value.toFixed(2)` and `String(value)`, always a `.`
    decimal. Unit selection (binary 1024 vs SI 1000), unit-label casing (`KB` vs `kB` via `unitLabel`), and tier
    coloring (`dynamicTierIndex`, `tierClassForUnit`) are separate and correct.
  - Raw bytes (`unit: 'bytes'`): `formatSizeTriads` splits the integer into 3-digit groups, colors each group by tier,
    and joins with a hardcoded thin space (U+2009). The per-triad coloring is why this is bespoke, not `Intl`.
  - `colorizeSizeString` / `tierClassForUnit` recover the unit by `text.lastIndexOf(' ')` (split on the last space).
    This is fragile to value↔unit spacing changes.
- **Dates.** `format-utils.ts` is already the single source of truth and already locale-aware for the `'system'` mode
  (`getSystemLocaleFormatter()` → `Intl.DateTimeFormat(undefined, { year, month, day, hour, minute })`, structural part
  typing, one cached instance). The iso/short/custom modes are fixed-token. `selection-info-utils.ts::formatDate` is a
  separate fixed ISO formatter (used by `getDateDisplay`).
- **Calendar bits.** `query-ui/filter-chips/filter-popover-helpers.ts` already uses `Intl.Locale`/`Intl.DateTimeFormat`
  for weekday/month names and first-day-of-week (legitimately locale-aware). `viewer-word.ts` uses `Intl.Segmenter` (not
  formatting; out of scope).
- **The hot path that constrains us.** `views/measure-column-widths.ts` shrink-wraps the Size/Modified columns. It calls
  `formatSizeForDisplay` (render path) and `formattedDate` per visible entry in `foldEntries`, and models
  `font-variant-numeric: tabular-nums` via `tabularize()` (substitutes every `[0-9]` with the font's widest digit before
  measuring). Because `tabularize` touches only digits, a localized decimal/group SEPARATOR is measured literally, which
  is correct ONLY IF the render path and the measure path produce the same separator, i.e. read the same locale. They
  already share `formatSizeForDisplay`, so consistency holds for free as long as the locale source is single.

## Design decisions

### Decision 1: the locale chokepoint

Add a tiny module (working name `src/lib/intl/locale.ts`) exporting `getLocale(): string`. It returns the runtime
default locale (the same source `Intl.DateTimeFormat(undefined, …)` already trusts today, e.g. via
`new Intl.NumberFormat().resolvedOptions().locale`), SSR-safe (no `window`; never throws under the SvelteKit static
adapter's prerender/Node pass; fall back to `'en-US'` if resolution is unavailable). This is the ONLY place that decides
the locale. Every formatter calls it. Step 2 will make its internals catalog-driven; callers won't change.

Do NOT cache the resolved locale in a way that makes a future locale change impossible to observe, but also do NOT build
reactivity now: a plain function call per formatter-construction is fine (the formatters themselves are cached, see
Decision 4).

### Decision 2: numbers follow the OS region (the behavior change to bless)

`formatNumber` drops `'en-US'` and formats with `getLocale()`. Consequence: a non-en-US-region user (whose Mac Region is
German, say) now sees `1.234` where they saw `1,234`. This is the intended, native-correct behavior and it aligns counts
with the dates that already localize. `LoadingIcon.svelte` routes through `formatNumber` instead of a bare
`toLocaleString()`. An en-US-region runtime is unchanged.

### Decision 3: file sizes (localize the numeric portion, keep units and tiers intact)

Surgical: change ONLY the number formatting inside the size path; leave unit selection, unit-label casing, and all tier
coloring exactly as-is.

- Human-friendly: replace `value.toFixed(2)` and the integer `String(value)` with a memoized locale `Intl.NumberFormat`
  (2 fraction digits for the forced/dynamic decimal case; 0 for the bytes-as-integer case). Keep composing the result as
  `` `${localizedValue} ${unitLabel}` `` with an explicit ASCII space between value and unit. Do NOT adopt `Intl`'s
  `style: 'unit'` (it changes the value↔unit spacing to NNBSP and would break `colorizeSizeString`'s last-space parse).
- Keep `colorizeSizeString` / `tierClassForUnit` working: because value and unit stay separated by a plain space, the
  `lastIndexOf(' ')` parse still finds the unit. Add a regression test that a localized value (German `"1,02 MB"`) still
  colorizes to the right tier. (If you find a cleaner way to pass the unit explicitly instead of re-parsing, that's a
  welcome small improvement, but it's optional and must not expand scope.)

### Decision 4: raw-byte triads, deriving the group separator from the locale (KEY REVIEWABLE DECISION)

`formatSizeTriads` keeps its per-triad tier coloring (split into 3-digit groups, color each), but sources the group
separator from the active locale instead of the hardcoded U+2009, so counts and byte grouping agree. Get the separator
via `Intl.NumberFormat(getLocale()).formatToParts(11111).find(p => p.type === 'group')?.value`, memoized per locale.

Rationale and the alternative: the current hardcoded thin space is a deliberate typographic choice
(`selection/CLAUDE.md`). The alternative is to KEEP the thin space always, locale-independent. Recommendation: derive
from locale, for consistency with `formatNumber` (otherwise a German user sees `1.234` counts but `1<thin-space>234`
byte sizes, which is incoherent). David should confirm this is the desired call before it ships; if he prefers the
always-thin-space look, this decision flips to "keep U+2009" and `formatNumber` should arguably match. Flag it in the
hand-off, don't silently pick.

### Decision 5: dates (route the existing `'system'` formatter through the chokepoint)

`getSystemLocaleFormatter()` currently passes `undefined` as the locale (runtime default). Change it to `getLocale()` so
there is one locale source. Behavior is identical today (the chokepoint returns the same runtime default), but the seam
is now single. Leave the cached-formatter pattern (one instance, rebuilt only on change) exactly as it is, since hot
virtualized re-renders depend on it. Do NOT touch iso/short/custom or the ISO `formatDate` helper.

### Decision 6: formatter caching (perf must, not optional)

`Intl.NumberFormat` construction is ~10× a format call (the date code already documents this for `DateTimeFormat`). The
size and count formatters run per-visible-entry in render AND again in the measurement fold over the prefetch buffer, so
constructing per call would regress scroll/measure performance on large directories. Memoize each `Intl.NumberFormat`
instance by (locale, options); rebuild only when `getLocale()` changes. Mirror the existing `getSystemLocaleFormatter()`
lazy-singleton shape.

## Things to watch (David's explicit call-outs)

- **Column width / tabular-nums interaction.** Render and measure MUST read the same locale (they share
  `formatSizeForDisplay`, so keep it that way; don't add a second formatting path for measurement). `tabularize` only
  substitutes digits, so a localized separator is measured at its real width, which is correct. Verify
  `measure-column-widths.test.ts` still passes and add a case under a comma-decimal locale to prove widths stay
  consistent (no clipping, no over-reserve) when the decimal/group separators differ from ASCII.
- **The `colorizeSizeString` last-space parse.** Keep value↔unit separated by a plain ASCII space; never let a localized
  formatter inject NNBSP between them. Tested per Decision 3.
- **SSR / prerender.** `getLocale()` and every formatter must be safe under the static-adapter Node pass (no `window`,
  no throw). The date code already guards SSR; match it.
- **en-US parity.** For an en-US-region runtime, every formatter's output must be byte-identical to today (regression
  net below). The localization shows up only for other regions.

## Test strategy

- **en-US parity snapshot.** Before changing formatters, capture current output of `formatNumber`,
  `formatFileSizeWithFormat` (binary + SI, forced + dynamic), `formatSizeTriads`, and the `'system'` date for a fixed
  input matrix under an `en-US` locale. After the change, assert identical output under `en-US`. This is how the
  reviewer trusts "current users see no change".
- **Locale behavior tests.** With `getLocale()` stubbed to `de-DE` (comma decimal, `.`/space grouping), assert
  `formatNumber`, the human-friendly size decimal, the byte-triad group separator, and the `'system'` date all switch.
  Inject the locale via the chokepoint (export a test seam, e.g. `_setLocaleForTests`, mirroring `_setMeasureForTests`),
  don't reach into `Intl` globals.
- **colorize regression.** `colorizeSizeString("1,02 MB")` (German) → correct `size-mb` tier span.
- **measure-column-widths.** Existing tests green; add a comma-decimal-locale case asserting consistent widths.
- **No-bypass check.** Grep-style test or a code-review checklist item: no user-facing `toLocaleString(`,
  `new Intl.NumberFormat`, or `new Intl.DateTimeFormat` outside `lib/intl/` and the central format utils. (If a Go check
  is cheap to add, propose it; otherwise leave a note. Do not block on it.)
- **Performance.** A test (or a benchmark note) confirming formatter instances are reused across many format calls (e.g.
  spy on the cached factory, assert one construction per locale).

## Implementation sequence

Each step compiles and passes `pnpm check --fast` before the next.

1. **Locale chokepoint.** Add `lib/intl/locale.ts` with `getLocale()` + `_setLocaleForTests`, SSR-safe, with tests.
2. **Parity snapshot.** Add the en-US parity tests for the four formatter families (they pass against current code).
3. **Numbers.** Point `formatNumber` at `getLocale()`; route `LoadingIcon.svelte` through `formatNumber`. Parity holds
   for en-US; add the de-DE behavior test.
4. **Sizes (human-friendly).** Memoized locale `Intl.NumberFormat` for the decimal/integer value inside
   `formatFileSizeWithFormat`; keep unit labels + tiers + the ASCII value-unit space. Add the colorize regression.
5. **Sizes (raw-byte triads).** Source the group separator from the locale in `formatSizeTriads` (Decision 4), keeping
   per-triad coloring. (If David vetoes, keep U+2009 and skip this step.)
6. **Dates.** Route `getSystemLocaleFormatter()` through `getLocale()`; confirm iso/short/custom and `formatDate`
   untouched.
7. **Sweep bypasses.** Inventory remaining user-facing `toLocaleString` / ad-hoc `Intl.*` formatter construction; route
   each through the central utils or the chokepoint (keep `filter-popover-helpers.ts` calendar logic, but feed it
   `getLocale()`; leave `Intl.Segmenter`).
8. **Column-measurement verification.** Run `measure-column-widths.test.ts`; add the comma-decimal case.
9. **Docs.** Update `selection/CLAUDE.md` (thin-space note → "group separator from locale via the formatter layer"),
   `settings/CLAUDE.md` date-source note (now via `getLocale()`), and add a one-line module note for `lib/intl/`. Touch
   `docs/architecture.md` only if a one-line pointer is warranted (it is a map). Add the `lib/intl/` `DETAILS.md`
   sibling per the doc-system contract.

## Files in scope (verify before editing)

- new: `src/lib/intl/locale.ts` (+ `.test.ts`), `src/lib/intl/CLAUDE.md` + `DETAILS.md`, plus a memoized
  `Intl.NumberFormat` factory (in `lib/intl/` or colocated with the size utils, whichever is the cleaner home).
- `src/lib/file-explorer/selection/selection-info-utils.ts` (`formatNumber`, `formatSizeTriads`, `colorizeSizeString`,
  `tierClassForUnit`) + `.test.ts`.
- `src/lib/settings/format-utils.ts` (`formatFileSizeWithFormat`, `getSystemLocaleFormatter`) + `.test.ts`.
- `src/lib/ui/LoadingIcon.svelte`.
- `src/lib/file-explorer/views/measure-column-widths.ts` (verify; likely no change) + `.test.ts` (add a case).
- `src/lib/query-ui/filter-chips/filter-popover-helpers.ts` (feed `getLocale()`).
- docs: `selection/CLAUDE.md`, `settings/CLAUDE.md`, and the new `lib/intl/` docs.

## Verification (definition of done)

- `pnpm check` green (Svelte + Go; no Rust changes).
- en-US-region output is byte-identical to pre-change for counts, both size modes, and the `'system'` date (parity
  tests).
- With the locale chokepoint set to `de-DE`, counts, human-friendly size decimals, raw-byte grouping, and the `'system'`
  date all render localized separators; the file-list Size/Modified columns size correctly (no clip, no over-reserve)
  under that locale.
- No user-facing `toLocaleString` / ad-hoc `Intl` formatter construction remains outside `lib/intl/` and the central
  format utils.
- `Intl` formatter instances are memoized (one construction per locale).
- Docs updated; `lib/intl/` has the `CLAUDE.md` + `DETAILS.md` pair.

## Hand-off note for the reviewer (David)

The one decision to bless before merge: **Decision 4** (raw-byte triad grouping follows the locale, replacing the
deliberate U+2009 thin space) and, with it, **Decision 2** (counts follow OS region rather than always `en-US`).
Together they mean a non-en-region user sees native separators everywhere instead of the current English look. That's
the native-correct behavior and the point of the step, but it's a visible change for those users, so confirm it's wanted
before this ships.
