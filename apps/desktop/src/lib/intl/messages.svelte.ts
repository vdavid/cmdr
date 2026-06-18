/**
 * The thin i18n runtime: resolve user-facing text from JSON message catalogs,
 * format it through `intl-messageformat` (ICU MessageFormat 1), reading the
 * active locale from `$lib/intl`'s single `getLocale()` source.
 *
 * Two entry points:
 *  - `t(key, params?)`: resolve + ICU-format. The path for ordinary copy,
 *    including plural/select sentences and `{name}` interpolation (one code
 *    path; trivial interpolation runs through the engine too).
 *  - `getMessage(key)`: resolve to the RAW catalog string WITHOUT ICU parsing,
 *    for callers that do their own composition and must not hit ICU's
 *    brace/apostrophe grammar (the error pipeline's `{system_settings}` tokens,
 *    `esc()` HTML entities, snarkdown).
 *
 * Reactivity (load-bearing): a module-level locale-version `$state` rune
 * (`.svelte.ts` is required for `$state`). It is a re-render SIGNAL, not a
 * second locale source. `getLocale()` stays the single source of truth for the
 * VALUE. `t()` and `getMessage()` MUST read the version rune UNCONDITIONALLY at
 * the top, BEFORE any compiled-message cache lookup; otherwise the reactive
 * dependency isn't tracked and `{t('key')}` won't re-run on a locale change.
 * Note `state_referenced_locally` is suppressed, so the compiler will NOT warn
 * on a wrong read; the reactivity test in `messages.svelte.test.ts` is the only
 * guard. Pattern mirrors `system-strings.svelte.ts`.
 *
 * `t()`/`getMessage()` are reactive only inside a reactive context (markup /
 * `$derived`). Called once in a plain `.ts` computation they're a snapshot,
 * the right semantics for transient strings (toasts, error copy).
 */

import { IntlMessageFormat, type PrimitiveType, type FormatXMLElementFn } from 'intl-messageformat'
import { getLocale, setLocaleOverride } from './locale'
import type { MessageKey } from './keys.gen'

const BASE_LOCALE = 'en'

/** A catalog: key → ICU message string, with `@key` metadata already stripped. */
type Catalog = Record<string, string>

/**
 * Drops ARB-style `@key` metadata entries (object values), keeping only the
 * renderable string messages. The raw JSON's inferred type mixes string
 * messages and metadata objects, so we narrow per-entry. The `@key` metadata is
 * thus never seen by the runtime (Decision 4).
 */
function stripMetadata(raw: Record<string, unknown>): Catalog {
  const out: Catalog = {}
  for (const [key, value] of Object.entries(raw)) {
    if (key.startsWith('@')) continue
    if (typeof value === 'string') out[key] = value
  }
  return out
}

// Static catalog imports for the bundled SPA: every `messages/<locale>/*.json` is
// eagerly globbed and merged into one per-locale map at module load, so adding a
// catalog file (or a whole new locale dir) needs no edit here. The dir segment is
// the locale tag (`messages/de-DE/foo.json` → `de-DE`); a path that doesn't look
// like a BCP-47 tag is NOT a locale and is skipped (the `screenshots/` capture-
// artifact dir lives alongside the locale dirs, and `en-XA/` is present only in
// dev builds). `ssr=false` (`+layout.ts`) means this runs client-side only; the
// merge touches no `window`, so it's safe regardless.
const allModules = import.meta.glob<Record<string, unknown>>('./messages/*/*.json', {
  eager: true,
  import: 'default',
})

/**
 * A loose BCP-47 tag matcher for the directory-name → locale gate: a 2-or-3
 * letter language subtag, optionally followed by `-` plus script/region/variant
 * subtags (letters or digits). Deliberately permissive (it just has to admit
 * real locale dirs like `en`, `pt-BR`, `en-XA` and reject `screenshots`); the
 * runtime never trusts the tag for anything beyond catalog keying and the
 * fallback split, and `Intl` validates it later when used as a format locale.
 */
const BCP47_DIR = /^[a-z]{2,3}(-[a-z0-9]+)*$/i

