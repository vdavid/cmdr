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
import { resolveGlobalKeyAction } from './global-keydown'
import { initShortcutDispatch, destroyShortcutDispatch } from '$lib/shortcuts/shortcut-dispatch'

// The resolver speaks the macOS combo vocabulary (⌘V, not Ctrl+V), and this is a
// macOS-only bug; `isMacOS()` reads the user agent.
vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })

/** A ⌘-modified keydown for `key`, with no other modifier held. */
function cmd(key: string): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, metaKey: true })
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
      expect(resolveGlobalKeyAction(cmd('v'), false)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
    })

    it('dispatches the pane refresh on ⌘R, so a manual re-read has a key at all', () => {
      // The one refresh key: it re-reads the focused pane's directory, and in the
      // network browser it re-scans hosts (`pane-commands.ts` routes on the view).
      expect(resolveGlobalKeyAction(cmd('r'), false)).toEqual({ kind: 'dispatch', commandId: 'pane.refresh' })
    })

    it('hands ⌘← / ⌘→ to a focused text input instead of dispatching', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('ArrowLeft'), false)).toEqual({ kind: 'ignore' })
      expect(resolveGlobalKeyAction(cmd('ArrowRight'), false)).toEqual({ kind: 'ignore' })
    })

    it('hands a bare typing key to a focused text input instead of dispatching', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', { key: 'Tab' }), false)).toEqual({ kind: 'ignore' })
    })

    it('ignores a combo no command claims', () => {
      expect(resolveGlobalKeyAction(new KeyboardEvent('keydown', { key: 'q' }), false)).toEqual({ kind: 'ignore' })
    })
  })

  describe('with a modal open', () => {
    it('blocks pane-scoped commands', () => {
      // ⌘T (new tab) fires with nothing open, and must not fire behind a dialog.
      expect(resolveGlobalKeyAction(cmd('t'), false)).toEqual({ kind: 'dispatch', commandId: 'tab.new' })
      expect(resolveGlobalKeyAction(cmd('t'), true)).toEqual({ kind: 'ignore' })
    })

    it('dispatches edit.paste when focus is in a text input, so WebKit does not ALSO paste', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('v'), true)).toEqual({ kind: 'dispatch', commandId: 'edit.paste' })
    })

    it('dispatches the rest of the text-editing family from a focused text input', () => {
      cleanupFocus = focus('textarea')
      expect(resolveGlobalKeyAction(cmd('c'), true)).toEqual({ kind: 'dispatch', commandId: 'edit.copy' })
      expect(resolveGlobalKeyAction(cmd('x'), true)).toEqual({ kind: 'dispatch', commandId: 'edit.cut' })
      expect(resolveGlobalKeyAction(cmd('a'), true)).toEqual({ kind: 'dispatch', commandId: 'selection.selectAll' })
    })

    it('leaves the text-editing family alone when focus is NOT in a text input', () => {
      cleanupFocus = focus('button')
      // ⌘A still gets suppressed so the browser doesn't select the whole page.
      expect(resolveGlobalKeyAction(cmd('a'), true)).toEqual({ kind: 'suppress' })
      expect(resolveGlobalKeyAction(cmd('v'), true)).toEqual({ kind: 'ignore' })
      expect(resolveGlobalKeyAction(cmd('c'), true)).toEqual({ kind: 'ignore' })
    })

    it('does not widen to a pane-scoped command that happens to be typed in an input', () => {
      cleanupFocus = focus('input')
      expect(resolveGlobalKeyAction(cmd('t'), true)).toEqual({ kind: 'ignore' })
    })

    it('does not match a modifier superset of a text-editing combo', () => {
      cleanupFocus = focus('input')
      // ⌥⌘V is "Paste as move" (a pane op), not ⌘V.
      const optionCmdV = new KeyboardEvent('keydown', { key: 'v', metaKey: true, altKey: true })
      expect(resolveGlobalKeyAction(optionCmdV, true)).toEqual({ kind: 'ignore' })
    })
  })
})
