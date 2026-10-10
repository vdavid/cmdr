import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createMenu } from './menu-controller.svelte'
import type { MenuController, MenuDeps } from './menu-controller.svelte'
import { disclosureRowValue } from './menu-navigation'
import type { MenuSection } from './menu-types'

/**
 * Controller tests for the house `Menu`. The controller owns everything that isn't
 * rendering: open state, the highlight, the keyboard contract, keyboard-vs-pointer
 * mode, submenus, and reorder. It holds no DOM, so it's exercised directly here;
 * `Menu.svelte.test.ts` covers what the surface renders.
 */

/** Two sections: a reorderable one and a plain one holding a disabled row and a submenu row. */
function makeSections(): MenuSection[] {
  return [
    {
      id: 'favorites',
      heading: 'Favorites',
      reorderable: true,
      items: [
        { value: 'fav-a', label: 'A', accelerator: '1' },
        { value: 'fav-b', label: 'B', accelerator: '2' },
        { value: 'fav-c', label: 'C', accelerator: '3' },
      ],
    },
    {
      id: 'volumes',
      heading: 'Volumes',
      items: [
        { value: 'vol-1', label: 'Macintosh HD' },
        // Declares an accelerator AND is disabled: the key must reach nothing.
        { value: 'vol-2', label: 'Backup', disabled: true, accelerator: '4' },
        {
          value: 'vol-3',
          label: 'Share',
          // TWO rows on purpose: a one-row submenu hides a cursor that lights every row.
          submenu: [
            { value: 'connect', label: 'Connect directly' },
            { value: 'forget', label: 'Forget this share' },
          ],
        },
      ],
    },
  ]
}

function keydown(key: string, modifiers: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...modifiers })
}

/** A real `MouseEvent`: a bare `{ clientX, clientY }` literal doesn't type-check, and an
 *  `as MouseEvent` cast gets stripped by the lint auto-fixer (`docs/testing.md`). */
function pointerAt(clientX: number, clientY: number): MouseEvent {
  return new MouseEvent('mousemove', { clientX, clientY })
}

/** An anchor element, in the document so the surface can measure it. */
function anchorEl(): HTMLElement {
  const el = document.createElement('button')
  document.body.appendChild(el)
  return el
}

let menus: MenuController[] = []

/** Builds a controller and registers it for teardown, so no document listener outlives a test. */
// `Partial<MenuDeps<undefined>>`, not `Parameters<typeof createMenu>[0]`: on a generic
// function that resolves `T` to `unknown`, which then can't be spread into a `<undefined>` menu.
function build(deps: Partial<MenuDeps> = {}): MenuController {
  const menu = createMenu({
    getSections: makeSections,
    onSelect: vi.fn(),
    ...deps,
  })
  menus.push(menu)
  return menu
}

beforeEach(() => {
  menus = []
})

afterEach(() => {
  for (const menu of menus) menu.destroy()
  document.body.innerHTML = ''
})

describe('open and close', () => {
  it('starts closed and opens under an anchor element', () => {
    const menu = build()
    expect(menu.isOpen).toBe(false)
    const el = anchorEl()
    menu.openUnder(el)
    expect(menu.isOpen).toBe(true)
    expect(menu.anchor).toEqual({ kind: 'element', element: el })
  })

  it('opens at a point for a context-menu style caller', () => {
    const menu = build()
    menu.openAt({ x: 40, y: 80 })
    expect(menu.anchor).toEqual({ kind: 'point', x: 40, y: 80 })
  })

  it('highlights the first navigable row on open', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    expect(menu.highlightedValue).toBe('fav-a')
  })

  it('toggleUnder closes an open menu', () => {
    const menu = build()
    const el = anchorEl()
    menu.toggleUnder(el)
    expect(menu.isOpen).toBe(true)
    menu.toggleUnder(el)
    expect(menu.isOpen).toBe(false)
  })

  it('reports open changes and restores focus once, on a real close', () => {
    const onOpenChange = vi.fn()
    const restoreFocus = vi.fn()
    const menu = build({ onOpenChange, restoreFocus })
    menu.openUnder(anchorEl())
    expect(onOpenChange).toHaveBeenCalledWith(true)
    menu.close()
    expect(onOpenChange).toHaveBeenLastCalledWith(false)
    expect(restoreFocus).toHaveBeenCalledTimes(1)
    menu.close()
    expect(restoreFocus).toHaveBeenCalledTimes(1)
  })

  it('clears the highlight and submenu on close', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.openSubmenu('vol-3', true)
    menu.close()
    expect(menu.highlightedValue).toBeNull()
    expect(menu.openSubmenuValue).toBeNull()
  })
})

