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
  ipc: { findFirstFuzzyMatch: vi.fn(), getFileAt: vi.fn(), getIpcErrorMessage: vi.fn((e: unknown) => String(e)) },
}))

vi.mock('$lib/tauri-commands', () => ({
  findFirstFuzzyMatch: ipc.findFirstFuzzyMatch,
  getFileAt: ipc.getFileAt,
}))
vi.mock('$lib/tauri-commands/ipc-types', () => ({ getIpcErrorMessage: ipc.getIpcErrorMessage }))
vi.mock('$lib/logging/logger', () => ({
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
    getIsMtpDeviceOnly: () => false,
    getIncludeHidden: () => true,
    getHasParent: () => true,
    setCursorIndex,
    onSyncMcp,
    ...over,
  }
  const ctl = createTypeToJumpController(deps)
  return { ctl, setCursorIndex, onSyncMcp }
}

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

  it('does not jump when there is no listing / while loading / no backend listing / MTP not connected', () => {
    for (const over of [
      { getListingId: () => '' },
      { getLoading: () => true },
      { getHasBackendListing: () => false },
      { getIsMtpDeviceOnly: () => true },
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
