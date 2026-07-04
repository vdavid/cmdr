/**
 * The `file.quickLook` handler's archive gate.
 *
 * Quick Look can't preview a file INSIDE an archive — the inner path isn't a real
 * file on disk, so the panel would open blank. The handler no-ops for such a path,
 * and crucially returns BEFORE flipping `quickLookState.isOpen`, so the open/closed
 * state stays consistent (a stale `isOpen: true` would make the next Space press
 * try to close a panel that never opened). F3 (the viewer temp-extract) is the
 * preview path inside a zip.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({
  showInFinder: vi.fn(),
  copyToClipboard: vi.fn(),
  quickLookOpen: vi.fn(() => Promise.resolve()),
  quickLookClose: vi.fn(() => Promise.resolve()),
  getInfo: vi.fn(),
  openInEditor: vi.fn(),
  cloudMakeAvailableOffline: vi.fn(),
  cloudRemoveDownload: vi.fn(),
}))
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPanePath: vi.fn(() => '/x'),
  getFocusedPaneVolumeId: vi.fn(() => 'root'),
}))
vi.mock('$lib/file-explorer/quick-look/quick-look-state.svelte', () => ({
  quickLookState: { isOpen: false },
  quickLookDispatchGuardJustFired: vi.fn(() => false),
  armQuickLookDispatchGuard: vi.fn(),
}))
// `pathInsideArchive` (the gate) stays REAL — that's what we're exercising. Its
// module pulls in the volume store, which needs no data here (pathInsideArchive is
// a pure string check), so a stubbed store keeps the import light.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

import { quickLookOpen } from '$lib/tauri-commands'
import { quickLookState } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import { fileHandlers } from './file-handlers'
import type { CommandHandlerContext } from './types'

/** A handler context whose cursor sits on `path`. */
function ctxAt(path: string): CommandHandlerContext {
  return {
    explorerRef: { getFileAndPathUnderCursor: () => ({ path, filename: path.split('/').pop() ?? path }) },
    ctx: {},
    dispatchArgs: undefined,
  } as unknown as CommandHandlerContext
}

beforeEach(() => {
  vi.clearAllMocks()
  quickLookState.isOpen = false
})

describe('file.quickLook archive gate', () => {
  it('does NOT open Quick Look for a file inside an archive, and leaves isOpen false', async () => {
    await fileHandlers['file.quickLook'](ctxAt('/x/foo.zip/inner.txt'))
    expect(quickLookOpen).not.toHaveBeenCalled()
    expect(quickLookState.isOpen).toBe(false)
  })

  it('opens Quick Look for a normal file (the gate only fires inside archives)', async () => {
    await fileHandlers['file.quickLook'](ctxAt('/x/normal.txt'))
    expect(quickLookOpen).toHaveBeenCalledWith('/x/normal.txt', 'root')
    expect(quickLookState.isOpen).toBe(true)
  })
})