describe('keyboard navigation', () => {
  it('moves the highlight past headings and skips disabled rows', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-1')
    menu.handleKey(keydown('ArrowDown'))
    // `vol-2` is disabled, so the cursor lands on `vol-3`.
    expect(menu.highlightedValue).toBe('vol-3')
  })

  it('wraps around at both ends', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-3') // the last navigable row
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBe('fav-a')
    menu.handleKey(keydown('ArrowUp'))
    expect(menu.highlightedValue).toBe('vol-3')
  })

  it('Home and End jump to the first and last navigable rows', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.handleKey(keydown('End'))
    expect(menu.highlightedValue).toBe('vol-3')
    menu.handleKey(keydown('Home'))
    expect(menu.highlightedValue).toBe('fav-a')
  })

  it('Enter and Space activate the highlighted row and close', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.highlight('vol-1')
    expect(menu.handleKey(keydown('Enter'))).toBe(true)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'vol-1' }), 'keyboard')
    expect(menu.isOpen).toBe(false)
  })

  it('Escape closes without selecting', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    expect(menu.handleKey(keydown('Escape'))).toBe(true)
    expect(menu.isOpen).toBe(false)
    expect(onSelect).not.toHaveBeenCalled()
  })

  it('never activates a disabled row', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.highlight('vol-2') // disabled: the highlight refuses to land there
    expect(menu.highlightedValue).not.toBe('vol-2')
    menu.handleKey(keydown('Enter'))
    expect(onSelect).not.toHaveBeenCalledWith(expect.objectContaining({ value: 'vol-2' }))
  })

  it('handles nothing while the menu is closed', () => {
    const menu = build()
    expect(menu.handleKey(keydown('ArrowDown'))).toBe(false)
  })
})

describe('the open menu owns the keyboard', () => {
  it('gives onKey the first look, before its own handling', () => {
    const onKey = vi.fn(() => true)
    const menu = build({ onKey })
    menu.openUnder(anchorEl())
    expect(menu.handleKey(keydown('ArrowDown'))).toBe(true)
    expect(onKey).toHaveBeenCalled()
    // The caller claimed it, so the cursor never moved.
    expect(menu.highlightedValue).toBe('fav-a')
  })

  // ❗ A key the CONSUMER claims is swallowed exactly like one the menu handles itself. It used
  // to be the one class that escaped, which the central dispatch hid only while
  // `isHeaderMenuOpen()` happened to know about every menu on this primitive.
  it('stops a key onKey claimed from reaching a listener further along', () => {
    const target = anchorEl()
    const downstream = vi.fn()
    target.addEventListener('keydown', downstream)
    const menu = build({ onKey: () => true })
    menu.openUnder(target)
    const event = keydown('ArrowDown')
    const prevent = vi.spyOn(event, 'preventDefault')
    target.dispatchEvent(event)
    expect(downstream).not.toHaveBeenCalled()
    // Swallowed, but never defaulted away: a consumer key can be a menu-bar accelerator.
    expect(prevent).not.toHaveBeenCalled()
  })

  it('falls through to its own handling when onKey passes', () => {
    const onKey = vi.fn(() => false)
    const menu = build({ onKey })
    menu.openUnder(anchorEl())
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBe('fav-b')
  })

  it('swallows a key it has no use for, so the pane behind stays inert', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    const event = keydown('x')
    const stop = vi.spyOn(event, 'stopPropagation')
    const prevent = vi.spyOn(event, 'preventDefault')
    expect(menu.handleKey(event)).toBe(true)
    expect(stop).toHaveBeenCalled()
    // Swallowed from the app, but never defaulted away: ⌘Q and friends still mean what they mean.
    expect(prevent).not.toHaveBeenCalled()
  })

  it('routes real document keydowns while open, and stops once closed', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    document.dispatchEvent(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBe('fav-b')
    menu.close()
    document.dispatchEvent(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBeNull()
  })

  it('hands every keystroke to an inline editor while isEditing', () => {
    let editing = false
    const menu = build({ isEditing: () => editing })
    menu.openUnder(anchorEl())
    editing = true
    const event = keydown('ArrowDown')
    const stop = vi.spyOn(event, 'stopPropagation')
    expect(menu.handleKey(event)).toBe(false)
    expect(stop).not.toHaveBeenCalled()
    expect(menu.highlightedValue).toBe('fav-a')
  })
})

