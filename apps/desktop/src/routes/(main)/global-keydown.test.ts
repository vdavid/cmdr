/**
 * The global keydown decision: which combos dispatch, which are suppressed, and
 * which are handed back to the browser — with and without a modal open.
 *
 * The load-bearing case is ⌘V with a modal open and focus in a text input. Two
 * actors can insert there: WebKit's native paste (the key event's default action)
 * and the macOS Edit > Paste accelerator, which reaches `edit.paste` through the
 * menu listener. If this resolver returns `ignore`, both run and the text lands
 * TWICE. Returning `dispatch` is what kills the native one and lets the
 * cross-source dedup swallow the menu twin.
 *
 * Runs against the REAL registry and shortcut store (no mocks), so a rebound
 * default would surface here rather than silently passing.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { keydownWithAltGraph } from '#lib/shortcuts/test-key-events.js'
import { resolveGlobalKeyAction, unclaimedDispatchWarning } from './global-keydown'
import { claimKey } from '#lib/shortcuts/claim-key.js'
import { commands } from '#lib/commands/command-registry.js'
import type { DialogsOnScreen } from './command-dispatch-context'
import { initShortcutDispatch, destroyShortcutDispatch } from '#lib/shortcuts/shortcut-dispatch.js'
import { registerToastStack } from '#lib/ui/toast/toast-focus.js'
import { withPlatform } from '#lib/platform/test-platform.js'

// The resolver speaks the macOS combo vocabulary (⌘V, not Ctrl+V), and this is a
// macOS-only bug.
withPlatform('macos')

const NOTHING_OPEN: DialogsOnScreen = { dialogOpen: false, paletteOpen: false }
const DIALOG_OPEN: DialogsOnScreen = { dialogOpen: true, paletteOpen: false }
const PALETTE_OPEN: DialogsOnScreen = { dialogOpen: false, paletteOpen: true }

/** A ⌘-modified keydown for `key`, with no other modifier held. */
function cmd(key: string): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, metaKey: true })
}

/** An unmodified keydown for `key`. */
function bare(key: string): KeyboardEvent {
  return new KeyboardEvent('keydown', { key })
}

/** Mounts a focused element of `tag` and returns its cleanup. */
function focus(tag: 'input' | 'textarea' | 'button'): () => void {
  const el = document.createElement(tag)
  document.body.appendChild(el)
  el.focus()
  return () => {
    el.remove()
  }
}

