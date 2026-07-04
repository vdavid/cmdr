/**
 * macOS "reduce transparency" integration.
 *
 * WKWebView does NOT reflect the `prefers-reduced-transparency` media query (it
 * parses the syntax and reflects `prefers-color-scheme`, but never wires this one
 * to the OS setting — verified: with the setting on, AppKit reports `true` while
 * the webview's `matchMedia` reports `false`). So we read the real value from the
 * Rust backend (`NSWorkspace.accessibilityDisplayShouldReduceTransparency`) and
 * toggle a `reduce-transparency` class on `<html>`. Every translucent surface keys
 * its opaque fallback off `html.reduce-transparency` (see `app.css` § Reduced
 * transparency). The Rust observer emits `reduce-transparency-changed`, so toggling
 * the OS setting updates live without a restart.
 *
 * Call `initReduceTransparency()` once per window on startup, and
 * `cleanupReduceTransparency()` on teardown.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { getShouldReduceTransparency, onReduceTransparencyChanged } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('reduce-transparency')

const CLASS = 'reduce-transparency'

let unlisten: UnlistenFn | undefined

function apply(reduce: boolean): void {
  document.documentElement.classList.toggle(CLASS, reduce)
}

/**
 * Reads the current "reduce transparency" value and applies it, then listens for
 * live OS changes. Safe on non-macOS: the backend command returns `false` there.
 */
export async function initReduceTransparency(): Promise<void> {
  try {
    apply(await getShouldReduceTransparency())
  } catch (error) {
    log.warn('Could not read reduce-transparency setting, leaving transparency on: {error}', { error })
  }

  try {
    unlisten = await onReduceTransparencyChanged((payload) => {
      apply(payload.reduce)
      log.debug('Reduce transparency changed: {reduce}', { reduce: payload.reduce })
    })
  } catch (error) {
    log.warn('Could not subscribe to reduce-transparency changes: {error}', { error })
  }
}

/** Cleans up the event listener. */
export function cleanupReduceTransparency(): void {
  unlisten?.()
  unlisten = undefined
}
