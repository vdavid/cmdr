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
import { loadSystemUiLocale, pickUiLocale } from '$lib/intl/ui-locale'
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
 * Keeps a SECONDARY window's UI language in sync with `appearance.language`.
 *
 * Each window is its own webview with its own i18n runtime instance, so the
 * main window's settings applier doesn't reach the Settings or Queue window:
 * they apply the language themselves, and re-apply on every change (including
 * the user's own pick in the Settings picker, which round-trips through the
 * store) so the whole window re-localizes live.
 *
 * Applies twice on purpose. The persisted value is available synchronously, so
 * an explicit language is right from the first paint; the `'system'` answer
 * comes from Rust over IPC, so it lands a tick later and the second apply is
 * what picks it up. ❌ Don't await the OS answer before the first apply: that
 * would gate the window's paint on a round-trip for a case (`'system'`) where
 * the webview default is already a close guess.
 *
 * @returns an unsubscribe for the change listener
 */
export function initWindowLanguageSync(): () => void {
  const apply = (value: string): void => {
    setLocale(pickUiLocale(value))
  }
  apply(getSetting('appearance.language'))
  void loadSystemUiLocale().then(() => {
    apply(getSetting('appearance.language'))
  })
  return onSpecificSettingChange('appearance.language', (_id, value) => {
    apply(value)
  })
}
