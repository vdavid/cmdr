/**
 * The demo's top menu bar, in place of the macOS one a browser doesn't have: a strip above the app
 * that draws the desktop's own bar (`menu-engine.ts`, over `crates/cmdr-menu`) and sends exactly
 * what a native click sends, so every handler in the app is the one the native menu reaches. This
 * file only draws and routes: which rows exist, what they say, their shortcuts, whether they're
 * enabled or checked, and what a click sends all come from the engine. See `DETAILS.md` § The menu
 * bar.
 *
 * Keys follow a macOS menu: ⌃F2 focuses the bar (macOS's own "Move focus to the menu bar"),
 * ←/→ move between menus, ↓ / Enter / Space open one, ↑/↓ walk its enabled rows, → opens a
 * submenu, ← closes it, Enter / Space pick, and Esc backs out one level.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { getMessage } from '#lib/intl/messages.svelte.js'
import type { EventTarget } from './backend.js'
import type { BarMenu, MenuClick, MenuEngineHandle, MenuRow, MenuRowLabel } from './menu-engine.js'
import { MENU_BAR_HEIGHT_PX, MENU_BAR_STYLE } from './menu-bar-style.js'
import type { WindowHost } from './windows.js'

export interface MenuBarDeps {
  menu: Pick<MenuEngineHandle, 'bar' | 'click' | 'ready'>
  emit: (event: string, payload: unknown, target: EventTarget) => void
  windows: Pick<WindowHost, 'front' | 'close'>
}

export interface MenuBarHandle {
  /** The bar's element, first in `<body>`. */
  readonly element: HTMLElement
  /** Closes every open menu. */
  close(): void
  /** Takes the bar down (a test's teardown). */
  destroy(): void
}