/**
 * A menu opened from INSIDE another one (a drive-index badge in a volume-switcher row).
 * Both hold a document-level capture listener, and `stopPropagation` doesn't stop a
 * listener on the same node, so without a rule every key would reach both: one Enter
 * would activate a row in each menu, and one Escape would close the pair.
 */
describe('two open menus', () => {
  it('leaves every key to the one that opened last', () => {
    const outer = build()
    const inner = build()
    outer.openUnder(anchorEl())
    inner.openUnder(anchorEl())

    document.dispatchEvent(keydown('ArrowDown'))
    expect(inner.highlightedValue).toBe('fav-b')
    expect(outer.highlightedValue).toBe('fav-a')
  })

  it('reports the key as unhandled for the menu underneath', () => {
    const outer = build()
    const inner = build()
    outer.openUnder(anchorEl())
    inner.openUnder(anchorEl())
    expect(outer.handleKey(keydown('ArrowDown'))).toBe(false)
  })

  it('closes only the inner menu on Escape, and hands the keys back', () => {
    const outer = build()
    const inner = build()
    outer.openUnder(anchorEl())
    inner.openUnder(anchorEl())

    document.dispatchEvent(keydown('Escape'))
    expect(inner.isOpen).toBe(false)
    expect(outer.isOpen).toBe(true)

    document.dispatchEvent(keydown('ArrowDown'))
    expect(outer.highlightedValue).toBe('fav-b')
  })

  it('hands the keys back when the inner menu is destroyed rather than closed', () => {
    const outer = build()
    const inner = build()
    outer.openUnder(anchorEl())
    inner.openUnder(anchorEl())
    inner.destroy()
    document.dispatchEvent(keydown('ArrowDown'))
    expect(outer.highlightedValue).toBe('fav-b')
  })
})

/**
 * Accelerators: a row that declares one activates when its digit is typed, from anywhere in the
 * open menu. The pure matching is pinned in `menu-navigation.test.ts`; this is where it sits in
 * the controller's order of business.
 */
describe('accelerators', () => {
  /** A digit as the keyboard actually sends it: the physical key, plus what the layout printed. */
  function digit(code: string, printed: string, modifiers: KeyboardEventInit = {}): KeyboardEvent {
    return keydown(printed, { code, ...modifiers })
  }

  it('activates the row that claims the digit, and closes', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    expect(menu.handleKey(digit('Digit2', '2'))).toBe(true)
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'fav-b', accelerator: '2' }), 'accelerator')
    expect(menu.isOpen).toBe(false)
  })

  it('takes the numpad digit too', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.handleKey(digit('Numpad3', '3'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'fav-c' }), 'accelerator')
  })

  // ❗ The row must not activate, and the key must not fall through to the pane behind either:
  // an open menu owns the keyboard whether or not a row wanted this one.
  it('swallows the digit of a disabled row without activating it', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    const event = digit('Digit4', '4')
    const stop = vi.spyOn(event, 'stopPropagation')
    expect(menu.handleKey(event)).toBe(true)
    expect(onSelect).not.toHaveBeenCalled()
    expect(stop).toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  it('swallows a digit no row claims, leaving the menu open', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    expect(menu.handleKey(digit('Digit9', '9'))).toBe(true)
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  it('gives onKey the first look, ahead of the accelerator', () => {
    const onSelect = vi.fn()
    const onKey = vi.fn(() => true)
    const menu = build({ onSelect, onKey })
    menu.openUnder(anchorEl())
    expect(menu.handleKey(digit('Digit2', '2'))).toBe(true)
    expect(onKey).toHaveBeenCalled()
    // The caller claimed the key, so no row opened.
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  it('hands the digit to an inline editor while isEditing', () => {
    const onSelect = vi.fn()
    let editing = false
    const menu = build({ onSelect, isEditing: () => editing })
    menu.openUnder(anchorEl())
    editing = true
    const event = digit('Digit2', '2')
    const stop = vi.spyOn(event, 'stopPropagation')
    // Untouched and unswallowed: a rename field typing "2" gets its "2".
    expect(menu.handleKey(event)).toBe(false)
    expect(stop).not.toHaveBeenCalled()
    expect(onSelect).not.toHaveBeenCalled()
  })

  it('leaves ⌘1 / ⌃1 / ⌥1 alone', () => {
    for (const modifier of ['metaKey', 'ctrlKey', 'altKey'] as const) {
      const onSelect = vi.fn()
      const menu = build({ onSelect })
      menu.openUnder(anchorEl())
      menu.handleKey(digit('Digit1', '1', { [modifier]: true }))
      expect(onSelect).not.toHaveBeenCalled()
      menu.close()
    }
  })
})

describe('keyboard versus pointer mode', () => {
  it('enters keyboard mode on an arrow key', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    expect(menu.keyboardMode).toBe(false)
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.keyboardMode).toBe(true)
  })

  it('leaves keyboard mode only after the pointer travels more than 5 px', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.handleKey(keydown('ArrowDown'))
    menu.surface.pointerMoved(pointerAt(100, 100))
    menu.surface.pointerMoved(pointerAt(103, 102))
    expect(menu.keyboardMode).toBe(true)
    menu.surface.pointerMoved(pointerAt(120, 100))
    expect(menu.keyboardMode).toBe(false)
  })

  // ❗ Otherwise there would be two cursors: `:hover` paints again the moment keyboard mode
  // drops, and no `mouseover` is coming for a row the pointer never left.
  it('takes the cursor to the row the pointer is already on when it leaves keyboard mode', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.handleKey(keydown('ArrowDown')) // keyboard mode, cursor on fav-b
    menu.surface.pointerMoved(pointerAt(100, 100), 'vol-1')
    menu.surface.pointerMoved(pointerAt(140, 100), 'vol-1')
    expect(menu.keyboardMode).toBe(false)
    expect(menu.highlightedValue).toBe('vol-1')
  })

  it('ignores hover while in keyboard mode, and follows it otherwise', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.handleKey(keydown('ArrowDown')) // keyboard mode, cursor on fav-b
    menu.surface.hover('vol-1')
    expect(menu.highlightedValue).toBe('fav-b')
    menu.surface.pointerMoved(pointerAt(0, 0))
    menu.surface.pointerMoved(pointerAt(200, 200))
    menu.surface.hover('vol-1')
    expect(menu.highlightedValue).toBe('vol-1')
  })
})

