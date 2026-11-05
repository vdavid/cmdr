/**
 * The thin i18n runtime: resolve user-facing text from JSON message catalogs,
 * format it through `intl-messageformat` (ICU MessageFormat 1), reading the
 * UI language from `#lib/intl`'s `getUiLocale()` source.
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
 * second locale source. `getUiLocale()` stays the single source of truth for the
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
import { defaultUiLocale, getUiLocale, setCatalogLocaleResolver, setUiLocaleOverride } from './locale'
import { inheritableAncestors } from './locale-inheritance'
import { setDocumentLanguage } from './document-language'
import { rememberedStartupCatalog, rememberStartupCatalog } from './startup-catalog'
import type { MessageKey } from './keys.gen'
import { displayPath } from '#lib/utils/display-path.js'
import enCatalog from 'virtual:cmdr-catalog/en'
import { loaders, manifestMessages, shippedLocales } from 'virtual:cmdr-locale-manifest'

const BASE_LOCALE = 'en'

/**
 * What the runtime needs from the rest of the app, handed in by the boot code
 * (`routes/+layout.ts`) and, in tests, by `test-setup.ts`. ❌ Never import the logger or
 * the settings store here, not even lazily: they reach IPC, all of `tauri-commands/`, and
 * the settings registry, and `test-setup.ts` loads this module, so every one of those
 * modules would select the whole vitest suite in `svelte-tests-related`
 * (`src/test-setup-closure.test.ts` guards it).
 */
export interface MessageRuntimeHooks {
  /** Logs a warning under the `i18n` category. */
  warn: (message: string, properties: Record<string, unknown>) => void
  /** Writes the `appearance.textSize` setting. Only the E2E capture API calls it. */
  setTextSize: (percent: number) => void
}

let hooks: MessageRuntimeHooks | undefined
/** Warnings raised before the boot code installed the hooks, flushed on install. */
const pendingWarnings: Array<[string, Record<string, unknown>]> = []

/** Boot seam: wire in the logger and the settings writer. The latest call wins. */
export function installMessageRuntimeHooks(next: MessageRuntimeHooks): void {
  hooks = next
  for (const [message, properties] of pendingWarnings.splice(0)) next.warn(message, properties)
}

function warnCatalogLoad(message: string, properties: Record<string, unknown>): void {
  if (hooks) hooks.warn(message, properties)
  else pendingWarnings.push([message, properties])
}

/**
 * A catalog: key → ICU message string. `@key` metadata never reaches the runtime:
 * `scripts/vite-locale-catalogs.ts` strips it while building each catalog module.
 */
type Catalog = Record<string, string>

// Catalogs come from `scripts/vite-locale-catalogs.ts`: English as a static import
// (every window needs it: it's the per-key fallback and Settings search's second
// language), every other locale as its own chunk behind the manifest's loaders. The
// manifest is the source of truth for WHICH locales ship; a new locale dir joins
// with no edit here. `ssr=false` (`+layout.ts`) means this runs client-side only.

/**
 * locale tag → its catalog, for the catalogs LOADED in this window. `undefined`
 * for a shipped tag that isn't loaded, and for a chain rung that doesn't ship.
 */
const catalogs: Record<string, Catalog | undefined> = { [BASE_LOCALE]: enCatalog }

/** In-flight loads, so two callers asking for the same catalog share one import. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a load-dedup map.
const inFlight = new Map<string, Promise<void>>()

type CatalogLoader = () => Promise<{ default: Catalog }>

/** Test-only stand-ins for the manifest's loaders, so a test controls WHEN a catalog lands. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a test seam.
const loaderOverrides = new Map<string, CatalogLoader>()

/** Loads one shipped catalog's chunk; a tag with no loader (`en`, or one that doesn't ship) is a no-op. */
function loadCatalog(tag: string): Promise<void> {
  const existing = inFlight.get(tag)
  if (existing !== undefined) return existing
  const loader = loaderOverrides.get(tag) ?? (Object.hasOwn(loaders, tag) ? loaders[tag] : undefined)
  if (loader === undefined) return Promise.resolve()
  const pending = loader()
    .then((mod) => {
      catalogs[tag] = mod.default
    })
    .finally(() => inFlight.delete(tag))
  inFlight.set(tag, pending)
  return pending
}

