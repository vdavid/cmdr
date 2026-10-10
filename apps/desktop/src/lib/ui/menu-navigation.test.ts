import { describe, it, expect } from 'vitest'
import {
  acceleratorChar,
  disclosureOf,
  disclosureRowValue,
  itemByAccelerator,
  menuKeyAction,
  navigableValues,
  nextValue,
  sectionOf,
} from './menu-navigation'
import type { MenuSection } from './menu-types'

/** Three sections: a reorderable one, a plain one holding a disabled row, and an empty one. */
const sections: MenuSection[] = [
  {
    id: 'favorites',
    heading: 'Favorites',
    reorderable: true,
    items: [
      { value: 'fav-a', label: 'A', accelerator: '1' },
      { value: 'fav-b', label: 'B', accelerator: '2' },
    ],
  },
  {
    id: 'volumes',
    heading: 'Volumes',
    items: [
      // In the SECOND section, so an accelerator is proven to reach across sections.
      { value: 'vol-1', label: 'Macintosh HD', accelerator: '4' },
      { value: 'vol-2', label: 'Backup', disabled: true, accelerator: '3' },
      { value: 'vol-3', label: 'Share', submenu: [{ value: 'connect', label: 'Connect directly' }] },
    ],
  },
  { id: 'empty', heading: 'Nothing', items: [], emptyLabel: '(Your favorites will show here)' },
]

/** A keydown-like object: the fields the pure matcher reads, all modifiers explicit. */
function key(name: string, modifiers: Partial<Record<'altKey' | 'metaKey' | 'ctrlKey' | 'shiftKey', boolean>> = {}) {
  return {
    key: name,
    code: '',
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    ...modifiers,
  } as unknown as KeyboardEvent
}

/**
 * A physical key by `code`, the way an accelerator is matched. `key` carries whatever the layout
 * would print, which is deliberately NOT what decides the match.
 */
function physical(
  code: string,
  printed = '',
  modifiers: Partial<Record<'altKey' | 'metaKey' | 'ctrlKey' | 'shiftKey', boolean>> = {},
) {
  return {
    key: printed,
    code,
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    ...modifiers,
  } as unknown as KeyboardEvent
}

describe('navigableValues', () => {
  it('lists every enabled row across sections, in display order', () => {
    expect(navigableValues(sections)).toEqual(['fav-a', 'fav-b', 'vol-1', 'vol-3'])
  })

  it('skips disabled rows, headings, and empty placeholders', () => {
    expect(navigableValues(sections)).not.toContain('vol-2')
    expect(navigableValues([{ id: 'x', heading: 'H', items: [], emptyLabel: 'nothing here' }])).toEqual([])
  })
})

describe('nextValue', () => {
  const values = ['a', 'b', 'c']

  it('moves by delta', () => {
    expect(nextValue(values, 'a', 1)).toBe('b')
    expect(nextValue(values, 'c', -1)).toBe('b')
  })

  it('wraps around both ends', () => {
    expect(nextValue(values, 'c', 1)).toBe('a')
    expect(nextValue(values, 'a', -1)).toBe('c')
  })

  it('starts from the first row going down and the last going up when nothing is highlighted', () => {
    expect(nextValue(values, null, 1)).toBe('a')
    expect(nextValue(values, null, -1)).toBe('c')
  })

  it('returns null for an empty list', () => {
    expect(nextValue([], 'a', 1)).toBeNull()
  })
})

describe('sectionOf', () => {
  it('finds the section and in-section index of a row', () => {
    expect(sectionOf(sections, 'fav-b')).toEqual({ section: sections[0], index: 1 })
    expect(sectionOf(sections, 'vol-3')).toEqual({ section: sections[1], index: 2 })
  })

  it('returns null for an unknown value', () => {
    expect(sectionOf(sections, 'nope')).toBeNull()
  })
})

