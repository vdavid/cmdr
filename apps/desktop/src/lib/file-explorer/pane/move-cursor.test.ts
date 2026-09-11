import { describe, it, expect, vi, beforeEach } from 'vitest'
import { moveCursorToTarget, type MoveCursorDeps } from './move-cursor'
import type { FilePaneAPI } from './types'

/** A FilePane stub whose mocks are returned as plain top-level properties
 *  (never read off `ref` itself), so assertions don't trip
 *  `@typescript-eslint/unbound-method` on a typed interface's methods. */
function makePaneRefStub(overrides: Partial<Record<string, ReturnType<typeof vi.fn>>> = {}) {
  const whenLoadSettles = overrides.whenLoadSettles ?? vi.fn().mockResolvedValue(undefined)
  const isInNetworkView = overrides.isInNetworkView ?? vi.fn().mockReturnValue(false)
  const getNetworkItemCount = overrides.getNetworkItemCount ?? vi.fn().mockReturnValue(0)
  const getEffectiveTotalCount = overrides.getEffectiveTotalCount ?? vi.fn().mockReturnValue(10)
  const setCursorIndex = overrides.setCursorIndex ?? vi.fn().mockResolvedValue(undefined)
  const syncStateToMcpNow = overrides.syncStateToMcpNow ?? vi.fn().mockResolvedValue(undefined)
  const ref = {
    whenLoadSettles,
    isInNetworkView,
    getNetworkItemCount,
    getEffectiveTotalCount,
    setCursorIndex,
    syncStateToMcpNow,
  } as unknown as FilePaneAPI
  return { ref, whenLoadSettles, isInNetworkView, getNetworkItemCount, getEffectiveTotalCount, setCursorIndex, syncStateToMcpNow }
}

function makeDeps(paneRef: FilePaneAPI | undefined) {
  const setFocusedPane = vi.fn()
  const moveCursorByName = vi.fn().mockResolvedValue(true)
  const focusContainer = vi.fn()
  const deps: MoveCursorDeps = { getPaneRef: () => paneRef, setFocusedPane, moveCursorByName, focusContainer }
  return { deps, setFocusedPane, moveCursorByName, focusContainer }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('moveCursorToTarget', () => {
  it('focuses the pane before anything else', async () => {
    const pane = makePaneRefStub()
    const { deps, setFocusedPane } = makeDeps(pane.ref)
    await moveCursorToTarget('left', 0, deps)
    expect(setFocusedPane).toHaveBeenCalledWith('left')
  })

  it('throws when the pane is unavailable', async () => {
    const { deps } = makeDeps(undefined)
    await expect(moveCursorToTarget('left', 0, deps)).rejects.toThrow('The left pane is unavailable')
  })

  it('waits for the current load to settle before touching the listing', async () => {
    const pane = makePaneRefStub()
    const { deps } = makeDeps(pane.ref)
    await moveCursorToTarget('left', 0, deps)
    expect(pane.whenLoadSettles).toHaveBeenCalled()
  })

  describe('numeric index', () => {
    it('sets the cursor when the index is in range', async () => {
      const pane = makePaneRefStub({ getEffectiveTotalCount: vi.fn().mockReturnValue(5) })
      const { deps } = makeDeps(pane.ref)
      await moveCursorToTarget('left', 3, deps)
      expect(pane.setCursorIndex).toHaveBeenCalledWith(3)
    })

    it('throws when the index is out of range against the file-listing count', async () => {
      const pane = makePaneRefStub({ getEffectiveTotalCount: vi.fn().mockReturnValue(5) })
      const { deps } = makeDeps(pane.ref)
      await expect(moveCursorToTarget('left', 5, deps)).rejects.toThrow('Index 5 is out of range')
      expect(pane.setCursorIndex).not.toHaveBeenCalled()
    })

    it('throws on a negative index', async () => {
      const pane = makePaneRefStub({ getEffectiveTotalCount: vi.fn().mockReturnValue(5) })
      const { deps } = makeDeps(pane.ref)
      await expect(moveCursorToTarget('left', -1, deps)).rejects.toThrow('Index -1 is out of range')
    })

    it('range-checks against the network row count, not the file-listing count, in a network view', async () => {
      const pane = makePaneRefStub({
        isInNetworkView: vi.fn().mockReturnValue(true),
        getNetworkItemCount: vi.fn().mockReturnValue(2),
        getEffectiveTotalCount: vi.fn().mockReturnValue(500),
      })
      const { deps } = makeDeps(pane.ref)
      // Pre-fix this read getEffectiveTotalCount and let an out-of-range host
      // index through, which the host browser then silently clamped.
      await expect(moveCursorToTarget('left', 2, deps)).rejects.toThrow('Index 2 is out of range')
    })
  })

  describe('filename', () => {
    it('moves to the named entry when found', async () => {
      const pane = makePaneRefStub()
      const { deps, moveCursorByName } = makeDeps(pane.ref)
      await moveCursorToTarget('left', 'report.pdf', deps)
      expect(moveCursorByName).toHaveBeenCalledWith(pane.ref, 'report.pdf')
    })

    it('throws when the name is not found', async () => {
      const pane = makePaneRefStub()
      const { deps, moveCursorByName } = makeDeps(pane.ref)
      moveCursorByName.mockResolvedValueOnce(false)
      await expect(moveCursorToTarget('left', 'missing.pdf', deps)).rejects.toThrow(
        '"missing.pdf" not found in the left pane listing',
      )
    })
  })

  it('re-anchors DOM focus and flushes state to MCP after a successful move', async () => {
    const pane = makePaneRefStub()
    const { deps, focusContainer } = makeDeps(pane.ref)
    await moveCursorToTarget('left', 0, deps)
    expect(focusContainer).toHaveBeenCalled()
    expect(pane.syncStateToMcpNow).toHaveBeenCalled()
  })
})
