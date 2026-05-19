/**
 * Old-WebKit feature detection. One-shot check at module load.
 *
 * Tauri's WKWebView tracks the system Safari, and macOS 12 Monterey ships
 * with Safari 15.x out of the box. Several CSS features Cmdr leans on landed
 * later (`color-mix()` in 16.2, `color-mix(in oklch, …)` in 16.4). When a
 * declaration fails to parse on old WebKit the variable is unset, and the
 * dependent UI silently loses its color. Code that needs to branch on this
 * imports the boolean from here so the check runs once per session.
 *
 * The matching CSS fallbacks live in `apps/desktop/src/app.css` under the
 * `@supports not (color: color-mix(in oklch, red, blue))` blocks. This module
 * is the JS-side companion: it powers the runtime tokens written by
 * `accent-color.ts` and `volume-tint.svelte.ts`, plus the one-shot telemetry
 * log below so we can spot affected users in error reports.
 */

import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('webkit-compat')

function checkColorMix(): boolean {
  if (typeof CSS === 'undefined' || typeof CSS.supports !== 'function') return true
  // Universal gate: anything we ship that uses `color-mix(in oklch, …)` also
  // covers the `in srgb` case (oklch is the strictly newer feature).
  return CSS.supports('color', 'color-mix(in oklch, red, blue)')
}

/** True on modern WebKit (Safari 16.4+ / current Chrome/Firefox). */
export const hasColorMix: boolean = checkColorMix()

/**
 * Logs WebKit-compatibility flags once at boot so old-WebKit users surface
 * in telemetry / crash reports. Wire from app startup; no-op if already
 * called.
 */
let logged = false
export function logWebkitCompat(): void {
  if (logged) return
  logged = true
  if (hasColorMix) {
    log.debug('WebKit compat OK: color-mix() supported')
  } else {
    log.info(
      'Old WebKit detected: color-mix() unsupported, applying static fallbacks (likely Safari < 16.2 / macOS 12)',
    )
  }
}
