/**
 * Tests for `hidden-files-resync.ts`, keeping a pane consistent after the
 * hidden-files toggle changes how many rows the listing has. They pin:
 * - the new total is published before any cursor math runs,
 * - the cursor follows the file it was on, with the `..` row offset applied,
 * - a cursor left past the end is clamped, and only then,
 * - a file that just became hidden falls back to the clamp,
 * - an empty listing puts the cursor at 0 rather than -1.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'

const { ipc } = vi.hoisted<{ ipc: { getTotalCount: Mock; findFileIndex: Mock } }>(() => ({
  ipc: { getTotalCount: vi.fn(), findFileIndex: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ getTotalCount: ipc.getTotalCount, findFileIndex: ipc.findFileIndex }))

import { resyncAfterHiddenFilesToggle } from './hidden-files-resync'

describe('resyncAfterHiddenFilesToggle', () => {
  let setTotalCount: Mock
  let setCursorIndex: Mock

  beforeEach(() => {
    vi.clearAllMocks()
    setTotalCount = vi.fn()
    setCursorIndex = vi.fn().mockResolvedValue(undefined)
    ipc.getTotalCount.mockResolvedValue(10)
    ipc.findFileIndex.mockResolvedValue(null)
  })

  function run(over: Partial<Parameters<typeof resyncAfterHiddenFilesToggle>[0]> = {}) {
    return resyncAfterHiddenFilesToggle({
      listingId: 'listing-1',
      includeHidden: true,
      nameToFollow: undefined,
      cursorIndex: 0,
      getHasParent: () => false,
      setTotalCount,
      setCursorIndex,
      ...over,
    })
  }

  it('publishes the new total count', async () => {
    await run()
    expect(ipc.getTotalCount).toHaveBeenCalledWith('listing-1', true)
    expect(setTotalCount).toHaveBeenCalledWith(10)
  })

  it('keeps the cursor on the same file', async () => {
    ipc.findFileIndex.mockResolvedValue(4)
    await run({ nameToFollow: 'a.txt', cursorIndex: 7 })
    expect(ipc.findFileIndex).toHaveBeenCalledWith('listing-1', 'a.txt', true)
    expect(setCursorIndex).toHaveBeenCalledWith(4)
  })

  it('offsets the found index by the `..` row', async () => {
    ipc.findFileIndex.mockResolvedValue(4)
    await run({ nameToFollow: 'a.txt', getHasParent: () => true })
    expect(setCursorIndex).toHaveBeenCalledWith(5)
  })

  it('leaves a still-valid cursor alone when there is no file to follow', async () => {
    await run({ cursorIndex: 3 })
    expect(setCursorIndex).not.toHaveBeenCalled()
  })

  it('clamps a cursor left past the end', async () => {
    ipc.getTotalCount.mockResolvedValue(3)
    await run({ cursorIndex: 8 })
    expect(setCursorIndex).toHaveBeenCalledWith(2)
  })

  it('counts the `..` row when deciding whether the cursor still fits', async () => {
    ipc.getTotalCount.mockResolvedValue(3)
    await run({ cursorIndex: 3, getHasParent: () => true })
    expect(setCursorIndex).not.toHaveBeenCalled()
  })

  it('clamps when the followed file just became hidden', async () => {
    ipc.getTotalCount.mockResolvedValue(2)
    ipc.findFileIndex.mockResolvedValue(null)
    await run({ nameToFollow: 'hidden.txt', cursorIndex: 5 })
    expect(setCursorIndex).toHaveBeenCalledWith(1)
  })

  it('puts the cursor at 0 on an emptied listing', async () => {
    ipc.getTotalCount.mockResolvedValue(0)
    await run({ cursorIndex: 4 })
    expect(setCursorIndex).toHaveBeenCalledWith(0)
  })
})
