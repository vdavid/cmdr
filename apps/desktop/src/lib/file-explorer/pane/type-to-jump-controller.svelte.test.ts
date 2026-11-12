/**
 * Tests for `type-to-jump-controller.svelte.ts`, the pane's buffer + fuzzy-match
 * runner. They pin:
 * - a keystroke fires the match and moves the cursor with the `..` offset applied,
 * - the input gates (no listing / loading / no backend listing / MTP not-connected),
 * - a match arriving after the buffer was cleared is dropped,
 * - a null backend match leaves the cursor put,
 * - `clearJumpState` resets the last-matched name and syncs MCP.
 *
 * Uses the real `createTypeToJumpState` (buffer/timer state), mocking only the
 * IPC. The factory holds `$state` but no `$effect`, so it's driven directly.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { ipc } = vi.hoisted(() => ({
  ipc: { findFirstFuzzyMatch: vi.fn(), getFileAt: vi.fn() },
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  findFirstFuzzyMatch: ipc.findFirstFuzzyMatch,
  getFileAt: ipc.getFileAt,
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createTypeToJumpController, type TypeToJumpControllerDeps } from './type-to-jump-controller.svelte'

function setup(over: Partial<TypeToJumpControllerDeps> = {}) {
  const setCursorIndex = vi.fn()
  const onSyncMcp = vi.fn()
  const deps: TypeToJumpControllerDeps = {
    getResetMs: () => 1000,
    getListingId: () => 'listing-1',
    getLoading: () => false,
    getHasBackendListing: () => true,
    getInMemoryJump: () => null,
    getIncludeHidden: () => true,
    getHasParent: () => true,
    setCursorIndex,
    onSyncMcp,
    ...over,
  }
  const ctl = createTypeToJumpController(deps)
  return { ctl, setCursorIndex, onSyncMcp }
}

describe('createTypeToJumpController held pattern', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.findFirstFuzzyMatch.mockResolvedValue(null)
    ipc.getFileAt.mockResolvedValue(null)
  })

  it('lowercases a pattern from the filter, since an uppercase query would turn the fuzzy match case-sensitive', () => {
    const { ctl } = setup()
    ctl.hold('Re')
    expect(ctl.buffer).toBe('re')
  })

  it('leaves the cursor where it is until the pattern changes', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(2)
    const { ctl, setCursorIndex } = setup()
    ctl.hold('re')
    expect(ipc.findFirstFuzzyMatch).not.toHaveBeenCalled()

    ctl.handleJumpKeystroke('p')
    expect(ipc.findFirstFuzzyMatch).toHaveBeenCalledWith('listing-1', 'rep', true)
    await vi.waitFor(() => {
      expect(setCursorIndex).toHaveBeenCalledWith(3)
    })
  })

  it('keeps the pattern and the matched name through clearJumpState, which every reset trigger calls', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(0)
    ipc.getFileAt.mockResolvedValue({ name: 'report.txt' })
    const { ctl } = setup()
    ctl.hold('r')
    ctl.handleJumpKeystroke('e')
    await vi.waitFor(() => {
      expect(ctl.lastMatchedName).toBe('report.txt')
    })
    ctl.clearJumpState()
    ctl.clear()
    expect(ctl.buffer).toBe('re')
    expect(ctl.lastMatchedName).toBe('report.txt')
  })

  it('release drops the pattern and the matched name', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(0)
    ipc.getFileAt.mockResolvedValue({ name: 'report.txt' })
    const { ctl } = setup()
    ctl.hold('r')
    ctl.handleJumpKeystroke('e')
    await vi.waitFor(() => {
      expect(ctl.lastMatchedName).toBe('report.txt')
    })
    ctl.release()
    expect(ctl.buffer).toBe('')
    expect(ctl.indicatorVisible).toBe(false)
    expect(ctl.lastMatchedName).toBeNull()
  })

  it('canMatch says whether there is anything to jump within', () => {
    expect(setup().ctl.canMatch()).toBe(true)
    expect(setup({ getLoading: () => true }).ctl.canMatch()).toBe(false)
    expect(setup({ getHasBackendListing: () => false }).ctl.canMatch()).toBe(false)
    expect(setup({ getHasBackendListing: () => false, getInMemoryJump: () => () => null }).ctl.canMatch()).toBe(true)
  })
})

describe('createTypeToJumpController over a list in memory', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  const networkView = (match: (prefix: string) => string | null) => ({
    getListingId: () => '',
    getHasBackendListing: () => false,
    getInMemoryJump: () => match,
  })

  it('asks the view with the whole buffer, never the backend, and shows the indicator', () => {
    const match = vi.fn((prefix: string) => (prefix === 'n' || prefix === 'na' ? 'Testnas' : null))
    const { ctl } = setup(networkView(match))
    ctl.handleJumpKeystroke('N')
    ctl.handleJumpKeystroke('a')
    expect(match.mock.calls).toEqual([['n'], ['na']])
    expect(ipc.findFirstFuzzyMatch).not.toHaveBeenCalled()
    expect(ctl.indicatorVisible).toBe(true)
    expect(ctl.buffer).toBe('na')
    expect(ctl.lastMatchedName).toBe('Testnas')
  })

  it('keeps the last match when a longer buffer matches nothing', () => {
    const { ctl } = setup(networkView((prefix) => (prefix === 'n' ? 'Testnas' : null)))
    ctl.handleJumpKeystroke('n')
    ctl.handleJumpKeystroke('z')
    expect(ctl.buffer).toBe('nz')
    expect(ctl.lastMatchedName).toBe('Testnas')
  })

  it('a view with no rows gets no buffer and no indicator', () => {
    const { ctl } = setup({ getListingId: () => '', getHasBackendListing: () => false })
    ctl.handleJumpKeystroke('n')
    expect(ctl.indicatorVisible).toBe(false)
    expect(ctl.buffer).toBe('')
  })
})

describe('createTypeToJumpController', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.findFirstFuzzyMatch.mockResolvedValue(null)
    ipc.getFileAt.mockResolvedValue(null)
  })

  it('a keystroke fires the match and moves the cursor with the parent-row offset', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(2)
    ipc.getFileAt.mockResolvedValue({ name: 'foo.txt' })
    const { ctl, setCursorIndex, onSyncMcp } = setup({ getHasParent: () => true })
    ctl.handleJumpKeystroke('f')
    expect(ipc.findFirstFuzzyMatch).toHaveBeenCalledWith('listing-1', 'f', true)
    await vi.waitFor(() => {
      expect(setCursorIndex).toHaveBeenCalledWith(3)
    })
    await vi.waitFor(() => {
      expect(ctl.lastMatchedName).toBe('foo.txt')
    })
    expect(onSyncMcp).toHaveBeenCalled()
    ctl.dispose()
  })

  it('applies no offset when the pane has no parent row', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(2)
    const { ctl, setCursorIndex } = setup({ getHasParent: () => false })
    ctl.handleJumpKeystroke('f')
    await vi.waitFor(() => {
      expect(setCursorIndex).toHaveBeenCalledWith(2)
    })
    ctl.dispose()
  })

  it('leaves the cursor put on a null backend match', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(null)
    const { ctl, setCursorIndex } = setup()
    ctl.handleJumpKeystroke('z')
    await vi.waitFor(() => {
      expect(ipc.findFirstFuzzyMatch).toHaveBeenCalled()
    })
    await Promise.resolve()
    expect(setCursorIndex).not.toHaveBeenCalled()
    ctl.dispose()
  })

  it('does not jump when there is no listing / while loading / no backend listing', () => {
    for (const over of [
      { getListingId: () => '' },
      { getLoading: () => true },
      { getHasBackendListing: () => false },
    ] as Partial<TypeToJumpControllerDeps>[]) {
      vi.clearAllMocks()
      const { ctl } = setup(over)
      ctl.handleJumpKeystroke('a')
      expect(ctl.isJumpActive()).toBe(false)
      expect(ipc.findFirstFuzzyMatch).not.toHaveBeenCalled()
      ctl.dispose()
    }
  })

  it('drops a match that resolves after the buffer was cleared', async () => {
    let resolveMatch!: (v: number | null) => void
    ipc.findFirstFuzzyMatch.mockReturnValue(new Promise<number | null>((r) => (resolveMatch = r)))
    const { ctl, setCursorIndex } = setup()
    ctl.handleJumpKeystroke('f')
    ctl.clear() // buffer -> '' before the match resolves
    resolveMatch(2)
    await Promise.resolve()
    await Promise.resolve()
    expect(setCursorIndex).not.toHaveBeenCalled()
    ctl.dispose()
  })

  it('clearJumpState resets the last-matched name and syncs MCP', async () => {
    ipc.findFirstFuzzyMatch.mockResolvedValue(0)
    ipc.getFileAt.mockResolvedValue({ name: 'bar' })
    const { ctl, onSyncMcp } = setup({ getHasParent: () => false })
    ctl.handleJumpKeystroke('b')
    await vi.waitFor(() => {
      expect(ctl.lastMatchedName).toBe('bar')
    })
    onSyncMcp.mockClear()
    ctl.clearJumpState()
    expect(ctl.lastMatchedName).toBeNull()
    expect(onSyncMcp).toHaveBeenCalled()
    ctl.dispose()
  })
})
