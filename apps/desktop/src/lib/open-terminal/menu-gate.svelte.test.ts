/**
 * The File menu's "Open terminal here" enabled-state sync.
 *
 * Chrome, not a guard (the module doc says why), so what matters here is that it
 * follows the FOCUSED pane's volume, doesn't chatter, and stops when torn down.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { startOpenTerminalMenuGate } from './menu-gate.svelte'
import { setOpenTerminalHereEnabled } from '$lib/tauri-commands'
import { explorerState } from '$lib/file-explorer/pane/explorer-state.svelte'
import { getActiveTab } from '$lib/file-explorer/tabs/tab-state-manager.svelte'

// Partial: `explorer-state.svelte.ts` reaches for `DEFAULT_VOLUME_ID` from the same
// barrel while building its default tab managers.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/tauri-commands')>()),
  setOpenTerminalHereEnabled: vi.fn(() => Promise.resolve()),
}))

// An empty volume store means every real id falls to the `local` default, which is
// what makes the two device ids below the interesting cases.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

let stop: (() => void) | null = null

/** Points the focused pane's active tab at a volume, the way navigation does. */
function focusVolume(volumeId: string): void {
  getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane())).volumeId = volumeId
  flushSync()
}

beforeEach(() => {
  vi.clearAllMocks()
  explorerState.setFocusedPane('left')
  focusVolume('root')
})

afterEach(() => {
  stop?.()
  stop = null
})

/** Starts the sync and settles the initial push. */
function start(): void {
  stop = startOpenTerminalMenuGate()
  flushSync()
}

describe('what the File menu is told', () => {
  it('starts by enabling the item on a local pane', () => {
    start()

    expect(setOpenTerminalHereEnabled).toHaveBeenCalledWith(true)
  })

  it('greys the item out when the focused pane moves to a phone, and restores it on the way back', () => {
    start()
    vi.mocked(setOpenTerminalHereEnabled).mockClear()

    focusVolume('mtp-1')
    expect(setOpenTerminalHereEnabled).toHaveBeenLastCalledWith(false)

    focusVolume('root')
    expect(setOpenTerminalHereEnabled).toHaveBeenLastCalledWith(true)
  })

  it('greys it out on the network browser and the search-results snapshot', () => {
    start()

    for (const volumeId of ['network', 'search-results']) {
      vi.mocked(setOpenTerminalHereEnabled).mockClear()
      focusVolume(volumeId)
      expect(setOpenTerminalHereEnabled, volumeId).toHaveBeenLastCalledWith(false)
      focusVolume('root')
    }
  })

  it('stays quiet while the verdict is unchanged', () => {
    // Switching tabs inside one volume would otherwise cross IPC on every move.
    start()
    vi.mocked(setOpenTerminalHereEnabled).mockClear()

    focusVolume('some-other-local-drive')
    focusVolume('root')

    expect(setOpenTerminalHereEnabled).not.toHaveBeenCalled()
  })

  it('stops pushing once torn down', () => {
    start()
    stop?.()
    stop = null
    vi.mocked(setOpenTerminalHereEnabled).mockClear()

    focusVolume('mtp-1')

    expect(setOpenTerminalHereEnabled).not.toHaveBeenCalled()
  })
})