describe('menuKeyAction', () => {
  const plain = { hasSubmenu: false, submenu: 'closed', reorderable: false } as const

  it('maps the bare navigation keys', () => {
    expect(menuKeyAction(key('ArrowDown'), plain)).toEqual({ kind: 'move', delta: 1 })
    expect(menuKeyAction(key('ArrowUp'), plain)).toEqual({ kind: 'move', delta: -1 })
    expect(menuKeyAction(key('Home'), plain)).toEqual({ kind: 'edge', edge: 'first' })
    expect(menuKeyAction(key('End'), plain)).toEqual({ kind: 'edge', edge: 'last' })
    expect(menuKeyAction(key('Enter'), plain)).toEqual({ kind: 'activate' })
    expect(menuKeyAction(key(' '), plain)).toEqual({ kind: 'activate' })
    expect(menuKeyAction(key('Escape'), plain)).toEqual({ kind: 'close' })
  })

  it('ignores a navigation key carrying a command modifier', () => {
    // ⌘↓ and ⌃↓ mean other things; only the bare arrow moves the cursor.
    expect(menuKeyAction(key('ArrowDown', { metaKey: true }), plain)).toEqual({ kind: 'none' })
    expect(menuKeyAction(key('Enter', { metaKey: true }), plain)).toEqual({ kind: 'none' })
  })

  it('opens a submenu with ArrowRight only when the row has one', () => {
    expect(menuKeyAction(key('ArrowRight'), { ...plain, hasSubmenu: true })).toEqual({ kind: 'openSubmenu' })
    expect(menuKeyAction(key('ArrowRight'), plain)).toEqual({ kind: 'none' })
  })

  it('closes an open submenu with ArrowLeft or Escape, and walks it with the arrows', () => {
    const open = { hasSubmenu: true, submenu: 'entered', reorderable: false } as const
    expect(menuKeyAction(key('ArrowLeft'), open)).toEqual({ kind: 'closeSubmenu' })
    expect(menuKeyAction(key('Escape'), open)).toEqual({ kind: 'closeSubmenu' })
    expect(menuKeyAction(key('Enter'), open)).toEqual({ kind: 'activate' })
    // An open submenu owns the cursor keys: they move WITHIN it, never the list behind it.
    expect(menuKeyAction(key('ArrowDown'), open)).toEqual({ kind: 'moveSubmenu', delta: 1 })
    expect(menuKeyAction(key('ArrowUp'), open)).toEqual({ kind: 'moveSubmenu', delta: -1 })
    // Nothing further right (one level), so it's swallowed rather than moving the parent's cursor.
    expect(menuKeyAction(key('ArrowRight'), open)).toEqual({ kind: 'absorb' })
  })

  it('leaves the arrows to the list while a hovered submenu shows no cursor', () => {
    const shown = { hasSubmenu: true, submenu: 'shown', reorderable: false } as const
    expect(menuKeyAction(key('ArrowDown'), shown)).toEqual({ kind: 'move', delta: 1 })
    expect(menuKeyAction(key('ArrowUp'), shown)).toEqual({ kind: 'move', delta: -1 })
    expect(menuKeyAction(key('Home'), shown)).toEqual({ kind: 'edge', edge: 'first' })
    expect(menuKeyAction(key('ArrowRight'), shown)).toEqual({ kind: 'openSubmenu' })
    expect(menuKeyAction(key('ArrowLeft'), shown)).toEqual({ kind: 'closeSubmenu' })
    expect(menuKeyAction(key('Escape'), shown)).toEqual({ kind: 'closeSubmenu' })
    expect(menuKeyAction(key('Enter'), shown)).toEqual({ kind: 'activate' })
  })

  it('reorders on exactly ⌥↑ / ⌥↓ inside a reorderable section', () => {
    const re = { ...plain, reorderable: true }
    expect(menuKeyAction(key('ArrowUp', { altKey: true }), re)).toEqual({ kind: 'reorder', delta: -1 })
    expect(menuKeyAction(key('ArrowDown', { altKey: true }), re)).toEqual({ kind: 'reorder', delta: 1 })
  })

  it('never reorders on ⌥ plus another modifier, nor outside a reorderable section', () => {
    const re = { ...plain, reorderable: true }
    // ⌥⌘↑ and ⇧⌥↑ mean other things and must not reorder on their way elsewhere.
    expect(menuKeyAction(key('ArrowUp', { altKey: true, metaKey: true }), re)).toEqual({ kind: 'none' })
    expect(menuKeyAction(key('ArrowUp', { altKey: true, shiftKey: true }), re)).toEqual({ kind: 'none' })
    expect(menuKeyAction(key('ArrowUp', { altKey: true }), plain)).toEqual({ kind: 'none' })
  })

  it('returns none for a key the menu has no use for', () => {
    expect(menuKeyAction(key('Tab'), plain)).toEqual({ kind: 'none' })
    expect(menuKeyAction(key('Tab'), plain)).toEqual({ kind: 'none' })
  })

  it('reads a digit as an accelerator, on both the number row and the numpad', () => {
    expect(menuKeyAction(physical('Digit1', '1'), plain)).toEqual({ kind: 'accelerator', char: '1' })
    expect(menuKeyAction(physical('Digit0', '0'), plain)).toEqual({ kind: 'accelerator', char: '0' })
    expect(menuKeyAction(physical('Numpad7', '7'), plain)).toEqual({ kind: 'accelerator', char: '7' })
  })

  it('reads an unmodified letter by what the layout types, regardless of case', () => {
    expect(menuKeyAction(key('a'), plain)).toEqual({ kind: 'accelerator', char: 'A' })
    expect(menuKeyAction(key('A', { shiftKey: true }), plain)).toEqual({ kind: 'accelerator', char: 'A' })
    expect(menuKeyAction(key('a', { metaKey: true }), plain)).toEqual({ kind: 'none' })
    const letterSections: MenuSection[] = [
      { id: 'letters', items: [{ value: 'favorite', label: 'Favorite', shortcut: 'A' }] },
    ]
    expect(itemByAccelerator(letterSections, 'A')?.value).toBe('favorite')
  })

  // AZERTY prints a digit only with Shift held, so ⇧ has to stay allowed or those layouts lose
  // the feature entirely. The physical key is what decides, whatever the layout printed.
  it('still reads a digit when Shift made it one', () => {
    expect(menuKeyAction(physical('Digit2', '@', { shiftKey: true }), plain)).toEqual({
      kind: 'accelerator',
      char: '2',
    })
  })

  it('never reads a digit carrying ⌘, ⌃, or ⌥: those are somebody else’s combos', () => {
    expect(menuKeyAction(physical('Digit1', '1', { metaKey: true }), plain)).toEqual({ kind: 'none' })
    expect(menuKeyAction(physical('Digit1', '1', { ctrlKey: true }), plain)).toEqual({ kind: 'none' })
    expect(menuKeyAction(physical('Digit1', '1', { altKey: true }), plain)).toEqual({ kind: 'none' })
  })

  it('reads a digit as an accelerator even with a submenu open', () => {
    const open = { hasSubmenu: true, submenu: 'entered', reorderable: false } as const
    expect(menuKeyAction(physical('Digit1', '1'), open)).toEqual({ kind: 'accelerator', char: '1' })
  })
})