describe('submenus', () => {
  it('opens one with ArrowRight only on a row that has one', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-1')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.openSubmenuValue).toBeNull()
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.openSubmenuValue).toBe('vol-3')
  })

  it('closes one with ArrowLeft, keeping the parent cursor', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('ArrowLeft'))
    expect(menu.openSubmenuValue).toBeNull()
    expect(menu.highlightedValue).toBe('vol-3')
  })

  it('activates the submenu item, not the parent row', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('Enter'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'connect' }), 'keyboard')
  })

  it('shows one cursor: an open submenu takes the parent row’s highlight', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.submenuHighlighted).toBe(true)
    expect(menu.parentHighlightSuppressed).toBe(true)
  })

  it('opens onto the submenu’s first row, and only that one', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.submenuHighlightedValue).toBe('connect')
  })

  it('walks the submenu’s own rows with the arrows, wrapping', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.submenuHighlightedValue).toBe('forget')
    // The parent's cursor never moves while its submenu owns the keys.
    expect(menu.highlightedValue).toBe('vol-3')
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.submenuHighlightedValue).toBe('connect')
    menu.handleKey(keydown('ArrowUp'))
    expect(menu.submenuHighlightedValue).toBe('forget')
  })

  it('activates the submenu row the cursor is on, not its first', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('ArrowDown'))
    menu.handleKey(keydown('Enter'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'forget' }), 'keyboard')
  })

  it('shows no submenu cursor when the pointer opened it, until the pointer reaches in', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.openSubmenu('vol-3', false)
    expect(menu.submenuHighlightedValue).toBeNull()
    expect(menu.submenuHighlighted).toBe(false)
    menu.surface.hoverSubmenu('forget')
    expect(menu.submenuHighlightedValue).toBe('forget')
  })

  it('opens a row’s submenu on hover, and closes it on hovering a row without one', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.hover('vol-3')
    expect(menu.openSubmenuValue).toBe('vol-3')
    menu.surface.hover('vol-1')
    expect(menu.openSubmenuValue).toBeNull()
  })

  // Pre-fix the surface opened a hovered row's submenu itself, keyboard mode or not, so a
  // resting mouse over a row with a submenu stole the keyboard's cursor.
  it('opens no submenu on hover while the keyboard has the cursor', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.handleKey(keydown('Home'))
    menu.surface.hover('vol-3')
    expect(menu.openSubmenuValue).toBeNull()
    expect(menu.highlightedValue).toBe('fav-a')
  })

  // Pre-fix a hover-opened submenu took the arrows the moment it appeared, so ↓ after a hover
  // jumped the cursor into the submenu instead of moving down the list.
  it('moves the parent cursor with the arrows while a hovered submenu shows no cursor', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.hover('vol-3')
    menu.handleKey(keydown('ArrowUp'))
    expect(menu.highlightedValue).toBe('vol-1')
    expect(menu.openSubmenuValue).toBeNull()
    expect(menu.submenuHighlightedValue).toBeNull()
  })

  it('enters a hovered submenu on ArrowRight, onto its first row', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.hover('vol-3')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.submenuHighlightedValue).toBe('connect')
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.submenuHighlightedValue).toBe('forget')
    expect(menu.highlightedValue).toBe('vol-3')
  })

  it('hands the arrows to a hovered submenu once the pointer reaches in', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.hover('vol-3')
    menu.surface.hoverSubmenu('connect')
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.submenuHighlightedValue).toBe('forget')
    expect(menu.highlightedValue).toBe('vol-3')
  })

  it('still activates the first row when Enter lands on a hover-opened submenu', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.highlight('vol-3')
    menu.surface.openSubmenu('vol-3', false)
    menu.handleKey(keydown('Enter'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'connect' }), 'keyboard')
  })
})

