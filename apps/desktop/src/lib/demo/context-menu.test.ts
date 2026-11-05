import { afterEach, describe, expect, it, vi } from 'vitest'
import { createContextMenuHost, type ContextMenuHost } from './context-menu.js'
import type { ContextPopup, MenuClick } from './menu-engine.js'

// A fake popup with the engine's shapes (`menu-engine.ts`), so the test pins what this file does
// with them: draw, walk, and send what a pick asks for.
const POPUP: ContextPopup = {
  anchor: null,
  rows: [
    { kind: 'header', label: { text: 'report.pdf · 2.1 MB' } },
    { kind: 'separator' },
    { kind: 'item', id: 'file_view', label: { key: 'menu.file.view', args: {} }, shortcut: 'F3', enabled: true },
    { kind: 'item', id: 'rename', label: { key: 'menu.file.rename', args: {} }, shortcut: null, enabled: false },
    {
      kind: 'submenu',
      label: { key: 'menu.context.selection', args: {} },
      enabled: true,
      rows: [
        {
          kind: 'item',
          id: 'toggle_selection',
          label: { key: 'menu.context.toggleSelection', args: {} },
          shortcut: 'Space',
          enabled: true,
        },
      ],
    },
    { kind: 'item', id: 'file_copy', label: { key: 'menu.file.copy', args: {} }, shortcut: 'F5', enabled: true },
  ],
}

let host: ContextMenuHost | undefined

function setup(
  contextClick: (id: string) => MenuClick = (id) => ({
    kind: 'emit',
    event: 'execute-command',
    payload: { commandId: id },
    to: 'main',
  }),
) {
  const emitted: [string, unknown, unknown][] = []
  const picked: string[] = []
  const invoke = vi.fn(() => Promise.resolve(null))
  host = createContextMenuHost({
    menu: {
      contextClick: (id) => {
        picked.push(id)
        return contextClick(id)
      },
    },
    emit: (event, payload, target) => emitted.push([event, payload, target]),
    invoke,
    windows: { front: vi.fn(() => undefined), close: vi.fn() },
  })
  return { emitted, picked, invoke }
}

function key(name: string): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true })
  ;(document.activeElement ?? document.body).dispatchEvent(event)
  return event
}

const menus = () => [...document.querySelectorAll<HTMLElement>('.cmdr-demo-menu')]
const rows = () => [...document.querySelectorAll<HTMLElement>('.cmdr-demo-menu-row')]

afterEach(() => {
  host?.destroy()
  host = undefined
  document.body.replaceChildren()
})

describe('demo context menu', () => {
  it('draws the rows as a menu named by its header, at the point it was asked for', () => {
    setup()
    host?.open(POPUP, { x: 40, y: 60 }, false)
    const [menu] = menus()
    expect(menu.getAttribute('role')).toBe('menu')
    expect(menu.getAttribute('aria-label')).toBe('report.pdf · 2.1 MB')
    expect(menu.style.left).toBe('40px')
    expect(menu.style.top).toBe('60px')
    const header = menu.querySelector('.cmdr-demo-menu-header')
    expect(header?.textContent).toBe('report.pdf · 2.1 MB')
    expect(header?.getAttribute('role')).toBe('presentation')
    expect(rows().map((row) => row.getAttribute('role'))).toEqual(['menuitem', 'menuitem', 'menuitem', 'menuitem'])
    expect(rows()[1].getAttribute('aria-disabled')).toBe('true')
    expect(rows()[0].getAttribute('aria-keyshortcuts')).toBe('F3')
  })

  it('starts on the first enabled row from the keyboard, skips greyed rows, and picks with Enter', () => {
    const { emitted, picked } = setup()
    const trigger = document.createElement('button')
    document.body.append(trigger)
    trigger.focus()
    host?.open(POPUP, { x: 0, y: 0 }, true)
    expect(document.activeElement?.textContent).toContain('View')
    key('ArrowDown')
    // Rename is greyed: the walk lands on the submenu row.
    expect(document.activeElement?.getAttribute('aria-haspopup')).toBe('menu')
    key('ArrowDown')
    const enter = key('Enter')
    expect(enter.defaultPrevented).toBe(true)
    expect(picked).toEqual(['file_copy'])
    expect(emitted).toEqual([['execute-command', { commandId: 'file_copy' }, { kind: 'label', label: 'main' }]])
    expect(menus()).toHaveLength(0)
    // Focus goes back where it was, as closing a native menu does.
    expect(document.activeElement).toBe(trigger)
  })

  it('opens a submenu with → and backs out with ← and Esc', () => {
    setup()
    host?.open(POPUP, { x: 0, y: 0 }, true)
    key('ArrowDown')
    key('ArrowRight')
    expect(menus()).toHaveLength(2)
    expect(document.activeElement?.textContent).toContain('Space')
    key('ArrowLeft')
    expect(menus()).toHaveLength(1)
    key('ArrowRight')
    key('Escape')
    expect(menus()).toHaveLength(1)
    key('Escape')
    expect(menus()).toHaveLength(0)
  })

  it('keeps every key from the app while it is up', () => {
    setup()
    const appKeys = vi.fn()
    window.addEventListener('keydown', appKeys)
    host?.open(POPUP, { x: 0, y: 0 }, true)
    key('x')
    key('ArrowDown')
    window.removeEventListener('keydown', appKeys)
    expect(appKeys).not.toHaveBeenCalled()
  })

  it('never picks a greyed row, and a click outside closes it', () => {
    const { picked } = setup()
    host?.open(POPUP, { x: 0, y: 0 }, false)
    rows()[1].click()
    expect(picked).toEqual([])
    expect(menus()).toHaveLength(1)
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    expect(menus()).toHaveLength(0)
  })

  it('runs a command pick through invoke, as the desktop calls `add_favorite`', () => {
    const { invoke } = setup(() => ({ kind: 'invoke', command: 'add_favorite', args: { path: '/a', name: null } }))
    host?.open(POPUP, { x: 0, y: 0 }, false)
    rows()[0].click()
    expect(invoke).toHaveBeenCalledWith('add_favorite', { path: '/a', name: null })
  })
})