describe('acceleratorChar', () => {
  it('uses physical digits and layout-aware letters', () => {
    expect(acceleratorChar(physical('Digit5', '5'))).toBe('5')
    expect(acceleratorChar(physical('Numpad5', '5'))).toBe('5')
    expect(acceleratorChar(physical('KeyA', 'a'))).toBe('A')
    expect(acceleratorChar(physical('KeyQ', 'a'))).toBe('A')
    expect(acceleratorChar(physical('NumpadAdd', '+'))).toBeNull()
    expect(acceleratorChar(physical('ArrowDown'))).toBeNull()
  })
})

describe('itemByAccelerator', () => {
  it('finds the claiming row in any section', () => {
    expect(itemByAccelerator(sections, '1')?.value).toBe('fav-a')
    // Second section: an accelerator matches across every one of them.
    expect(itemByAccelerator(sections, '4')?.value).toBe('vol-1')
  })

  it('lets a disabled row claim nothing, so its accelerator can never activate it', () => {
    // `vol-2` declares '3' and is disabled: the key matches no row, and the controller does
    // nothing with it — but an open menu still swallows it.
    expect(itemByAccelerator(sections, '3')).toBeNull()
  })

  it('returns null for a character no row declares', () => {
    expect(itemByAccelerator(sections, '9')).toBeNull()
  })
})

