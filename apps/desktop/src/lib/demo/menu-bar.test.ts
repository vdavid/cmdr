import { flushSync } from 'svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { _setCatalogForTests, setLocale } from '#lib/intl/messages.svelte.js'
import { installMenuBar, labelText, type MenuBarHandle } from './menu-bar.svelte.js'
import type { BarMenu, MenuClick } from './menu-engine.js'

// A fake engine with the real one's shapes (`menu-engine.ts`), so the test pins what this file
// does with them: draw, walk, and send what a click asks for.
const BAR: BarMenu[] = [
  {
    id: 'menu_app',
    title: { text: 'Cmdr' },
    rows: [
      { kind: 'item', id: 'about', label: { key: 'menu.app.about', args: {} }, shortcut: null, enabled: true },
      { kind: 'separator' },
      { kind: 'system', label: { key: 'menu.app.quit', args: {} } },
    ],
  },
  {
    id: 'menu_view',
    title: { key: 'menu.bar.view', args: {} },
    rows: [
      { kind: 'item', id: 'file_copy', label: { key: 'menu.file.copy', args: {} }, shortcut: 'F5', enabled: false },
      {
        kind: 'check',
        id: 'show_hidden_files',
        label: { key: 'menu.view.showHiddenFiles', args: {} },
        shortcut: '⇧⌘.',
        enabled: true,
        checked: true,
      },
      {
        kind: 'submenu',
        label: { key: 'menu.view.sortBy', args: {} },
        enabled: true,
        rows: [
          {
            kind: 'check',
            id: 'sort_by_name',
            label: { key: 'menu.sort.name', args: {} },
            shortcut: '⌃3',
            enabled: true,
            checked: false,
          },
        ],
      },
      { kind: 'item', id: 'go_home', label: { key: 'menu.go.home', args: {} }, shortcut: '⇧⌘H', enabled: true },
    ],
  },
]

let handle: MenuBarHandle | undefined

function setup(
  click: (id: string) => MenuClick = (id) => ({
    kind: 'emit',
    event: 'execute-command',
    payload: { commandId: id },
    to: 'main',
  }),
) {
  const emitted: [string, unknown, unknown][] = []
  const clicked: string[] = []
  const windows = { front: vi.fn(() => undefined), close: vi.fn() }
  handle = installMenuBar({
    menu: {
      ready: Promise.resolve(true),
      bar: () => BAR,
      click: (id) => {
        clicked.push(id)
        return click(id)
      },
    },
    emit: (event, payload, target) => emitted.push([event, payload, target]),
    invoke: vi.fn(() => Promise.resolve(null)),
    windows,
  })
  return { emitted, clicked, windows }
}

function key(name: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ...init })
  ;(document.activeElement ?? document.body).dispatchEvent(event)
  return event
}

const titles = () => [...document.querySelectorAll<HTMLElement>('.cmdr-demo-menubar-title')]
const rows = () => [...document.querySelectorAll<HTMLElement>('.cmdr-demo-menu-row')]

afterEach(() => {
  handle?.destroy()
  handle = undefined
  document.body.replaceChildren()
})