/**
 * Every tag a fallback chain may land on: the shipped ones, loaded or not, plus a
 * synthetic one a test registered through `_setCatalogForTests`. Whether a catalog
 * is LOADED never shapes a chain, or `zh-Hant` could resolve differently depending
 * on what else happened to be in memory.
 */
function knownTags(): readonly string[] {
  const synthetic = Object.keys(catalogs).filter((tag) => !shippedLocales.includes(tag))
  return synthetic.length === 0 ? shippedLocales : [...shippedLocales, ...synthetic]
}

/**
 * A message from a catalog that may not be loaded, for the few keys the manifest
 * carries from EVERY catalog (`MANIFEST_KEYS` in the plugin; today the Language
 * label, for Settings search). Walks `locale`'s chain like `t()` does, then the key.
 */
export function manifestMessage(locale: string, key: MessageKey): string {
  for (const tag of fallbackChain(locale)) {
    const value = manifestMessages[tag]?.[key]
    if (value !== undefined) return value
  }
  return key
}

/**
 * The shipped locale tags, sorted, with the base `en` first, whichever catalogs
 * this window has loaded. Drives the Language picker, so a newly added locale dir
 * auto-appears. `en-XA` appears only when it was on disk at build time.
 */
export function availableLocales(): string[] {
  const tags = [...shippedLocales]
  return tags.sort((a, b) => {
    if (a === BASE_LOCALE) return -1
    if (b === BASE_LOCALE) return 1
    return a.localeCompare(b)
  })
}

/**
 * Re-render signal. Bumped by `setLocale()` so markup that read it re-runs.
 * NOT the locale value: that always comes from `getUiLocale()`.
 */
let localeVersion = $state(0)

/** Compiled-`IntlMessageFormat` cache, keyed on `${locale}\u0000${key}`. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a pure parse-once perf cache. Reactivity comes from the `localeVersion` rune, not the cache; a SvelteMap would add tracking overhead for no behavior change.
const compiledCache = new Map<string, IntlMessageFormat>()

/**
 * Per-locale fallback chain cache. Building a chain asks `Intl` for CLDR likely
 * subtags, which is far too much work for a hot `t()` call; the answer only
 * changes when the set of known tags does, which outside tests is never.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a pure resolve-once perf cache, cleared by `_setCatalogForTests`. Reactivity comes from the `localeVersion` rune.
const chainCache = new Map<string, string[]>()

/**
 * The catalogs to try for `locale`, in order: itself, then the ancestors it may
 * inherit from (same language AND same script, see `locale-inheritance.ts`), then
 * `en`. So `pt-PT` reads `pt` before English, while `zh-Hant` skips Simplified
 * `zh` entirely rather than putting text its reader can't read on screen.
 */
function fallbackChain(locale: string): string[] {
  const cached = chainCache.get(locale)
  if (cached) return cached
  const chain = [locale, ...inheritableAncestors(locale, knownTags()), BASE_LOCALE]
  chainCache.set(locale, chain)
  return chain
}

/**
 * The catalog actually supplying this locale's text: the first tag in the
 * fallback chain that ships a catalog. `zh-Hant` stays `zh-Hant` (the chain
 * refuses to cross a script boundary), `pt-PT` becomes `pt`, and `ja-JP` becomes
 * `en` because we ship no Japanese at all.
 *
 * This is a different question from "what did the user ask for", and `<html
 * lang>` wants THIS one: announcing `lang="ja-JP"` over English text hands a
 * screen reader a Japanese voice for English words, which is worse than the
 * blunt-but-true `en`. Per-key resolution can still reach further down the chain
 * for a key an overlay doesn't fork; the head of the chain is the honest answer
 * for the window as a whole.
 */
export function resolvedCatalogLocale(locale: string): string {
  const cached = resolvedCache.get(locale)
  if (cached !== undefined) return cached
  const tags = knownTags()
  const resolved = fallbackChain(locale).find((tag) => tags.includes(tag)) ?? BASE_LOCALE
  resolvedCache.set(locale, resolved)
  return resolved
}