/**
 * The payload type is the API's promise to consumers, so it's pinned here rather than
 * eyeballed: the no-type-argument call must compile, and a typed one must reach the snippets.
 */
describe('the payload type', () => {
  it('needs no type argument, and `unknown` is what a row without one carries', () => {
    const plain = createMenu({ getSections: makeSections, onSelect: () => {} })
    // @ts-expect-error `data` is `unknown` here, so it can't be used as a string without narrowing.
    const _payload: string = plain.sections[0].items[0].data
    expect(plain.isOpen).toBe(false)
  })

  it('hands a typed payload back on select', () => {
    interface Place {
      id: string
    }
    let captured: string | null = null
    const menu = createMenu<Place>({
      getSections: () => [{ id: 'places', items: [{ value: 'a', label: 'A', data: { id: 'x' } }] }],
      onSelect: (item) => {
        // Compile-time half: this assignment fails if `data` ever widens back to `unknown`.
        const payload: Place | undefined = item.data
        captured = payload ? payload.id : null
      },
    })
    menus.push(menu)
    menu.openUnder(anchorEl())
    menu.surface.activate('a')
    expect(captured).toBe('x')
  })
})

describe('keyboard reorder', () => {
  it('moves the row and reports the section’s new order', () => {
    const onReorder = vi.fn()
    const menu = build({ onReorder })
    menu.openUnder(anchorEl())
    menu.highlight('fav-a')
    expect(menu.handleKey(keydown('ArrowDown', { altKey: true }))).toBe(true)
    expect(onReorder).toHaveBeenCalledWith({
      sectionId: 'favorites',
      orderedValues: ['fav-b', 'fav-a', 'fav-c'],
      from: 0,
      to: 1,
    })
  })

  it('carries the highlight with the moved row', () => {
    const menu = build({ onReorder: vi.fn() })
    menu.openUnder(anchorEl())
    menu.highlight('fav-a')
    menu.handleKey(keydown('ArrowDown', { altKey: true }))
    expect(menu.highlightedValue).toBe('fav-a')
  })

  it('does nothing at either edge of the section', () => {
    const onReorder = vi.fn()
    const menu = build({ onReorder })
    menu.openUnder(anchorEl())
    menu.highlight('fav-a')
    menu.handleKey(keydown('ArrowUp', { altKey: true }))
    expect(onReorder).not.toHaveBeenCalled()
    menu.highlight('fav-c')
    menu.handleKey(keydown('ArrowDown', { altKey: true }))
    expect(onReorder).not.toHaveBeenCalled()
  })

  it('does nothing in a section that is not reorderable', () => {
    const onReorder = vi.fn()
    const menu = build({ onReorder })
    menu.openUnder(anchorEl())
    menu.highlight('vol-1')
    menu.handleKey(keydown('ArrowDown', { altKey: true }))
    expect(onReorder).not.toHaveBeenCalled()
  })
})