/** Extracts the locale-dir segment from a `./messages/<locale>/<file>.json` glob path. */
function localeOfPath(path: string): string {
  // path is `./messages/<locale>/<file>.json`; the locale is the third segment.
  return path.split('/')[2]
}

/**
 * locale tag → its merged, metadata-stripped catalog, built from every globbed
 * `messages/<locale>/*.json`. Non-locale dirs (`screenshots/`) are filtered out
 * by `BCP47_DIR`. The value is `Catalog | undefined` because the fallback chain
 * in `resolveRaw` genuinely indexes by a locale that may have no catalog (the
 * whole point of the chain).
 */
const catalogs: Record<string, Catalog | undefined> = (() => {
  const out: Record<string, Catalog> = {}
  for (const [path, raw] of Object.entries(allModules)) {
    const locale = localeOfPath(path)
    if (!BCP47_DIR.test(locale)) continue // skip `screenshots/` and any other non-locale dir
    out[locale] ??= {}
    Object.assign(out[locale], stripMetadata(raw))
  }
  return out
})()

/**
 * The locale tags that have a loaded catalog, sorted, with the base `en` first.
 * Drives the language-selector options so a newly-added locale dir auto-appears.
 * `en` always exists (the base catalog ships); `en-XA` appears only in dev builds.
 */
export function availableLocales(): string[] {
  const tags = Object.keys(catalogs)
  return tags.sort((a, b) => {
    if (a === BASE_LOCALE) return -1
    if (b === BASE_LOCALE) return 1
    return a.localeCompare(b)
  })
}

/**
 * Re-render signal. Bumped by `setLocale()` so markup that read it re-runs.
 * NOT the locale value: that always comes from `getLocale()`.
 */
let localeVersion = $state(0)

/** Compiled-`IntlMessageFormat` cache, keyed on `${locale}\u0000${key}`. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a pure parse-once perf cache. Reactivity comes from the `localeVersion` rune, not the cache; a SvelteMap would add tracking overhead for no behavior change.
const compiledCache = new Map<string, IntlMessageFormat>()

/** The base language subtag of a BCP 47 tag (`de-DE` → `de`), lowercased. */
function baseLanguageOf(locale: string): string {
  return locale.split('-')[0].toLowerCase()
}

/**
 * Resolves a key's raw catalog string via the fallback chain
 * locale → base language → `en` → the key itself (so a missing key is visible,
 * never a crash). Does NOT read the version rune; callers must read it first.
 */
function resolveRaw(locale: string, key: string): string {
  const lang = baseLanguageOf(locale)
  return catalogs[locale]?.[key] ?? catalogs[lang]?.[key] ?? catalogs[BASE_LOCALE]?.[key] ?? key
}

// ── Capture mode (capture build only; absent everywhere else) ─────────────────
//
// A screenshot-coupling harness drives the app surface-by-surface, records which
// catalog keys render on each surface, and writes `@key.screenshot` couplings
// from the result (see `test/e2e-playwright/i18n-capture.spec.ts` and
// `scripts/couple-screenshots.js`). The runtime is the only place that knows the
// RESOLVED key behind every `t()`/`getMessage()`/`<Trans>` call, so the
// instrumentation lives here.
//
// Gated on `__CMDR_I18N_CAPTURE__`, a Vite `define` compile-time constant that is
// TRUE only in the dedicated capture build (the i18n-capture orchestrator sets
// `CMDR_I18N_CAPTURE_BUILD=1`) and FALSE in prod and ordinary dev/E2E builds.
// Because it's a constant, esbuild dead-code-eliminates the entire block below
// (the sink, `recordCapturedKey`, the API, and `if (false && captureActive) …`
// in the hot path) when it's false: true zero overhead and verifiably ABSENT
// from prod. Why a build constant, not the runtime `getAppMode()`: the install
// runs at module load, before `initAppMode()` resolves over IPC, and the E2E
// binary is a production Vite build (`import.meta.env.DEV` false), so the runtime
// gate read `'prod'` at load and never installed the API.