/**
 * Per-locale {@link resolvedCatalogLocale} answers. The number formatters ask it per visible
 * file-list entry, so it can't walk the chain each time; like the chain, the answer moves
 * only when the set of known tags does, and `_setCatalogForTests` clears both.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a pure resolve-once perf cache, cleared by `_setCatalogForTests`. Reactivity comes from the `localeVersion` rune.
const resolvedCache = new Map<string, string>()

// The number locale (`locale.ts::getCatalogLocale`) is the resolved catalog tag, read through
// the rune: markup or a `$derived` that formats a number re-runs on a language switch even when
// no message is involved. Installed here, not imported there, so `locale.ts` stays a leaf and a
// test that mocks this module away still formats numbers.
setCatalogLocaleResolver(() => {
  // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- load-bearing rune read: tracks the reactive dependency; see header.
  localeVersion
  return resolvedCatalogLocale(getUiLocale())
})

/**
 * `resolvedCatalogLocale(getUiLocale())`, read reactively: a `$derived` or markup that calls it
 * re-runs on a language switch, like one that calls `t()`. For a cache keyed by the language on
 * screen (Settings search's index), whose cache hits would otherwise read no message at all.
 */
export function uiCatalogLocale(): string {
  // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- load-bearing rune read: tracks the reactive dependency; see header.
  localeVersion
  return resolvedCatalogLocale(getUiLocale())
}

/**
 * Resolves a key's raw catalog string down the fallback chain, ending at the key
 * itself (so a missing key is visible, never a crash). Does NOT read the version
 * rune; callers must read it first.
 */
function resolveRaw(locale: string, key: string): string {
  for (const tag of fallbackChain(locale)) {
    const value = catalogs[tag]?.[key]
    if (value !== undefined) return value
  }
  return key
}

// ── Capture mode (E2E builds only; absent from production) ────────────────────
//
// A screenshot-coupling harness drives the app surface-by-surface, records which
// catalog keys render on each surface, and writes `@key.screenshot` couplings
// from the result (see `test/e2e-playwright/i18n-capture.spec.ts` and
// `scripts/couple-screenshots.ts`). The runtime is the only place that knows the
// RESOLVED key behind every `t()`/`getMessage()`/`<Trans>` call, so the
// instrumentation lives here.
//
// Gated on `__CMDR_E2E_BUILD__`, a Vite `define` compile-time constant that is
// TRUE in every E2E build (`CMDR_E2E_BUILD=1`; the i18n screenshot run shares the
// Playwright lane's binary) and FALSE in prod and dev. Because it's a constant,
// esbuild dead-code-eliminates the entire block below (the sink,
// `recordCapturedKey`, the API, and `if (false && captureActive) …` in the hot
// path) when it's false: true zero overhead and verifiably ABSENT from prod. In an
// E2E build the API installs inert: nothing records until the harness calls
// `enable()`. Why a build constant, not the runtime `getAppMode()`: the install
// runs at module load, before `initAppMode()` resolves over IPC, and the E2E
// binary is a production Vite build (`import.meta.env.DEV` false), so a runtime
// gate would read `'prod'` at load and never install the API.

/** True only while a capture run is active. Exists only in E2E builds. */
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
 * shared type. Installed once, only in E2E builds.
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
   * rune via `setLocale(getUiLocale())`; the resolved text is identical, so
   * there's no visible change.
   */
  rerender: () => void
  /**
   * Switches the app's active locale to `tag` (e.g. `en-XA`, the pseudolocale)
   * for the overflow-capture pass, so every surface renders in the expanded,
   * accented strings. `null` reverts to the OS default. The driver calls this
   * once after the app is ready, before capturing surfaces, and awaits the
   * returned promise: the switch lands once `tag`'s catalog chunk has loaded.
   * The chunk exists only if the dir was on disk at build time, so `en-XA` has
   * to be there when the E2E binary is built.
   */
  setLocale: (tag: string | null) => Promise<void>
  /**
   * Sets the UI zoom (the `appearance.textSize` percentage, 75/100/125/150) for
   * the WORST-CASE overflow pass, so every surface renders at the largest
   * supported zoom while the pseudolocale already inflates the strings. Drives
   * the exact production path the Language/zoom UI uses: `setSetting` updates the
   * store, which cross-window-syncs and re-runs `text-size.svelte`'s
   * `computeAndApply` (the `--font-scale` root var + the reactive scale). The
   * write goes through the boot-installed `MessageRuntimeHooks.setTextSize`, so
   * the intl runtime stays decoupled from settings. Returns a promise the driver
   * awaits before the shot so the new scale has applied.
   */
  setTextSize: (percent: number | string) => Promise<void>
}

