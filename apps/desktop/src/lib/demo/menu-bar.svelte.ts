/**
 * The demo's top menu bar, in place of the macOS one a browser doesn't have: a strip above the app
 * that draws the desktop's own bar (`menu-engine.ts`, over `crates/cmdr-menu`) and sends exactly
 * what a native click sends, so every handler in the app is the one the native menu reaches. This
 * file only draws the titles and routes keys: which rows exist, what they say, their shortcuts,
 * whether they're enabled or checked, and what a click sends all come from the engine; an open
 * menu is `menu-surfaces.ts`'s, and a pick is `menu-click.ts`'s. See `DETAILS.md` § The menu bar.
 *
 * Keys follow a macOS menu: ⌃F2 focuses the bar (macOS's own "Move focus to the menu bar"),
 * ←/→ move between menus, ↓ / Enter / Space open one, ↑/↓ walk its enabled rows, → opens a
 * submenu, ← closes it, Enter / Space pick, and Esc backs out one level.
 */
import { getMessage } from '#lib/intl/messages.svelte.js'
import type { BarMenu, MenuEngineHandle } from './menu-engine.js'
import { MENU_BAR_HEIGHT_PX, MENU_BAR_STYLE } from './menu-bar-style.js'
import { type MenuClickDeps, performMenuClick } from './menu-click.js'
import { createMenuStack, ensureMenuStyle, labelText } from './menu-surfaces.js'

export { labelText }

export interface MenuBarDeps extends MenuClickDeps {
  menu: Pick<MenuEngineHandle, 'bar' | 'click' | 'ready'>
}

export interface MenuBarHandle {
  /** The bar's element, first in `<body>`. */
  readonly element: HTMLElement
  /** Closes every open menu. */
  close(): void
  /** Takes the bar down (a test's teardown). */
  destroy(): void
}

export function installMenuBar(deps: MenuBarDeps): MenuBarHandle {
  const style = document.createElement('style')
  style.textContent = MENU_BAR_STYLE
  document.head.append(style)
  const dropMenuStyle = ensureMenuStyle()

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
  // What had focus before the bar took it, so a pick or Esc puts it back, as closing a native menu does.
  let returnFocus: HTMLElement | null = null

  const stack = createMenuStack((row) => {
    const outcome = deps.menu.click(row.id)
    closeAll(true)
    performMenuClick(outcome, deps)
  })

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
    if (active instanceof HTMLElement && !bar.contains(active) && !stack.contains(active)) {
      returnFocus = active
    }
  }

  function restoreFocus(): void {
    const target = returnFocus
    returnFocus = null
    if (target?.isConnected) target.focus({ preventScroll: true })
    else (document.activeElement as HTMLElement | null)?.blur()
  }

  function closeAll(restore: boolean): void {
    stack.close()
    if (openIndex !== null) {
      titles[openIndex]?.classList.remove('is-open')
      titles[openIndex]?.setAttribute('aria-expanded', 'false')
    }
    openIndex = null
    if (restore) restoreFocus()
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
    const rect = title.getBoundingClientRect()
    const surface = stack.open(
      menu.rows,
      { left: rect.left - 2, top: bar.getBoundingClientRect().bottom + 1, labelledBy: title.id },
      fromKeyboard,
    )
    title.setAttribute('aria-controls', surface.id)
  }

  function focusTitle(index: number): void {
    const count = titles.length
    if (count === 0) return
    const target = titles[((index % count) + count) % count]
    for (const title of titles) title.tabIndex = title === target ? 0 : -1
    target.focus({ preventScroll: true })
  }

  /** Opens the menu `step` titles over from the open one, or moves focus along the bar. */
  function moveAlongBar(step: 1 | -1, titleIndex: number): void {
    const count = titles.length
    if (openIndex !== null) open((openIndex + step + count) % count, true)
    else focusTitle(titleIndex + step)
  }

  function walkRows(step: 1 | -1, titleIndex: number): void {
    if (openIndex === null || !stack.isOpen) open(Math.max(0, titleIndex), true)
    else stack.walk(step)
  }

  function pick(titleIndex: number): void {
    if (stack.pickHighlighted()) return
    if (openIndex === null && titleIndex >= 0) open(titleIndex, true)
  }

  function escape(): void {
    if (stack.backOut()) return
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
    ArrowRight: (titleIndex) => {
      if (!stack.openHighlightedSubmenu()) moveAlongBar(1, titleIndex)
    },
    ArrowLeft: (titleIndex) => {
      if (!stack.backOut()) moveAlongBar(-1, titleIndex)
    },
    Home: () => {
      stack.jumpToEnd(true)
    },
    End: () => {
      stack.jumpToEnd(false)
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

  /** Whether the bar, or one of ITS menus, has the keyboard (a right-click menu has its own keys). */
  function ownsFocus(): boolean {
    const active = document.activeElement
    return active instanceof HTMLElement && (bar.contains(active) || (openIndex !== null && stack.contains(active)))
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
    if (target && (bar.contains(target) || stack.contains(target))) return
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
      dropMenuStyle()
      document.documentElement.style.removeProperty('--cmdr-demo-window-top')
    },
  }
}
