/**
 * System accent color integration.
 *
 * Reads the macOS system accent color from the Rust backend on startup
 * and listens for live changes when the user updates their accent color
 * in System Settings. Applies the color based on the user's "App color"
 * setting: either the macOS system accent or the Cmdr brand gold.
 *
 * --color-system-accent is always set to the system color (for the
 * settings preview). --color-accent is set based on the user's choice.
 * When 'cmdr-gold' is selected, the inline --color-accent is removed
 * so the CSS fallback in app.css takes effect.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands } from '$lib/ipc/bindings'
import { getAppLogger } from '$lib/logging/logger'
import { clearDirectoryIconCache } from '$lib/icon-cache'
import { getSetting, onSpecificSettingChange } from '$lib/settings'
import { mixSrgb, withAlpha } from '$lib/utils/srgb-mix'

const log = getAppLogger('accent-color')

let unlisten: UnlistenFn | undefined
let unlistenSetting: (() => void) | undefined
let darkModeQuery: MediaQueryList | undefined
let darkModeListener: ((e: MediaQueryListEvent) => void) | undefined
let lastSystemColor: string | undefined

// Resting Cmdr gold per scheme. Must mirror `--color-accent` in `app.css`.
// When the user picks "Cmdr gold" we don't write `--color-accent` and let the
// CSS fallback win, but the *derived* tokens still need a concrete hex to
// compute against — same source values, kept in sync here.
const CMDR_GOLD_LIGHT = '#d4a006'
const CMDR_GOLD_DARK = '#ffc206'

function isDarkMode(): boolean {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches
}

function activeAccentHex(): string {
  if (lastSystemColor && getSetting('appearance.appColor') === 'system') return lastSystemColor
  return isDarkMode() ? CMDR_GOLD_DARK : CMDR_GOLD_LIGHT
}

function applySystemAccentPreview(hex: string): void {
  document.documentElement.style.setProperty('--color-system-accent', hex)
  lastSystemColor = hex
}

/**
 * Writes the accent-derived tokens (`--color-accent-hover`, `-subtle`, `-text`)
 * as concrete sRGB hex/rgba on `:root`.
 *
 * Why: those tokens are defined in `app.css` as `color-mix(...)` expressions.
 * macOS 12 Monterey still ships with Safari 15.x in the wild, and Tauri's
 * WKWebView tracks the system Safari — `color-mix()` arrived in 16.2,
 * `color-mix(in oklch, …)` in 16.4. On old WebKit a `color-mix()` declaration
 * is invalid → the variable goes unset → the primary-button hover background
 * falls through to a black-looking transparent. Computing the result here
 * gives every user a real color regardless of WebKit version, and the values
 * land before any paint that depends on them.
 *
 * Re-run whenever the accent color or `prefers-color-scheme` changes; the mix
 * shares differ slightly per scheme.
 */
function applyDerivedAccentTokens(): void {
  const root = document.documentElement.style
  const accent = activeAccentHex()
  const dark = isDarkMode()
  // Hover: light lightens by 15% white; dark by 10% (less luminance headroom).
  const hoverWhitePct = dark ? 0.1 : 0.15
  root.setProperty('--color-accent-hover', mixSrgb(accent, '#ffffff', hoverWhitePct))
  // Subtle: same alpha in both schemes.
  root.setProperty('--color-accent-subtle', withAlpha(accent, 0.15))
  // Text-on-bg: light mixes 65% black for ≥4.5:1; dark keeps the bright accent
  // since it already reads well against dark surfaces.
  root.setProperty('--color-accent-text', dark ? accent : mixSrgb(accent, '#000000', 0.65))
}

function applyAccentForCurrentSetting(): void {
  const appColor = getSetting('appearance.appColor')
  if (appColor === 'system' && lastSystemColor) {
    document.documentElement.style.setProperty('--color-accent', lastSystemColor)
    log.debug('Applied system accent color: {hex}', { hex: lastSystemColor })
  } else {
    // Remove inline override: CSS fallback (Cmdr gold) takes effect
    document.documentElement.style.removeProperty('--color-accent')
    log.debug('Removed accent override, using CSS fallback (Cmdr gold)')
  }
  applyDerivedAccentTokens()
}

/**
 * Fetches the system accent color and applies it based on the user's
 * "App color" setting, then listens for both OS and setting changes.
 * Call once on app startup.
 */
export async function initAccentColor(): Promise<void> {
  // Always seed derived tokens from the resting Cmdr gold first, so the
  // primary-button hover and friends have a real value even if the IPC fails
  // or the user is on the Cmdr-gold (non-system) accent.
  applyDerivedAccentTokens()

  // Recompute the derived tokens when the system flips between light and dark.
  // We don't track this for `--color-accent` itself (CSS handles the swap via
  // the dark-mode `@media` block), but the *mix shares* differ per scheme.
  if (typeof window !== 'undefined') {
    darkModeQuery = window.matchMedia('(prefers-color-scheme: dark)')
    darkModeListener = () => applyDerivedAccentTokens()
    darkModeQuery.addEventListener('change', darkModeListener)
  }

  // Load system accent color
  try {
    const hex = await commands.getAccentColor()
    applySystemAccentPreview(hex)
    applyAccentForCurrentSetting()
    log.debug('System accent color loaded: {hex}', { hex })
  } catch (error) {
    log.warn('Could not read system accent color, using CSS fallback: {error}', { error })
  }

  // Listen for OS-level accent color changes
  try {
    unlisten = await listen<string>('accent-color-changed', (event) => {
      applySystemAccentPreview(event.payload)
      applyAccentForCurrentSetting()
      // macOS renders folder icons with the accent color baked in,
      // so we need to flush cached folder bitmaps and re-fetch them.
      void clearDirectoryIconCache()
      log.debug('System accent color changed: {hex}', { hex: event.payload })
    })
  } catch (error) {
    log.warn('Could not subscribe to accent color changes: {error}', { error })
  }

  // Listen for setting changes
  unlistenSetting = onSpecificSettingChange('appearance.appColor', () => {
    applyAccentForCurrentSetting()
    // Flush folder icon cache since accent color affects folder icons
    void clearDirectoryIconCache()
  })
}

/** Cleans up event listeners. */
export function cleanupAccentColor(): void {
  unlisten?.()
  unlisten = undefined
  unlistenSetting?.()
  unlistenSetting = undefined
  if (darkModeQuery && darkModeListener) {
    darkModeQuery.removeEventListener('change', darkModeListener)
  }
  darkModeQuery = undefined
  darkModeListener = undefined
  log.debug('Accent color listeners cleaned up')
}
