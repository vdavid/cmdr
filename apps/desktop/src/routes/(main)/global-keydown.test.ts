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
import { resolveGlobalKeyAction, unclaimedDispatchWarning } from './global-keydown'
import { claimKey } from '$lib/shortcuts/claim-key'
import { commands } from '$lib/commands/command-registry'
import type { DialogsOnScreen } from './command-dispatch-context'
import { initShortcutDispatch, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'

// The resolver speaks the macOS combo vocabulary (⌘V, not Ctrl+V), and this is a
// macOS-only bug; `isMacOS()` reads the user agent.
vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })

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
      ]) {
        expect(resolveGlobalKeyAction(key(init), NOTHING_OPEN)).toEqual({ kind: 'ignore' })
      }
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
        .filter((command) => command.shortcuts.includes('Escape'))
        .map((command) => command.id)
        .sort()
      expect(escapeBound).toEqual(['about.close', 'palette.close', 'share.back', 'volume.close'])
    })
  })
})