/** True only while a capture run is active. Exists only in the capture build. */
let captureActive = false
/** The surface label every recorded key is tagged with until it changes. */
let captureSurface = ''
/** surface label → the set of catalog keys that resolved while it was active. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- a dev/E2E-only diagnostic sink, never rendered; reactivity would be pure overhead.
const captureSink = new Map<string, Set<string>>()

/** Records `key` against the current surface. Called only when `captureActive`. */
function recordCapturedKey(key: string): void {
  let keys = captureSink.get(captureSurface)
  if (keys === undefined) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- a dev/E2E-only diagnostic sink, never rendered; reactivity would be pure overhead.
    keys = new Set<string>()
    captureSink.set(captureSurface, keys)
  }
  keys.add(key)
}

/**
 * The window-exposed capture control surface. Typed loosely on `window` so the
 * Playwright driver (which talks to the webview by name) can call it without a
 * shared type. Installed once, only in the capture build.
 */
interface I18nCaptureApi {
  /** Turns recording on. Returns whether it's now active. */
  enable: () => boolean
  /** Turns recording off. */
  disable: () => void
  /** Sets the surface label that subsequent resolves are tagged with. */
  setSurface: (label: string) => void
  /** Returns surface → sorted key array (JSON-serializable) for the driver. */
  dump: () => Record<string, string[]>
  /** Clears all recorded keys (keeps the active flag and surface). */
  reset: () => void
  /**
   * Forces every reactive `t()`/`getMessage()`/`<Trans>` in mounted markup to
   * re-run WITHOUT changing the locale, so the keys an already-mounted surface
   * renders get recorded under the current surface. Bumps the locale-version
   * rune via `setLocale(getLocale())`; the resolved text is identical, so
   * there's no visible change.
   */
  rerender: () => void
  /**
   * Switches the app's active locale to `tag` (e.g. `en-XA`, the pseudolocale)
   * for the overflow-capture pass, so every surface renders in the expanded,
   * accented strings. `null` reverts to the OS default. The driver calls this
   * once after the app is ready, before capturing surfaces. Relies on the
   * `tag`'s catalog being loaded (the glob includes it only if the dir existed
   * at build time, so generate `en-XA` BEFORE the capture build).
   */
  setLocale: (tag: string | null) => void
  /**
   * Sets the UI zoom (the `appearance.textSize` percentage, 75/100/125/150) for
   * the WORST-CASE overflow pass, so every surface renders at the largest
   * supported zoom while the pseudolocale already inflates the strings. Drives
   * the exact production path the Language/zoom UI uses: `setSetting` updates the
   * store, which cross-window-syncs and re-runs `text-size.svelte`'s
   * `computeAndApply` (the `--font-scale` root var + the reactive scale). Lazily
   * imports `$lib/settings` so the intl runtime stays decoupled from settings
   * outside the capture build. Returns a promise the driver awaits before the
   * shot so the new scale has applied.
   */
  setTextSize: (percent: number) => Promise<void>
}

if (__CMDR_I18N_CAPTURE__ && typeof window !== 'undefined') {
  const api: I18nCaptureApi = {
    enable() {
      captureActive = true
      return true
    },
    disable() {
      captureActive = false
    },
    setSurface(label: string) {
      captureSurface = label
    },
    dump() {
      const out: Record<string, string[]> = {}
      for (const [surface, keys] of captureSink) out[surface] = [...keys].sort()
      return out
    },
    reset() {
      captureSink.clear()
    },
    rerender() {
      setLocale(getLocale())
    },
    setLocale(tag: string | null) {
      setLocale(tag)
    },
    async setTextSize(percent: number) {
      // Lazy import: keeps the always-loaded intl runtime free of a settings
      // dependency; this method only ever runs in the capture build.
      const { setSetting } = await import('$lib/settings')
      setSetting('appearance.textSize', percent)
    },
  }
  ;(window as unknown as { __cmdrI18nCapture?: I18nCaptureApi }).__cmdrI18nCapture = api
}

/**
 * Test-only: reset capture state between unit tests so the always-false prod
 * invariant and the recording behavior can be asserted in isolation.
 */
export function _resetCaptureForTests(): void {
  captureActive = false
  captureSurface = ''
  captureSink.clear()
}

