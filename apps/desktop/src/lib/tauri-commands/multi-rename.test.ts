/**
 * Tests for the Multi-Rename Tauri command wrappers: typed results pass through
 * as `{ ok, value }` / `{ ok, error }`, and the session, paging, and presets are
 * plain pass-throughs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    openMultiRename: vi.fn(),
    previewMultiRename: vi.fn(),
    getMultiRenamePreviewRows: vi.fn(),
    renderMultiRenameExamples: vi.fn(),
    applyMultiRename: vi.fn(),
    closeMultiRename: vi.fn(),
    getMultiRenamePresets: vi.fn(),
    saveMultiRenamePreset: vi.fn(),
    deleteMultiRenamePreset: vi.fn(),
    renameMultiRenamePreset: vi.fn(),
    updateMultiRenamePreset: vi.fn(),
    getMultiRenameLastSettings: vi.fn(),
    saveMultiRenameLastSettings: vi.fn(),
    writeMultiRenameNames: vi.fn(),
    readMultiRenameNames: vi.fn(),
    clearMultiRenameNames: vi.fn(),
  },
}))

import { commands, type MultiRenameSpec } from '$lib/ipc/bindings'
import {
  applyMultiRename,
  clearMultiRenameNames,
  closeMultiRename,
  deleteMultiRenamePreset,
  getMultiRenameLastSettings,
  getMultiRenamePresets,
  getMultiRenamePreviewRows,
  openMultiRename,
  previewMultiRename,
  readMultiRenameNames,
  renameMultiRenamePreset,
  renderMultiRenameExamples,
  saveMultiRenameLastSettings,
  saveMultiRenamePreset,
  updateMultiRenamePreset,
  writeMultiRenameNames,
} from './multi-rename'

const spec = { nameMask: '[N]', extensionMask: '[E]' } as unknown as MultiRenameSpec

describe('multi-rename wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('opens a session over a selection, or says the selection moved on', async () => {
    vi.mocked(commands.openMultiRename).mockResolvedValueOnce({
      status: 'ok',
      data: { sessionId: 'S', count: 2 },
    } as never)
    expect(await openMultiRename('L', false, [2, 5], 7)).toEqual({ ok: true, value: { sessionId: 'S', count: 2 } })
    expect(commands.openMultiRename).toHaveBeenCalledWith('L', false, [2, 5], 7)

    vi.mocked(commands.openMultiRename).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'selectionChanged', listingId: 'L' },
    } as never)
    expect(await openMultiRename('L', false, null, 7)).toEqual({
      ok: false,
      error: { type: 'selectionChanged', listingId: 'L' },
    })
  })

  it('answers a preview with its first rows, pages the rest, or says why there are none', async () => {
    const rows = [{ row: 0, oldName: 'a', newName: 'b', status: { type: 'ready' } }]
    const answer = { previewId: 1, counts: { ready: 1, unchanged: 0, problems: 0 }, rows }
    vi.mocked(commands.previewMultiRename).mockResolvedValueOnce({ status: 'ok', data: answer } as never)
    expect(await previewMultiRename('S', spec)).toEqual({ ok: true, value: answer })
    expect(commands.previewMultiRename).toHaveBeenCalledWith('S', spec)

    vi.mocked(commands.previewMultiRename).mockResolvedValueOnce({ status: 'error', error: { type: 'gone' } } as never)
    expect(await previewMultiRename('S', spec)).toEqual({ ok: false, error: { type: 'gone' } })

    vi.mocked(commands.getMultiRenamePreviewRows).mockResolvedValueOnce({ status: 'ok', data: rows } as never)
    expect(await getMultiRenamePreviewRows('S', 1, 200, 100, 'problems')).toEqual({ ok: true, value: rows })
    expect(commands.getMultiRenamePreviewRows).toHaveBeenCalledWith('S', 1, 200, 100, 'problems')
  })

  it('starts a rename from the preview the user saw, and reports a refusal', async () => {
    const started = { operationId: 'op', renaming: 1, swapsLeftOut: 0 }
    vi.mocked(commands.applyMultiRename).mockResolvedValueOnce({ status: 'ok', data: started } as never)
    expect(await applyMultiRename('S', 3)).toEqual({ ok: true, value: started })
    expect(commands.applyMultiRename).toHaveBeenCalledWith('S', 3)

    vi.mocked(commands.applyMultiRename).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'previewOutOfDate' },
    } as never)
    expect(await applyMultiRename('S', 3)).toEqual({ ok: false, error: { type: 'previewOutOfDate' } })
  })

  it('writes the Results names, reads them back, and clears them, or says why not', async () => {
    vi.mocked(commands.writeMultiRenameNames).mockResolvedValueOnce({ status: 'ok', data: '/tmp/S.txt' } as never)
    expect(await writeMultiRenameNames('S', 4)).toEqual({ ok: true, value: '/tmp/S.txt' })
    expect(commands.writeMultiRenameNames).toHaveBeenCalledWith('S', 4)

    vi.mocked(commands.readMultiRenameNames).mockResolvedValueOnce({ status: 'ok', data: 2 } as never)
    expect(await readMultiRenameNames('S')).toEqual({ ok: true, value: 2 })
    vi.mocked(commands.readMultiRenameNames).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'namesFileGone', detail: 'x' },
    } as never)
    expect(await readMultiRenameNames('S')).toEqual({ ok: false, error: { type: 'namesFileGone', detail: 'x' } })

    vi.mocked(commands.clearMultiRenameNames).mockResolvedValueOnce({ status: 'ok', data: null } as never)
    expect(await clearMultiRenameNames('S')).toEqual({ ok: true, value: null })
    expect(commands.clearMultiRenameNames).toHaveBeenCalledWith('S')
  })

  it('renames and updates a preset in place, and keeps the last settings', async () => {
    await renameMultiRenamePreset('p', 'New')
    expect(commands.renameMultiRenamePreset).toHaveBeenCalledWith('p', 'New')
    await updateMultiRenamePreset('p', spec)
    expect(commands.updateMultiRenamePreset).toHaveBeenCalledWith('p', spec)

    const last = { spec, preset: { kind: 'saved', id: 'p' } }
    vi.mocked(commands.getMultiRenameLastSettings).mockResolvedValueOnce(last as never)
    expect(await getMultiRenameLastSettings()).toEqual(last)
    await saveMultiRenameLastSettings(spec, null)
    expect(commands.saveMultiRenameLastSettings).toHaveBeenCalledWith(spec, null)
  })

  it('closes the session', async () => {
    await closeMultiRename('S')
    expect(commands.closeMultiRename).toHaveBeenCalledWith('S')
  })

  it('passes presets through', async () => {
    const preset = { id: 'p', name: 'Plain', spec }
    vi.mocked(commands.getMultiRenamePresets).mockResolvedValueOnce([preset] as never)
    expect(await getMultiRenamePresets()).toEqual([preset])
    await saveMultiRenamePreset(preset)
    expect(commands.saveMultiRenamePreset).toHaveBeenCalledWith(preset)
    await deleteMultiRenamePreset('p')
    expect(commands.deleteMultiRenamePreset).toHaveBeenCalledWith('p')
  })

  it('passes the examples through and hands back what the engine rendered', async () => {
    const examples = [{ fileName: 'a.txt', spec: {} as never }]
    vi.mocked(commands.renderMultiRenameExamples).mockResolvedValueOnce(['a.txt', null])
    expect(await renderMultiRenameExamples(examples)).toEqual(['a.txt', null])
    expect(commands.renderMultiRenameExamples).toHaveBeenCalledWith(examples)
  })
})
