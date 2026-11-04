/**
 * The demo's right-click menus, in place of the native popups a browser doesn't have: when the
 * app sends a `show_*_context_menu` command, the backend asks the engine for the menu's rows
 * (`crates/cmdr-listing-wasm/src/context_menu.rs`, over `crates/cmdr-menu`'s `context`) and this
 * draws them, macOS 27 style, in the top frame (so a menu can reach past a panel's edge, as a
 * native one passes a window's). A pick sends what the native item sends (`menu-click.ts`). No
 * menu logic here: rows, words, shortcuts, enabled state, and what a pick does are the engine's.
 *
 * Keys are a Mac context menu's: ↑/↓ walk the enabled rows, Home/End jump, → opens a submenu,
 * ← or Esc closes it, Esc closes the menu, Enter / Space pick, Tab closes. A menu opened from the
 * keyboard (the app sends an anchor) starts on its first enabled row. Every key goes to the open
 * menu, never the app behind it.
 */
import type { ContextPopup, MenuEngineHandle } from './menu-engine.js'
import { type MenuClickDeps, performMenuClick } from './menu-click.js'
import { createMenuStack, ensureMenuStyle, labelText } from './menu-surfaces.js'

export interface ContextMenuDeps extends MenuClickDeps {
  menu: Pick<MenuEngineHandle, 'contextClick'>
}

export interface ContextMenuHost {
  /**
   * Opens `popup` at `at`, in the top frame's viewport pixels. `fromKeyboard` starts on the first
   * enabled row, as a menu opened by a key does.
   */
  open(popup: ContextPopup, at: { x: number; y: number }, fromKeyboard: boolean): void
  /** Closes the open menu, if any. */
  close(): void
  /** Takes the host down (a test's teardown). */
  destroy(): void
}

export function createContextMenuHost(deps: ContextMenuDeps): ContextMenuHost {
  const dropMenuStyle = ensureMenuStyle()
  // What had focus when the menu went up (in a panel, its iframe), so a pick or Esc puts it back.
  let returnFocus: HTMLElement | null = null

  const stack = createMenuStack((row) => {
    const outcome = deps.menu.contextClick(row.id)
    close(true)
    performMenuClick(outcome, deps)
  })

  function close(restore: boolean): void {
    if (!stack.isOpen) return
    stack.close()
    window.removeEventListener('keydown', onKey, true)
    document.removeEventListener('pointerdown', onPointerDown, true)
    window.removeEventListener('blur', onBlur)
    const target = returnFocus
    returnFocus = null
    if (restore && target?.isConnected) target.focus({ preventScroll: true })
  }

  const keys: Partial<Record<string, () => void>> = {
    ArrowDown: () => {
      stack.walk(1)
    },
    ArrowUp: () => {
      stack.walk(-1)
    },
    ArrowRight: () => {
      stack.openHighlightedSubmenu()
    },
    ArrowLeft: () => {
      stack.backOut()
    },
    Home: () => {
      stack.jumpToEnd(true)
    },
    End: () => {
      stack.jumpToEnd(false)
    },
    Enter: () => {
      stack.pickHighlighted()
    },
    ' ': () => {
      stack.pickHighlighted()
    },
    Escape: () => {
      if (!stack.backOut()) close(true)
    },
    Tab: () => {
      close(true)
    },
  }

  /** Every key while the menu is up is the menu's, as an open native menu swallows them. */
  function onKey(event: KeyboardEvent): void {
    const action = event.metaKey || event.altKey ? undefined : keys[event.key]
    if (action) {
      event.preventDefault()
      action()
    }
    event.stopPropagation()
  }

  function onPointerDown(event: PointerEvent): void {
    const target = event.target as Node | null
    if (target && stack.contains(target)) return
    // A click elsewhere closes the menu and goes where it went, as on a Mac.
    close(false)
  }

  function onBlur(): void {
    // The page lost focus (another tab, another app): a native menu closes too.
    close(false)
  }

  return {
    open(popup, at, fromKeyboard) {
      close(false)
      const active = document.activeElement
      returnFocus = active instanceof HTMLElement ? active : null
      // The header line, when the menu has one, names what the menu acts on: its accessible name.
      const header = popup.rows.find((row) => row.kind === 'header')
      stack.open(
        popup.rows,
        { left: at.x, top: at.y, label: header?.kind === 'header' ? labelText(header.label) : undefined },
        fromKeyboard,
      )
      window.addEventListener('keydown', onKey, true)
      document.addEventListener('pointerdown', onPointerDown, true)
      window.addEventListener('blur', onBlur)
    },
    close: () => {
      close(false)
    },
    destroy: () => {
      close(false)
      dropMenuStyle()
    },
  }
}
