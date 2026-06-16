/**
 * Byte-for-byte parity net for command-handler toast copy (the GAP-2 i18n move).
 *
 * The favorites / tab / cloud / zoom handlers now resolve their toast strings
 * from the `commands.handler.*` catalog (via `tString`) instead of inline
 * literals. This test pins the exact `addToast` text each handler produces to
 * the pre-migration English, so the move stays behavior-preserving.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/downloads/go-to-latest', () => ({ goToLatestDownload: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({
  addFavorite: vi.fn(() => Promise.resolve()),
  cloudMakeAvailableOffline: vi.fn(),
  cloudRemoveDownload: vi.fn(),
  showInFinder: vi.fn(),
  copyToClipboard: vi.fn(),
  quickLookOpen: vi.fn(),
  quickLookClose: vi.fn(),
  getInfo: vi.fn(),
  openInEditor: vi.fn(),
}))
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPanePath: vi.fn(() => '/Users/me/Documents'),
  getFocusedPaneVolumeId: vi.fn(() => 'default'),
}))
vi.mock('$lib/file-explorer/quick-look/quick-look-state.svelte', () => ({
  quickLookState: { isOpen: false },
  quickLookDispatchGuardJustFired: vi.fn(() => false),
  armQuickLookDispatchGuard: vi.fn(),
}))
vi.mock('$lib/settings', () => ({ getSetting: vi.fn(() => 100), setSetting: vi.fn() }))
vi.mock('$lib/shortcuts', () => ({ getEffectiveShortcuts: vi.fn(() => []) }))

import { addToast } from '$lib/ui/toast'
import { addFavorite } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { miscHandlers } from './misc-handlers'
import { tabHandlers } from './tab-handlers'
import { fileHandlers } from './file-handlers'
import { viewHandlers } from './view-handlers'
import type { CommandHandlerContext } from './types'

const mockedToast = vi.mocked(addToast)

function ctxWith(explorerRef: unknown): CommandHandlerContext {
  return { explorerRef, ctx: {}, dispatchArgs: undefined } as unknown as CommandHandlerContext
}

// The zoom preset handlers take no args (they read settings directly).
function callZoom(id: 'view.zoom.set75' | 'view.zoom.set100' | 'view.zoom.set125'): void {
  viewHandlers[id]()
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(getSetting).mockReturnValue(100)
  vi.mocked(getEffectiveShortcuts).mockReturnValue([])
})

describe('command-handler toast copy parity', () => {
  it('favorites.add success names the folder', async () => {
    vi.mocked(addFavorite).mockResolvedValueOnce(undefined)
    await (miscHandlers['favorites.add'] as (h: CommandHandlerContext) => Promise<void>)(ctxWith(undefined))
    expect(mockedToast).toHaveBeenCalledWith('Added "Documents" to favorites', { level: 'success' })
  })

  it('favorites.add failure', async () => {
    vi.mocked(addFavorite).mockRejectedValueOnce(new Error('IPC down'))
    await (miscHandlers['favorites.add'] as (h: CommandHandlerContext) => Promise<void>)(ctxWith(undefined))
    expect(mockedToast).toHaveBeenCalledWith("Couldn't add that folder to favorites. Try again?", { level: 'error' })
  })

  it('tab.new at the limit', () => {
    tabHandlers['tab.new'](ctxWith({ newTab: () => false }))
    expect(mockedToast).toHaveBeenCalledWith('Tab limit reached', { level: 'warn' })
  })

  it('tab.reopen with no closed tabs', () => {
    tabHandlers['tab.reopen'](ctxWith({ reopenLastClosedTab: () => 'empty' }))
    expect(mockedToast).toHaveBeenCalledWith('No recently closed tabs in this pane.', { level: 'warn' })
  })

  it('tab.reopen at the limit', () => {
    tabHandlers['tab.reopen'](ctxWith({ reopenLastClosedTab: () => 'cap' }))
    expect(mockedToast).toHaveBeenCalledWith('Tab limit reached', { level: 'warn' })
  })

  it('cloud.makeOffline failure appends the raw error', async () => {
    const { cloudMakeAvailableOffline } = await import('$lib/tauri-commands')
    vi.mocked(cloudMakeAvailableOffline).mockRejectedValueOnce('boom')
    const hctx = ctxWith({ getFileAndPathUnderCursor: () => ({ path: '/p', filename: 'f' }) })
    await (fileHandlers['cloud.makeOffline'] as (h: CommandHandlerContext) => Promise<void>)(hctx)
    expect(mockedToast).toHaveBeenCalledWith("Couldn't download from cloud. boom", { level: 'error' })
  })

  it('cloud.removeDownload failure appends the raw error', async () => {
    const { cloudRemoveDownload } = await import('$lib/tauri-commands')
    vi.mocked(cloudRemoveDownload).mockRejectedValueOnce('boom')
    const hctx = ctxWith({ getFileAndPathUnderCursor: () => ({ path: '/p', filename: 'f' }) })
    await (fileHandlers['cloud.removeDownload'] as (h: CommandHandlerContext) => Promise<void>)(hctx)
    expect(mockedToast).toHaveBeenCalledWith("Couldn't remove the download. boom", { level: 'error' })
  })

  it('zoom increase with a bound reset shortcut', () => {
    vi.mocked(getSetting).mockReturnValue(100)
    vi.mocked(getEffectiveShortcuts).mockReturnValue(['⌘0'])
    callZoom('view.zoom.set125')
    expect(mockedToast).toHaveBeenCalledWith('Zoom increased to 125%. You can reset the zoom level to 100% by ⌘0.', {
      level: 'info',
      id: 'zoom-change',
    })
  })

  it('zoom decrease with no reset shortcut (menu hint)', () => {
    vi.mocked(getSetting).mockReturnValue(100)
    vi.mocked(getEffectiveShortcuts).mockReturnValue([])
    callZoom('view.zoom.set75')
    expect(mockedToast).toHaveBeenCalledWith(
      'Zoom decreased to 75%. You can reset the zoom level to 100% at View > Zoom > 100%.',
      { level: 'info', id: 'zoom-change' },
    )
  })

  it('zoom reset to 100%', () => {
    vi.mocked(getSetting).mockReturnValue(125)
    callZoom('view.zoom.set100')
    expect(mockedToast).toHaveBeenCalledWith('Zoom reset to 100%.', { level: 'info', id: 'zoom-change' })
  })
})
