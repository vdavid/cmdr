/**
 * The fire-and-forget arms never leak a rejection.
 *
 * F3 / F5 / F6 open their viewer or dialog without awaiting it, so the gesture
 * dispatcher's `catch` never sees what the opener throws. A pane whose backend
 * listing is gone makes every one of them throw (`Listing … isn't cached`), and
 * before `detached` each press landed in the log as an unhandled rejection.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({ trackEvent: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPanePath: vi.fn(() => '/x'),
  getFocusedPaneVolumeId: vi.fn(() => 'root'),
}))
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

import { fileHandlers } from './file-handlers'
import type { CommandHandlerContext } from './types'

const gone = () => Promise.reject(new Error('Listing 983dd591 isn’t cached'))

function ctxWithGoneListing(): CommandHandlerContext {
  return {
    explorerRef: { openViewerForCursor: gone, openCopyDialog: gone, openMoveDialog: gone },
    ctx: {},
    dispatchArgs: undefined,
  } as unknown as CommandHandlerContext
}

const unhandled: unknown[] = []
const onUnhandled = (reason: unknown) => unhandled.push(reason)

afterEach(() => {
  process.off('unhandledRejection', onUnhandled)
  unhandled.length = 0
})

describe('fire-and-forget file arms', () => {
  it.each(['file.view', 'file.copy', 'file.move'] as const)('%s absorbs the opener’s rejection', async (id) => {
    process.on('unhandledRejection', onUnhandled)
    fileHandlers[id](ctxWithGoneListing())
    // Node reports an unhandled rejection after the microtask queue drains.
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(unhandled).toEqual([])
  })
})
