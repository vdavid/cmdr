/**
 * Panes, tabs, and remembered paths following an edit that moved a connected
 * place's root or start folder (`volume-root-changed`).
 *
 * Where a path goes is `../navigation/root-change-follow.ts`; this applies that
 * rule everywhere a path on the place is held, in one order:
 *
 * 1. ❗ The volume store's row moves FIRST (`applyVolumeRootChanged`). The event
 *    beats the debounced `volumes-changed`, and `navigate()` checks a server
 *    target against the row's root, so a pane following a WIDER root would be
 *    refused against the old one.
 * 2. Each pane's active tab moves through `navigate()`, as a terminal
 *    `'fallback'` select with no history push. Following an edit isn't a step
 *    the person took, so it grows no Back target and forks no pinned tab.
 * 3. A tab no pane is showing has nothing to navigate, so its path is rewritten
 *    in place and its pane's tabs are saved.
 * 4. `lastUsedPaths[volumeId]`, so the next switch onto the place lands inside it.
 *
 * Each window's explorer subscribes on its own. The rule is idempotent, so two
 * windows rewriting the one shared remembered path agree.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { VolumeRootChanged } from '$lib/ipc/bindings'
import { onVolumeRootChanged, type Location } from '$lib/tauri-commands'
import { getLastUsedPathForVolume, saveLastUsedPathForVolume } from '$lib/app-status-store'
import { applyVolumeRootChanged } from '$lib/stores/volume-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { pathAfterRootChange } from '../navigation/root-change-follow'
import { getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import type { NavigateIntent, NavigateResult } from './navigate'

const log = getAppLogger('fileExplorer')

export interface VolumeRootFollowDeps {
  /** Moves the volume store's row to the new root and landing. */
  applyToVolumeList: (change: VolumeRootChanged) => void
  getTabMgr: (pane: 'left' | 'right') => TabManager
  navigate: (intent: NavigateIntent) => NavigateResult
  /** Persists one pane's tabs, for a tab no pane is showing. */
  saveTabs: (pane: 'left' | 'right') => void
  getLastUsedPath: (volumeId: string) => Promise<string | undefined>
  saveLastUsedPath: (record: Location) => Promise<void>
}

/** Moves every path held on `change.volumeId` to where the edit put it. */
export async function followVolumeRootChange(change: VolumeRootChanged, deps: VolumeRootFollowDeps): Promise<void> {
  deps.applyToVolumeList(change)

  for (const pane of ['left', 'right'] as const) {
    const mgr = deps.getTabMgr(pane)
    const activeId = getActiveTab(mgr).id
    let movedBehind = false
    for (const tab of mgr.tabs) {
      if (tab.volumeId !== change.volumeId) continue
      const path = pathAfterRootChange(tab.path, change)
      if (path === tab.path) continue
      if (tab.id === activeId) {
        deps.navigate({
          pane,
          to: { selectVolume: { volumeId: change.volumeId, path } },
          source: 'fallback',
          pushHistory: false,
        })
      } else {
        tab.path = path
        // The row it remembered was in a folder the place no longer opens on.
        tab.cursorFilename = null
        movedBehind = true
      }
    }
    if (movedBehind) deps.saveTabs(pane)
  }

  const remembered = await deps.getLastUsedPath(change.volumeId)
  if (remembered === undefined) return
  const path = pathAfterRootChange(remembered, change)
  if (path !== remembered) await deps.saveLastUsedPath({ volumeId: change.volumeId, path })
}

export interface VolumeRootFollow {
  /** Subscribes to `volume-root-changed`. Call once the tab managers exist. */
  init: () => Promise<void>
  cleanup: () => void
}

/**
 * The explorer's subscription. It takes only what the explorer owns; the store
 * and the remembered-path writers are the app's own.
 */
export function createVolumeRootFollow(
  deps: Pick<VolumeRootFollowDeps, 'getTabMgr' | 'navigate' | 'saveTabs'>,
): VolumeRootFollow {
  const full: VolumeRootFollowDeps = {
    ...deps,
    applyToVolumeList: applyVolumeRootChanged,
    getLastUsedPath: getLastUsedPathForVolume,
    saveLastUsedPath: ({ volumeId, path }) => saveLastUsedPathForVolume(volumeId, path),
  }
  let unlisten: UnlistenFn | undefined
  return {
    init: async () => {
      unlisten = await onVolumeRootChanged((change) => {
        followVolumeRootChange(change, full).catch((e: unknown) => {
          log.warn('Following the edited place {volumeId} broke down: {error}', {
            volumeId: change.volumeId,
            error: String(e),
          })
        })
      })
    },
    cleanup: () => {
      unlisten?.()
      unlisten = undefined
    },
  }
}
