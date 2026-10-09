/**
 * The Multi-Rename sheet's state: the preview follows edits after a short delay,
 * a stale answer never lands, errors keep or clear the rows, the table's window
 * pages in from the backend, presets round-trip, and Start sends the preview it
 * showed and returns the operation or the reason it didn't start.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const { ipc } = vi.hoisted(() => ({
  ipc: {
    previewMultiRename: vi.fn(),
    getMultiRenamePreviewRows: vi.fn(),
    applyMultiRename: vi.fn(),
    getMultiRenamePresets: vi.fn(),
    saveMultiRenamePreset: vi.fn(),
    deleteMultiRenamePreset: vi.fn(),
  },
}))
vi.mock('$lib/tauri-commands', () => ipc)

import { PREVIEW_DELAY_MS, createMultiRenameState } from './multi-rename-state.svelte'

const row = (index: number, oldName: string, newName: string, type = 'ready') => ({
  row: index,
  oldName,
  newName,
  status: { type },
})

/** A preview answer: `rows` is the first page, `total` the whole preview's row count. */
function preview(previewId: number, rows: ReturnType<typeof row>[], total = rows.length) {
  const ready = rows.filter((r) => r.status.type === 'ready').length
  return { ok: true, value: { previewId, counts: { ready, unchanged: total - ready, problems: 0 }, rows } }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) await Promise.resolve()
}

describe('createMultiRenameState', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue(preview(1, [row(0, 'a.txt', 'a.txt', 'unchanged')]))
    ipc.getMultiRenamePresets.mockResolvedValue([])
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('previews right away, then after each edit settles', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(1)
    expect(ipc.previewMultiRename.mock.calls[0][0]).toBe('S')

    tool.update({ nameMask: '[N] x' })
    tool.update({ nameMask: '[N] xy' })
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(2)
    expect((ipc.previewMultiRename.mock.calls[1][1] as { nameMask: string }).nameMask).toBe('[N] xy')
    tool.dispose()
  })

  it('drops an answer a newer edit overtook', async () => {
    let releaseSlow: (v: unknown) => void = () => {}
    ipc.previewMultiRename
      .mockReturnValueOnce(new Promise((r) => (releaseSlow = r)))
      .mockResolvedValueOnce(preview(2, [row(0, 'a.txt', 'new.txt')]))
    const tool = createMultiRenameState('S')
    tool.update({ nameMask: 'new' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    releaseSlow(preview(1, [row(0, 'a.txt', 'old.txt')]))
    await settle()
    expect(tool.rowAt(0)?.newName).toBe('new.txt')
    tool.dispose()
  })

  it('keeps the last good rows under a spec error, and clears them otherwise', async () => {
    const tool = createMultiRenameState('S')
    await settle()
    ipc.previewMultiRename.mockResolvedValueOnce({
      ok: false,
      error: { type: 'spec', error: { type: 'nameMask', error: { type: 'unclosed', at: 0 } } },
    })
    tool.update({ nameMask: '[N' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(tool.error?.type).toBe('spec')
    expect(tool.total).toBe(1)
    expect(tool.rowAt(0)?.oldName).toBe('a.txt')

    ipc.previewMultiRename.mockResolvedValueOnce({ ok: false, error: { type: 'gone', listingId: 'L' } })
    tool.update({ nameMask: '[N]' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    expect(tool.total).toBe(0)
    expect(tool.rowAt(0)).toBeUndefined()
    tool.dispose()
  })

  it('pages in the rows the table scrolls to, from the preview it shows', async () => {
    ipc.previewMultiRename.mockResolvedValue(preview(7, [row(0, 'a', 'a', 'unchanged')], 5000))
    ipc.getMultiRenamePreviewRows.mockImplementation((_s: string, _p: number, offset: number, limit: number) =>
      Promise.resolve({
        ok: true,
        value: Array.from({ length: limit }, (_, i) => row(offset + i, `f${String(offset + i)}`, 'x', 'unchanged')),
      }),
    )
    const tool = createMultiRenameState('S')
    await settle()

    tool.show({ start: 4000, end: 4040 })
    await settle()

    expect(ipc.getMultiRenamePreviewRows).toHaveBeenCalledWith('S', 7, 4000, 40)
    expect(tool.rowAt(4039)?.oldName).toBe('f4039')
    expect(tool.rowAt(3999)).toBeUndefined()
    tool.dispose()
  })

  it('saves a preset under its name, replacing one with the same name', async () => {
    ipc.getMultiRenamePresets.mockResolvedValue([{ id: 'p1', name: 'Bez diakritiky', spec: {} }])
    const tool = createMultiRenameState('S')
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

  it('starts the rename from the preview it showed, and keeps a failed start retryable', async () => {
    ipc.previewMultiRename.mockResolvedValue(preview(3, [row(0, 'ž.txt', 'z.txt')]))
    const tool = createMultiRenameState('S')
    await settle()
    const started = { operationId: 'op1', renaming: 1, swapsLeftOut: 0 }
    ipc.applyMultiRename.mockResolvedValueOnce({ ok: true, value: started })
    expect(await tool.apply()).toEqual(started)
    expect(ipc.applyMultiRename).toHaveBeenCalledWith('S', 3)

    ipc.applyMultiRename.mockResolvedValueOnce({ ok: false, error: { type: 'nothingToRename' } })
    expect(await tool.apply()).toBeNull()
    expect(tool.applyError?.type).toBe('nothingToRename')
    expect(tool.error).toBeNull()
    tool.dispose()
  })

  it('waits for the preview of the last edit before it starts', async () => {
    const tool = createMultiRenameState('S')
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
    const tool = createMultiRenameState('S')
    await settle()
    ipc.applyMultiRename.mockResolvedValueOnce({ ok: false, error: { type: 'previewOutOfDate' } })
    await tool.apply()
    await settle()
    expect(ipc.previewMultiRename).toHaveBeenCalledTimes(2)
    tool.dispose()
  })
})
