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
    renameMultiRenamePreset: vi.fn(),
    updateMultiRenamePreset: vi.fn(),
  },
}))
vi.mock('$lib/tauri-commands', () => ipc)

import { PREVIEW_DELAY_MS, createMultiRenameState } from './multi-rename-state.svelte'
import { DEFAULT_SPEC } from './spec'
import type { MultiRenamePreset } from '$lib/tauri-commands'

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

    expect(ipc.getMultiRenamePreviewRows).toHaveBeenCalledWith('S', 7, 4000, 40, 'all')
    expect(tool.rowAt(4039)?.oldName).toBe('f4039')
    expect(tool.rowAt(3999)).toBeUndefined()
    tool.dispose()
  })

  it('is a windowed source for the list: its count, its rows, and the range it pages in', async () => {
    ipc.previewMultiRename.mockResolvedValue(preview(7, [row(0, 'a', 'b')], 5000))
    ipc.getMultiRenamePreviewRows.mockImplementation((_s: string, _p: number, offset: number, limit: number) =>
      Promise.resolve({
        ok: true,
        value: Array.from({ length: limit }, (_, i) => row(offset + i, `f${String(offset + i)}`, 'x')),
      }),
    )
    const tool = createMultiRenameState('S')
    await settle()
    expect(tool.source.count).toBe(5000)
    expect(tool.source.getRow(0)?.newName).toBe('b')

    tool.source.onRangeChange?.({ start: 100, end: 120 })
    await settle()

    expect(ipc.getMultiRenamePreviewRows).toHaveBeenCalledWith('S', 7, 100, 20, 'all')
    expect(tool.source.getRow(119)?.oldName).toBe('f119')
    tool.dispose()
  })

  it('lists the problem rows alone, paged from the backend, and back to every row', async () => {
    const first = [row(0, 'a', 'b'), row(1, 'c', 'd', 'duplicate'), row(2, 'e', 'f', 'targetExists')]
    ipc.previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 7, counts: { ready: 1, unchanged: 0, problems: 2 }, rows: first },
    })
    ipc.getMultiRenamePreviewRows.mockImplementation(
      (_s: string, _p: number, offset: number, limit: number, filter: string) =>
        Promise.resolve({
          ok: true,
          value: (filter === 'problems' ? first.slice(1) : first).slice(offset, offset + limit),
        }),
    )
    const tool = createMultiRenameState('S')
    await settle()
    tool.show({ start: 0, end: 40 })
    await settle()

    tool.setProblemsOnly(true)
    expect(tool.problemsOnly).toBe(true)
    expect(tool.total).toBe(2)
    await settle()
    expect(ipc.getMultiRenamePreviewRows).toHaveBeenLastCalledWith('S', 7, 0, 2, 'problems')
    expect(tool.rowAt(0)?.oldName).toBe('c')
    expect(tool.rowAt(1)?.oldName).toBe('e')

    tool.setProblemsOnly(false)
    expect(tool.total).toBe(3)
    await settle()
    expect(tool.rowAt(0)?.oldName).toBe('a')
    expect(tool.rowAt(2)?.oldName).toBe('e')
    tool.dispose()
  })

  it('a new preview while listing problems pages its problem rows in, never its first page', async () => {
    const first = [row(0, 'a', 'b'), row(1, 'c', 'd', 'duplicate')]
    ipc.previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 7, counts: { ready: 1, unchanged: 0, problems: 1 }, rows: first },
    })
    ipc.getMultiRenamePreviewRows.mockImplementation(() => Promise.resolve({ ok: true, value: [first[1]] }))
    const tool = createMultiRenameState('S')
    await settle()
    tool.setProblemsOnly(true)
    tool.show({ start: 0, end: 40 })
    await settle()

    ipc.previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 8, counts: { ready: 1, unchanged: 0, problems: 1 }, rows: first },
    })
    tool.update({ nameMask: '[N]x' })
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS)
    await settle()

    expect(ipc.getMultiRenamePreviewRows).toHaveBeenLastCalledWith('S', 8, 0, 1, 'problems')
    expect(tool.rowAt(0)?.oldName).toBe('c')
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

  describe('presets', () => {
    const photos: MultiRenamePreset = { id: 'p1', name: 'Photos', spec: { ...DEFAULT_SPEC, nameMask: 'IMG_[C]' } }
    const music: MultiRenamePreset = { id: 'p2', name: 'Music', spec: { ...DEFAULT_SPEC, case: 'lower' } }
    let stored: MultiRenamePreset[] = []

    /** A backend that keeps its list, as `presets.rs` does. */
    beforeEach(() => {
      stored = [photos, music]
      ipc.getMultiRenamePresets.mockImplementation(() => Promise.resolve(stored.map((p) => ({ ...p }))))
      ipc.saveMultiRenamePreset.mockImplementation((preset: MultiRenamePreset) => {
        stored = [preset, ...stored.filter((p) => p.name.toLowerCase() !== preset.name.toLowerCase())]
        return Promise.resolve()
      })
      ipc.renameMultiRenamePreset.mockImplementation((id: string, name: string) => {
        stored = stored.flatMap((p) =>
          p.id === id ? [{ ...p, name }] : p.name.toLowerCase() === name.toLowerCase() ? [] : [p],
        )
        return Promise.resolve()
      })
      ipc.updateMultiRenamePreset.mockImplementation((id: string, spec: MultiRenamePreset['spec']) => {
        stored = stored.map((p) => (p.id === id ? { ...p, spec } : p))
        return Promise.resolve()
      })
      ipc.deleteMultiRenamePreset.mockImplementation((id: string) => {
        stored = stored.filter((p) => p.id !== id)
        return Promise.resolve()
      })
    })

    it('loads a saved preset, and marks it edited only while a field differs from it', async () => {
      const tool = createMultiRenameState('S')
      await tool.loadPresets()
      expect(tool.loaded).toBeNull()
      expect(tool.edited).toBe(false)

      tool.loadPreset({ kind: 'saved', id: 'p1' })
      expect(tool.spec.nameMask).toBe('IMG_[C]')
      expect(tool.loaded).toEqual({ kind: 'saved', id: 'p1' })
      expect(tool.edited).toBe(false)

      tool.update({ nameMask: 'IMG_[C]_x' })
      expect(tool.edited).toBe(true)
      tool.update({ nameMask: 'IMG_[C]' })
      expect(tool.edited).toBe(false)
      tool.dispose()
    })

    it('loads a built-in preset, and Reset all fields leaves nothing loaded', () => {
      const tool = createMultiRenameState('S')
      tool.loadPreset({ kind: 'builtIn', id: 'builtin:remove-diacritics' })
      expect(tool.spec.removeDiacritics).toBe(true)
      expect(tool.loaded).toEqual({ kind: 'builtIn', id: 'builtin:remove-diacritics' })
      tool.update({ case: 'upper' })
      expect(tool.edited).toBe(true)

      tool.resetFields()
      expect(tool.spec).toEqual(DEFAULT_SPEC)
      expect(tool.loaded).toBeNull()
      expect(tool.edited).toBe(false)
      tool.dispose()
    })

    it('saves the current fields as a new preset, which becomes the loaded one', async () => {
      const tool = createMultiRenameState('S')
      await tool.loadPresets()
      tool.update({ search: 'draft' })
      await tool.savePreset('  Drafts ')
      const saved = stored.find((p) => p.name === 'Drafts')
      expect(saved?.spec.search).toBe('draft')
      expect(tool.loaded).toEqual({ kind: 'saved', id: saved?.id })
      expect(tool.edited).toBe(false)
      tool.dispose()
    })

    it('finds a saved preset by name the way the backend does, ignoring case and spaces', async () => {
      const tool = createMultiRenameState('S')
      await tool.loadPresets()
      expect(tool.presetNamed('  photos ')?.id).toBe('p1')
      expect(tool.presetNamed('Videos')).toBeUndefined()
      tool.dispose()
    })

    it('renames a preset in place, and its replacement drops the preset that had the name', async () => {
      const tool = createMultiRenameState('S')
      await tool.loadPresets()
      await tool.renamePreset({ id: 'p2', name: ' photos ' })
      expect(ipc.renameMultiRenamePreset).toHaveBeenCalledWith('p2', 'photos')
      expect(tool.presets.map((p) => p.name)).toEqual(['photos'])
      tool.dispose()
    })

    it('updates a preset with the current fields, which loads it unedited', async () => {
      const tool = createMultiRenameState('S')
      await tool.loadPresets()
      tool.loadPreset({ kind: 'saved', id: 'p1' })
      tool.update({ nameMask: '[N]-[C:3]' })
      expect(tool.edited).toBe(true)

      await tool.updatePreset('p1')
      expect(ipc.updateMultiRenamePreset).toHaveBeenCalledWith('p1', expect.objectContaining({ nameMask: '[N]-[C:3]' }))
      expect(tool.loaded).toEqual({ kind: 'saved', id: 'p1' })
      expect(tool.edited).toBe(false)
      tool.dispose()
    })

    it('deleting the loaded preset leaves nothing loaded, and keeps the fields', async () => {
      const tool = createMultiRenameState('S')
      await tool.loadPresets()
      tool.loadPreset({ kind: 'saved', id: 'p1' })
      await tool.deletePreset('p1')
      expect(tool.loaded).toBeNull()
      expect(tool.spec.nameMask).toBe('IMG_[C]')
      expect(tool.presets.map((p) => p.id)).toEqual(['p2'])
      tool.dispose()
    })
  })
})
