/**
 * The five recovery / fallback navigation edge-flows, lifted out of
 * `DualPaneExplorer`. Each does its flow-specific async orchestration (resolve
 * the default volume, clear `tab.unreachable`, refresh volumes, re-anchor DOM
 * focus) and routes the actual state change through `navigate({ source:
 * 'fallback' | 'cancel' })`. None carries a direct `saveAppStatus` /
 * `saveTabsForPaneSide` call — the store mutation `navigate()`'s commit makes
 * drives the persistence subscriber (A5).
 *
 * Two byte-for-byte behaviors the fold preserves (see `pane/DETAILS.md` § "The
 * five edge-flow handlers fold onto navigate()"):
 *
 * - **History-push asymmetry.** MTP-fatal / retry / open-home push a history
 *   entry (default `pushHistory`); the volume-unmount redirect does NOT
 *   (`pushHistory: false`), so ejecting a volume can't inject a spurious Back
 *   target. Unmount redirects EACH affected pane independently (left and right).
 * - **Per-source focus.** `shiftsFocus(source)` in `navigate.ts` owns the rule;
 *   the `'fallback'` / `'cancel'` flows only re-anchor DOM focus on the container
 *   (they don't shift the focused pane).
 */

import { getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { getDefaultVolumeId, resolvePathVolume, pathExists, type Location } from '$lib/tauri-commands'
import { requestVolumeRefresh } from '$lib/stores/volume-store.svelte'
import { resolveValidPath } from '../navigation/path-resolution'
import { getAppLogger } from '$lib/logging/logger'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult, NavigateTo } from './navigate'
import type { ReturnPoint } from './return-point'
import type { CancelLoadingPayload, ListingLoad } from './types'

const log = getAppLogger('fileExplorer')

export interface EdgeFlowHandlersDeps {
  navigate: (intent: NavigateIntent) => NavigateResult
  /** The pane's return point while one is in force (`navigate.ts::returnPointFor`). */
  getReturnPoint: (pane: 'left' | 'right') => ReturnPoint | null
  getPaneVolumeId: (pane: 'left' | 'right') => string
  getTabMgr: (pane: 'left' | 'right') => TabManager
  getVolumes: () => VolumeInfo[]
  focusContainer: () => void
}

export interface EdgeFlowHandlers {
  handleCancelLoading: (pane: 'left' | 'right', cancelled: CancelLoadingPayload) => void
  handleMtpFatalError: (pane: 'left' | 'right', errorMessage: string) => Promise<void>
  handleRetryUnreachable: (pane: 'left' | 'right') => Promise<void>
  handleOpenHome: (pane: 'left' | 'right') => Promise<void>
  handleVolumeUnmount: (unmountedId: string) => Promise<void>
}

/**
 * The entry the cursor lands on when a cancel returns to `shown`: the folder the load
 * was opening when it sits right inside, or, for a cancelled re-list of `shown`
 * itself, the entry that load was bringing under the cursor.
 */
function selectionOnReturn(shown: Location, cancelled: ListingLoad): string | undefined {
  if (shown.volumeId !== cancelled.volumeId) return undefined
  if (shown.path === cancelled.path) return cancelled.selectName
  const cut = cancelled.path.lastIndexOf('/')
  const parent = cut <= 0 ? '/' : cancelled.path.slice(0, cut)
  return parent === shown.path ? cancelled.path.slice(cut + 1) : undefined
}