/**
 * A section folded behind a disclosure row. The row is navigable like any other, and a
 * collapsed section's rows are simply not there: not walked, and not claiming a key.
 */
describe('disclosure sections', () => {
  function folded(expanded: boolean): MenuSection[] {
    return [
      {
        id: 'favorites',
        disclosure: { expanded, label: 'Favorites' },
        items: [
          { value: 'fav-a', label: 'A', shortcut: 'A' },
          { value: 'fav-b', label: 'B' },
        ],
      },
      { id: 'volumes', items: [{ value: 'vol-1', label: 'Macintosh HD' }] },
    ]
  }

  it('walks the disclosure row, then the rows only while expanded', () => {
    const header = disclosureRowValue('favorites')
    expect(navigableValues(folded(false))).toEqual([header, 'vol-1'])
    expect(navigableValues(folded(true))).toEqual([header, 'fav-a', 'fav-b', 'vol-1'])
  })

  it('names the section a disclosure row belongs to, and nothing for an ordinary row', () => {
    expect(disclosureOf(folded(true), disclosureRowValue('favorites'))?.id).toBe('favorites')
    expect(disclosureOf(folded(true), 'fav-a')).toBeNull()
  })

  it('lets a collapsed section’s rows claim no key, so a hidden row never opens', () => {
    expect(itemByAccelerator(folded(false), 'A')).toBeNull()
    expect(itemByAccelerator(folded(true), 'A')?.value).toBe('fav-a')
  })

  const collapsedRow = { hasSubmenu: false, submenu: 'closed', reorderable: false, disclosure: 'collapsed' } as const
  const expandedRow = { hasSubmenu: false, submenu: 'closed', reorderable: false, disclosure: 'expanded' } as const

  it('expands on → and collapses on ←, the way a disclosure does', () => {
    expect(menuKeyAction(key('ArrowRight'), collapsedRow)).toEqual({ kind: 'disclose', expanded: true })
    expect(menuKeyAction(key('ArrowLeft'), expandedRow)).toEqual({ kind: 'disclose', expanded: false })
    // Already where the key points: nothing to do, and the cursor doesn't wander off.
    expect(menuKeyAction(key('ArrowRight'), expandedRow)).toEqual({ kind: 'none' })
    expect(menuKeyAction(key('ArrowLeft'), collapsedRow)).toEqual({ kind: 'none' })
  })

  it('flips on Enter and Space instead of activating anything', () => {
    expect(menuKeyAction(key('Enter'), collapsedRow)).toEqual({ kind: 'disclose', expanded: true })
    expect(menuKeyAction(key(' '), expandedRow)).toEqual({ kind: 'disclose', expanded: false })
  })

  it('keeps the list keys on a disclosure row', () => {
    expect(menuKeyAction(key('ArrowDown'), collapsedRow)).toEqual({ kind: 'move', delta: 1 })
    expect(menuKeyAction(key('Escape'), expandedRow)).toEqual({ kind: 'close' })
  })

  it('takes ← from a row inside an open section back to its disclosure row', () => {
    const inside = { hasSubmenu: true, submenu: 'closed', reorderable: true, insideDisclosure: true } as const
    expect(menuKeyAction(key('ArrowLeft'), inside)).toEqual({ kind: 'toDisclosure' })
    // → still opens the row's own submenu.
    expect(menuKeyAction(key('ArrowRight'), inside)).toEqual({ kind: 'openSubmenu' })
  })
})
