/**
 * What `appearance.language: 'system'` actually resolves to.
 *
 * The naive answer is the webview's own locale, and it's wrong twice over. The
 * webview exposes exactly ONE tag, so a user whose macOS preferences read
 * `[hu-HU, sv-SE]` could never reach Swedish when Hungarian isn't shipped: their
 * own second choice was structurally unreachable. And that single tag goes
 * through the catalog resolver's base-language fallback, which happily lands
 * `zh-Hant-TW` on the Simplified `zh` catalog.
 *
 * So Rust answers instead. It reads the ordered `AppleLanguages` list, walks it
 * for the first catalog we ship, and refuses to cross a script boundary
 * (`src-tauri/src/intl/`). This module fetches that answer once per window and
 * hands it to `setLocale()`.
 *
 * Every window resolves for itself: each is its own webview with its own i18n
 * runtime instance, so the main window's answer doesn't reach the Settings or
 * Queue window.
 */

// Aliased: `getUiLocale` is also the sync reader in `locale.ts` (the language
// the app currently speaks). This one is the IPC round-trip that asks the OS.
import { getUiLocale as fetchOsUiLocale } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('ui-locale')

/**
 * The OS answer, or `null` for "no OS answer": either it hasn't arrived yet, or
 * this platform has no preference list (Linux), where the webview default is
 * the right locale to fall back on.
 */
let systemUiLocale: string | null = null

/** The in-flight (or settled) fetch, so N callers cost one IPC round-trip. */
let pending: Promise<string | null> | null = null

/**
 * Fetches the OS-resolved UI locale once per window and caches it. Idempotent:
 * later calls return the same promise, so it's safe to call from every consumer
 * that needs the answer rather than ordering them by hand.
 *
 * Never throws. A failed fetch leaves the answer `null`, which means the webview
 * default stands: the app comes up in a reasonable language rather than not at
 * all.
 */
export function loadSystemUiLocale(): Promise<string | null> {
  pending ??= fetchOsUiLocale()
    .then((locale) => {
      systemUiLocale = locale ?? null
      log.debug('OS-resolved UI locale: {locale}', { locale: systemUiLocale ?? '(none)' })
      return systemUiLocale
    })
    .catch((error: unknown) => {
      log.warn('Could not read the OS-resolved UI locale, using the webview default: {error}', { error })
      return null
    })
  return pending
}

/**
 * The locale to hand `setLocale()` for a given `appearance.language` value.
 *
 * `'system'` is a sentinel we never write a resolved tag back into (decision 5
 * of the auto-language plan: an implicit choice must stay implicit, or the user
 * is frozen out of following the OS). So it's resolved here, on every read.
 *
 * `null` means "no override": the formatting layer falls back to the webview
 * default. That's the honest answer before `loadSystemUiLocale()` settles and
 * on any platform without an OS preference list.
 * @param setting the persisted `appearance.language` value
 */
export function pickUiLocale(setting: string): string | null {
  return setting === 'system' ? systemUiLocale : setting
}

/** Test seam: pin (or clear, with `null`) the OS answer without an IPC call. */
export function _setSystemUiLocaleForTests(locale: string | null): void {
  systemUiLocale = locale
  pending = locale === null ? null : Promise.resolve(locale)
}