describe('demo menu bar', () => {
  it('draws the titles in a menubar, the app menu first and bold', async () => {
    setup()
    await Promise.resolve()
    const bar = document.querySelector('.cmdr-demo-menubar')
    expect(bar?.getAttribute('role')).toBe('menubar')
    expect(document.body.firstElementChild).toBe(bar)
    expect(titles().map((t) => t.textContent)).toEqual(['Cmdr', 'View'])
    expect(titles()[0].classList.contains('is-app')).toBe(true)
    expect(titles().every((t) => t.getAttribute('aria-haspopup') === 'menu')).toBe(true)
  })

  it('opens a menu on click with its rows, checks, shortcuts, and greyed rows', async () => {
    setup()
    await Promise.resolve()
    titles()[1].dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }))
    expect(titles()[1].getAttribute('aria-expanded')).toBe('true')
    const [copy, hidden, sortBy, home] = rows()
    expect(copy.getAttribute('aria-disabled')).toBe('true')
    expect(hidden.getAttribute('role')).toBe('menuitemcheckbox')
    expect(hidden.getAttribute('aria-checked')).toBe('true')
    expect(hidden.querySelector('.cmdr-demo-menu-shortcut')?.textContent).toBe('⇧⌘.')
    expect(hidden.querySelectorAll('.cmdr-demo-menu-shortcut span')).toHaveLength(3)
    expect(sortBy.getAttribute('aria-haspopup')).toBe('menu')
    expect(home.textContent).toContain('Home')
  })

  it("sends a picked row's event to the main window, then closes", async () => {
    const { emitted, clicked } = setup()
    await Promise.resolve()
    titles()[1].dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }))
    rows()[3].click()
    expect(clicked).toEqual(['go_home'])
    expect(emitted).toEqual([['execute-command', { commandId: 'go_home' }, { kind: 'label', label: 'main' }]])
    expect(rows()).toHaveLength(0)
  })

  it('never picks a greyed row', async () => {
    const { clicked } = setup()
    await Promise.resolve()
    titles()[1].dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }))
    rows()[0].click()
    expect(clicked).toEqual([])
  })

  it('walks the keyboard the way a Mac menu does, and keeps the keys from the app', async () => {
    const { clicked } = setup()
    await Promise.resolve()
    const appKeys: string[] = []
    const listener = (event: KeyboardEvent) => appKeys.push(event.key)
    document.addEventListener('keydown', listener)
    try {
      // ⌃F2 focuses the bar; → moves to View; ↓ opens it on its first ENABLED row.
      key('F2', { ctrlKey: true })
      expect(document.activeElement).toBe(titles()[0])
      key('ArrowRight')
      expect(document.activeElement).toBe(titles()[1])
      key('ArrowDown')
      expect(document.activeElement?.textContent).toContain('Show hidden files')
      // ↓ to the submenu row, → opens it on its first row, ← closes it.
      key('ArrowDown')
      key('ArrowRight')
      expect(document.activeElement?.textContent).toContain('Name')
      key('ArrowLeft')
      expect(document.activeElement?.textContent).toContain('Sort by')
      key('ArrowDown')
      key('Enter')
      expect(clicked).toEqual(['go_home'])
      expect(appKeys).toEqual([])
    } finally {
      document.removeEventListener('keydown', listener)
    }
  })

  it('backs out one level per Escape, then gives focus back', async () => {
    setup()
    await Promise.resolve()
    const pane = document.createElement('button')
    document.body.append(pane)
    pane.focus()
    key('F2', { ctrlKey: true })
    key('ArrowDown')
    expect(rows().length).toBeGreaterThan(0)
    key('Escape')
    expect(rows()).toHaveLength(0)
    expect(document.activeElement).toBe(titles()[0])
    key('Escape')
    expect(document.activeElement).toBe(pane)
  })

  it('closes the window in front for Close tab while another window has focus', async () => {
    const frame = document.createElement('iframe')
    const { windows } = setup(() => ({ kind: 'closeFocusedWindow' }))
    windows.front.mockReturnValue({ label: 'settings', frame } as never)
    await Promise.resolve()
    titles()[1].dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }))
    rows()[3].click()
    expect(windows.close).toHaveBeenCalledWith('settings')
  })

  it('fills a live label’s tokens literally, as the native menu does', () => {
    expect(labelText({ key: 'menu.select.sameExtension', args: { extension: 'pdf' } })).toBe(
      'Select all with extension *.pdf',
    )
    expect(labelText({ text: 'Cmdr' })).toBe('Cmdr')
  })

  it('redraws the closed strip in the new language at once, as the rest of the UI does', async () => {
    _setCatalogForTests('hu', { 'menu.bar.view': 'Nézet', 'ui.menuBar.label': 'Menüsor' })
    try {
      setup()
      await Promise.resolve()
      expect(titles().map((t) => t.textContent)).toEqual(['Cmdr', 'View'])
      await setLocale('hu')
      flushSync()
      expect(titles().map((t) => t.textContent)).toEqual(['Cmdr', 'Nézet'])
      expect(document.querySelector('.cmdr-demo-menubar')?.getAttribute('aria-label')).toBe('Menüsor')
      expect(rows()).toHaveLength(0)
    } finally {
      await setLocale(null)
      _setCatalogForTests('hu', null)
    }
  })
})
