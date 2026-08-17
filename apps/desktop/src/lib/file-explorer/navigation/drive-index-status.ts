// Pure mapping helpers for the per-drive index freshness badge + its menu.
//
// The badge surfaces five visible states (gray/blue/green/yellow/red); the backend
// `VolumeIndexStatus` carries `enabled` + a nullable `freshness`. This module is
// the single source of truth for that mapping and for the badge's tooltip, menu
// items, and footer copy — kept pure (no Svelte, no DOM) so the state→color and
// state→copy contracts are unit-testable without mounting a component.

import type { MessageKey } from '$lib/intl/keys.gen'
import type { EnableIndexingOutcome, Freshness, SmbIndexGateReason, VolumeIndexStatus } from '$lib/ipc/bindings'

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

export function driveIndexMenuActions(state: DriveIndexState, masterEnabled = true): DriveIndexMenuAction[] {
  // The master switch (`indexing.enabled`) outranks every per-drive choice: while
  // it's off nothing can index, so offering per-drive actions would promise work
  // the backend refuses. The menu shows the explanatory note instead. The drive's
  // own choice is untouched and comes back when the master does.
  if (!masterEnabled) return []
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
  /** Whole hours since that sweep, never below 1; `null` while a check is
   *  running (the scan clears the completed-at marker, so there's no honest
   *  window to name, and the variant used then doesn't ask for one). */
  hours: number | null
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
 * While a check is actually RUNNING (blue), the note says so and stops there: the
 * repair the other variants promise for later is happening right now, and the
 * scan cleared `scanCompletedAt` at its start, so there's no honest "in the last
 * N hours" window left to name. Any full walk repairs this drift and resets the
 * count, so the running variant doesn't care which kind of run it is.
 *
 * Four deliberate silences:
 * - `count === 0`: nothing was skipped, so the normal tooltip stands alone.
 * - `disabled` / `failed`: no live index the note could describe.
 * - No `scanCompletedAt` on a settled (green/yellow) drive: the count is "since
 *   the last completed sweep", so there's no honest window to name.
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
  if (state === 'scanning') {
    return {
      key: 'fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning',
      count,
      hours: null,
      remaining: null,
    }
  }
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

/** The footnote a finished index owes when it couldn't read everything. */
export interface DriveIndexUnreadableNote {
  key: MessageKey
  /** How many PLACES, for the plural branch. */
  count: number
}

/**
 * The "done, with holes" footnote, or `null` when there are none.
 *
 * A completed index can hold no rows for folders a walk was refused, ones Cmdr
 * declines to read at all, and ones that stopped answering. Without this line
 * "Indexed 2026-08-15" is quietly untrue for exactly the people least likely to
 * find out another way — someone who never searches never sees search's coverage
 * note, which is the only other place that says it.
 *
 * ❌ A footnote, never a warning: no error styling, no badge colour change,
 * nothing asked of the reader. Whether Cmdr comes back to that ground on its own
 * is the one distinction worth making here, and it picks the wording; WHICH of
 * the three causes it was, and what to do about it, is search's note. A badge
 * tooltip is not where somebody grants Full Disk Access.
 *
 * Only for a finished index: a drive still being covered hasn't reached that
 * ground YET, which is a different sentence and the checklist is already saying
 * it.
 */
export function driveIndexUnreadableNote(status: VolumeIndexStatus): DriveIndexUnreadableNote | null {
  const count = status.unreadableLocations
  if (count <= 0) return null
  const state = driveIndexState(status)
  if (state !== 'fresh' && state !== 'stale') return null
  return {
    key: status.unreadableRetried
      ? 'fileExplorer.navigation.driveIndex.tooltipUnreadableRetried'
      : 'fileExplorer.navigation.driveIndex.tooltipUnreadable',
    count,
  }
}

/** What the badge menu owes the user after an enable or a rescan. */
export type DriveIndexActionFeedback =
  /** The badge itself shows what happened, so a toast would be noise. */
  | { kind: 'silent' }
  /** Say this, at this level. */
  | { kind: 'toast'; key: MessageKey; level: 'info' | 'error' }
  /** A typed SMB refusal the caller routes: `credentials_needed` goes to the
   *  reconnect flow, everything else to `driveIndexRefusalMessageKey`. */
  | { kind: 'refusal'; reason: SmbIndexGateReason }

/**
 * What to tell the user about an enable or rescan, from the TYPED outcome alone.
 *
 * Pure, so the whole contract is unit-testable: the component runs the answer, it
 * doesn't work one out. Two of these matter more than they look:
 *
 * - **Both `deferred_*`** outcomes are promises, not refusals. Something else holds
 *   the drive (a search walking it, or a full walk already running), so the backend
 *   remembers the request and runs it when that holder ends. Silence here is what
 *   made "Rescan now" look like a dead button. They stay apart because the user's
 *   next question differs: one drive is being searched, the other already indexed.
 * - **`status: 'error'`** reaches the caller as a VALUE. `typedError` rethrows only
 *   real `Error` instances, and a Rust `Err(String)` isn't one, so a `catch` never
 *   sees it and an unhandled branch means a click that says nothing at all.
 */
export function driveIndexActionFeedback(
  action: 'enable' | 'rescan',
  result: { status: 'ok'; data: EnableIndexingOutcome } | { status: 'error'; error: string },
): DriveIndexActionFeedback {
  if (result.status === 'error') {
    return { kind: 'toast', key: 'fileExplorer.navigation.driveIndex.refusedGeneric', level: 'error' }
  }
  switch (result.data.status) {
    case 'started':
      return { kind: 'silent' }
    case 'deferred_until_search_ends':
      return {
        kind: 'toast',
        key:
          action === 'enable'
            ? 'fileExplorer.navigation.driveIndex.deferredEnable'
            : 'fileExplorer.navigation.driveIndex.deferredRescan',
        level: 'info',
      }
    // One line for both buttons: the drive is being indexed right now either way,
    // and what the user asked for is next.
    case 'deferred_until_scan_ends':
      return { kind: 'toast', key: 'fileExplorer.navigation.driveIndex.queuedBehindScan', level: 'info' }
    // The master switch went off between the menu opening and the click (or the
    // action came from MCP). Say so rather than leaving a click that quietly did
    // nothing.
    case 'indexing_disabled':
      return { kind: 'toast', key: 'fileExplorer.navigation.driveIndex.refusedIndexingOff', level: 'info' }
    case 'refused':
      return { kind: 'refusal', reason: result.data.reason }
  }
}

/**
 * The toast message key for a typed SMB index refusal, or `null` for
 * `credentials_needed` (which routes into the reconnect/login flow instead of a
 * toast). Branch on the typed variant, never the message string.
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
    // The master switch is off. Not a share problem, so it gets the settings-
    // oriented copy rather than reconnect advice. Normally unreachable from the
    // UI (the menu offers no actions while the master is off), but MCP and a
    // stale open menu can still land here.
    case 'indexing_disabled':
      return 'fileExplorer.navigation.driveIndex.refusedIndexingOff'
  }
}
