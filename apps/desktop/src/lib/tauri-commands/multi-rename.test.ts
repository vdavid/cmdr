/**
 * Tests for the Multi-Rename Tauri command wrappers: typed results pass through
 * as `{ ok, value }` / `{ ok, error }`, and presets are plain pass-throughs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    previewMultiRename: vi.fn(),
    applyMultiRename: vi.fn(),
    getMultiRenamePresets: vi.fn(),
    saveMultiRenamePreset: vi.fn(),
    deleteMultiRenamePreset: vi.fn(),
  },
}))

import { commands, type MultiRenameSpec } from '$lib/ipc/bindings'
import {
  applyMultiRename,
  deleteMultiRenamePreset,
  getMultiRenamePresets,
  previewMultiRename,
  saveMultiRenamePreset,
} from './multi-rename'

const spec = { nameMask: '[N]', extensionMask: '[E]' } as unknown as MultiRenameSpec

describe('multi-rename wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('answers a preview with its rows, or with why there are none', async () => {
    const rows = [{ row: 0, oldName: 'a', newName: 'b', status: { type: 'ready' } }]
    vi.mocked(commands.previewMultiRename).mockResolvedValueOnce({ status: 'ok', data: rows } as never)
    expect(await previewMultiRename('L', false, null, spec)).toEqual({ ok: true, value: rows })
    expect(commands.previewMultiRename).toHaveBeenCalledWith('L', false, null, spec)

    vi.mocked(commands.previewMultiRename).mockResolvedValueOnce({ status: 'error', error: { type: 'gone' } } as never)
    expect(await previewMultiRename('L', false, [2, 0], spec)).toEqual({ ok: false, error: { type: 'gone' } })
  })

  it('starts a rename with what the user saw, and reports a refusal', async () => {
    const expected = [{ row: 0, oldName: 'a', newName: 'b' }]
    vi.mocked(commands.applyMultiRename).mockResolvedValueOnce({
      status: 'ok',
      data: { operationId: 'op', renaming: 1 },
    } as never)
    expect(await applyMultiRename('L', true, null, spec, expected)).toEqual({
      ok: true,
      value: { operationId: 'op', renaming: 1 },
    })
    expect(commands.applyMultiRename).toHaveBeenCalledWith('L', true, null, spec, expected)

    vi.mocked(commands.applyMultiRename).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'previewOutOfDate' },
    } as never)
    expect(await applyMultiRename('L', true, null, spec, expected)).toEqual({
      ok: false,
      error: { type: 'previewOutOfDate' },
    })
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
})
