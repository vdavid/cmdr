/**
 * The two locale sources for the whole frontend: the language we SPEAK and the
 * conventions we FORMAT in. Every consumer reads one of them from here, so
 * "what locale is active" has two answers and two places to change, never more.
 *
 * They're separate because macOS keeps them separate. System Settings >
 * General > Language & Region is two settings, and a person can legitimately
 * read Hungarian while living in Sweden. Picking a UI language is not
 * permission to overwrite the number, size, and date conventions they chose:
 *
 *  - {@link getUiLocale} follows the `appearance.language` setting (`'system'`
 *    resolves through `os-locales.ts`). Catalog text reads this.
 *  - {@link getFormatLocale} always follows the OS, whatever the setting says.
 *    Numbers, sizes, dates, and calendar conventions read this.
 *
 * Both answers arrive from Rust, which reads the OS directly (`os-locales.ts`
 * fetches them). The webview can't work the formatting one out for itself: it
 * drops the region override macOS keeps on the locale, so a US-English Mac set
 * to the Swedish region formats US dates here and Swedish ones everywhere else.
 * `DETAILS.md` has the measurement.
 */

/** Test override for the UI half; also written by `setLocale()`. `null` = runtime default. */
let uiLocaleOverride: string | null = null

/** Test-only override for the formatting half, ahead of the OS answer. */
let formatLocaleOverride: string | null = null

/** The OS's own formatting tag, composed in Rust. `null` until (or unless) it answers. */
let osFormatLocale: string | null = null

/** Fallback when the runtime can't resolve a locale (defensive; `Intl` is always present in our targets). */
const FALLBACK_LOCALE = 'en-US'

/**
 * The webview's own locale, which is the OS's answer before anything of ours
 * touches it. Cheap (a single `Intl` resolve), and deliberately not cached: see
 * the note on {@link getFormatLocale}.
 */
function runtimeLocale(): string {
  try {
    const resolved = new Intl.NumberFormat().resolvedOptions().locale
    return resolved.length > 0 ? resolved : FALLBACK_LOCALE
  } catch {
    return FALLBACK_LOCALE
  }
}

/**
 * The language the app SPEAKS, as a BCP 47 tag (e.g. `"en-US"`, `"hu"`).
 * Catalog resolution reads this and nothing else; formatters must not.
 *
 * Falls back to the webview's locale when nothing has been set, which is the
 * honest answer before `settings-applier.ts` has applied the stored language
 * and on any platform without an OS preference list.
 *
 * SSR-safe: touches no `window`/DOM and never throws, so it's usable under the
 * SvelteKit static adapter's prerender/Node pass and inside the
 * capability-restricted viewer window.
 */
export function getUiLocale(): string {
  return uiLocaleOverride ?? runtimeLocale()
}

/**
 * The locale whose CONVENTIONS the user formats in, as a BCP 47 tag. Numbers,
 * file sizes, dates, and calendar facts (which day the week starts on) read
 * this. It follows the OS and ❌ never the `appearance.language` setting.
 *
 * Prefers the tag Rust composed from the OS's language and REGION, because the
 * webview's own locale silently drops the region override. Falls back to that
 * webview locale when there's no OS answer: before the fetch settles, off
 * macOS, and when the region is missing or unreadable. It's a working answer,
 * which a malformed composed tag would not be.
 *
 * SSR-safe on the same terms as {@link getUiLocale}.
 *
 * Not cached: returns the live answer on every call so a locale change is
 * observable. The formatters that call this ARE cached (keyed on the returned
 * locale), so the per-call cost here is a string read or a single cheap `Intl`
 * resolve, not formatter construction. See `number-format.ts`.
 */
export function getFormatLocale(): string {
  return formatLocaleOverride ?? osFormatLocale ?? runtimeLocale()
}

/**
 * Adopts (or drops, with `null`) the OS's formatting tag. Called by
 * `os-locales.ts` when Rust answers and whenever the OS moves underneath us;
 * ❌ nothing else should write it, and no setting feeds it.
 *
 * Value only, like {@link setUiLocaleOverride}: the re-render comes from the
 * caller re-applying `appearance.language`, which bumps the message runtime's
 * version rune. So adopt the new tag BEFORE that call, or the re-render reads
 * the old conventions.
 */
export function setOsFormatLocale(locale: string | null): void {
  osFormatLocale = locale
}

/**
 * Sets (or clears, with `null`) the UI-language override: the value half of a
 * language switch. The reactivity half (re-rendering open `t()`/`<Trans>`
 * usages) lives in `messages.svelte.ts`'s `setLocale()`, which calls this AND
 * bumps a version rune. Call `setLocale()` from app code, not this, so
 * re-render fires.
 *
 * Deliberately reaches the UI half only: a language pick must leave the user's
 * formatting conventions where System Settings put them.
 */
export function setUiLocaleOverride(locale: string | null): void {
  uiLocaleOverride = locale
}

/**
 * Test seam: pin BOTH locales to one tag, the shape of a machine whose language
 * and region agree (mirrors `_setMeasureForTests` in
 * `measure-column-widths.ts`). Pass `null` to revert both to the runtime
 * default. Use `_setFormatLocaleForTests` after this to pull them apart.
 *
 * Value only: it does NOT bump the message runtime's version rune, so it won't
 * drive a markup re-render (use `setLocale()` for that).
 */
export function _setLocaleForTests(locale: string | null): void {
  uiLocaleOverride = locale
  formatLocaleOverride = locale
}

/**
 * Test seam: pin (or clear, with `null`) the FORMATTING locale alone, so a test
 * can put a Hungarian UI on a Swedish Mac. Production has no equivalent: the
 * formatting locale is the OS's to decide.
 */
export function _setFormatLocaleForTests(locale: string | null): void {
  formatLocaleOverride = locale
}