describe('pointer drag reorder', () => {
  /** Three favorite rows 20px tall from y=0: midpoints 10, 30, 50. */
  function bindMidpoints(menu: MenuController): void {
    menu.surface.bindSurface({ getRowMidpoints: () => [10, 30, 50] })
  }

  function mouse(type: string, clientY: number): MouseEvent {
    return new MouseEvent(type, { clientY, clientX: 0, button: 0, bubbles: true })
  }

  it('treats a press without travel as a plain click, not a reorder', () => {
    const onSelect = vi.fn()
    const onReorder = vi.fn()
    const menu = build({ onSelect, onReorder })
    menu.openUnder(anchorEl())
    bindMidpoints(menu)
    menu.surface.startDrag('fav-a', mouse('mousedown', 10))
    window.dispatchEvent(mouse('mouseup', 11))
    expect(onReorder).not.toHaveBeenCalled()
    // A press that never travelled is a click, and the provenance says so.
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'fav-a' }), 'pointer')
  })

  it('shows the drop cue at the gap the pointer is over, then reorders on drop', () => {
    const onSelect = vi.fn()
    const onReorder = vi.fn()
    const menu = build({ onSelect, onReorder })
    menu.openUnder(anchorEl())
    bindMidpoints(menu)
    menu.surface.startDrag('fav-a', mouse('mousedown', 10))
    window.dispatchEvent(mouse('mousemove', 45))
    expect(menu.draggingValue).toBe('fav-a')
    // Below midpoints 10 and 30, above 50: the gap above the last row.
    expect(menu.dropSlot).toBe(2)
    window.dispatchEvent(mouse('mouseup', 45))
    expect(onReorder).toHaveBeenCalledWith({
      sectionId: 'favorites',
      orderedValues: ['fav-b', 'fav-a', 'fav-c'],
      from: 0,
      to: 1,
    })
    // A drag is not a click: the row must not also open.
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.draggingValue).toBeNull()
    expect(menu.dropSlot).toBeNull()
  })

  it('hides the cue where a drop would leave the row where it is', () => {
    const menu = build({ onReorder: vi.fn() })
    menu.openUnder(anchorEl())
    bindMidpoints(menu)
    menu.surface.startDrag('fav-a', mouse('mousedown', 10))
    window.dispatchEvent(mouse('mousemove', 20))
    expect(menu.dropSlot).toBeNull()
  })

  /**
   * The drop-line cue, pinned as pure input to output: a row geometry, a pointer Y, and the
   * gap the line lands in. The cue rides the RAW insertion slot (the visual gap), which is
   * what keeps a downward drag from drawing the line one row too high. Rows are 20px tall
   * from y=0 here, so the midpoints are 10 / 30 / 50, and slot `k` means "the line above row
   * k" (slot `length` means below the last row).
   *
   * These three came from the switcher's own suite with the port: the behavior is the
   * primitive's now, and only the three helpers below had to be re-pointed.
   */
  function grabRow(menu: MenuController, value: string, atY: number): void {
    bindMidpoints(menu)
    menu.surface.startDrag(value, mouse('mousedown', atY))
  }
  function movePointerTo(y: number): void {
    window.dispatchEvent(mouse('mousemove', y))
  }
  function releasePointerAt(y: number): void {
    window.dispatchEvent(mouse('mouseup', y))
  }

  it('puts the drop-line cue on the gap under the pointer, dragging DOWN', () => {
    const menu = build({ onReorder: vi.fn() })
    menu.openUnder(anchorEl())
    grabRow(menu, 'fav-a', 10)
    // Past row 1's midpoint: the line belongs in the gap above row 2.
    movePointerTo(35)
    expect(menu.dropSlot).toBe(2)
    // Past the last midpoint: the line belongs below the last row.
    movePointerTo(55)
    expect(menu.dropSlot).toBe(3)
    releasePointerAt(55)
  })

  it('puts the drop-line cue on the gap under the pointer, dragging UP', () => {
    const menu = build({ onReorder: vi.fn() })
    menu.openUnder(anchorEl())
    grabRow(menu, 'fav-c', 50)
    // Above every midpoint: the line belongs above row 0.
    movePointerTo(5)
    expect(menu.dropSlot).toBe(0)
    // Between the first two midpoints: the line belongs above row 1.
    movePointerTo(20)
    expect(menu.dropSlot).toBe(1)
    releasePointerAt(20)
  })

  it('draws no cue where a drop would leave the row where it already is', () => {
    const menu = build({ onReorder: vi.fn() })
    menu.openUnder(anchorEl())
    grabRow(menu, 'fav-b', 30)
    // Both gaps touching the grabbed row (slot 1 above it, slot 2 below it) leave it in
    // place, so neither draws a line.
    movePointerTo(25)
    expect(menu.dropSlot).toBeNull()
    movePointerTo(35)
    expect(menu.dropSlot).toBeNull()
    releasePointerAt(35)
  })

  it('never drags from a section that is not reorderable', () => {
    const onReorder = vi.fn()
    const menu = build({ onReorder })
    menu.openUnder(anchorEl())
    bindMidpoints(menu)
    menu.surface.startDrag('vol-1', mouse('mousedown', 10))
    window.dispatchEvent(mouse('mousemove', 45))
    expect(menu.draggingValue).toBeNull()
    window.dispatchEvent(mouse('mouseup', 45))
    expect(onReorder).not.toHaveBeenCalled()
  })

  it('drops its window listeners on destroy, mid-drag', () => {
    const onReorder = vi.fn()
    const menu = build({ onReorder })
    menu.openUnder(anchorEl())
    bindMidpoints(menu)
    menu.surface.startDrag('fav-a', mouse('mousedown', 10))
    menu.destroy()
    window.dispatchEvent(mouse('mousemove', 45))
    window.dispatchEvent(mouse('mouseup', 45))
    expect(onReorder).not.toHaveBeenCalled()
  })
})

