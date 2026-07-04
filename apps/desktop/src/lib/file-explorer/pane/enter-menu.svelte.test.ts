import { describe, expect, it, vi, beforeEach } from 'vitest'
import { createEnterMenu } from './enter-menu.svelte'
import type { FileEntry } from '$lib/file-explorer/types'

const openSettingsWindow = vi.fn(() => Promise.resolve())
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: (...args: unknown[]) => openSettingsWindow(...args),
}))

function makeEntry(name: string, isArchive = true): FileEntry {
  return { name, path: `/left/${name}`, isDirectory: false, isSymlink: false, isArchive }
}

function makeDeps() {
  return {
    getPaneElement: () => null,
    browse: vi.fn(),
    open: vi.fn(),
    restoreFocus: vi.fn(),
  }
}

describe('createEnterMenu', () => {
  beforeEach(() => {
    openSettingsWindow.mockClear()
  })

  it('starts closed', () => {
    const menu = createEnterMenu(makeDeps())
    expect(menu.open).toBe(false)
    expect(menu.items.map((i) => i.value)).toEqual(['browse', 'open', 'configure'])
  })

  it('openFor opens the menu and leads with the resolved action', () => {
    const menu = createEnterMenu(makeDeps())
    menu.openFor(makeEntry('a.zip'), 'open')
    expect(menu.open).toBe(true)
    expect(menu.highlight).toBe('open')

    menu.openFor(makeEntry('b.zip'), 'ask')
    expect(menu.highlight).toBe('browse')
  })

  it('onSelect routes browse and open to the deps with the pending entry', () => {
    const deps = makeDeps()
    const menu = createEnterMenu(deps)
    const entry = makeEntry('a.zip')

    menu.openFor(entry, 'ask')
    menu.onSelect('browse')
    expect(deps.browse).toHaveBeenCalledWith(entry)
    expect(menu.open).toBe(false)

    menu.openFor(entry, 'ask')
    menu.onSelect('open')
    expect(deps.open).toHaveBeenCalledWith(entry)
  })

  it('onSelect configure deep-links to the Archives settings section', () => {
    const menu = createEnterMenu(makeDeps())
    menu.openFor(makeEntry('a.zip'), 'ask')
    menu.onSelect('configure')
    expect(openSettingsWindow).toHaveBeenCalledWith(['Behavior', 'Archives'])
  })

  it('onOpenChange(false) restores focus to the pane', () => {
    const deps = makeDeps()
    const menu = createEnterMenu(deps)
    menu.openFor(makeEntry('a.zip'), 'ask')
    menu.onOpenChange(false)
    expect(menu.open).toBe(false)
    expect(deps.restoreFocus).toHaveBeenCalled()
  })
})
