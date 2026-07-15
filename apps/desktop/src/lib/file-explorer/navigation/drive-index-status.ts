// Pure mapping helpers for the per-drive index freshness badge + its menu.
//
// The badge surfaces four visible states (gray/blue/green/yellow); the backend
// `VolumeIndexStatus` carries `enabled` + a nullable `freshness`. This module is
// the single source of truth for that mapping and for the badge's tooltip, menu
// items, and footer copy — kept pure (no Svelte, no DOM) so the state→color and
// state→copy contracts are unit-testable without mounting a component.

import type { MessageKey } from '$lib/intl/keys.gen'
import type { Freshness, SmbIndexGateReason, VolumeIndexStatus } from '$lib/ipc/bindings'

/** The four visible badge states. `disabled` is gray (no live index). */
export type DriveIndexState = 'disabled' | 'scanning' | 'fresh' | 'stale'

/**
 * Map a backend status to its visible badge state.
 *
 * Gray (`disabled`) is the ABSENCE of a live index: either `enabled: false` or a
 * registered index that somehow carries no `freshness` yet. Otherwise the
 * `freshness` value maps 1:1 (`scanning`→blue, `fresh`→green, `stale`→yellow).
 */
export function driveIndexState(status: VolumeIndexStatus): DriveIndexState {
  if (!status.enabled || status.freshness == null) return 'disabled'
  return freshnessToState(status.freshness)
}

function freshnessToState(freshness: Freshness): DriveIndexState {
  switch (freshness) {
    case 'scanning':
      return 'scanning'
    case 'fresh':
      return 'fresh'
    case 'stale':
      return 'stale'
  }
}

/** The CSS modifier suffix for a state (`drive-index-badge-{suffix}`). */
export function driveIndexColorClass(state: DriveIndexState): string {
  return state
}

/**
 * The menu actions available for a state, in display order. The menu renders a
 * row per id; `enable`/`rescan`/`disable`/`stop`/`forget` map to the per-drive
 * IPC commands. A `disabled` drive offers only enable; a `scanning` one stop +
 * forget; fresh/stale share rescan + disable + forget. `forget` deletes the
 * drive's index DB outright (vs `disable`, which keeps it on disk to resume);
 * it's the recovery path for an index stuck in a bad state.
 */
export type DriveIndexMenuAction = 'enable' | 'rescan' | 'disable' | 'stop' | 'forget'

export function driveIndexMenuActions(state: DriveIndexState): DriveIndexMenuAction[] {
  switch (state) {
    case 'disabled':
      return ['enable']
    case 'scanning':
      return ['stop', 'forget']
    case 'fresh':
    case 'stale':
      return ['rescan', 'disable', 'forget']
  }
}

/** The catalog key for a menu action's label. */
export function driveIndexMenuLabelKey(action: DriveIndexMenuAction): MessageKey {
  switch (action) {
    case 'enable':
      return 'fileExplorer.navigation.driveIndex.menuEnable'
    case 'rescan':
      return 'fileExplorer.navigation.driveIndex.menuRescan'
    case 'disable':
      return 'fileExplorer.navigation.driveIndex.menuDisable'
    case 'stop':
      return 'fileExplorer.navigation.driveIndex.menuStop'
    case 'forget':
      return 'fileExplorer.navigation.driveIndex.menuForget'
  }
}

/**
 * Format a millisecond scan duration as a friendly string key + params, e.g.
 * "2 min, 14 s" or "14 s". Returns `null` when there's no duration to show.
 * Resolving to text is the caller's job (it owns `t()`), keeping this pure.
 */
export function driveIndexDuration(
  scanDurationMs: number | null,
): { key: MessageKey; params: Record<string, string> } | null {
  if (scanDurationMs == null || scanDurationMs < 0) return null
  const totalSeconds = Math.round(scanDurationMs / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  if (minutes > 0) {
    return {
      key: 'fileExplorer.navigation.driveIndex.durationMinSec',
      params: { minutes: String(minutes), seconds: String(seconds) },
    }
  }
  return {
    key: 'fileExplorer.navigation.driveIndex.durationSec',
    params: { seconds: String(seconds) },
  }
}

/**
 * Whether a state should render the "last indexed … took …" footer/date. Only a
 * fresh index with a recorded completed scan has meaningful last-scan facts.
 */
export function hasLastScanFacts(status: VolumeIndexStatus): boolean {
  return status.scanCompletedAt != null && status.scanDurationMs != null
}

/**
 * The toast message key for a typed SMB index refusal, or `null` for
 * `credentials_needed` (which routes into the reconnect/login flow instead of a
 * toast). Branch on the typed variant, never the message string
 * (`no-string-matching`).
 *
 * `not_registered` / `not_an_smb_volume` map to the INTERNAL-error copy, not
 * reconnect advice: a drive the user can turn indexing on for can't reach those
 * states through a healthy path, so they signal a "shouldn't happen" internal
 * snag rather than something reconnecting would fix. The remaining SMB-specific
 * reasons keep their share-oriented copy.
 */
export function driveIndexRefusalMessageKey(reason: SmbIndexGateReason): MessageKey | null {
  switch (reason) {
    case 'credentials_needed':
      return null
    case 'upgrade_failed':
      return 'fileExplorer.navigation.driveIndex.refusedUpgradeFailed'
    case 'disconnected':
      return 'fileExplorer.navigation.driveIndex.refusedDisconnected'
    case 'not_registered':
    case 'not_an_smb_volume':
      return 'fileExplorer.navigation.driveIndex.refusedInternal'
  }
}