/**
 * The raw catalog value for `key` (RAW string, no ICU parsing), via the
 * fallback chain. Reads the version rune first so it's reactive in markup.
 */
export function getMessage(key: MessageKey): string {
  // Read the rune UNCONDITIONALLY and FIRST. See the reactivity note above.
  // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- load-bearing rune read: tracks the reactive dependency before any cache lookup; see header.
  localeVersion
  if (__CMDR_I18N_CAPTURE__ && captureActive) recordCapturedKey(key)
  const locale = getLocale()
  return resolveRaw(locale, key)
}

/** Params accepted by `t()`: primitives, dates, or `<tag>` handler functions. */
export type TranslationParams = Record<string, PrimitiveType | Date | FormatXMLElementFn<unknown>>

/**
 * The result of `t()`: a string for plain/interpolated messages, or an array of
 * strings and tag-handler return values for rich-text (`<tag>`) messages. Most
 * call sites pass no tag handlers and get a string; `<Trans>` reads the array.
 * `IntlMessageFormat['format']` already returns `string | unknown[]`.
 */
export type TranslationResult = ReturnType<IntlMessageFormat['format']>

/** A compiled `IntlMessageFormat` for `(locale, key)`, memoized. */
function getCompiled(locale: string, key: string): IntlMessageFormat {
  const cacheKey = `${locale}\u0000${key}`
  let compiled = compiledCache.get(cacheKey)
  if (compiled === undefined) {
    compiled = new IntlMessageFormat(resolveRaw(locale, key), locale)
    compiledCache.set(cacheKey, compiled)
  }
  return compiled
}

/**
 * Resolves and ICU-formats `key` with `params`. The path for ordinary
 * user-facing copy, including plural/select and `{name}` interpolation.
 * Reactive in markup. For rich-text messages, supply each `<tag>` as a handler
 * function in `params` and read the returned array (see `Trans.svelte`).
 */
export function t(key: MessageKey, params?: TranslationParams): TranslationResult {
  // Read the rune UNCONDITIONALLY and FIRST, before any cache lookup, or the
  // reactive dependency isn't tracked. See the reactivity note above.
  // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- load-bearing rune read: tracks the reactive dependency before any cache lookup; see header.
  localeVersion
  if (__CMDR_I18N_CAPTURE__ && captureActive) recordCapturedKey(key)
  const locale = getLocale()
  return getCompiled(locale, key).format(params)
}

/**
 * Convenience for the common case: `t()` for a message with no `<tag>` handlers,
 * narrowed to `string`. Throws if the key resolves to rich-text (a misuse).
 */
export function tString(key: MessageKey, params?: TranslationParams): string {
  const result = t(key, params)
  if (typeof result !== 'string') {
    throw new Error(`Message "${key}" produced rich-text output; use <Trans> or t() and read the array`)
  }
  return result
}

/**
 * The locale-switch seam, driven by the Settings > Appearance > Language picker
 * (and by tests). Writes the locale VALUE into the single `getLocale()` source
 * AND bumps the version rune so open `t()`/`<Trans>` usages re-render (and the
 * number/date formatters, which read the same source, reformat). Pass `null` to
 * revert to the OS default. Clears the compiled cache so a re-resolve picks up
 * the new locale.
 */
export function setLocale(locale: string | null): void {
  setLocaleOverride(locale)
  compiledCache.clear()
  localeVersion += 1
}

/** Test seam: drop the compiled-message cache so a memoization assertion starts clean. */
export function _clearCompiledCacheForTests(): void {
  compiledCache.clear()
}

/**
 * Test seam: register (or clear, with `null`) an extra locale catalog so a test
 * can observe a real cross-locale resolution and re-render. Only `en` ships;
 * this lets the reactivity/fallback tests prove behavior without a real second
 * locale catalog in the repo. Clears the compiled cache for that locale's keys.
 */
export function _setCatalogForTests(locale: string, catalog: Catalog | null): void {
  if (catalog === null) {
    Reflect.deleteProperty(catalogs, locale)
  } else {
    catalogs[locale] = catalog
  }
  compiledCache.clear()
}
