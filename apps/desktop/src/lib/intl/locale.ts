/**
 * The single locale source for the whole frontend.
 *
 * Every user-facing number, file size, and date formatter reads the active
 * locale from here, so "what locale is active" has exactly one answer and one
 * place to change. No formatter hardcodes a locale tag and no formatter
 * resolves its own locale from `Intl` directly.
 *
 * Today the locale is the OS runtime default (what `Intl.DateTimeFormat(undefined, …)`
 * already trusts). A later i18n step (catalog tool) will own locale switching
 * and replace this function's internals; callers won't change.
 */

/** Test override; `null` means "use the runtime default". */
let localeOverride: string | null = null

/** Fallback when the runtime can't resolve a locale (defensive; `Intl` is always present in our targets). */
const FALLBACK_LOCALE = 'en-US'

/**
 * The active locale as a BCP 47 tag (e.g. `"en-US"`, `"de-DE"`, `"sv-SE"`).
 *
 * SSR-safe: touches no `window`/DOM and never throws, so it's usable under the
 * SvelteKit static adapter's prerender/Node pass and inside the
 * capability-restricted viewer window.
 *
 * Not cached: returns the live runtime default on every call so a future
 * locale-switching layer can change it observably. The formatters that call
 * this ARE cached (keyed on the returned locale), so the per-call cost here is
 * a single cheap `Intl` resolve, not formatter construction. See
 * `number-format.ts`.
 */
export function getLocale(): string {
  if (localeOverride !== null) return localeOverride
  try {
    const resolved = new Intl.NumberFormat().resolvedOptions().locale
    return resolved.length > 0 ? resolved : FALLBACK_LOCALE
  } catch {
    return FALLBACK_LOCALE
  }
}

/**
 * Sets (or clears, with `null`) the active-locale override: the single locale
 * VALUE the whole formatting layer reads. This is the value half of a locale
 * switch; the reactivity half (re-rendering open `t()`/`<Trans>` usages) lives
 * in `messages.svelte.ts`'s `setLocale()`, which calls this AND bumps a version
 * rune. Call `setLocale()` from app code, not this, so re-render fires.
 */
export function setLocaleOverride(locale: string | null): void {
  localeOverride = locale
}

/**
 * Test seam: pin the locale value only (mirrors `_setMeasureForTests` in
 * `measure-column-widths.ts`). Pass `null` to revert to the runtime default.
 * Use for non-reactive value-snapshot tests; it does NOT bump the message
 * runtime's version rune, so it won't drive a markup re-render (use
 * `setLocale()` for that).
 */
export function _setLocaleForTests(locale: string | null): void {
  localeOverride = locale
}
