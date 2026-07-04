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

import { getCurrentEntry, canGoBack, type NavigationHistory } from '../navigation/navigation-history'
import { getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { getDefaultVolumeId, resolvePathVolume, pathExists } from '$lib/tauri-commands'
import { requestVolumeRefresh } from '$lib/stores/volume-store.svelte'
import { resolveValidPath } from '../navigation/path-resolution'
import { getAppLogger } from '$lib/logging/logger'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'
import type { FilePaneAPI } from './types'

const log = getAppLogger('fileExplorer')

export interface EdgeFlowHandlersDeps {
  navigate: (intent: NavigateIntent) => NavigateResult
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getPaneHistory: (pane: 'left' | 'right') => NavigationHistory
  getPaneVolumeId: (pane: 'left' | 'right') => string
  getTabMgr: (pane: 'left' | 'right') => TabManager
  getVolumes: () => VolumeInfo[]
  focusContainer: () => void
}

export interface EdgeFlowHandlers {
  handleCancelLoading: (pane: 'left' | 'right', cancelledPath: string, selectName?: string) => void
  handleMtpFatalError: (pane: 'left' | 'right', errorMessage: string) => Promise<void>
  handleRetryUnreachable: (pane: 'left' | 'right') => Promise<void>
  handleOpenHome: (pane: 'left' | 'right') => Promise<void>
  handleVolumeUnmount: (unmountedId: string) => Promise<void>
}

export function createEdgeFlowHandlers(deps: EdgeFlowHandlersDeps): EdgeFlowHandlers {
  function handleCancelLoading(pane: 'left' | 'right', cancelledPath: string, selectName?: string): void {
    const history = deps.getPaneHistory(pane)
    const entry = getCurrentEntry(history)
    const paneRef = deps.getPaneRef(pane)

    if (entry.volumeId === 'network') {
      // Network restore: re-commit the network entry without leaving the
      // volume and without a load. A `'fallback'` volume "switch" to the same
      // network volume is a terminal commit (no old-path pre-save, no
      // correction); `pushHistory: false` keeps history put (the entry's
      // already current). The subscriber persists the store mutation.
      deps.navigate({
        pane,
        to: { selectVolume: { volumeId: 'network', path: entry.path } },
        source: 'fallback',
        pushHistory: false,
      })
      paneRef?.setNetworkHost(entry.networkHost ?? null)
      deps.focusContainer()
      return
    }

    if (entry.path === cancelledPath) {
      // Listing completed before cancel; history has the cancelled path pushed. Go back.
      // The history-back walk + commit lives in `navigate()`'s history arm.
      // `navigate()` re-checks `canGoBack`, so the gate here is the
      // cancel-specific guard, not a duplicate.
      if (canGoBack(history)) {
        deps.navigate({ pane, to: { history: 'back' }, source: 'cancel' })
        return
      }

      // Edge case: tab opened directly at this path, no history. Walk up to nearest valid parent.
      const parentPath = entry.path.substring(0, Math.max(1, entry.path.lastIndexOf('/')))
      const volumeRoot = deps.getVolumes().find((v) => v.id === entry.volumeId)?.path
      void resolveValidPath(parentPath, { volumeRoot }).then((validPath) => {
        const target = validPath ?? '~'
        const isOutsideVolume = entry.volumeId !== 'root' && (target === '~' || target === '/')
        // Volume root unreachable ⇒ switch to root volume; otherwise stay on
        // the current volume at the resolved parent. Either way a terminal
        // `'fallback'` commit (no history push — the walk-up is a correction
        // to the cancelled destination, not a new Back target). The
        // subscriber persists the store mutation.
        deps.navigate({
          pane,
          to: {
            selectVolume: { volumeId: isOutsideVolume ? 'root' : deps.getPaneVolumeId(pane), path: target },
          },
          source: 'fallback',
          pushHistory: false,
        })
        deps.focusContainer()
      })
      return
    }

    // Listing didn't complete; history still points at the previous folder (correct destination).
    // setPanePath won't trigger FilePane's $effect (path unchanged), so call navigateToPath directly.
    void paneRef?.navigateToPath(entry.path, selectName)
    deps.focusContainer()
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
