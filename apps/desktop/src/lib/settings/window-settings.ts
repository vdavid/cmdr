/**
 * The one call every window makes to get settings.
 *
 * Each Cmdr window is its own webview with its own module graph, so the
 * settings store and the reactive layer both start empty in every one of them.
 * `initWindowSettings()` seeds both, picking the right store access for the
 * route, and the ROOT layout (`routes/+layout.svelte`) calls it — so a new
 * window route gets settings without doing anything, and can't forget.
 *
 * A window that gates its body on settings being ready (the queue, settings,
 * shortcuts, and viewer windows do) should `await` this itself in `onMount`.
 * That's not a second initialization: the call is promise-memoized, so the
 * page's await and the root layout's await resolve off the same run.
 *
 * Coverage guard: `routes/reactive-settings-coverage.test.ts`.
 */

import { setLocale } from '$lib/intl/messages.svelte'
import { loadSystemLocales, pickUiLocale, watchSystemLocales } from '$lib/intl/os-locales'
import { noteStartupLanguage } from '$lib/intl/language-analytics'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { initReactiveSettings } from './reactive-settings.svelte'
import { getSetting, onSpecificSettingChange } from './settings-store'

/**
 * How a window reaches the settings store.
 *
 * - `'full'`: the window's capability file grants `store:default`, so settings
 *   load from `settings.json` through `tauri-plugin-store`.
 * - `'restricted'`: the capability file deliberately drops `store:default` (the
 *   viewer renders hostile file content; the queue has no persistence in v1).
 *   Those windows seed from the backend's `get_restricted_window_settings`
 *   allowlist snapshot and stay current through cross-window change events.
 *
 * Keyed by route path, matching `src-tauri/capabilities/*.json`. Pinned by
 * `window-settings.test.ts`: every route with a `+page.svelte` has an entry, so
 * adding a window forces the choice rather than inheriting a guess.
 */
export const WINDOW_SETTINGS_ACCESS = {
  '/': 'full',
  '/debug': 'full',
  '/dev/components': 'full',
  '/dev/graphics': 'full',
  '/queue': 'restricted',
  '/settings': 'full',
  '/shortcuts': 'full',
  '/viewer': 'restricted',
} as const satisfies Record<string, 'full' | 'restricted'>

export type WindowSettingsAccess = (typeof WINDOW_SETTINGS_ACCESS)[keyof typeof WINDOW_SETTINGS_ACCESS]

/**
 * Store access for a window's route path. Unknown paths get `'full'`: a
 * capability file has `store:default` unless someone deliberately dropped it,
 * and guessing `'restricted'` would silently strand a real window on registry
 * defaults, which is the failure this module exists to prevent.
 */
export function windowSettingsAccess(pathname: string): WindowSettingsAccess {
  // Trailing slashes come from the static adapter's directory-style URLs.
  const normalized = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname
  if (!Object.hasOwn(WINDOW_SETTINGS_ACCESS, normalized)) return 'full'
  return WINDOW_SETTINGS_ACCESS[normalized as keyof typeof WINDOW_SETTINGS_ACCESS]
}

/**
 * Load settings and seed the reactive layer for the current window. Idempotent
 * and safe to call from several places in one window.
 *
 * @param pathname Route path to classify; defaults to the live location.
 */
export async function initWindowSettings(pathname?: string): Promise<void> {
  const route = pathname ?? (typeof window === 'undefined' ? '/' : window.location.pathname)
  await initReactiveSettings({ restrictedWindow: windowSettingsAccess(route) === 'restricted' })
}

/**
 * Keeps a SECONDARY window's language and formats in sync with the OS and with
 * `appearance.language`.
 *
 * Each window is its own webview with its own i18n runtime instance, so the
 * main window's settings applier doesn't reach the Settings or Queue window:
 * they apply the language themselves, and re-apply on every change (including
 * the user's own pick in the Settings picker, which round-trips through the
 * store) so the whole window re-localizes live.
 *
 * Applies twice on purpose. The persisted value is available synchronously, so
 * an explicit language is right from the first paint; the OS answers come from
 * Rust over IPC, so they land a tick later and the second apply is what picks
 * them up. ❌ Don't await them before the first apply: that would gate the
 * window's paint on a round-trip for a case (`'system'`) where the webview
 * default is already a close guess.
 *
 * Then it follows two sources of change for the rest of the window's life: the
 * user's own pick in the settings picker, and the OS moving underneath us
 * (`'system'` tracks the CURRENT system language, and the formatters track the
 * CURRENT region, so a switch in System Settings re-localizes the window
 * without a restart).
 *
 * @returns a teardown that stops following both
 */
export function initWindowLanguageSync(): () => void {
  const apply = (value: string): void => {
    setLocale(pickUiLocale(value))
  }
  apply(getSetting('appearance.language'))
  void loadSystemLocales().then(() => {
    apply(getSetting('appearance.language'))
    // Remember what this window came up in, so a pick in the Settings picker can
    // say what the user left. Here, not before the first apply: under `'system'`
    // that one still runs on the webview default. `language_resolved` is the main
    // window's event and never fires here.
    noteStartupLanguage()
  })

  // The subscription lands a tick later than the teardown could be called (a
  // window closing mid-startup), so a teardown that ran first unlistens on
  // arrival rather than leaving a listener behind.
  let unlistenOsLocale: UnlistenFn | undefined
  let stopped = false
  void watchSystemLocales(() => {
    apply(getSetting('appearance.language'))
  }).then((unlisten) => {
    if (stopped) unlisten()
    else unlistenOsLocale = unlisten
  })

  const unsubscribeSetting = onSpecificSettingChange('appearance.language', (_id, value) => {
    apply(value)
  })

  return () => {
    stopped = true
    unlistenOsLocale?.()
    unsubscribeSetting()
  }
}
