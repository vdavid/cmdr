/**
 * The Multi-Rename sheet's state: the preview follows edits after a short delay,
 * a stale answer never lands, errors keep or clear the rows, presets round-trip,
 * and Start returns the operation or the reason it didn't start.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const { ipc } = vi.hoisted(() => ({
  ipc: {
    previewMultiRename: vi.fn(),
    applyMultiRename: vi.fn(),
    getMultiRenamePresets: vi.fn(),
    saveMultiRenamePreset: vi.fn(),
    deleteMultiRenamePreset: vi.fn(),
  },
}))
vi.mock('$lib/tauri-commands', () => ipc)

import { PREVIEW_DELAY_MS, createMultiRenameState } from './multi-rename-state.svelte'

const target = { listingId: 'L', includeHidden: false, rows: [2, 5] }
const ready = (oldName: string, newName: string) => ({ row: 0, oldName, newName, status: { type: 'ready' } })

async function settle(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

describe('createMultiRenameState', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: [ready('a.txt', 'a.txt')] })
    ipc.getMultiRenamePresets.mockResolvedValue([])
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('previews right away, then after each edit settles', async () => {
    const tool = createMultiRenameState(target)
    await settle()
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(1)
    expect(ipc.previewMultiRename.mock.calls[0].slice(0, 3)).toEqual(['L', false, [2, 5]])

    tool.update({ nameMask: '[N] x' })
    tool.update({ nameMask: '[N] xy' })
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(2)
    expect((ipc.previewMultiRename.mock.calls[1][3] as { nameMask: string }).nameMask).toBe('[N] xy')
    tool.dispose()
  })

  it('drops an answer a newer edit overtook', async () => {
    let releaseSlow: (v: unknown) => void = () => {}
    ipc.previewMultiRename
      .mockReturnValueOnce(new Promise((r) => (releaseSlow = r)))
      .mockResolvedValueOnce({ ok: true, value: [ready('a.txt', 'new.txt')] })
    const tool = createMultiRenameState(target)
    tool.update({ nameMask: 'new' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    releaseSlow({ ok: true, value: [ready('a.txt', 'old.txt')] })
    await settle()
    expect(tool.rows[0].newName).toBe('new.txt')
    tool.dispose()
  })

  it('keeps the last good rows under a spec error, and clears them otherwise', async () => {
    const tool = createMultiRenameState(target)
    await settle()
    ipc.previewMultiRename.mockResolvedValueOnce({
      ok: false,
      error: { type: 'spec', error: { type: 'nameMask', error: { type: 'unclosed', at: 0 } } },
    })
    tool.update({ nameMask: '[N' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(tool.error?.type).toBe('spec')
    expect(tool.rows).toHaveLength(1)

    ipc.previewMultiRename.mockResolvedValueOnce({ ok: false, error: { type: 'gone', listingId: 'L' } })
    tool.update({ nameMask: '[N]' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(tool.rows).toHaveLength(0)
    tool.dispose()
  })

  it('saves a preset under its name, replacing one with the same name', async () => {
    ipc.getMultiRenamePresets.mockResolvedValue([{ id: 'p1', name: 'Bez diakritiky', spec: {} }])
    const tool = createMultiRenameState(target)
    await tool.loadPresets()
    tool.update({ removeDiacritics: true })
    await tool.savePreset('  bez DIAKRITIKY ')
    expect(ipc.saveMultiRenamePreset).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'p1',
        name: 'bez DIAKRITIKY',
        spec: expect.objectContaining({ removeDiacritics: true }) as unknown,
      }),
    )
    tool.dispose()
  })

  it('starts the rename with what the user saw, and keeps a failed start retryable', async () => {
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: [ready('ž.txt', 'z.txt')] })
    const tool = createMultiRenameState(target)
    await settle()
    ipc.applyMultiRename.mockResolvedValueOnce({ ok: true, value: { operationId: 'op1', renaming: 1 } })
    expect(await tool.apply()).toEqual({ operationId: 'op1', renaming: 1 })
    expect(ipc.applyMultiRename.mock.calls[0][4]).toEqual([{ row: 0, oldName: 'ž.txt', newName: 'z.txt' }])

    ipc.applyMultiRename.mockResolvedValueOnce({ ok: false, error: { type: 'nothingToRename' } })
    expect(await tool.apply()).toBeNull()
    expect(tool.applyError?.type).toBe('nothingToRename')
    expect(tool.error).toBeNull()
    tool.dispose()
  })

  it('waits for the preview of the last edit before it starts', async () => {
    const tool = createMultiRenameState(target)
    await settle()
    tool.update({ nameMask: 'new' })
    expect(tool.pending).toBe(true)
    expect(await tool.apply()).toBeNull()
    expect(ipc.applyMultiRename).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(tool.pending).toBe(false)
    tool.dispose()
  })

  it('re-previews when the folder changed since the preview', async () => {
    const tool = createMultiRenameState(target)
    await settle()
    ipc.applyMultiRename.mockResolvedValueOnce({ ok: false, error: { type: 'previewOutOfDate' } })
    await tool.apply()
    await settle()
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(2)
    tool.dispose()
  })
})
