/**
 * Tests for the favorites Tauri command wrappers and the `fav-` id-prefix stripping.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    addFavorite: vi.fn(),
    removeFavorite: vi.fn(),
    renameFavorite: vi.fn(),
    reorderFavorites: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { addFavorite, removeFavorite, renameFavorite, reorderFavorites, stripFavoritePrefix } from './favorites'

const ok = { status: 'ok' as const, data: null }
const err = { status: 'error' as const, error: { message: 'nope', timedOut: false } }

describe('stripFavoritePrefix', () => {
  it('strips the fav- prefix to recover the bare favorite id', () => {
    expect(stripFavoritePrefix('fav-9f1c4e2a')).toBe('9f1c4e2a')
  })

  it('leaves an already-bare id unchanged (no double-strip)', () => {
    expect(stripFavoritePrefix('9f1c4e2a')).toBe('9f1c4e2a')
  })

  it('only strips the leading prefix, not an interior occurrence', () => {
    expect(stripFavoritePrefix('fav-fav-x')).toBe('fav-x')
  })
})

describe('favorites wrappers', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('addFavorite forwards path and name', async () => {
    vi.mocked(commands.addFavorite).mockResolvedValueOnce(ok)
    await addFavorite('/Users/me/Docs', null)
    expect(commands.addFavorite).toHaveBeenCalledWith('/Users/me/Docs', null)
  })

  it('removeFavorite forwards the bare id', async () => {
    vi.mocked(commands.removeFavorite).mockResolvedValueOnce(ok)
    await removeFavorite('9f1c4e2a')
    expect(commands.removeFavorite).toHaveBeenCalledWith('9f1c4e2a')
  })

  it('renameFavorite forwards id and name', async () => {
    vi.mocked(commands.renameFavorite).mockResolvedValueOnce(ok)
    await renameFavorite('9f1c4e2a', 'Work')
    expect(commands.renameFavorite).toHaveBeenCalledWith('9f1c4e2a', 'Work')
  })

  it('reorderFavorites forwards the full ordered id list', async () => {
    vi.mocked(commands.reorderFavorites).mockResolvedValueOnce(ok)
    await reorderFavorites(['a', 'b', 'c'])
    expect(commands.reorderFavorites).toHaveBeenCalledWith(['a', 'b', 'c'])
  })

  it('throws the IpcError on an error result', async () => {
    vi.mocked(commands.addFavorite).mockResolvedValueOnce(err)
    await expect(addFavorite('/x', null)).rejects.toBeTruthy()
  })
})
