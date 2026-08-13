/**
 * The native menu's enabled-state sync.
 *
 * Chrome, not a guard (the module doc says why), so what matters here is that it
 * tracks BOTH inputs, doesn't chatter, and treats Ask Cmdr focus as blocking while
 * mere visibility isn't.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { startMenuOperationGate } from './menu-operation-gate.svelte'
import { setFileOperationsBlocked } from '$lib/tauri-commands'
import { markDialogOpen, markDialogClosed, _resetOpenDialogsForTesting } from '$lib/ui/open-dialogs.svelte'
import { explorerState } from '$lib/file-explorer/pane/explorer-state.svelte'

// Partial: `explorer-state.svelte.ts` reaches for `DEFAULT_VOLUME_ID` from the same
// barrel while building its default tab managers.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/tauri-commands')>()),
  setFileOperationsBlocked: vi.fn(() => Promise.resolve()),
}))

let stop: (() => void) | null = null

beforeEach(() => {
  vi.clearAllMocks()
  _resetOpenDialogsForTesting()
  explorerState.setRailFocused(false)
})

afterEach(() => {
  stop?.()
  stop = null
})

/** Starts the sync and settles the initial push. */
function start() {
  stop = startMenuOperationGate()
  flushSync()
}

describe('what the menu is told', () => {
  it('starts by saying nothing is blocked', () => {
    start()

    expect(setFileOperationsBlocked).toHaveBeenCalledWith(false)
  })

  it('greys the items out when a dialog opens, and restores them when it closes', () => {
    start()
    vi.mocked(setFileOperationsBlocked).mockClear()

    markDialogOpen('transfer-progress')
    flushSync()
    expect(setFileOperationsBlocked).toHaveBeenLastCalledWith(true)

    markDialogClosed('transfer-progress')
    flushSync()
    expect(setFileOperationsBlocked).toHaveBeenLastCalledWith(false)
  })

  it('greys them out while the Ask Cmdr composer has focus', () => {
    // Ask Cmdr is not a dialog, so it never reaches the open-dialog set; FOCUS is
    // the whole test, ❌ not visibility. The rail sits docked next to the panes most
    // of the time, and blocking on visibility would take Copy away from anyone who
    // leaves it open.
    start()
    vi.mocked(setFileOperationsBlocked).mockClear()

    explorerState.setRailFocused(true)
    flushSync()
    expect(setFileOperationsBlocked).toHaveBeenLastCalledWith(true)

    explorerState.setRailFocused(false)
    flushSync()
    expect(setFileOperationsBlocked).toHaveBeenLastCalledWith(false)
  })

  it('stays quiet while the verdict is unchanged', () => {
    // Every dialog open and close would otherwise cross IPC, including the ones
    // that change nothing (a second dialog stacking over the first).
    start()
    vi.mocked(setFileOperationsBlocked).mockClear()

    markDialogOpen('transfer-progress')
    flushSync()
    markDialogOpen('rollback-confirmation')
    flushSync()

    expect(setFileOperationsBlocked).toHaveBeenCalledTimes(1)
  })

  it('says nothing for a dialog that lets operations through', () => {
    start()
    vi.mocked(setFileOperationsBlocked).mockClear()

    markDialogOpen('viewer-copy-confirm')
    flushSync()

    expect(setFileOperationsBlocked).not.toHaveBeenCalled()
  })

  it('stops pushing once torn down', () => {
    start()
    stop?.()
    stop = null
    vi.mocked(setFileOperationsBlocked).mockClear()

    markDialogOpen('search')
    flushSync()

    expect(setFileOperationsBlocked).not.toHaveBeenCalled()
  })
})
