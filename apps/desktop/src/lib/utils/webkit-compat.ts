/**
 * Old-WebKit feature detection. One-shot check at module load.
 *
 * Tauri's WKWebView tracks the system Safari, and the bundle's macOS floor is
 * 10.15 Catalina, so the WebKit under Cmdr can be old in two different ways:
 *
 * - **Degraded but working** (`hasColorMix`). `color-mix()` landed in Safari
 *   16.2 and `color-mix(in oklch, …)` in 16.4. When a declaration fails to
 *   parse the variable is unset and the dependent UI silently loses its color,
 *   so the accent and volume-tint paths mix in JS instead. The matching CSS
 *   fallbacks live in `apps/desktop/src/app.css` under the
 *   `@supports not (color: color-mix(in oklch, red, blue))` blocks.
 * - **Below the floor** (`meetsWebkitFloor`). Four things the app calls
 *   unconditionally arrived in Safari 15.4, and esbuild's `build.target` lowers
 *   syntax, never runtime APIs, so nothing rescues a WebKit without them.
 *
 * The real block for the second case is the inline guard in
 * `apps/desktop/src/app.html`, which runs before this module can be parsed at
 * all. This module is the in-app companion: it powers the runtime tokens
 * written by `accent-color.ts` and `volume-tint.svelte.ts`, the one-shot
 * telemetry log below so affected users show up in error reports, and the
 * "this Mac is below the range we test on" signal.
 *
 * Depth: `DETAILS.md` § srgb-mix.ts / webkit-compat.ts, and
 * `docs/notes/system-requirements-and-es2025.md`.
 */

import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('webkit-compat')

/**
 * Dev override, two levels, so both old-WebKit paths are reachable from a
 * modern Mac without tracking down a real Safari 15.x environment:
 *
 *  - `VITE_CMDR_FORCE_OLD_WEBKIT=1` (or `=old`) pretends `color-mix()` is
 *    unsupported. `hasColorMix` goes `false`, routing `accent-color.ts` and
 *    `volume-tint.svelte.ts` through the sRGB-mix path, and
 *    `data-force-old-webkit` lands on `<html>`, activating the mirror of the
 *    `@supports not (...)` blocks in `app.css` (modern WebKit parses
 *    `color-mix()` happily, so the CSS fallback needs the attribute to fire).
 *  - `VITE_CMDR_FORCE_OLD_WEBKIT=unsupported` pretends the WebKit is below the
 *    Safari 15.4 floor as well. The inline guard in `app.html` reads the same
 *    variable at build time and blocks the app outright, so this level is
 *    mostly about keeping the two answers consistent for anything that runs
 *    before the block paints.
 *
 * Vite only exposes env vars to client code when they're prefixed with `VITE_`.
 * The flag is read at module load, so set it before `pnpm dev` starts.
 */
const FORCE_LEVEL: string = String(import.meta.env.VITE_CMDR_FORCE_OLD_WEBKIT ?? '')
const FORCE_BELOW_FLOOR = FORCE_LEVEL === 'unsupported'
const FORCE_OLD_WEBKIT = FORCE_LEVEL === '1' || FORCE_LEVEL === 'old' || FORCE_BELOW_FLOOR

function checkColorMix(): boolean {
  if (FORCE_OLD_WEBKIT) return false
  if (typeof CSS === 'undefined' || typeof CSS.supports !== 'function') return true
  // Universal gate: anything we ship that uses `color-mix(in oklch, …)` also
  // covers the `in srgb` case (oklch is the strictly newer feature).
  return CSS.supports('color', 'color-mix(in oklch, red, blue)')
}

/** True on modern WebKit (Safari 16.4+ / current Chrome/Firefox). */
export const hasColorMix: boolean = checkColorMix()