describe('pointer selection', () => {
  it('activates a row and closes', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.surface.activate('vol-1')
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'vol-1' }), 'pointer')
    expect(menu.isOpen).toBe(false)
  })

  it('refuses a disabled row', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.surface.activate('vol-2')
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  // Right-click is the second door to a row's submenu: the same rows, never a second list.
  it('opens the right-clicked row’s submenu, cursor on the row and none inside yet', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.contextMenu('vol-3')
    expect(menu.isOpen).toBe(true)
    expect(menu.openSubmenuValue).toBe('vol-3')
    expect(menu.highlightedValue).toBe('vol-3')
    expect(menu.submenuHighlightedValue).toBeNull()
  })

  it('opens no submenu on a right-click at a row without one', () => {
    const onSelect = vi.fn()
    const menu = build({ onSelect })
    menu.openUnder(anchorEl())
    menu.surface.contextMenu('fav-b')
    expect(menu.openSubmenuValue).toBeNull()
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  it('closes another row’s open submenu when a row without one is right-clicked', () => {
    const menu = build()
    menu.openUnder(anchorEl())
    menu.surface.contextMenu('vol-3')
    menu.surface.contextMenu('vol-1')
    expect(menu.openSubmenuValue).toBeNull()
  })
})

/**
 * A submenu row can be greyed (a busy volume's Eject) and can keep the menu up after a pick
 * (an eject, so several drives go in a row; a rename, which happens in the row itself).
 */
describe('submenu row states', () => {
  function sectionsWith(submenu: MenuSection['items']): () => MenuSection[] {
    return () => [{ id: 'volumes', items: [{ value: 'share', label: 'Share', submenu }] }]
  }

  it('skips a disabled submenu row with the arrows', () => {
    const menu = build({
      getSections: sectionsWith([
        { value: 'eject', label: 'Eject', disabled: true },
        { value: 'rename', label: 'Rename' },
        { value: 'forget', label: 'Forget' },
      ]),
    })
    menu.openUnder(anchorEl())
    menu.highlight('share')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.submenuHighlightedValue).toBe('rename')
    menu.handleKey(keydown('ArrowDown'))
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.submenuHighlightedValue).toBe('rename')
  })

  it('never puts the pointer’s cursor on a disabled submenu row', () => {
    const menu = build({
      getSections: sectionsWith([
        { value: 'eject', label: 'Eject', disabled: true },
        { value: 'rename', label: 'Rename' },
      ]),
    })
    menu.openUnder(anchorEl())
    menu.surface.openSubmenu('share', false)
    menu.surface.hoverSubmenu('eject')
    expect(menu.submenuHighlightedValue).toBeNull()
  })

  it('still opens a submenu whose every row is disabled, so the reason shows, with no cursor', () => {
    const onSelect = vi.fn()
    const menu = build({
      onSelect,
      getSections: sectionsWith([{ value: 'eject', label: 'Eject (busy)', disabled: true }]),
    })
    menu.openUnder(anchorEl())
    menu.highlight('share')
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.openSubmenuValue).toBe('share')
    expect(menu.submenuHighlightedValue).toBeNull()
    menu.handleKey(keydown('Enter'))
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.isOpen).toBe(true)
  })

  it('keeps the menu open after a keepsMenuOpen pick, closing only the submenu', () => {
    const onSelect = vi.fn()
    const menu = build({
      onSelect,
      getSections: sectionsWith([
        { value: 'eject', label: 'Eject', keepsMenuOpen: true },
        { value: 'forget', label: 'Forget' },
      ]),
    })
    menu.openUnder(anchorEl())
    menu.highlight('share')
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('Enter'))
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ value: 'eject' }), 'keyboard')
    expect(menu.isOpen).toBe(true)
    expect(menu.openSubmenuValue).toBeNull()
    // The parent row keeps the cursor, so the next ArrowRight goes straight back in.
    expect(menu.highlightedValue).toBe('share')
  })

  it('closes the whole menu after an ordinary submenu pick', () => {
    const menu = build({
      getSections: sectionsWith([
        { value: 'eject', label: 'Eject', keepsMenuOpen: true },
        { value: 'forget', label: 'Forget' },
      ]),
    })
    menu.openUnder(anchorEl())
    menu.surface.openSubmenu('share', false)
    menu.surface.activate('forget')
    expect(menu.isOpen).toBe(false)
  })
})

