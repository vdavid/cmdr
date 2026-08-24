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
  // Both the open and the archive refusal report `quick_look_used`.
  trackEvent: vi.fn(() => Promise.resolve()),
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

import { quickLookOpen, trackEvent } from '$lib/tauri-commands'
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

// Quick Look sits behind a gate (an inner-archive path has no real file to
// preview), and a gated feature whose only metric is its last step can't tell
// "nobody uses it" from "everybody is refused". These pin both arms.
describe('file.quickLook reports every outcome', () => {
  /** The `outcome` prop of the single `quick_look_used` event this dispatch produced. */
  function outcome(): unknown {
    const calls = vi.mocked(trackEvent).mock.calls.filter(([name]) => name === 'quick_look_used')
    expect(calls).toHaveLength(1)
    return calls[0][1]?.outcome
  }

  it('reports insideArchive for the refusal, so the gate has a number', async () => {
    await fileHandlers['file.quickLook'](ctxAt('/x/foo.zip/inner.txt'))
    expect(outcome()).toBe('insideArchive')
  })

  it('reports opened for a normal file', async () => {
    await fileHandlers['file.quickLook'](ctxAt('/x/normal.txt'))
    expect(outcome()).toBe('opened')
  })

  it('reports closed when the toggle closes an open panel', async () => {
    quickLookState.isOpen = true
    await fileHandlers['file.quickLook'](ctxAt('/x/normal.txt'))
    expect(outcome()).toBe('closed')
  })

  it('reports noTarget when the cursor sits on nothing', async () => {
    await fileHandlers['file.quickLook']({
      explorerRef: { getFileAndPathUnderCursor: () => undefined },
      ctx: {},
      dispatchArgs: undefined,
    } as unknown as CommandHandlerContext)
    expect(outcome()).toBe('noTarget')
  })
})