/** One open menu surface: the top-level dropdown, or a submenu. */
interface Surface {
  element: HTMLElement
  rows: { row: MenuRow; element: HTMLElement }[]
  highlighted: number
  /** The submenu row this surface hangs off, for a submenu. */
  parentRow?: HTMLElement
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

export function installMenuBar(deps: MenuBarDeps): MenuBarHandle {
  const style = document.createElement('style')
  style.textContent = MENU_BAR_STYLE
  document.head.append(style)

  const bar = document.createElement('nav')
  bar.className = 'cmdr-demo-menubar'
  bar.setAttribute('role', 'menubar')
  bar.setAttribute('aria-label', getMessage('ui.menuBar.label'))
  document.body.prepend(bar)
  // Where the main window starts under the bar, for the demo's other chrome that sits on the window.
  document.documentElement.style.setProperty('--cmdr-demo-window-top', `${String(MENU_BAR_HEIGHT_PX)}px`)

  let menus: BarMenu[] = []
  let titles: HTMLElement[] = []
  let openIndex: number | null = null
  let surfaces: Surface[] = []
  // What had focus before the bar took it, so a pick or Esc puts it back, as closing a native menu does.
  let returnFocus: HTMLElement | null = null

  function renderTitles(): void {
    const fresh = deps.menu.bar()
    if (!fresh) return
    menus = fresh
    bar.setAttribute('aria-label', getMessage('ui.menuBar.label'))
    const focusedIndex = titles.indexOf(document.activeElement as HTMLElement)
    bar.replaceChildren()
    titles = menus.map((menu, index) => {
      const title = document.createElement('div')
      title.className = 'cmdr-demo-menubar-title'
      if (index === 0) title.classList.add('is-app')
      title.id = `cmdr-demo-menubar-${menu.id || String(index)}`
      title.textContent = labelText(menu.title)
      title.setAttribute('role', 'menuitem')
      title.setAttribute('aria-haspopup', 'menu')
      title.setAttribute('aria-expanded', 'false')
      title.tabIndex = index === 0 ? 0 : -1
      title.addEventListener('pointerdown', (event) => {
        if (event.button !== 0) return
        event.preventDefault()
        rememberFocus()
        if (openIndex === index) closeAll(true)
        else open(index, false)
      })
      title.addEventListener('pointerenter', () => {
        if (openIndex !== null && openIndex !== index) open(index, false)
      })
      bar.append(title)
      return title
    })
    // A redraw under a focused title (the language moved while the bar had the keyboard) keeps its place.
    if (focusedIndex >= 0) focusTitle(focusedIndex)
  }

  /**
   * Redraws the closed strip whenever its words move, as `followWindowTitle` does for a native
   * title: `labelText` reads the catalog's locale-version rune, so the effect re-runs on every
   * `setLocale()`, the same moment the rest of the UI re-renders. The `set_ui_language` push the app
   * sends the native menu can't drive this: it arrives before the new catalog chunk loads, so a
   * redraw on it would still read the old words. An open menu is left alone; its next open redraws.
   */
  function followLanguage(): () => void {
    return $effect.root(() => {
      $effect(() => {
        if (openIndex === null) renderTitles()
      })
    })
  }

  function rememberFocus(): void {
    const active = document.activeElement
    if (active instanceof HTMLElement && !bar.contains(active) && !active.closest('.cmdr-demo-menu')) {
      returnFocus = active
    }
  }

  function restoreFocus(): void {
    const target = returnFocus
    returnFocus = null
    if (target?.isConnected) target.focus({ preventScroll: true })
    else (document.activeElement as HTMLElement | null)?.blur()
  }

  function closeSurfacesFrom(depth: number): void {
    for (const surface of surfaces.splice(depth)) {
      surface.element.remove()
      surface.parentRow?.setAttribute('aria-expanded', 'false')
    }
  }

  function closeAll(restore: boolean): void {
    closeSurfacesFrom(0)
    if (openIndex !== null) {
      titles[openIndex]?.classList.remove('is-open')
      titles[openIndex]?.setAttribute('aria-expanded', 'false')
    }
    openIndex = null
    if (restore) restoreFocus()
  }

  function buildSurface(rows: MenuRow[], labelledBy: string): Surface {
    const element = document.createElement('div')
    element.className = 'cmdr-demo-menu'
    element.id = `cmdr-demo-menu-${String(nextId++)}`
    element.setAttribute('role', 'menu')
    element.setAttribute('aria-labelledby', labelledBy)
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
      const rowElement = document.createElement('div')
      rowElement.className = 'cmdr-demo-menu-row'
      rowElement.id = `${element.id}-${String(surface.rows.length)}`
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

  function place(element: HTMLElement, left: number, top: number, flipFrom?: number): void {
    document.body.append(element)
    const { width, height } = element.getBoundingClientRect()
    let x = left
    if (x + width > window.innerWidth - 4) x = flipFrom === undefined ? window.innerWidth - 4 - width : flipFrom - width
    const y = Math.min(top, Math.max(0, window.innerHeight - 4 - height))
    element.style.left = `${String(Math.max(4, x))}px`
    element.style.top = `${String(y)}px`
  }

  function open(index: number, fromKeyboard: boolean): void {
    renderTitles()
    const menu = menus.at(index)
    const title = titles.at(index)
    if (!menu || !title) return
    closeAll(false)
    openIndex = index
    title.classList.add('is-open')
    title.setAttribute('aria-expanded', 'true')
    const surface = buildSurface(menu.rows, title.id)
    title.setAttribute('aria-controls', surface.element.id)
    surfaces = [surface]
    const rect = title.getBoundingClientRect()
    place(surface.element, rect.left - 2, bar.getBoundingClientRect().bottom + 1)
    if (fromKeyboard) highlight(surface, nextEnabled(surface, -1, 1))
    else surface.element.focus({ preventScroll: true })
  }

  function openSubmenu(parent: Surface, index: number, fromKeyboard: boolean): void {
    const entry = parent.rows.at(index)
    if (entry?.row.kind !== 'submenu' || !entry.row.enabled) return
    const depth = surfaces.indexOf(parent) + 1
    if (surfaces[depth]?.parentRow === entry.element) {
      if (fromKeyboard) highlight(surfaces[depth], nextEnabled(surfaces[depth], -1, 1))
      return
    }
    closeSurfacesFrom(depth)
    const surface = buildSurface(entry.row.rows, entry.element.id)
    surface.parentRow = entry.element
    entry.element.setAttribute('aria-expanded', 'true')
    surfaces.push(surface)
    const rect = entry.element.getBoundingClientRect()
    place(surface.element, rect.right + 2, rect.top - 5, rect.left - 2)
    if (fromKeyboard) highlight(surface, nextEnabled(surface, -1, 1))
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

  function pointerOver(surface: Surface, index: number): void {
    const depth = surfaces.indexOf(surface)
    if (depth < 0) return
    const entry = surface.rows[index]
    if (!isEnabled(entry.row)) {
      highlight(surface, -1)
      closeSurfacesFrom(depth + 1)
      return
    }
    highlight(surface, index)
    if (entry.row.kind === 'submenu') openSubmenu(surface, index, false)
    else closeSurfacesFrom(depth + 1)
  }

  function activate(surface: Surface, index: number): void {
    const entry = surface.rows.at(index)
    if (!entry || !isEnabled(entry.row)) return
    if (entry.row.kind === 'submenu') {
      openSubmenu(surface, index, true)
      return
    }
    if (entry.row.kind !== 'item' && entry.row.kind !== 'check') return
    const outcome = deps.menu.click(entry.row.id)
    closeAll(true)
    perform(outcome)
  }

  /** Sends what the click asked for, after focus went back where it was, as a native pick does. */
  function perform(outcome: MenuClick): void {
    switch (outcome.kind) {
      case 'emit':
        deps.emit(outcome.event, outcome.payload, { kind: 'label', label: 'main' })
        return
      case 'nativeEdit': {
        // The responder chain's stand-in: the edit goes to the window in front, which has the text focus.
        const front = deps.windows.front()
        if (!front) return
        front.frame.focus()
        // eslint-disable-next-line @typescript-eslint/no-deprecated -- the one way a page can hand an edit to the focused field, which is what the native selector does; browsers keep it for exactly this
        front.frame.contentDocument?.execCommand(outcome.action)
        return
      }
      case 'closeFocusedWindow': {
        const front = deps.windows.front()
        if (front) deps.windows.close(front.label)
        return
      }
      case 'nothing':
        return
    }
  }

  function focusTitle(index: number): void {
    const count = titles.length
    if (count === 0) return
    const target = titles[((index % count) + count) % count]
    for (const title of titles) title.tabIndex = title === target ? 0 : -1
    target.focus({ preventScroll: true })
  }

  /** Closes the open submenu and puts the cursor back on its row; false when none is open. */
  function backOutOfSubmenu(): boolean {
    if (surfaces.length < 2) return false
    closeSurfacesFrom(surfaces.length - 1)
    const parent = surfaces.at(-1)
    if (parent) highlight(parent, parent.highlighted)
    return true
  }

  /** Opens the menu `step` titles over from the open one, or moves focus along the bar. */
  function moveAlongBar(step: 1 | -1, titleIndex: number): void {
    const count = titles.length
    if (openIndex !== null) open((openIndex + step + count) % count, true)
    else focusTitle(titleIndex + step)
  }

  function walkRows(step: 1 | -1, titleIndex: number): void {
    const surface = surfaces.at(-1)
    if (openIndex === null || !surface) open(Math.max(0, titleIndex), true)
    else highlight(surface, nextEnabled(surface, surface.highlighted, step))
  }

  function arrowRight(titleIndex: number): void {
    const surface = surfaces.at(-1)
    if (surface?.rows.at(surface.highlighted)?.row.kind === 'submenu') openSubmenu(surface, surface.highlighted, true)
    else moveAlongBar(1, titleIndex)
  }

  function jumpToEnd(toFirst: boolean): void {
    const surface = surfaces.at(-1)
    if (surface) highlight(surface, nextEnabled(surface, toFirst ? -1 : 0, toFirst ? 1 : -1))
  }

  function pick(titleIndex: number): void {
    const surface = surfaces.at(-1)
    if (surface && surface.highlighted >= 0) activate(surface, surface.highlighted)
    else if (openIndex === null && titleIndex >= 0) open(titleIndex, true)
  }

  function escape(): void {
    if (backOutOfSubmenu()) return
    if (openIndex === null) {
      restoreFocus()
      return
    }
    const index = openIndex
    closeAll(false)
    focusTitle(index)
  }

  /** The bar's keys, a Mac menu's: each takes the index of the bar title with focus (-1 for none). */
  const keys: Partial<Record<string, (titleIndex: number) => void>> = {
    ArrowDown: (titleIndex) => {
      walkRows(1, titleIndex)
    },
    ArrowUp: (titleIndex) => {
      walkRows(-1, titleIndex)
    },
    ArrowRight: arrowRight,
    ArrowLeft: (titleIndex) => {
      if (!backOutOfSubmenu()) moveAlongBar(-1, titleIndex)
    },
    Home: () => {
      jumpToEnd(true)
    },
    End: () => {
      jumpToEnd(false)
    },
    Enter: pick,
    ' ': pick,
    Escape: escape,
    Tab: () => {
      closeAll(true)
    },
  }

  /** Keys while the bar or one of its menus has focus: theirs, never the panes' behind them. */
  function onKey(event: KeyboardEvent): void {
    if (event.metaKey || event.altKey) return
    const action = keys[event.key]
    const handled = action !== undefined
    action?.(titles.indexOf(document.activeElement as HTMLElement))
    if (handled) event.preventDefault()
    // Whatever the menu didn't use stays out of the app, as an open native menu swallows it.
    event.stopPropagation()
  }

  function ownsFocus(): boolean {
    const active = document.activeElement
    return active instanceof HTMLElement && (bar.contains(active) || active.closest('.cmdr-demo-menu') !== null)
  }

  function onWindowKey(event: KeyboardEvent): void {
    const onlyControl = event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey
    if (onlyControl && event.key === 'F2') {
      event.preventDefault()
      event.stopPropagation()
      rememberFocus()
      renderTitles()
      focusTitle(0)
      return
    }
    if (ownsFocus()) onKey(event)
  }

  function onPointerDown(event: PointerEvent): void {
    if (openIndex === null) return
    const target = event.target as Node | null
    if (target && (bar.contains(target) || surfaces.some((s) => s.element.contains(target)))) return
    closeAll(false)
  }

  function onBlur(): void {
    // The page lost focus (a click into a panel's frame, another tab): a native menu closes too.
    closeAll(false)
  }

  window.addEventListener('keydown', onWindowKey, true)
  document.addEventListener('pointerdown', onPointerDown, true)
  window.addEventListener('blur', onBlur)
  let stopFollowing: (() => void) | undefined
  let destroyed = false
  void deps.menu.ready.then(() => {
    if (destroyed) return
    // Drawn now, not on the effect's first (scheduled) run, so the bar is up the moment the engine is.
    renderTitles()
    stopFollowing = followLanguage()
  })

  return {
    element: bar,
    close: () => {
      closeAll(false)
    },
    destroy: () => {
      destroyed = true
      stopFollowing?.()
      closeAll(false)
      window.removeEventListener('keydown', onWindowKey, true)
      document.removeEventListener('pointerdown', onPointerDown, true)
      window.removeEventListener('blur', onBlur)
      bar.remove()
      style.remove()
      document.documentElement.style.removeProperty('--cmdr-demo-window-top')
    },
  }
}
