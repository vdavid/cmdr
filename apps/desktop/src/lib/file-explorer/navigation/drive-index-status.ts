// Pure mapping helpers for the per-drive index freshness badge + its menu.
//
// The badge surfaces four visible states (gray/blue/green/yellow); the backend
// `VolumeIndexStatus` carries `enabled` + a nullable `freshness`. This module is
// the single source of truth for that mapping and for the badge's tooltip, menu
// items, and footer copy — kept pure (no Svelte, no DOM) so the state→color and
// state→copy contracts are unit-testable without mounting a component.

import { formatElapsedClock } from '$lib/indexing/elapsed'
import type { MessageKey } from '$lib/intl/keys.gen'
import { formatInteger } from '$lib/intl/number-format'
import type { Freshness, VolumeIndexStatus } from '$lib/ipc/bindings'

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
 * The live "Indexing… N files" tooltip for a scanning badge, as a key + params
 * the caller resolves with `t()`. Folds in elapsed time ("· 0:42") once the
 * scan has been running for at least a second; below that (or with no recorded
 * start) it's count-only, so the clock never flickers a misleading "0:00".
 *
 * Deliberately count + elapsed only, never an ETA: a phone's FIRST scan has no
 * prior calibration to seed one, and a fabricated estimate would mislead.
 *
 * `nowMs` is passed in (not read here) so a component's ticking clock drives the
 * elapsed value while this stays pure and testable.
 */
export function driveIndexScanProgress(
  entriesScanned: number,
  scanStartedAt: number,
  nowMs: number,
): { key: MessageKey; params: Record<string, string | number> } {
  const countText = formatInteger(entriesScanned)
  const elapsed = formatElapsedClock(nowMs - scanStartedAt)
  if (elapsed != null) {
    return {
      key: 'fileExplorer.navigation.driveIndex.tooltipScanningCountElapsed',
      params: { countText, count: entriesScanned, elapsed },
    }
  }
  return {
    key: 'fileExplorer.navigation.driveIndex.tooltipScanningCount',
    params: { countText, count: entriesScanned },
  }
}
