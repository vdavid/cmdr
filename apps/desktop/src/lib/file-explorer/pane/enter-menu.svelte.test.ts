import { describe, expect, it, vi, beforeEach } from 'vitest'
import { createEnterMenu } from './enter-menu.svelte'
import type { FileEntry } from '$lib/file-explorer/types'

const { openSettingsWindowMock } = vi.hoisted(() => ({
  openSettingsWindowMock: vi.fn<(section?: string[]) => Promise<void>>(),
}))
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: openSettingsWindowMock,
}))

function makeEntry(name: string, isArchive = true): FileEntry {
  // Only name/isDirectory/isArchive are read; a partial cast keeps the test focused.
  return { name, path: `/left/${name}`, isDirectory: false, isSymlink: false, isArchive } as unknown as FileEntry
}

function makeDeps() {
  return {
    getPaneElement: () => null,
    browse: vi.fn(),
    open: vi.fn(),
    restoreFocus: vi.fn(),
  }
}

/** A minimal keydown-like object the controller reads (`key` + the two stoppers). */
function key(name: string): KeyboardEvent {
  return { key: name, preventDefault: vi.fn(), stopPropagation: vi.fn() } as unknown as KeyboardEvent
}

describe('createEnterMenu', () => {
  beforeEach(() => {
    openSettingsWindowMock.mockClear()
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
    expect(menu.highlighted).toBe('open')

    menu.openFor(makeEntry('b.zip'), 'ask')
    expect(menu.highlighted).toBe('browse')
  })

  it('onSelect (pointer) routes browse and open to the deps with the pending entry', () => {
    const deps = makeDeps()
    const menu = createEnterMenu(deps)
    const entry = makeEntry('a.zip')

    menu.openFor(entry, 'ask')
    menu.onSelect('browse')
    expect(deps.browse).toHaveBeenCalledWith(entry)
    expect(menu.open).toBe(false)
    expect(deps.restoreFocus).toHaveBeenCalled()

    menu.openFor(entry, 'ask')
    menu.onSelect('open')
    expect(deps.open).toHaveBeenCalledWith(entry)
  })

  it('onSelect configure deep-links to the Archives settings section', () => {
    const menu = createEnterMenu(makeDeps())
    menu.openFor(makeEntry('a.zip'), 'ask')
    menu.onSelect('configure')
    expect(openSettingsWindowMock).toHaveBeenCalledWith(['Behavior', 'Archives'])
  })

  it('onOpenChange(false) restores focus only on a real close transition', () => {
    const deps = makeDeps()
    const menu = createEnterMenu(deps)
    menu.openFor(makeEntry('a.zip'), 'ask')
    menu.onOpenChange(false)
    expect(menu.open).toBe(false)
    expect(deps.restoreFocus).toHaveBeenCalledTimes(1)
  })

  describe('handleKey', () => {
    it('is a no-op when the menu is closed', () => {
      const menu = createEnterMenu(makeDeps())
      expect(menu.handleKey(key('Enter'))).toBe(false)
    })

    it('ArrowDown / ArrowUp move the highlight, clamped at the ends', () => {
      const menu = createEnterMenu(makeDeps())
      menu.openFor(makeEntry('a.zip'), 'ask') // highlighted = browse
      expect(menu.handleKey(key('ArrowDown'))).toBe(true)
      expect(menu.highlighted).toBe('open')
      menu.handleKey(key('ArrowDown'))
      expect(menu.highlighted).toBe('configure')
      menu.handleKey(key('ArrowDown')) // clamp at the last row
      expect(menu.highlighted).toBe('configure')
      menu.handleKey(key('ArrowUp'))
      expect(menu.highlighted).toBe('open')
    })

    it('Enter selects the highlighted row and closes', () => {
      const deps = makeDeps()
      const menu = createEnterMenu(deps)
      const entry = makeEntry('a.zip')
      menu.openFor(entry, 'ask') // browse
      menu.handleKey(key('ArrowDown')) // open
      expect(menu.handleKey(key('Enter'))).toBe(true)
      expect(deps.open).toHaveBeenCalledWith(entry)
      expect(menu.open).toBe(false)
    })

    it('Escape closes without selecting', () => {
      const deps = makeDeps()
      const menu = createEnterMenu(deps)
      menu.openFor(makeEntry('a.zip'), 'ask')
      expect(menu.handleKey(key('Escape'))).toBe(true)
      expect(menu.open).toBe(false)
      expect(deps.browse).not.toHaveBeenCalled()
      expect(deps.open).not.toHaveBeenCalled()
    })

    it('ignores unrelated keys', () => {
      const menu = createEnterMenu(makeDeps())
      menu.openFor(makeEntry('a.zip'), 'ask')
      expect(menu.handleKey(key('a'))).toBe(false)
    })
  })
})
