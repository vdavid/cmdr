/**
 * An open menu and its submenus, drawn as macOS 27 draws them: the one stack of surfaces both
 * the demo's menu bar (`menu-bar.svelte.ts`) and its right-click menus (`context-menu.ts`) open.
 * It draws rows the engine answered and walks them; what a pick SENDS is the caller's.
 *
 * Keys a Mac menu has, once the caller routes them here: ↑/↓ walk the enabled rows, Home/End jump,
 * → opens a submenu, ← closes it, Enter / Space pick. The pointer highlights and opens submenus on
 * hover, and a click picks. A greyed row never picks.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { getMessage } from '#lib/intl/messages.svelte.js'
import { MENU_SURFACE_STYLE } from './menu-bar-style.js'
import type { MenuRow, MenuRowLabel } from './menu-engine.js'

/** A row that picks: a command or a check item. */
export type PickableRow = Extract<MenuRow, { kind: 'item' | 'check' }>

/** One open surface: a menu, or a submenu. */
interface Surface {
  element: HTMLElement
  rows: { row: MenuRow; element: HTMLElement }[]
  highlighted: number
  /** The submenu row this surface hangs off, for a submenu. */
  parentRow?: HTMLElement
}

export interface MenuStack {
  /** Opens a menu of `rows` at `(left, top)` in viewport pixels, closing whatever was open. */
  open(
    rows: MenuRow[],
    at: { left: number; top: number; labelledBy?: string; label?: string },
    fromKeyboard: boolean,
  ): HTMLElement
  /** Closes every surface. */
  close(): void
  /** Whether any surface is open. */
  readonly isOpen: boolean
  /** Whether `node` is inside an open surface. */
  contains(node: Node): boolean
  /** Moves the highlight on the innermost surface: ↑ / ↓. */
  walk(step: 1 | -1): void
  /** Home / End. */
  jumpToEnd(toFirst: boolean): void
  /** → on a submenu row opens it; false when the highlighted row isn't one. */
  openHighlightedSubmenu(): boolean
  /** ← and Esc: closes the innermost submenu; false when none is open. */
  backOut(): boolean
  /** Enter / Space: picks the highlighted row; false when nothing is highlighted. */
  pickHighlighted(): boolean
}

const CHECK_SVG =
  '<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6.5 4.8 9.3 10 3" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>'
const ARROW_SVG =
  '<svg viewBox="0 0 7 12" aria-hidden="true"><path d="M1.5 1.5 6 6l-4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>'

/** A row's words: the catalog's, with each `{token}` replaced literally, as the desktop's `menu_t_with` does. */
export function labelText(label: MenuRowLabel): string {
  if ('text' in label) return label.text
  let text = getMessage(label.key as MessageKey)
  for (const [token, value] of Object.entries(label.args)) text = text.split(`{${token}}`).join(value)
  return text
}

/** A shortcut's glyphs, one span per modifier and one for the key, which is how a Mac spaces them. */
function shortcutElement(shortcut: string): HTMLElement {
  const element = document.createElement('span')
  element.className = 'cmdr-demo-menu-shortcut'
  element.setAttribute('aria-hidden', 'true')
  let rest = shortcut
  while (rest.length > 1 && '⌃⌥⇧⌘'.includes(rest[0])) {
    const glyph = document.createElement('span')
    glyph.textContent = rest[0]
    element.append(glyph)
    rest = rest.slice(1)
  }
  const key = document.createElement('span')
  key.textContent = rest
  element.append(key)
  return element
}

function isEnabled(row: MenuRow): boolean {
  return (row.kind === 'item' || row.kind === 'check' || row.kind === 'submenu') && row.enabled
}

let nextId = 0

const STYLE_ID = 'cmdr-demo-menu-style'

/** Injects the menus' look into this document once, for whichever of the bar and the right-click menus opens first. Answers the teardown. */
export function ensureMenuStyle(): () => void {
  if (document.getElementById(STYLE_ID)) return () => {}
  const style = document.createElement('style')
  style.id = STYLE_ID
  style.textContent = MENU_SURFACE_STYLE
  document.head.append(style)
  return () => {
    style.remove()
  }
}

