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

/**
 * Dev override: when `VITE_CMDR_FORCE_OLD_WEBKIT=1` is passed to `pnpm dev`,
 * we pretend `color-mix()` is unsupported even on modern WebKit. This is the
 * way to visually verify the fallback path on your current Mac without
 * tracking down a real Safari 15.x environment. Two effects:
 *
 *  - `hasColorMix` is forced to `false`, which routes the JS branches in
 *    `accent-color.ts` and `volume-tint.svelte.ts` through the sRGB-mix path.
 *  - `data-force-old-webkit` is set on `<html>`, which activates the mirror of
 *    the `@supports not (...)` CSS blocks in `app.css`. Without this, the
 *    CSS-side fallback wouldn't trigger (modern WebKit happily parses
 *    `color-mix()`).
 *
 * Vite only exposes env vars to client code when they're prefixed with `VITE_`.
 * The flag is read at module load, so set it before `pnpm dev` starts.
 */
const FORCE_OLD_WEBKIT = import.meta.env.VITE_CMDR_FORCE_OLD_WEBKIT === '1'

function checkColorMix(): boolean {
  if (FORCE_OLD_WEBKIT) return false
  if (typeof CSS === 'undefined' || typeof CSS.supports !== 'function') return true
  // Universal gate: anything we ship that uses `color-mix(in oklch, …)` also
  // covers the `in srgb` case (oklch is the strictly newer feature).
  return CSS.supports('color', 'color-mix(in oklch, red, blue)')
}

/** True on modern WebKit (Safari 16.4+ / current Chrome/Firefox). */
export const hasColorMix: boolean = checkColorMix()

// Apply the dev override's CSS side as early as possible — before first paint
// would be ideal. Module-level code on a SvelteKit client script runs after
// document parse, so there can be a brief flash of the modern values; that's
// acceptable for a dev-only knob.
if (FORCE_OLD_WEBKIT && typeof document !== 'undefined') {
  document.documentElement.setAttribute('data-force-old-webkit', '')
}

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
