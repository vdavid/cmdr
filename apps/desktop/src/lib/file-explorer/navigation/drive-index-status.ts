// Pure mapping helpers for the per-drive index freshness badge + its menu.
//
// The badge surfaces five visible states (gray/blue/green/yellow/red); the backend
// `VolumeIndexStatus` carries `enabled` + a nullable `freshness`. This module is
// the single source of truth for that mapping and for the badge's tooltip, menu
// items, and footer copy — kept pure (no Svelte, no DOM) so the state→color and
// state→copy contracts are unit-testable without mounting a component.

import type { MessageKey } from '$lib/intl/keys.gen'
import type { Freshness, SmbIndexGateReason, VolumeIndexStatus } from '$lib/ipc/bindings'

/**
 * The five visible badge states. `disabled` is gray (no live index); `failed` is
 * red (the index DB died with a storage error and indexing stopped).
 */
export type DriveIndexState = 'disabled' | 'scanning' | 'fresh' | 'stale' | 'failed'

/**
 * Map a backend status to its visible badge state.
 *
 * `failed` (red) comes FIRST: a failed index is registered but reports
 * `enabled: false` (its writer is torn down), so it must render its own distinct
 * state, not fall through to gray. Gray (`disabled`) is the ABSENCE of a live
 * index: either `enabled: false` or a registered index with no `freshness` yet.
 * Otherwise the `freshness` value maps 1:1 (`scanning`→blue, `fresh`→green,
 * `stale`→yellow).
 */
export function driveIndexState(status: VolumeIndexStatus): DriveIndexState {
  if (status.freshness === 'failed') return 'failed'
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
    case 'failed':
      return 'failed'
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
    // A failed index can't resume in place; `rescan` rebuilds it from scratch (the
    // retry), `forget` deletes its dead DB. No `disable` — there's nothing running.
    case 'failed':
      return ['rescan', 'forget']
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

/** The coalesced-signal note's key plus the numbers its plural branches select on. */
export interface DriveIndexCoalescedNote {
  key: MessageKey
  /** Signals macOS coalesced since the last completed sweep. */
  count: number
  /** Whole hours since that sweep, never below 1. */
  hours: number
  /** Whole hours until the next sweep, never below 1; `null` when none is promised. */
  remaining: number | null
}

const SECONDS_PER_HOUR = 3600

/** Whole hours in a second span, rounded up, never below one. */
function hoursAtLeastOne(seconds: number): number {
  return Math.max(1, Math.ceil(seconds / SECONDS_PER_HOUR))
}

/**
 * The extra tooltip paragraph for a drive where macOS reported it had lost track
 * of file system changes and we deliberately waited instead of rescanning, or
 * `null` when there's nothing to say. Resolving the key is the caller's job (it
 * owns `t()`), keeping this pure.
 *
 * The badge stays GREEN through all of this: once-a-day sweeping is the designed
 * operating state, not a fault, so the transparency lives here rather than in the
 * dot's color.
 *
 * Four deliberate silences:
 * - `count === 0`: nothing was skipped, so the normal tooltip stands alone.
 * - Any state but `fresh`/`stale`: while scanning, the sweep may be the very scan
 *   in flight; disabled and failed have no live index the note could describe.
 * - No `scanCompletedAt`: the count is "since the last completed sweep", so with
 *   no completed scan there's no honest window to name.
 * - No `nextSweepDueAt` (a volume with no daily sweep: an external drive runs a
 *   45-second debounce, which promises nothing), or a sweep already due: the
 *   "next full check in N hours" clause would be a lie, so a variant WITHOUT it
 *   is used, never a zero.
 *
 * Both spans round UP to whole hours with a floor of one, so the tooltip never
 * reads "in the last 0 hours" / "in 0 hours", the window it names always covers
 * what happened, and the wait it promises is never shorter than the real one.
 */
export function driveIndexCoalescedNote(status: VolumeIndexStatus, nowSeconds: number): DriveIndexCoalescedNote | null {
  const count = status.coalescedSignalsSinceSweep
  if (count <= 0) return null

  const state = driveIndexState(status)
  if (state !== 'fresh' && state !== 'stale') return null

  if (status.scanCompletedAt == null) return null
  const hours = hoursAtLeastOne(nowSeconds - status.scanCompletedAt)

  const secondsToSweep = status.nextSweepDueAt != null ? status.nextSweepDueAt - nowSeconds : null
  if (secondsToSweep == null || secondsToSweep <= 0) {
    return { key: 'fileExplorer.navigation.driveIndex.tooltipCoalescedNoNextCheck', count, hours, remaining: null }
  }
  return {
    key: 'fileExplorer.navigation.driveIndex.tooltipCoalesced',
    count,
    hours,
    remaining: hoursAtLeastOne(secondsToSweep),
  }
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
