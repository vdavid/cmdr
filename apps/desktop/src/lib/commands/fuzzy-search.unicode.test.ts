/**
 * The palette (and the Keyboard shortcuts section through `searchAllCommands`) finds commands
 * named in any script, accents optional. A mocked registry stands in for a translated catalog.
 */
import { describe, it, expect, vi } from 'vitest'

// Inlined in the factory: `vi.mock` is hoisted above every top-level binding.
vi.mock('./command-registry', () => {
  const mockCommands = [
    { id: 'file.copyPath', name: 'Копировать путь', displayName: 'Копировать путь', showInPalette: true },
    { id: 'app.about', name: '关于 Cmdr', displayName: '关于 Cmdr', showInPalette: true },
    { id: 'view.toggleTheme', name: 'Téma váltása', displayName: 'Téma váltása', showInPalette: true },
  ].map((c) => ({ ...c, scope: 'App', shortcuts: [] }))
  return { commands: mockCommands, getPaletteCommands: () => mockCommands }
})

import { searchAllCommands, searchCommands } from './fuzzy-search'

describe('commands named in other scripts', () => {
  it('finds a Cyrillic command name', () => {
    expect(searchCommands('путь').map((r) => r.command.id)).toEqual(['file.copyPath'])
  })

  it('finds a Chinese command name, also from the Keyboard shortcuts search', () => {
    expect(searchCommands('关于').map((r) => r.command.id)).toEqual(['app.about'])
    expect(searchAllCommands('关于').map((r) => r.command.id)).toEqual(['app.about'])
  })

  it('finds an accented name typed without accents and highlights the accented letters', () => {
    const [match] = searchCommands('tema')
    expect(match.command.id).toBe('view.toggleTheme')
    expect(match.matchedIndices).toEqual([0, 1, 2, 3])
  })
})