export function createEdgeFlowHandlers(deps: EdgeFlowHandlersDeps): EdgeFlowHandlers {
  /**
   * Escape during a load puts the pane back on what it last showed
   * (`pane/DETAILS.md` § "Escape during a load"): undo the commits that ran ahead of
   * the listing, else re-list the last shown location, else walk up from the
   * cancelled folder, since nothing was shown yet.
   */
  function handleCancelLoading(pane: 'left' | 'right', { cancelled, lastShown }: CancelLoadingPayload): void {
    const point = deps.getReturnPoint(pane)
    if (point) {
      returnFromCancel(pane, { returnTo: point }, selectionOnReturn(point.shown, cancelled))
    } else if (cancelled.volumeId === deps.getPaneVolumeId(pane)) {
      if (lastShown?.volumeId !== cancelled.volumeId) {
        walkUpFrom(pane, cancelled)
        return
      }
      returnFromCancel(pane, { goTo: lastShown }, selectionOnReturn(lastShown, cancelled))
    }
    // Otherwise the load was one the pane had already left (for the Servers hub,
    // say), and stopping it was the whole job.
    deps.focusContainer()
  }

  function returnFromCancel(pane: 'left' | 'right', to: NavigateTo, selectName: string | undefined): void {
    const result = deps.navigate({ pane, to, source: 'cancel', selectName })
    // Nobody awaits the return trip: a listing error there shows in the pane, and a
    // second Escape cancels it.
    if (result.status === 'started') void result.settled.catch(() => {})
  }

  function walkUpFrom(pane: 'left' | 'right', cancelled: ListingLoad): void {
    const parentPath = cancelled.path.substring(0, Math.max(1, cancelled.path.lastIndexOf('/')))
    // Asking the load's own volume: the boot disk says "gone" for a phone's or server's folders.
    const volume = deps.getVolumes().find((v) => v.id === cancelled.volumeId)
    void resolveValidPath(parentPath, { volumeRoot: volume?.path, volumeId: volume?.id }).then((validPath) => {
      const target = validPath ?? '~'
      const isOutsideVolume = cancelled.volumeId !== 'root' && (target === '~' || target === '/')
      // Volume root unreachable ⇒ switch to the root volume; otherwise stay on the
      // pane's volume at the resolved parent. No history push: the walk-up corrects
      // the cancelled destination, it's no new Back target. The subscriber persists
      // the store mutation.
      deps.navigate({
        pane,
        to: {
          selectVolume: { volumeId: isOutsideVolume ? 'root' : deps.getPaneVolumeId(pane), path: target },
        },
        source: 'cancel',
        pushHistory: false,
      })
      deps.focusContainer()
    })
  }

  async function handleMtpFatalError(pane: 'left' | 'right', errorMessage: string): Promise<void> {
    log.warn('{pane} pane MTP fatal error, falling back to default volume: {error}', { pane, error: errorMessage })
    const defaultVolumeId = await getDefaultVolumeId()
    const defaultVolume = deps.getVolumes().find((v) => v.id === defaultVolumeId)
    const defaultPath = defaultVolume?.path ?? '~'

    // Fallback to the default volume, pushing a history entry. The subscriber
    // persists the store mutation `navigate()`'s commit makes.
    deps.navigate({
      pane,
      to: { selectVolume: { volumeId: defaultVolumeId, path: defaultPath } },
      source: 'fallback',
    })
  }

  async function handleRetryUnreachable(pane: 'left' | 'right'): Promise<void> {
    const tab = getActiveTab(deps.getTabMgr(pane))
    if (!tab.unreachable) return

    const originalPath = tab.unreachable.originalPath
    tab.unreachable = { originalPath, retrying: true }

    // Try to resolve the volume via statfs (backend has its own 2s timeout).
    // The resolve-timeout fallback to `getDefaultVolumeId` survives.
    const result = await resolvePathVolume(originalPath)

    const volumeId = result.volume ? result.volume.id : await getDefaultVolumeId()

    // Clear unreachable BEFORE navigating, then commit + refresh (ordering
    // preserved). Let FilePane try to load the directory directly: even if
    // volume resolution timed out, the directory itself may be reachable.
    tab.unreachable = null
    deps.navigate({ pane, to: { selectVolume: { volumeId, path: originalPath } }, source: 'fallback' })

    // Sync the volume selector; retry may have fixed a mount that was stale.
    requestVolumeRefresh()

    log.info('Volume retry navigating to {path} on volume {vol}', {
      path: originalPath,
      vol: volumeId,
    })
  }

  async function handleOpenHome(pane: 'left' | 'right'): Promise<void> {
    const tab = getActiveTab(deps.getTabMgr(pane))
    tab.unreachable = null

    const defaultId = await getDefaultVolumeId()
    const homePath = '~'
    deps.navigate({ pane, to: { selectVolume: { volumeId: defaultId, path: homePath } }, source: 'fallback' })
    log.info('Unreachable tab opened home folder for {pane} pane', { pane })
  }

  async function handleVolumeUnmount(unmountedId: string): Promise<void> {
    const defaultVolumeId = await getDefaultVolumeId()
    // Navigate to home directory, falling back to / if home doesn't exist
    const homePath = (await pathExists('~')) ? '~' : '/'

    // Redirect each affected pane (independently — left and right) to the
    // default volume at home. `pushHistory: false` is the history-push
    // asymmetry: an unmount must NOT grow a Back target (unlike the MTP-fatal /
    // retry / open-home fallbacks, which DO push). The subscriber persists each
    // store mutation `navigate()`'s commit makes.
    for (const pane of ['left', 'right'] as const) {
      if (deps.getPaneVolumeId(pane) === unmountedId) {
        deps.navigate({
          pane,
          to: { selectVolume: { volumeId: defaultVolumeId, path: homePath } },
          source: 'fallback',
          pushHistory: false,
        })
      }
    }

    // Volume list is now maintained reactively by the volume store
  }

  return { handleCancelLoading, handleMtpFatalError, handleRetryUnreachable, handleOpenHome, handleVolumeUnmount }
}