/**
 * A section folded behind a disclosure row. The caller owns whether it's open (the
 * controller persists nothing), so these play the caller: a flip comes back through
 * `onDisclosureChange`, and the test applies it to the data the menu reads.
 */
describe('disclosure sections', () => {
  const HEADER = disclosureRowValue('favorites')

  function foldable(onDisclosureChange = vi.fn()) {
    let expanded = false
    const onSelect = vi.fn()
    const menu = build({
      getSections: () => [
        {
          id: 'favorites',
          disclosure: { expanded, label: 'Favorites' },
          items: [
            { value: 'fav-a', label: 'A', submenu: [{ value: 'rename', label: 'Rename' }] },
            { value: 'fav-b', label: 'B' },
          ],
        },
        { id: 'volumes', items: [{ value: 'vol-1', label: 'Macintosh HD' }] },
      ],
      onSelect,
      onDisclosureChange: (change) => {
        onDisclosureChange(change)
        expanded = change.expanded
      },
    })
    menu.openUnder(anchorEl())
    return { menu, onSelect, onDisclosureChange }
  }

  it('lands the cursor on the disclosure row when it leads the menu', () => {
    const { menu } = foldable()
    expect(menu.highlightedValue).toBe(HEADER)
  })

  it('opens the section on Enter, keeps the menu up, and picks nothing', () => {
    const { menu, onSelect, onDisclosureChange } = foldable()
    menu.handleKey(keydown('Enter'))
    expect(onDisclosureChange).toHaveBeenCalledWith({ sectionId: 'favorites', expanded: true })
    expect(menu.isOpen).toBe(true)
    expect(onSelect).not.toHaveBeenCalled()
    expect(menu.highlightedValue).toBe(HEADER)
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBe('fav-a')
  })

  it('walks past a collapsed section’s rows', () => {
    const { menu } = foldable()
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBe('vol-1')
  })

  it('flips on a click, which also leaves the menu open', () => {
    const { menu, onSelect, onDisclosureChange } = foldable()
    menu.surface.activate(HEADER)
    expect(onDisclosureChange).toHaveBeenCalledWith({ sectionId: 'favorites', expanded: true })
    expect(menu.isOpen).toBe(true)
    expect(onSelect).not.toHaveBeenCalled()
  })

  it('collapses from a row inside it with ←, ← (back to the row, then shut)', () => {
    const { menu, onDisclosureChange } = foldable()
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('ArrowDown'))
    menu.handleKey(keydown('ArrowDown'))
    expect(menu.highlightedValue).toBe('fav-b')
    menu.handleKey(keydown('ArrowLeft'))
    expect(menu.highlightedValue).toBe(HEADER)
    menu.handleKey(keydown('ArrowLeft'))
    expect(onDisclosureChange).toHaveBeenLastCalledWith({ sectionId: 'favorites', expanded: false })
  })

  it('still opens a row’s own submenu on → inside an open section', () => {
    const { menu } = foldable()
    menu.handleKey(keydown('ArrowRight'))
    menu.handleKey(keydown('ArrowDown'))
    menu.handleKey(keydown('ArrowRight'))
    expect(menu.openSubmenuValue).toBe('fav-a')
  })
})

describe('live sections', () => {
  it('reads the caller’s data on every access, so the menu tracks its state', () => {
    let sections = makeSections()
    const menu = build({ getSections: () => sections })
    menu.openUnder(anchorEl())
    sections = [{ id: 'favorites', items: [{ value: 'fav-z', label: 'Z' }] }]
    menu.handleKey(keydown('Home'))
    expect(menu.highlightedValue).toBe('fav-z')
  })
})
