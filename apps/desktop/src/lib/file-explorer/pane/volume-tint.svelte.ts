/**
 * Per-pane background tinting by volume type.
 *
 * Three reactive state slots track the current values of
 * `appearance.tint{Local,Smb,Mtp}`. `FilePane.svelte` calls
 * `getPaneTintBg(...)` on every render of its volume info; the call resolves
 * the pane's volume kind and returns the `color-mix(...)` expression (or
 * `null` when the user picked "none" for that kind).
 *
 * The light/dark switch is handled automatically by `--color-bg-primary`
 * in `app.css`. The mix percentage is exposed as `--pane-tint-fg-pct` /
 * `--pane-tint-bg-pct` so dark mode and `prefers-contrast: more` can each
 * dial it up (10% light → 15% dark; 15% light-contrast → 25% dark-contrast)
 * without touching the call sites.
 */

import { getSetting, onSpecificSettingChange, type VolumeTintColor } from '$lib/settings'
import { isMtpVolumeId } from '$lib/mtp/mtp-path-utils'
import type { LocationCategory } from '$lib/file-explorer/types'

let tintLocal = $state<VolumeTintColor>('none')
let tintSmb = $state<VolumeTintColor>('none')
let tintMtp = $state<VolumeTintColor>('none')

let initialized = false
let unsubs: Array<() => void> = []

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
  initialized = true
}

/** Tear down subscriptions (used by tests and on app shutdown). */
export function cleanupVolumeTints(): void {
  for (const u of unsubs) u()
  unsubs = []
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

/**
 * Reactive: returns the `background-color` value for the pane, or `null`
 * when no tint is configured for this volume's kind.
 *
 * The mix percentages flow through CSS variables so the
 * `prefers-contrast: more` override in `app.css` raises the intensity
 * without re-evaluating this function.
 */
export function getPaneTintBg(
  volumeId: string,
  fsType: string | undefined,
  category: LocationCategory | undefined,
): string | null {
  const kind = volumeKindFor(volumeId, fsType, category)
  const tint = tintForKind(kind)
  if (tint === 'none') return null
  return `color-mix(in oklch, var(--color-bg-primary) var(--pane-tint-bg-pct, 90%), var(--color-tint-${tint}) var(--pane-tint-fg-pct, 10%))`
}