describe('resolveGlobalKeyAction', () => {
  let cleanupFocus: (() => void) | undefined

  beforeEach(() => {
    initShortcutDispatch()
  })

  afterEach(() => {
    cleanupFocus?.()
    cleanupFocus = undefined
    destroyShortcutDispatch()
  })

  describe('with nothing open', () => {
    it('dispatches the command bound to the combo', () => {
      expect(resolveGlobalKeyAction(cmd('v'), NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
    })

    it('dispatches the pane refresh on ⌘R, so a manual re-read has a key at all', () => {
      // The one refresh key: it re-reads the focused pane's directory, and in the
      // network browser it re-scans hosts (`pane-commands.ts` routes on the view).
      expect(resolveGlobalKeyAction(cmd('r'), NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId: 'pane.refresh' })
    })

    it('hands ⌘← / ⌘→ to a focused text input instead of dispatching', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('ArrowLeft'), NOTHING_OPEN)).toEqual({ kind: 'ignore' })
      expect(resolveGlobalKeyAction(cmd('ArrowRight'), NOTHING_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('hands a bare typing key to a focused text input instead of dispatching', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', { key: 'Tab' }), NOTHING_OPEN)).toEqual({
        kind: 'ignore',
      })
    })

    it('dispatches the bare-key pane commands', () => {
      expect(resolveGlobalKeyAction(bare('Tab'), NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId: 'pane.switch' })
      expect(resolveGlobalKeyAction(bare(' '), NOTHING_OPEN)).toEqual({
        kind: 'dispatch',
        commandId: 'selection.toggle',
      })
    })

    it('ignores a combo no command claims', () => {
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', { key: 'q' }), NOTHING_OPEN)).toEqual({
        kind: 'ignore',
      })
    })
  })

  /**
   * Characterization pins for the screen-reader focus work (#437): the navigation keys
   * resolve the same wherever focus sits in the explorer, except on pane chrome (below),
   * which keeps Enter, Space, the arrows, and Home/End for the focused control.
   */
  describe('navigation keys, wherever focus sits in the explorer', () => {
    const IGNORE = { kind: 'ignore' }
    const NAV_KEYS: [string, KeyboardEventInit, unknown][] = [
      ['Enter', { key: 'Enter' }, { kind: 'dispatch', commandId: 'nav.open' }],
      ['Space', { key: ' ' }, { kind: 'dispatch', commandId: 'selection.toggle' }],
      // The arrows aren't Tier 1 commands: the pane handles them on the container's keydown.
      ['↑', { key: 'ArrowUp' }, IGNORE],
      ['↓', { key: 'ArrowDown' }, IGNORE],
      ['←', { key: 'ArrowLeft' }, IGNORE],
      ['→', { key: 'ArrowRight' }, IGNORE],
      ['Home', { key: 'Home' }, { kind: 'dispatch', commandId: 'nav.home' }],
      ['End', { key: 'End' }, { kind: 'dispatch', commandId: 'nav.end' }],
      ['Tab', { key: 'Tab' }, { kind: 'dispatch', commandId: 'pane.switch' }],
      // Bound to nothing, so the browser keeps it.
      ['⇧Tab', { key: 'Tab', shiftKey: true }, IGNORE],
    ]

    /** The explorer container holding focus, with a button inside it. */
    function mountExplorer(): { container: HTMLElement; button: HTMLButtonElement } {
      const container = document.createElement('div')
      container.className = 'dual-pane-explorer'
      container.tabIndex = 0
      const button = document.createElement('button')
      button.className = 'path-segment'
      container.appendChild(button)
      document.body.appendChild(container)
      cleanupFocus = () => {
        container.remove()
      }
      return { container, button }
    }

    it.each(NAV_KEYS)('%s resolves the same with the container focused', (_name, init, expected) => {
      const { container } = mountExplorer()
      container.focus()
      expect(document.activeElement).toBe(container)
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', init), NOTHING_OPEN)).toEqual(expected)
    })

    it.each(NAV_KEYS)('%s resolves the same with a button inside the explorer focused', (_name, init, expected) => {
      const { button } = mountExplorer()
      button.focus()
      expect(document.activeElement).toBe(button)
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', init), NOTHING_OPEN)).toEqual(expected)
    })

    /**
     * A focused pane-chrome control (`pane-chrome.ts`) keeps the keys a button or a toolbar
     * answers to; `Tab` stays `pane.switch` and modifier shortcuts still run.
     */
    const CHROME_KEYS: [string, KeyboardEventInit, unknown][] = [
      ['Enter', { key: 'Enter' }, IGNORE],
      ['Space', { key: ' ' }, IGNORE],
      ['↑', { key: 'ArrowUp' }, IGNORE],
      ['↓', { key: 'ArrowDown' }, IGNORE],
      ['←', { key: 'ArrowLeft' }, IGNORE],
      ['→', { key: 'ArrowRight' }, IGNORE],
      ['Home', { key: 'Home' }, IGNORE],
      ['End', { key: 'End' }, IGNORE],
      ['Tab', { key: 'Tab' }, { kind: 'dispatch', commandId: 'pane.switch' }],
      ['⇧Tab', { key: 'Tab', shiftKey: true }, IGNORE],
      ['⌘R', { key: 'r', metaKey: true }, { kind: 'dispatch', commandId: 'pane.refresh' }],
      ['⌘↓', { key: 'ArrowDown', metaKey: true }, { kind: 'dispatch', commandId: 'nav.open' }],
    ]

    it.each(CHROME_KEYS)('%s resolves for the control with a pane chrome button focused', (_name, init, expected) => {
      const { button } = mountExplorer()
      button.parentElement?.setAttribute('data-pane-chrome', '')
      button.focus()
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', init), NOTHING_OPEN)).toEqual(expected)
    })

    /**
     * A button in a toast (reached with "Move focus to the notifications", `toast-focus.ts`)
     * keeps the same keys, plus Tab, which walks the toasts there: Enter on Undo presses Undo,
     * it doesn't open the cursor row, and Tab doesn't switch panes.
     */
    const TOAST_KEYS: [string, KeyboardEventInit, unknown][] = CHROME_KEYS.map(([name, init, expected]) => [
      name,
      init,
      init.key === 'Tab' ? IGNORE : expected,
    ])

    it.each(TOAST_KEYS)('%s resolves for the button with a toast button focused', (_name, init, expected) => {
      const stack = document.createElement('div')
      const button = document.createElement('button')
      stack.appendChild(button)
      document.body.appendChild(stack)
      const unregister = registerToastStack({ element: () => stack, announce: () => {} })
      cleanupFocus = () => {
        unregister()
        stack.remove()
      }
      button.focus()
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', init), NOTHING_OPEN)).toEqual(expected)
    })
  })

  /**
   * With focus on a pane's file list, WebKit scrolls the list on any arrow, Page, Home/End,
   * or Space nobody `preventDefault`s, yanking the virtual view off the cursor (↓ on the last
   * row of Full view moved `scrollTop` 7,481 → 7,485 in the 2026-10-15 spike). The pane
   * claims the keys it acts on; what reaches here unclaimed from a list is suppressed.
   */
  describe('native scrolling of a focused file list', () => {
    /** Resolves `init` as a keydown dispatched on `target`, the way a real keypress lands. */
    function resolveOn(target: HTMLElement, init: KeyboardEventInit): unknown {
      let action: unknown
      const listener = (event: Event): void => {
        action = resolveGlobalKeyAction(event as KeyboardEvent, NOTHING_OPEN)
      }
      document.addEventListener('keydown', listener)
      target.dispatchEvent(new KeyboardEvent('keydown', { ...init, bubbles: true, cancelable: true }))
      document.removeEventListener('keydown', listener)
      return action
    }

    /** A Full-view-shaped list: the scroller (`data-file-list-surface`) around the listbox. */
    function mountList(): { scroller: HTMLElement; listbox: HTMLElement } {
      const scroller = document.createElement('div')
      scroller.setAttribute('data-file-list-surface', '')
      scroller.tabIndex = -1
      const listbox = document.createElement('div')
      listbox.setAttribute('role', 'listbox')
      listbox.tabIndex = -1
      scroller.appendChild(listbox)
      document.body.appendChild(scroller)
      cleanupFocus = () => {
        scroller.remove()
      }
      return { scroller, listbox }
    }

    it.each([
      ['↓', { key: 'ArrowDown' }],
      ['↑', { key: 'ArrowUp' }],
      ['←', { key: 'ArrowLeft' }],
      ['→', { key: 'ArrowRight' }],
    ])('suppresses an unclaimed %s on the focused list', (_name, init) => {
      const { listbox } = mountList()
      listbox.focus()
      expect(resolveOn(listbox, init)).toEqual({ kind: 'suppress' })
    })

    it('suppresses it on the scroller around the listbox too', () => {
      const { scroller } = mountList()
      scroller.focus()
      expect(resolveOn(scroller, { key: 'ArrowDown' })).toEqual({ kind: 'suppress' })
    })

    it('still dispatches a key a command claims', () => {
      const { listbox } = mountList()
      listbox.focus()
      expect(resolveOn(listbox, { key: 'Home' })).toEqual({ kind: 'dispatch', commandId: 'nav.home' })
      expect(resolveOn(listbox, { key: 'PageDown' })).toEqual({ kind: 'dispatch', commandId: 'nav.pageDown' })
      expect(resolveOn(listbox, { key: 'Tab' })).toEqual({ kind: 'dispatch', commandId: 'pane.switch' })
    })

    it('leaves the key alone outside a list', () => {
      const container = document.createElement('div')
      container.tabIndex = 0
      document.body.appendChild(container)
      cleanupFocus = () => {
        container.remove()
      }
      container.focus()
      expect(resolveOn(container, { key: 'ArrowDown' })).toEqual({ kind: 'ignore' })
    })

    // The rename editor lives inside the listbox: its caret keys are its own.
    it('leaves the key alone in a text input inside the list', () => {
      const { listbox } = mountList()
      const input = document.createElement('input')
      listbox.appendChild(input)
      input.focus()
      expect(resolveOn(input, { key: 'ArrowLeft' })).toEqual({ kind: 'ignore' })
    })
  })

  // Keypresses as macOS reports them (Safari key log, 2026-10-05: US layout plus a
  // PC-layout keyboard), and the layouts the character rule exists for.
  describe('keys from any keyboard and layout', () => {
    const key = (init: KeyboardEventInit) => new KeyboardEvent('keydown', init)

    it.each([
      ['US ⇧8', { key: '*', code: 'Digit8', shiftKey: true }, 'selection.invert'],
      ["Swedish ⇧'", { key: '*', code: 'Backslash', shiftKey: true }, 'selection.invert'],
      ['numpad *', { key: '*', code: 'NumpadMultiply' }, 'selection.invert'],
      ['AltGr-style ⌥ *', { key: '*', code: 'Slash', altKey: true }, 'selection.invert'],
      ['US ⇧=', { key: '+', code: 'Equal', shiftKey: true }, 'selection.selectFiles'],
      ['Hungarian ⇧3', { key: '+', code: 'Digit3', shiftKey: true }, 'selection.selectFiles'],
      ['numpad +', { key: '+', code: 'NumpadAdd' }, 'selection.selectFiles'],
      ['numpad -', { key: '-', code: 'NumpadSubtract' }, 'selection.deselectFiles'],
      ['US ⌥⇧= (types ±)', { key: '±', code: 'Equal', altKey: true, shiftKey: true }, 'selection.selectSameKind'],
      ['numpad ⌥+', { key: '+', code: 'NumpadAdd', altKey: true }, 'selection.selectSameKind'],
      ['forward delete ⌦', { key: 'Delete', code: 'Delete' }, 'file.delete'],
      ['PC Insert (macOS says Help)', { key: 'Help', code: 'Help' }, 'selection.toggleAndDown'],
      // GitHub #1044: a Russian layout reports the Cyrillic letter, so these were dead.
      ['Russian ⌘T (types е)', { key: 'е', code: 'KeyT', metaKey: true }, 'tab.new'],
      ['Russian ⌃D (types в)', { key: 'в', code: 'KeyD', ctrlKey: true }, 'favorites.open'],
    ])('%s dispatches %s', (_name, init, commandId) => {
      expect(resolveGlobalKeyAction(key(init), NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId })
    })

    it('leaves typed symbols to a focused text input, whichever modifier typed them', () => {
      cleanupFocus = focus('input')
      for (const init of [
        { key: '*', code: 'Digit8', shiftKey: true },
        { key: '*', code: 'Slash', altKey: true },
        { key: '±', code: 'Equal', altKey: true, shiftKey: true },
        { key: 'Delete', code: 'Delete' },
        { key: 'В', code: 'KeyD', shiftKey: true },
        { key: 'в', code: 'KeyD' },
      ]) {
        expect(resolveGlobalKeyAction(key(init), NOTHING_OPEN)).toEqual({ kind: 'ignore' })
      }
    })

    // Windows reports AltGr as Ctrl+Alt, so by key position German AltGr+Q (`@`) also meant
    // `Ctrl+Alt+Q` and opened the queue. AltGr types; only what it typed may reach a binding.
    describe('AltGr on Windows', () => {
      const windowsAltGr = (init: KeyboardEventInit) => keydownWithAltGraph({ ctrlKey: true, altKey: true, ...init })

      let undoPlatform: () => void
      beforeEach(() => {
        undoPlatform = withPlatform('windows')
        // Rebuild the dispatch map on the Windows keymap.
        destroyShortcutDispatch()
        initShortcutDispatch()
      })
      afterEach(() => {
        undoPlatform()
      })

      it.each([
        ['German AltGr+Q (types @)', { key: '@', code: 'KeyQ' }],
        ['Hungarian AltGr+V (types @)', { key: '@', code: 'KeyV' }],
        ['Hungarian AltGr+C (types &)', { key: '&', code: 'KeyC' }],
        ['Polish AltGr+A (types ą)', { key: 'ą', code: 'KeyA' }],
      ])('%s runs no Ctrl+Alt command', (_name, init) => {
        expect(resolveGlobalKeyAction(windowsAltGr(init), NOTHING_OPEN)).toEqual({ kind: 'ignore' })
      })

      it('lets an AltGr-typed * reach its binding, and leaves it to a focused text input', () => {
        const hungarianStar = { key: '*', code: 'Slash' }
        expect(resolveGlobalKeyAction(windowsAltGr(hungarianStar), NOTHING_OPEN)).toEqual({
          kind: 'dispatch',
          commandId: 'selection.invert',
        })
        cleanupFocus = focus('input')
        expect(resolveGlobalKeyAction(windowsAltGr(hungarianStar), NOTHING_OPEN)).toEqual({ kind: 'ignore' })
      })

      it('still runs Ctrl+Alt+Q pressed on a US layout, which has no AltGr', () => {
        const usCtrlAltQ = key({ key: 'q', code: 'KeyQ', ctrlKey: true, altKey: true })
        expect(resolveGlobalKeyAction(usCtrlAltQ, NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId: 'queue.show' })
      })
    })
  })

  describe('with a modal open', () => {
    it('blocks pane-scoped commands', () => {
      // ⌘T (new tab) fires with nothing open, and must not fire behind a dialog.
      expect(resolveGlobalKeyAction(cmd('t'), NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId: 'tab.new' })
      expect(resolveGlobalKeyAction(cmd('t'), DIALOG_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('blocks the BARE-key pane commands, so Tab moves focus inside the dialog', () => {
      // The asymmetry a caller that under-reports its dialogs produces: ⇧Tab is bound to
      // nothing, so it resolves to `ignore` and the browser moves focus, while Tab reaches
      // `pane.switch`, gets `preventDefault`ed, and focus never moves at all. Space is the
      // same shape (`selection.toggle`), and so are F5 / F6 / F7 / Insert.
      expect(resolveGlobalKeyAction(bare('Tab'), DIALOG_OPEN)).toEqual({ kind: 'ignore' })
      expect(resolveGlobalKeyAction(bare(' '), DIALOG_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('dispatches edit.paste when focus is in a text input, so WebKit does not ALSO paste', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('v'), DIALOG_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
    })

    it('claims a Russian-layout ⌘V in a text input too, so the clipboard text lands once', () => {
      // The menu's Paste fires by key position on any layout; left unclaimed, the `⌘М`
      // keydown would let WebKit paste a second time.
      cleanupFocus = focus('input')
      const russianCmdV = new KeyboardEvent('keydown', { key: 'м', code: 'KeyV', metaKey: true })
      expect(resolveGlobalKeyAction(russianCmdV, DIALOG_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
      expect(resolveGlobalKeyAction(russianCmdV, NOTHING_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
    })

    it('dispatches the rest of the text-editing family from a focused text input', () => {
      cleanupFocus = focus('textarea')
      expect(resolveGlobalKeyAction(cmd('c'), DIALOG_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.copy' })
      expect(resolveGlobalKeyAction(cmd('x'), DIALOG_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.cut' })
      expect(resolveGlobalKeyAction(cmd('a'), DIALOG_OPEN)).toEqual({
        kind: 'dispatch',
        commandId: 'selection.selectAll',
      })
    })

    it('leaves the text-editing family alone when focus is NOT in a text input', () => {
      cleanupFocus = focus('button')
      // ⌘A still gets suppressed so the browser doesn't select the whole page.
      expect(resolveGlobalKeyAction(cmd('a'), DIALOG_OPEN)).toEqual({ kind: 'suppress' })
      expect(resolveGlobalKeyAction(cmd('v'), DIALOG_OPEN)).toEqual({ kind: 'ignore' })
      expect(resolveGlobalKeyAction(cmd('c'), DIALOG_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('does not widen to a pane-scoped command that happens to be typed in an input', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('t'), DIALOG_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('does not match a modifier superset of a text-editing combo', () => {
      cleanupFocus = focus('input')
      // ⌥⌘V is "Paste as move" (a pane op), not ⌘V.
      const optionCmdV = new KeyboardEvent('keydown', { key: 'v', metaKey: true, altKey: true })
      expect(resolveGlobalKeyAction(optionCmdV, DIALOG_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('still dispatches a command that runs over dialogs, like Settings on ⌘,', () => {
      expect(resolveGlobalKeyAction(cmd(','), DIALOG_OPEN)).toEqual({ kind: 'dispatch', commandId: 'app.settings' })
    })
  })

  describe('with the command palette open', () => {
    it('blocks pane-scoped commands behind it', () => {
      expect(resolveGlobalKeyAction(cmd('t'), PALETTE_OPEN)).toEqual({ kind: 'ignore' })
    })

    it("dispatches paste into the palette's search field", () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('v'), PALETTE_OPEN)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
    })
  })

  /**
   * The alarm for the bug class this resolver can't prevent: `preventDefault` alone
   * doesn't stop a dispatch, so a local handler that acted without claiming gets its
   * command run a second time. Four shipped bugs came from exactly that, all of them
   * invisible (an OS `open` that re-focuses a window, an idempotent Home key).
   */
  describe('the unclaimed-dispatch alarm', () => {
    /** A real keydown is cancelable; `preventDefault` is a no-op on one that isn't. */
    function cancelable(key: string): KeyboardEvent {
      return new KeyboardEvent('keydown', { key, cancelable: true })
    }

    it('says so when the key arrives with its default already prevented', () => {
      const e = cancelable('Enter')
      e.preventDefault()
      const warning = unclaimedDispatchWarning(e, 'nav.open')
      expect(warning).toContain('nav.open')
      expect(warning).toContain('SECOND time')
    })

    it('stays quiet for a key no local handler touched', () => {
      expect(unclaimedDispatchWarning(cancelable('Enter'), 'nav.open')).toBeNull()
    })

    it('trips on a `preventDefault`-only handler and not on a claimed one', () => {
      // The whole distinction in one test: both handlers prevent the default, and
      // only the unclaimed one leaves the event able to reach this road at all.
      const prevented = cancelable('PageDown')
      prevented.preventDefault()
      expect(unclaimedDispatchWarning(prevented, 'nav.pageDown')).not.toBeNull()

      const claimed = cancelable('PageDown')
      const stopped = vi.spyOn(claimed, 'stopPropagation')
      claimKey(claimed)
      expect(stopped).toHaveBeenCalled()
    })
  })

  // Anything but `ignore` for an unprevented Escape means `+page.svelte` prevents
  // it, and a prevented Escape never reaches AppKit, which would leave full screen.
  describe('Escape', () => {
    it('is an unused Escape when nothing is open and nothing used it', () => {
      expect(resolveGlobalKeyAction(bare('Escape'), NOTHING_OPEN)).toEqual({ kind: 'unusedEscape' })
    })

    it('is left alone when a local handler already used it', () => {
      const e = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })
      e.preventDefault()
      expect(resolveGlobalKeyAction(e, NOTHING_OPEN)).toEqual({ kind: 'ignore' })
    })

    it('is only suppressed over a dialog or the palette, never an unused one', () => {
      expect(resolveGlobalKeyAction(bare('Escape'), DIALOG_OPEN)).toEqual({ kind: 'suppress' })
      expect(resolveGlobalKeyAction(bare('Escape'), PALETTE_OPEN)).toEqual({ kind: 'suppress' })
    })

    it('is only suppressed in a text field', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(bare('Escape'), NOTHING_OPEN)).toEqual({ kind: 'suppress' })
    })

    it('stays off the dispatch road: every Escape binding is handled where it lives', () => {
      // If this fails, a command started relying on the central dispatch for Escape,
      // which `resolveEscape` never does. Wire it there, or handle it in its component.
      const escapeBound = commands
        .filter((command) => Object.values(command.shortcuts).some((keys) => keys.includes('Escape')))
        .map((command) => command.id)
        .sort()
      expect(escapeBound).toEqual(['about.close', 'palette.close', 'queue.close', 'share.back', 'volume.close'])
    })
  })
})