/** A stack whose picks go to `onPick`, which closes it (or not) as it sees fit. */
export function createMenuStack(onPick: (row: PickableRow) => void): MenuStack {
  let surfaces: Surface[] = []

  function closeFrom(depth: number): void {
    for (const surface of surfaces.splice(depth)) {
      surface.element.remove()
      surface.parentRow?.setAttribute('aria-expanded', 'false')
    }
  }

  function rowElementFor(surface: Surface, row: Exclude<MenuRow, { kind: 'separator' }>): HTMLElement {
    const rowElement = document.createElement('div')
    rowElement.id = `${surface.element.id}-${String(surface.rows.length)}`
    if (row.kind === 'header') {
      // A context menu's first line: a label, never a stop on the walk.
      rowElement.className = 'cmdr-demo-menu-header'
      rowElement.setAttribute('role', 'presentation')
      rowElement.textContent = labelText(row.label)
      return rowElement
    }
    rowElement.className = 'cmdr-demo-menu-row'
    rowElement.tabIndex = -1
    rowElement.setAttribute('role', row.kind === 'check' ? 'menuitemcheckbox' : 'menuitem')
    if (row.kind === 'check') rowElement.setAttribute('aria-checked', String(row.checked))
    if (!isEnabled(row)) {
      rowElement.classList.add('is-disabled')
      rowElement.setAttribute('aria-disabled', 'true')
    }
    const check = document.createElement('span')
    check.className = 'cmdr-demo-menu-check'
    if (row.kind === 'check' && row.checked) check.innerHTML = CHECK_SVG
    const label = document.createElement('span')
    label.className = 'cmdr-demo-menu-label'
    label.textContent = labelText(row.label)
    rowElement.append(check, label)
    if ((row.kind === 'item' || row.kind === 'check') && row.shortcut) {
      rowElement.append(shortcutElement(row.shortcut))
      rowElement.setAttribute('aria-keyshortcuts', row.shortcut)
    }
    if (row.kind === 'submenu') {
      rowElement.setAttribute('aria-haspopup', 'menu')
      rowElement.setAttribute('aria-expanded', 'false')
      const arrow = document.createElement('span')
      arrow.className = 'cmdr-demo-menu-arrow'
      arrow.innerHTML = ARROW_SVG
      rowElement.append(arrow)
    }
    return rowElement
  }

  function build(rows: MenuRow[], naming: { labelledBy?: string; label?: string }): Surface {
    const element = document.createElement('div')
    element.className = 'cmdr-demo-menu'
    element.id = `cmdr-demo-menu-${String(nextId++)}`
    element.setAttribute('role', 'menu')
    if (naming.labelledBy) element.setAttribute('aria-labelledby', naming.labelledBy)
    if (naming.label) element.setAttribute('aria-label', naming.label)
    element.tabIndex = -1
    const surface: Surface = { element, rows: [], highlighted: -1 }
    for (const row of rows) {
      if (row.kind === 'separator') {
        const separator = document.createElement('div')
        separator.className = 'cmdr-demo-menu-separator'
        separator.setAttribute('role', 'separator')
        element.append(separator)
        continue
      }
      const rowElement = rowElementFor(surface, row)
      const index = surface.rows.length
      rowElement.addEventListener('pointerenter', () => {
        pointerOver(surface, index)
      })
      rowElement.addEventListener('click', () => {
        activate(surface, index)
      })
      element.append(rowElement)
      surface.rows.push({ row, element: rowElement })
    }
    return surface
  }

  /** Puts `element` at `(left, top)`, kept on screen: flipped to end at `flipFrom` when it would overflow the right edge. */
  function place(element: HTMLElement, left: number, top: number, flipFrom?: number): void {
    document.body.append(element)
    const { width, height } = element.getBoundingClientRect()
    let x = left
    if (x + width > window.innerWidth - 4) x = flipFrom === undefined ? window.innerWidth - 4 - width : flipFrom - width
    const y = Math.min(top, Math.max(0, window.innerHeight - 4 - height))
    element.style.left = `${String(Math.max(4, x))}px`
    element.style.top = `${String(y)}px`
  }

  function highlight(surface: Surface, index: number): void {
    surface.rows.at(surface.highlighted)?.element.classList.remove('is-highlighted')
    surface.highlighted = index
    const entry = surface.rows.at(index)
    if (index < 0 || !entry) {
      surface.element.focus({ preventScroll: true })
      return
    }
    entry.element.classList.add('is-highlighted')
    entry.element.focus({ preventScroll: true })
    entry.element.scrollIntoView({ block: 'nearest' })
  }

  function nextEnabled(surface: Surface, from: number, step: 1 | -1): number {
    const count = surface.rows.length
    for (let i = 1; i <= count; i++) {
      const index = (((from + step * i) % count) + count) % count
      if (isEnabled(surface.rows[index].row)) return index
    }
    return -1
  }

  function openSubmenu(parent: Surface, index: number, fromKeyboard: boolean): void {
    const entry = parent.rows.at(index)
    if (entry?.row.kind !== 'submenu' || !entry.row.enabled) return
    const depth = surfaces.indexOf(parent) + 1
    if (surfaces[depth]?.parentRow === entry.element) {
      if (fromKeyboard) highlight(surfaces[depth], nextEnabled(surfaces[depth], -1, 1))
      return
    }
    closeFrom(depth)
    const surface = build(entry.row.rows, { labelledBy: entry.element.id })
    surface.parentRow = entry.element
    entry.element.setAttribute('aria-expanded', 'true')
    surfaces.push(surface)
    const rect = entry.element.getBoundingClientRect()
    place(surface.element, rect.right + 2, rect.top - 5, rect.left - 2)
    if (fromKeyboard) highlight(surface, nextEnabled(surface, -1, 1))
  }

  function pointerOver(surface: Surface, index: number): void {
    const depth = surfaces.indexOf(surface)
    if (depth < 0) return
    const entry = surface.rows[index]
    if (!isEnabled(entry.row)) {
      highlight(surface, -1)
      closeFrom(depth + 1)
      return
    }
    highlight(surface, index)
    if (entry.row.kind === 'submenu') openSubmenu(surface, index, false)
    else closeFrom(depth + 1)
  }

  function activate(surface: Surface, index: number): void {
    const entry = surface.rows.at(index)
    if (!entry || !isEnabled(entry.row)) return
    if (entry.row.kind === 'submenu') {
      openSubmenu(surface, index, true)
      return
    }
    if (entry.row.kind === 'item' || entry.row.kind === 'check') onPick(entry.row)
  }

  return {
    get isOpen() {
      return surfaces.length > 0
    },
    open(rows, at, fromKeyboard) {
      closeFrom(0)
      const surface = build(rows, at)
      surfaces = [surface]
      place(surface.element, at.left, at.top)
      if (fromKeyboard) highlight(surface, nextEnabled(surface, -1, 1))
      else surface.element.focus({ preventScroll: true })
      return surface.element
    },
    close: () => {
      closeFrom(0)
    },
    contains: (node) => surfaces.some((surface) => surface.element.contains(node)),
    walk(step) {
      const surface = surfaces.at(-1)
      if (surface) highlight(surface, nextEnabled(surface, surface.highlighted, step))
    },
    jumpToEnd(toFirst) {
      const surface = surfaces.at(-1)
      if (surface) highlight(surface, nextEnabled(surface, toFirst ? -1 : 0, toFirst ? 1 : -1))
    },
    openHighlightedSubmenu() {
      const surface = surfaces.at(-1)
      if (surface?.rows.at(surface.highlighted)?.row.kind !== 'submenu') return false
      openSubmenu(surface, surface.highlighted, true)
      return true
    },
    backOut() {
      if (surfaces.length < 2) return false
      closeFrom(surfaces.length - 1)
      const parent = surfaces.at(-1)
      if (parent) highlight(parent, parent.highlighted)
      return true
    },
    pickHighlighted() {
      const surface = surfaces.at(-1)
      if (!surface || surface.highlighted < 0) return false
      activate(surface, surface.highlighted)
      return true
    },
  }
}