if (__CMDR_E2E_BUILD__ && typeof window !== 'undefined') {
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
      // The current language's catalogs are loaded, so this re-render is synchronous.
      void setLocale(getUiLocale())
    },
    setLocale(tag: string | null) {
      // Returned so the driver's `captureCall` (a `page.evaluate`) waits for the catalog.
      return setLocale(tag)
    },
    setTextSize(percent: number | string) {
      if (!hooks) return Promise.reject(new Error('The message runtime hooks are not installed yet'))
      // Coerce: the E2E driver reaches this through `captureCall`, which
      // JSON-stringifies every arg, so `percent` arrives as a string ("100").
      // `appearance.textSize` is a typed number setting and rejects a string, so
      // normalize before writing (hence `number | string` on the signature).
      hooks.setTextSize(Number(percent))
      return Promise.resolve()
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
  if (__CMDR_E2E_BUILD__ && captureActive) recordCapturedKey(key)
  const locale = getUiLocale()
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

/**
 * A compiled `IntlMessageFormat` for `(locale, key)`, memoized. The engine gets the RESOLVED
 * catalog tag (the number locale's answer), because it picks plural categories and
 * writes `#` and `{n, number}` digits with it: both have to match the text on screen, so a
 * `ja-JP` Mac reading English gets English plurals and English digits.
 */
function getCompiled(locale: string, key: string): IntlMessageFormat {
  const cacheKey = `${locale}\u0000${key}`
  let compiled = compiledCache.get(cacheKey)
  if (compiled === undefined) {
    compiled = new IntlMessageFormat(resolveRaw(locale, key), resolvedCatalogLocale(locale))
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
  if (__CMDR_E2E_BUILD__ && captureActive) recordCapturedKey(key)
  const locale = getUiLocale()
  return asShown(getCompiled(locale, key).format(params))
}

/**
 * A formatted message as a person reads it: a same-named twin's pick taken out of
 * any path a param carried (`#lib/utils/display-path`). Catalog text never holds
 * a NUL, so this only ever touches what a param put there.
 */
function asShown(result: TranslationResult): TranslationResult {
  if (typeof result === 'string') return displayPath(result)
  if (!Array.isArray(result)) return result
  return result.map((part: unknown) => (typeof part === 'string' ? displayPath(part) : part))
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
 * `tString` in a given locale rather than the UI's, down that locale's own fallback chain, as a
 * snapshot (never reactive: markup always wants the UI locale, through `t()`). Settings search
 * reads English as a fallback this way.
 *
 * Only for a language this window HOLDS: English, or the UI language's chain. Any other catalog
 * isn't loaded, and resolving through it would quietly answer in English. So it throws outside
 * production builds; a text needed from EVERY language goes in the manifest instead
 * (`manifestMessage`).
 */
export function tStringIn(locale: string, key: MessageKey, params?: TranslationParams): string {
  const unloaded = fallbackChain(locale).find((tag) => shippedLocales.includes(tag) && catalogs[tag] === undefined)
  if (unloaded !== undefined && !import.meta.env.PROD) {
    throw new Error(`tStringIn("${locale}"): the "${unloaded}" catalog is not loaded in this window`)
  }
  const result = asShown(getCompiled(locale, key).format(params))
  if (typeof result !== 'string') {
    throw new Error(`Message "${key}" produced rich-text output; use <Trans> or t() and read the array`)
  }
  return result
}

/**
 * The language-switch seam, driven by the Settings > Appearance > Language
 * picker (and by tests). Writes the value into the `getUiLocale()` source AND
 * bumps the version rune so open `t()`/`<Trans>` usages re-render. Pass `null`
 * to revert to the OS default. Clears the compiled cache so a re-resolve picks
 * up the new locale.
 *
 * Moves copy and numbers (`getCatalogLocale()` reads the same tag). Dates keep
 * following the OS through `getFormatLocale()`, so a Hungarian speaker in
 * Sweden keeps their Swedish dates (`locale.ts`).
 *
 * Also announces the language to the DOM (`document-language.ts`). This is the
 * seam every window and every live change funnels through, which is what makes
 * one write here cover all of them.
 *
 * Loads first, switches second. A window holds only English and the catalogs
 * its language reads, so a switch first loads the new language's chain (an
 * overlay brings its base), and only then flips the locale, bumps the rune, and
 * writes `<html lang>`, in one synchronous step. Until then the window shows
 * the OLD language in full: never a frame of English, never half-switched.
 * When the chain is already loaded the switch is synchronous and the returned
 * promise is already settled. The latest call wins: a load that lands after a
 * newer `setLocale()` started applies nothing. A chunk that fails to load
 * leaves the old language on screen (and the promise resolves: there's nothing
 * a caller could do about it).
 */
export function setLocale(locale: string | null): Promise<void> {
  const call = ++latestSwitch
  const load = ensureCatalogs(locale ?? defaultUiLocale())
  if (load === null) {
    applyLocale(locale)
    return Promise.resolve()
  }
  return load.then(
    () => {
      if (call === latestSwitch) applyLocale(locale)
    },
    (error: unknown) => {
      warnCatalogLoad('Kept the current language: a catalog for {locale} did not load: {error}', { locale, error })
    },
  )
}

/** Numbers every `setLocale()` call; a load applies only if no newer call started since. */
let latestSwitch = 0

/** The synchronous half of a switch: everything a reader of the UI language sees moves at once. */
function applyLocale(locale: string | null): void {
  setUiLocaleOverride(locale)
  compiledCache.clear()
  evictUnreadCatalogs()
  localeVersion += 1
  // Read back through `getUiLocale()` rather than using `locale`: a `null` here
  // means "no override", and the effective tag is then the webview's own.
  const resolved = resolvedCatalogLocale(getUiLocale())
  setDocumentLanguage(resolved)
  rememberStartupCatalog(resolved)
}

/**
 * Loads every shipped catalog `locale`'s chain reads that isn't here yet. `null`
 * when the whole chain is already loaded, so a caller can stay synchronous.
 */
function ensureCatalogs(locale: string): Promise<void> | null {
  const missing = fallbackChain(locale).filter((tag) => shippedLocales.includes(tag) && catalogs[tag] === undefined)
  if (missing.length === 0) return null
  return Promise.all(missing.map(loadCatalog)).then(() => undefined)
}

/**
 * Drops every shipped catalog the current language doesn't read, English aside,
 * so arrowing through the picker doesn't end with every catalog in memory. A
 * test's synthetic catalog (`_setCatalogForTests`) is never dropped.
 */
function evictUnreadCatalogs(): void {
  if (keepAllCatalogs) return
  const read = new Set([BASE_LOCALE, ...fallbackChain(getUiLocale())])
  for (const tag of Object.keys(catalogs)) {
    if (shippedLocales.includes(tag) && !read.has(tag)) Reflect.deleteProperty(catalogs, tag)
  }
}

/** Set by `_loadAllCatalogsForTests`: nothing is evicted, so every later switch stays synchronous. */
let keepAllCatalogs = false

/** Test seam: drop the compiled-message cache so a memoization assertion starts clean. */
export function _clearCompiledCacheForTests(): void {
  compiledCache.clear()
}

/**
 * What a shipped catalog held before a test replaced it, so `null` puts the real
 * one back. Keyed by tag; an entry with `undefined` means nothing shipped under
 * that tag, so restoring means removing it again.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state; a test-only stash with no renderable content.
const shippedCatalogBackups = new Map<string, Catalog | undefined>()

/**
 * Test seam: register (or restore, with `null`) a locale catalog so a test can
 * observe a real cross-locale resolution and re-render, including against tags
 * that DO ship (the script-guard tests stand a fake `zh` up against a fake
 * `zh-Hant`).
 *
 * `null` restores whatever shipped under that tag rather than deleting it, so a
 * teardown can't strip a real catalog out of the runtime for every test that
 * follows it in the file. Ordering between describes then carries no meaning,
 * which is the only way this stays true as tests get added.
 *
 * Clears the compiled cache, and the chain cache with it: a chain is derived
 * from which catalogs exist, so seeding or restoring one invalidates them.
 */
export function _setCatalogForTests(locale: string, catalog: Catalog | null): void {
  if (!shippedCatalogBackups.has(locale)) {
    shippedCatalogBackups.set(locale, catalogs[locale])
  }
  if (catalog === null) {
    const shipped = shippedCatalogBackups.get(locale)
    shippedCatalogBackups.delete(locale)
    if (shipped === undefined) {
      Reflect.deleteProperty(catalogs, locale)
    } else {
      catalogs[locale] = shipped
    }
  } else {
    catalogs[locale] = catalog
  }
  compiledCache.clear()
  chainCache.clear()
  resolvedCache.clear()
}

/** Test seam: the tags whose catalog this window holds right now, sorted. */
export function _loadedCatalogsForTests(): string[] {
  return Object.keys(catalogs).sort()
}

/**
 * Test seam: load `tag`'s catalog through `loader` instead of its chunk (`null` restores the
 * chunk), so a test decides when, and in what order, catalogs land.
 */
export function _setCatalogLoaderForTests(tag: string, loader: CatalogLoader | null): void {
  if (loader === null) loaderOverrides.delete(tag)
  else loaderOverrides.set(tag, loader)
}

/** Test seam: back to a window's startup shape, English only. Undo with `_loadAllCatalogsForTests`. */
export function _unloadCatalogsForTests(): void {
  keepAllCatalogs = false
  for (const tag of Object.keys(catalogs))
    if (shippedLocales.includes(tag) && tag !== BASE_LOCALE) Reflect.deleteProperty(catalogs, tag)
  compiledCache.clear()
}

/** Test seam: load every shipped catalog. */
export async function _loadAllCatalogsForTests(): Promise<void> {
  keepAllCatalogs = true
  await Promise.all(shippedLocales.map(loadCatalog))
}

/**
 * Settles once this window's starting catalogs are loaded: the ones the last switch in
 * any window landed on (`startup-catalog.ts`), else the webview's own language's. The
 * remembered one also becomes the language until `appearance.language` applies, so a
 * language picked against the OS shows from the first frame. Never rejects: a chunk that
 * won't load leaves the window on the webview's language (English, at worst).
 *
 * The first render must not start before this, or every `t()` in it reads English for a
 * frame. The root `+layout.ts` `load` awaits it, which gates every window's first render.
 *
 * ❌ Not a top-level `await`. WebKitGTK 2.52 (the Linux E2E webview) resolved a secondary
 * window's route-node `import()` while this module's top-level await was still pending,
 * so the page component ran before its own module body had finished: every Settings,
 * viewer, and queue window crashed on `undefined is not a function` (CI run 39989092928).
 * `no-restricted-syntax` in `eslint.config.js` bans top-level await in app code.
 */
export const startupCatalogsReady: Promise<void> = (() => {
  const startupCatalog = rememberedStartupCatalog(shippedLocales)
  return (ensureCatalogs(startupCatalog ?? defaultUiLocale()) ?? Promise.resolve()).then(
    () => {
      if (startupCatalog === null) return
      setUiLocaleOverride(startupCatalog)
      setDocumentLanguage(startupCatalog)
    },
    (error: unknown) => {
      warnCatalogLoad('Starting without the catalog for {locale}: it did not load: {error}', {
        locale: startupCatalog ?? defaultUiLocale(),
        error,
      })
    },
  )
})()
