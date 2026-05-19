/**
 * Per-pane background tinting by volume type.
 *
 * Three reactive state slots track the current values of
 * `appearance.tint{Local,Smb,Mtp}`. `FilePane.svelte` calls
 * `getPaneTintBg(...)` on every render of its volume info; the call resolves
 * the pane's volume kind and returns either a `color-mix(...)` expression
 * (modern WebKit) or a precomputed sRGB hex (old WebKit), or `null` when the
 * user picked "none" for that kind.
 *
 * Two implementations, picked once at module load:
 * - Modern path: returns the `color-mix(in oklch, ...)` string. CSS evaluates
 *   it at paint time, so `--color-bg-primary`'s dark/light swap and the
 *   `prefers-contrast: more` percentage bump apply automatically.
 * - Old-WebKit path (Safari < 16.2, common on macOS 12 Monterey): `color-mix()`
 *   doesn't parse, so we read the live CSS vars via `getComputedStyle` and mix
 *   in sRGB ourselves, returning a hex string. A reactive `mediaTick` $state
 *   re-fires `$derived` callers when `prefers-color-scheme` or
 *   `prefers-contrast` flips, since CSS no longer re-evaluates for us.
 *
 * The light/dark switch on the modern path is handled automatically by
 * `--color-bg-primary` in `app.css`. Mix percentages flow through
 * `--pane-tint-fg-pct` / `--pane-tint-bg-pct` so dark mode and
 * `prefers-contrast: more` can each dial it up (10% light → 15% dark; 15%
 * light-contrast → 25% dark-contrast) without touching the call sites.
 */

import { getSetting, onSpecificSettingChange, type VolumeTintColor } from '$lib/settings'
import { isMtpVolumeId } from '$lib/mtp/mtp-path-utils'
import type { LocationCategory } from '$lib/file-explorer/types'
import { hasColorMix } from '$lib/utils/webkit-compat'
import { mixSrgb } from '$lib/utils/srgb-mix'

let tintLocal = $state<VolumeTintColor>('none')
let tintSmb = $state<VolumeTintColor>('none')
let tintMtp = $state<VolumeTintColor>('none')
// Reactive trigger so `getPaneTintBg`'s `$derived` callers recompute when the
// color scheme or contrast preference flips. Only meaningful on the old-WebKit
// branch (the modern `color-mix()` string is re-evaluated by CSS at paint).
let mediaTick = $state(0)

let initialized = false
let unsubs: Array<() => void> = []
let mediaListeners: Array<{ mq: MediaQueryList; fn: (e: MediaQueryListEvent) => void }> = []

/** Initialise reactive subscriptions. Call once from app startup. */
export function initVolumeTints(): void {
  if (initialized) return
  tintLocal = getSetting('appearance.tintLocal')
  tintSmb = getSetting('appearance.tintSmb')
  tintMtp = getSetting('appearance.tintMtp')
  unsubs.push(
    onSpecificSettingChange('appearance.tintLocal', (_id, v) => {
      tintLocal = v
    }),
    onSpecificSettingChange('appearance.tintSmb', (_id, v) => {
      tintSmb = v
    }),
    onSpecificSettingChange('appearance.tintMtp', (_id, v) => {
      tintMtp = v
    }),
  )
  // Only the JS-mix branch needs to listen to media changes — the CSS-mix
  // branch re-evaluates the `color-mix(...)` string on every paint already.
  if (!hasColorMix && typeof window !== 'undefined') {
    for (const q of ['(prefers-color-scheme: dark)', '(prefers-contrast: more)']) {
      const mq = window.matchMedia(q)
      const fn = () => {
        mediaTick++
      }
      mq.addEventListener('change', fn)
      mediaListeners.push({ mq, fn })
    }
  }
  initialized = true
}

/** Tear down subscriptions (used by tests and on app shutdown). */
export function cleanupVolumeTints(): void {
  for (const u of unsubs) u()
  unsubs = []
  for (const { mq, fn } of mediaListeners) mq.removeEventListener('change', fn)
  mediaListeners = []
  initialized = false
}

export type VolumeKind = 'local' | 'smb' | 'mtp' | 'other'

/**
 * Pure classifier: pick the tint bucket for a volume.
 *
 * `other` covers favorites and the synthetic "network" browser view, which
 * don't carry a meaningful "this is a real volume" identity. Those panes
 * stay untinted regardless of settings.
 */
export function volumeKindFor(
  volumeId: string,
  fsType: string | undefined,
  category: LocationCategory | undefined,
): VolumeKind {
  if (isMtpVolumeId(volumeId) || category === 'mobile_device') return 'mtp'
  if (category === 'network' || fsType === 'smbfs') return 'smb'
  if (
    volumeId === 'root' ||
    category === 'main_volume' ||
    category === 'attached_volume' ||
    category === 'cloud_drive'
  ) {
    return 'local'
  }
  return 'other'
}

/** Returns the selected tint for a given volume kind (reactive). */
function tintForKind(kind: VolumeKind): VolumeTintColor {
  if (kind === 'local') return tintLocal
  if (kind === 'smb') return tintSmb
  if (kind === 'mtp') return tintMtp
  return 'none'
}

function readCssVar(name: string): string {
  if (typeof window === 'undefined') return ''
  return window.getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}

function parsePercent(value: string, fallback: number): number {
  const match = /^([\d.]+)\s*%$/.exec(value)
  if (!match) return fallback
  return parseFloat(match[1]) / 100
}

/**
 * Old-WebKit JS-side sRGB mix: returns a hex string equivalent to
 * `color-mix(in srgb, --color-bg-primary X%, --color-tint-Y (100-X)%)`.
 *
 * The Cmdr-default uses oklch interpolation, but for the gentle tints we
 * apply (10–25%) the sRGB equivalent is visually indistinguishable, and
 * removing the WebKit-version dependency is worth more than the
 * perceptual-uniformity boost.
 */
function computeTintHex(tint: VolumeTintColor): string | null {
  if (tint === 'none') return null
  const bg = readCssVar('--color-bg-primary')
  const tintHex = readCssVar(`--color-tint-${tint}`)
  if (!bg.startsWith('#') || !tintHex.startsWith('#')) return null
  const fgPct = parsePercent(readCssVar('--pane-tint-fg-pct'), 0.1)
  return mixSrgb(bg, tintHex, fgPct)
}

/**
 * Reactive: returns the `background-color` value for the pane, or `null`
 * when no tint is configured for this volume's kind.
 *
 * On modern WebKit (the default) the returned string is a `color-mix(...)`
 * expression that CSS re-evaluates on every paint, so the dark/light swap
 * and `prefers-contrast: more` apply automatically. On old WebKit
 * (`color-mix()` unsupported), we precompute a hex string via
 * `getComputedStyle` and re-fire reactivity through `mediaTick` whenever a
 * relevant media query flips.
 */
export function getPaneTintBg(
  volumeId: string,
  fsType: string | undefined,
  category: LocationCategory | undefined,
): string | null {
  const kind = volumeKindFor(volumeId, fsType, category)
  const tint = tintForKind(kind)
  if (tint === 'none') return null
  if (hasColorMix) {
    return `color-mix(in oklch, var(--color-bg-primary) var(--pane-tint-bg-pct, 90%), var(--color-tint-${tint}) var(--pane-tint-fg-pct, 10%))`
  }
  // Touch the reactive trigger so $derived callers recompute on media flips.
  void mediaTick
  return computeTintHex(tint)
}
