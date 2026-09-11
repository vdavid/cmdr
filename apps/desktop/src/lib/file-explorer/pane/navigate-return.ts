/**
 * The glue between `navigate()` and a pane's return point (`return-point.ts`): the
 * commit wrapper that keeps the point, its lookup, and the `{ returnTo }` arm. Split
 * out of `navigate.ts` for length. The cancel flow they serve: `pane/DETAILS.md` §
 * "Escape during a load".
 */
import { getActiveTab } from '../tabs/tab-state-manager.svelte'
import { pointBeforeCommit, pointInForce, returnIndexIn, type ReturnPoint } from './return-point'
import {
  commit,
  mintToken,
  SETTLED_NOOP,
  type NavigateDeps,
  type NavigateIntent,
  type NavigateResult,
} from './navigate-commit'

/**
 * Runs a commit that moves the pane's state ahead of its listing (a volume switch,
 * a history walk, a background correction) and keeps the pane's return point: the
 * one already in force, else the tab as it stood. A destination that shows without
 * a listing leaves nothing to cancel, so it clears the point.
 */
export function commitAhead(deps: NavigateDeps, pane: 'left' | 'right', apply: () => void): void {
  const before = pointBeforeCommit(getActiveTab(deps.getTabMgr(pane)), returnPointFor(deps, pane))
  apply()
  const tab = getActiveTab(deps.getTabMgr(pane))
  if (deps.volumeHasListing(tab.volumeId)) {
    deps.returnPoints.set(pane, { ...before, ahead: { volumeId: tab.volumeId, path: tab.path } })
  } else {
    deps.returnPoints.delete(pane)
  }
}

/** The pane's return point, while its tab still sits where the commits left it. */
export function returnPointFor(deps: NavigateDeps, pane: 'left' | 'right'): ReturnPoint | null {
  return pointInForce(getActiveTab(deps.getTabMgr(pane)), deps.returnPoints.get(pane))
}

/**
 * The `{ returnTo }` arm: a cancelled load hands the pane back to what it showed
 * before commits ran ahead of its listing. One commit restores the volume, the path,
 * and the history index (no push, no Back walk), and a background correction still
 * resolving for the cancelled switch is dropped so it can't move the pane again. A
 * same-volume return re-lists through the FilePane primitive, which is how
 * `selectName` reaches the cursor; a cross-volume one re-lists from the pane's
 * props, like any volume switch.
 */
export function returnToShown(deps: NavigateDeps, intent: NavigateIntent, point: ReturnPoint): NavigateResult {
  const { pane } = intent
  mintToken(deps, pane)
  deps.correctionGen.value += 1
  deps.returnPoints.delete(pane)
  const leavingVolumeId = deps.getPaneVolumeId(pane)
  const index = returnIndexIn(deps.getPaneHistory(pane), point)
  commit(deps, {
    pane,
    volumeId: point.shown.volumeId,
    path: point.shown.path,
    // An entry a later push truncated away comes back as a fresh one.
    history: index === null ? 'push-entry' : { moveTo: index },
    networkHost: point.entry.networkHost,
  })

  const paneRef = deps.getPaneRef(pane)
  if (point.shown.volumeId === 'network') paneRef?.setNetworkHost(point.entry.networkHost ?? null)
  if (point.shown.volumeId !== leavingVolumeId || !paneRef) return { status: 'started', settled: SETTLED_NOOP }
  return { status: 'started', settled: paneRef.navigateToPath(point.shown.path, intent.selectName) }
}
