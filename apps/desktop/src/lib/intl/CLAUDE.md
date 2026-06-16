# Locale-aware formatting layer

One locale source feeding one formatting layer. Every user-facing number, file size, and date formats per the active
locale, read from exactly one place here.

## Module map

- `locale.ts`: `getLocale()`, the single locale source (OS runtime default today). `_setLocaleForTests` injects a
  locale.
- `number-format.ts`: memoized `Intl.NumberFormat` factory (`getNumberFormatter`), plus `formatInteger` (counts) and
  `getGroupSeparator` (byte-triad separator).

## Must-knows

- **Read the locale ONLY via `getLocale()`; format ONLY through this layer + `$lib/settings/format-utils`.** Don't
  hardcode a locale, call `toLocaleString`, or build an `Intl.NumberFormat`/`DateTimeFormat` in feature code. Enforced
  by `cmdr/no-raw-locale-format` (off for `*.test.ts`). Counts go through `formatInteger`/`formatNumber`, sizes through
  `formatSizeForDisplay`, dates through `formatDateForDisplay`. `Intl.Segmenter`/`Intl.Locale` aren't formatters and are
  fine.
- **`getLocale()` must stay SSR-safe**: no `window`/DOM, never throws (SvelteKit static-adapter Node pass + the
  capability-restricted viewer both call it). It returns the live default per call, uncached, by design; the formatters
  it feeds are cached, so don't add a locale cache that would hide a future locale change.
- **Keep `Intl` formatters memoized by (locale, options).** They run per-visible-entry in render AND in the
  column-measurement fold; per-call construction (~10× a format call) regresses scroll/measure on large directories.
- **en-US output is byte-identical to the pre-locale code EXCEPT raw-byte triads** (now comma-grouped, was a U+2009 thin
  space). Human-friendly sizes use `useGrouping: false` so en-US stays identical (a forced `10000.00 MB` must not become
  `10,000.00`). The parity net is `en-us-parity.test.ts`.

Depth (the seam's scope, why uncached, the en-US triad change, step-2 hand-off): [DETAILS.md](DETAILS.md).
