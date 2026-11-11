/**
 * The macOS 27 Liquid Glass slider (Appearance), mirrored onto `<html>` as the `--glass-tint`
 * variable, `0` clearest to `1` most tinted, which drives the glass fill and blur in `app.css`.
 * The backend re-reads it whenever the app becomes active (`src-tauri/src/system_look/platform/macos/glass_tint.rs`).
 *
 * The other setting that shapes the material, Reduce transparency, arrives as the
 * `reduce-transparency` class from `#lib/display-accessibility`, alongside the other macOS
 * display accessibility options.
 *
 * Follows live OS changes without a restart. Call `initGlassMaterial()` once per window on
 * startup, and `cleanupGlassMaterial()` on teardown.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { getGlassTintAmount, onGlassTintChanged } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('glass-material')

const TINT_VAR = '--glass-tint'

/**
 * The tint when macOS reports none (Linux, pre-27 macOS, or a renamed key): the slider's
 * middle notch, which is also where macOS 27 starts.
 */
export const DEFAULT_GLASS_TINT = 0.5

let unlisteners: UnlistenFn[] = []

function applyTint(amount: number | null): void {
  document.documentElement.style.setProperty(TINT_VAR, String(amount ?? DEFAULT_GLASS_TINT))
}

/**
 * Reads the current tint and applies it, then listens for live OS changes. Safe off macOS:
 * the backend reports no tint there.
 */
export async function initGlassMaterial(): Promise<void> {
  try {
    applyTint(await getGlassTintAmount())
  } catch (error) {
    applyTint(null)
    log.warn('Could not read the Liquid Glass tint, using the default: {error}', { error })
  }

  try {
    unlisteners.push(
      await onGlassTintChanged((payload) => {
        applyTint(payload.amount)
        log.debug('Liquid Glass tint changed: {amount}', { amount: payload.amount })
      }),
    )
  } catch (error) {
    log.warn('Could not subscribe to glass material changes: {error}', { error })
  }
}

/** Cleans up the event listeners. */
export function cleanupGlassMaterial(): void {
  for (const unlisten of unlisteners) unlisten()
  unlisteners = []
}
