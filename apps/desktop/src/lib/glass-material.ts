/**
 * The two macOS settings that shape our frosted-glass material, mirrored onto `<html>`:
 *
 * - **Reduce transparency** (Accessibility > Display) toggles the `reduce-transparency`
 *   class. WKWebView does NOT reflect the `prefers-reduced-transparency` media query (it
 *   parses the syntax and reflects `prefers-color-scheme`, but never wires this one to the
 *   OS setting — verified: with the setting on, AppKit reports `true` while the webview's
 *   `matchMedia` reports `false`). So we read the real value from the Rust backend
 *   (`NSWorkspace.accessibilityDisplayShouldReduceTransparency`). Every translucent surface
 *   keys its opaque fallback off `html.reduce-transparency` (see `app.css` § Reduced
 *   transparency).
 * - **The Liquid Glass slider** (macOS 27 Appearance) sets the `--glass-tint` variable,
 *   `0` clearest to `1` most tinted, which drives the glass fill and blur in `app.css`.
 *   The backend re-reads it whenever the app becomes active (`src-tauri/src/glass_tint.rs`).
 *
 * Both follow live OS changes without a restart. Call `initGlassMaterial()` once per window
 * on startup, and `cleanupGlassMaterial()` on teardown.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  getGlassTintAmount,
  getShouldReduceTransparency,
  onGlassTintChanged,
  onReduceTransparencyChanged,
} from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('glass-material')

const REDUCE_CLASS = 'reduce-transparency'
const TINT_VAR = '--glass-tint'

/**
 * The tint when macOS reports none (Linux, pre-27 macOS, or a renamed key): the slider's
 * middle notch, which is also where macOS 27 starts.
 */
export const DEFAULT_GLASS_TINT = 0.5

let unlisteners: UnlistenFn[] = []

function applyReduce(reduce: boolean): void {
  document.documentElement.classList.toggle(REDUCE_CLASS, reduce)
}

function applyTint(amount: number | null): void {
  document.documentElement.style.setProperty(TINT_VAR, String(amount ?? DEFAULT_GLASS_TINT))
}

/**
 * Reads both current values and applies them, then listens for live OS changes. Safe off
 * macOS: the backend reports no reduction and no tint there.
 */
export async function initGlassMaterial(): Promise<void> {
  const [reduce, tint] = await Promise.allSettled([getShouldReduceTransparency(), getGlassTintAmount()])
  if (reduce.status === 'fulfilled') applyReduce(reduce.value)
  else
    log.warn('Could not read reduce-transparency setting, leaving transparency on: {error}', { error: reduce.reason })
  if (tint.status === 'fulfilled') applyTint(tint.value)
  else {
    applyTint(null)
    log.warn('Could not read the Liquid Glass tint, using the default: {error}', { error: tint.reason })
  }

  try {
    unlisteners.push(
      await onReduceTransparencyChanged((payload) => {
        applyReduce(payload.reduce)
        log.debug('Reduce transparency changed: {reduce}', { reduce: payload.reduce })
      }),
    )
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
