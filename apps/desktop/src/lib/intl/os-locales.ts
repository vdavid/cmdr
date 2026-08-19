/**
 * The OS's answers to "which language?" and "whose conventions?", fetched once
 * per window and kept current.
 *
 * Neither answer can be worked out in the webview. For the LANGUAGE, the naive
 * answer is the webview's own locale, and it's wrong twice over: the webview
 * exposes exactly ONE tag, so a user whose macOS preferences read
 * `[hu-HU, sv-SE]` could never reach Swedish when Hungarian isn't shipped, and
 * that single tag goes through the catalog resolver's base-language fallback,
 * which happily lands `zh-Hant-TW` on the Simplified `zh` catalog. For the
 * FORMATS, WebKit drops the region override macOS keeps on the locale, so a Mac
 * set to US English with a Swedish region resolves to plain `en-US` here while
 * every native app writes `2026-08-19` and `1 234 567,89`.
 *
 * So Rust answers both (`src-tauri/src/intl/`), and this module fetches the
 * pair and hands each half where it belongs: the language to `pickUiLocale()`
 * for the caller to apply, the formatting tag straight into `locale.ts`, since
 * no setting gets a say in it.
 *
 * Every window resolves for itself: each is its own webview with its own i18n
 * runtime instance, so the main window's answer doesn't reach the Settings or
 * Queue window. That includes following a LIVE change: `'system'` means the
 * language the user reads now and the region they set now, not the ones they
 * had at launch, so every window also subscribes through
 * {@link watchSystemLocales}.
 */

import { getOsLocales, onOsLocalesChanged } from '$lib/tauri-commands'
import type { OsLocales } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'
import { setOsFormatLocale } from './locale'
import type { UnlistenFn } from '@tauri-apps/api/event'

const log = getAppLogger('os-locales')

/** No OS answer at all: before the fetch settles, off macOS, and when it fails. */
const NO_ANSWER: OsLocales = { ui: null, format: null }

/**
 * The OS answers this window is running on. A `null` half means "no answer":
 * either it hasn't arrived yet, or this platform has nothing to say (Linux),
 * where the webview default is the right thing to fall back on.
 */
let systemLocales: OsLocales = NO_ANSWER

/** The in-flight (or settled) fetch, so N callers cost one IPC round-trip. */
let pending: Promise<OsLocales> | null = null

/**
 * Takes the OS's answers into use: caches the language for `pickUiLocale()` and
 * pushes the formatting tag into `locale.ts`, where every formatter reads it.
 *
 * The formatting half needs no caller: no setting overrides it, so there's
 * nothing to decide and nobody to forget.
 *
 * Accepts `undefined` although the command's type rules it out, because a
 * stubbed IPC layer really does resolve to nothing (a test that replaces every
 * `tauri-commands` function with a no-op promise), and one window coming up
 * with a broken locale beats one throwing during startup.
 */
function adopt(locales: OsLocales | undefined): void {
  systemLocales = locales ?? NO_ANSWER
  setOsFormatLocale(systemLocales.format)
}

/**
 * Fetches the OS answers once per window and caches them. Idempotent: later
 * calls return the same promise, so it's safe to call from every consumer that
 * needs an answer rather than ordering them by hand.
 *
 * Never throws. A failed fetch leaves both answers `null`, which means the
 * webview default stands: the app comes up in a reasonable language with
 * reasonable formats rather than not at all.
 */
export function loadSystemLocales(): Promise<OsLocales> {
  pending ??= getOsLocales()
    .then((locales) => {
      adopt(locales)
      log.debug('OS locales: language {ui}, formats {format}', {
        ui: systemLocales.ui ?? '(none)',
        format: systemLocales.format ?? '(none)',
      })
      return systemLocales
    })
    .catch((error: unknown) => {
      log.warn('Could not read the OS locales, using the webview defaults: {error}', { error })
      return NO_ANSWER
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
 * `null` means "no override": the UI falls back to the webview default. That's
 * the honest answer before `loadSystemLocales()` settles and on any platform
 * without an OS preference list. ❌ There's no formatting equivalent of this
 * function on purpose: the formatting tag answers to the OS alone.
 * @param setting the persisted `appearance.language` value
 */
export function pickUiLocale(setting: string): string | null {
  return setting === 'system' ? systemLocales.ui : setting
}

/**
 * Follows a live OS change for this window: keeps the cached answers current
 * and calls `onMoved` when either one actually moved.
 *
 * The caller's job is to re-apply the language setting, which is what turns a
 * new answer into a re-render. Under an explicit `appearance.language` that
 * re-apply is a no-op for the copy, and it's still the right call: the rune bump
 * re-renders the formatters against whatever the OS now formats in. Both halves
 * are adopted BEFORE `onMoved` runs, so that re-render reads the new answers.
 *
 * ❌ Don't call `onMoved` on an event that doesn't move either answer. The
 * backend already drops those, and this second guard is what keeps a stray or
 * replayed event from re-rendering every open `t()` in the window for nothing.
 *
 * @param onMoved run once per real change
 * @returns an unlisten for the subscription
 */
export function watchSystemLocales(onMoved: () => void): Promise<UnlistenFn> {
  return onOsLocalesChanged(({ locales }) => {
    if (locales.ui === systemLocales.ui && locales.format === systemLocales.format) return
    log.info('The OS locales moved: language {ui}, formats {format}', {
      ui: locales.ui ?? '(none)',
      format: locales.format ?? '(none)',
    })
    adopt(locales)
    onMoved()
  })
}

/**
 * Test seam: pin the OS answers without an IPC call. Two `null`s reset the
 * module to its pre-fetch state, so the next `loadSystemLocales()` really
 * fetches.
 */
export function _setSystemLocalesForTests(locales: OsLocales): void {
  adopt(locales)
  pending = locales.ui === null && locales.format === null ? null : Promise.resolve(locales)
}