/**
 * The Safari 15.4 capabilities the app calls unconditionally, named for the log
 * line and for the test that holds this list and the `app.html` guard together.
 *
 * ❗ Adding one here means adding the matching probe to the inline guard in
 * `apps/desktop/src/app.html` too. Nothing but `app-boot-guard.test.ts` ties the
 * two together: the guard has to be ES5 that runs before any module loads, so it
 * can't import from here.
 */
export const WEBKIT_FLOOR_CAPABILITIES = [
  'crypto.randomUUID',
  'Object.hasOwn',
  'Array.prototype.findLast',
  ':has()',
] as const

function findMissingCapabilities(): string[] {
  if (FORCE_BELOW_FLOOR) return [...WEBKIT_FLOOR_CAPABILITIES]
  const missing: string[] = []
  if (typeof crypto === 'undefined' || typeof crypto.randomUUID !== 'function') {
    missing.push('crypto.randomUUID')
  }
  if (typeof Object.hasOwn !== 'function') missing.push('Object.hasOwn')
  if (typeof Array.prototype.findLast !== 'function') missing.push('Array.prototype.findLast')
  // A webview that can't answer isn't evidence against itself: assume the
  // selector is there rather than blocking on a missing probe.
  if (typeof CSS !== 'undefined' && typeof CSS.supports === 'function' && !CSS.supports('selector(:has(*))')) {
    missing.push(':has()')
  }
  return missing
}

/** Which floor capabilities this WebKit lacks. Empty on anything Safari 15.4 and up. */
export const missingWebkitCapabilities: readonly string[] = findMissingCapabilities()

/**
 * True when this WebKit can run the app at all (Safari 15.4+).
 *
 * In a shipped build this is effectively always true, because the `app.html`
 * guard replaces the page before the bundle loads. It's the in-app answer for
 * the dev override and for anything that wants to reason about the floor
 * without re-probing.
 */
export const meetsWebkitFloor: boolean = missingWebkitCapabilities.length === 0

/**
 * The oldest macOS Cmdr is developed and tested against. The bundle's
 * `minimumSystemVersion` sits lower (10.15), deliberately: 10.15 and 11 are
 * best-effort, and this constant is where "best effort" is defined.
 */
export const SUPPORTED_MACOS_MAJOR = 12

/**
 * True when this Mac is below the range Cmdr is tested on, so the UI can say so.
 *
 * Takes the major version rather than fetching it, which keeps this module free
 * of IPC. Callers pass what `commands.getMacosMajorVersion()` returned; Catalina
 * reports `10`, not `10.15`. A version that didn't parse (`0`, `NaN`) reads as
 * supported: a probe that failed is no reason to warn anybody.
 */
export function isBelowSupportedMacOs(macosMajor: number): boolean {
  if (!Number.isFinite(macosMajor) || macosMajor <= 0) return false
  return macosMajor < SUPPORTED_MACOS_MAJOR
}

/**
 * How to write this Mac's major version in UI copy.
 *
 * `sw_vers -productVersion` reports Catalina as `10.15`, so its major is `10`,
 * and "macOS 10" names a release nobody recognizes. The bundle's
 * `minimumSystemVersion` is 10.15, so a `10` that got as far as running Cmdr
 * can only be 10.15; Big Sur onward each name themselves by the major alone.
 *
 * Returns a string, never a number: a version is a label, and passing it through
 * a locale's number formatting would turn 10.15 into "10,15" in half of Europe.
 */
export function macosVersionLabel(macosMajor: number): string {
  return macosMajor === 10 ? '10.15' : String(macosMajor)
}

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
  if (missingWebkitCapabilities.length > 0) {
    // Reachable in a shipped build only if the `app.html` guard let this
    // through, so it's worth an error line rather than a note.
    log.error(`WebKit below the Safari 15.4 floor, missing: ${missingWebkitCapabilities.join(', ')}`)
  }
  if (hasColorMix) {
    log.debug('WebKit compat OK: color-mix() supported')
  } else {
    log.info(
      'Old WebKit detected: color-mix() unsupported, applying static fallbacks (likely Safari < 16.2 / macOS 12)',
    )
  }
}
