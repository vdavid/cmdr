/**
 * The macOS Accessibility > Display options the UI honors, mirrored onto `<html>` as classes:
 *
 * - **Reduce transparency** → `reduce-transparency`: every translucent material goes opaque
 *   (`app.css` § Reduced transparency).
 * - **Increase contrast** → `increase-contrast`: stronger borders, secondary text, focus rings,
 *   and pane tints (`app.css` § Increase contrast).
 * - **Differentiate without color** → `differentiate-without-color`: the shape cues for the focused
 *   pane (cursor hairlines, tab accent band) step up to 3:1 (`app.css` § Differentiate without color).
 * - **Show borders** → `show-borders`: controls that draw no edge of their own gain one
 *   (`app-utilities.css` § Show borders).
 *
 * Invert colors is read too, but gets no class: it only changes the icons macOS draws, which
 * `#lib/icon-cache` follows on its own.
 *
 * Classes, not media queries: WKWebView never reflects `prefers-reduced-transparency` (verified:
 * with the setting on, AppKit reports `true` while the webview's `matchMedia` reports `false`),
 * and no media query exists for Differentiate without color or Show borders at all. So the Rust backend reads all
 * three from `NSWorkspace` (`src-tauri/src/system_look/platform/macos/display_accessibility.rs`). Increase contrast ALSO
 * honors `prefers-contrast: more`, so a webview that does reflect it (Linux) gets the same class.
 *
 * Follows live OS changes without a restart. Call `initDisplayAccessibility()` once per window on
 * startup, and `cleanupDisplayAccessibility()` on teardown.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { getDisplayAccessibility, onDisplayAccessibilityChanged } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('display-accessibility')

export interface DisplayAccessibilityState {
  reduceTransparency: boolean
  increaseContrast: boolean
  differentiateWithoutColor: boolean
  invertColors: boolean
  showBorders: boolean
}

/** The options mirrored as an `<html>` class. */
type ClassOption = Exclude<keyof DisplayAccessibilityState, 'invertColors'>

const CLASS_BY_OPTION: Record<ClassOption, string> = {
  reduceTransparency: 'reduce-transparency',
  increaseContrast: 'increase-contrast',
  differentiateWithoutColor: 'differentiate-without-color',
  showBorders: 'show-borders',
}

const CONTRAST_QUERY = '(prefers-contrast: more)'

const allOff = (): DisplayAccessibilityState => ({
  reduceTransparency: false,
  increaseContrast: false,
  differentiateWithoutColor: false,
  invertColors: false,
  showBorders: false,
})

let fromOs = allOff()
let contrastMediaMatches = false
let contrastMedia: MediaQueryList | undefined
let unlisten: UnlistenFn | undefined

function onContrastMediaChange(event: { matches: boolean }): void {
  contrastMediaMatches = event.matches
  apply()
}

function apply(): void {
  const classes = document.documentElement.classList
  for (const option of Object.keys(CLASS_BY_OPTION) as ClassOption[]) {
    let on = fromOs[option]
    if (option === 'increaseContrast') on ||= contrastMediaMatches
    classes.toggle(CLASS_BY_OPTION[option], on)
  }
}

/**
 * Reads the current options and applies them, then listens for live OS changes. Safe off macOS:
 * the backend reports every option off there.
 */
export async function initDisplayAccessibility(): Promise<void> {
  if (typeof window.matchMedia === 'function') {
    contrastMedia = window.matchMedia(CONTRAST_QUERY)
    contrastMediaMatches = contrastMedia.matches
    contrastMedia.addEventListener('change', onContrastMediaChange)
  }

  try {
    fromOs = await getDisplayAccessibility()
  } catch (error) {
    log.warn('Could not read the display accessibility options, leaving them off: {error}', { error })
  }
  apply()

  try {
    unlisten = await onDisplayAccessibilityChanged((payload) => {
      fromOs = payload.options
      apply()
      log.debug('Display accessibility changed: {options}', { options: payload.options })
    })
  } catch (error) {
    log.warn('Could not subscribe to display accessibility changes: {error}', { error })
  }
}

/** Cleans up the listeners. */
export function cleanupDisplayAccessibility(): void {
  unlisten?.()
  unlisten = undefined
  contrastMedia?.removeEventListener('change', onContrastMediaChange)
  contrastMedia = undefined
  contrastMediaMatches = false
  fromOs = allOff()
}
