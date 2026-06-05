/**
 * Navigation handlers: open / parent / back / forward, the Home/End/PageUp/PageDown
 * key forwards, and the cursor positioning ids. `nav.openUnderCursor` and
 * `cursor.moveTo` are the two MCP round-trip ids: they stay `async` + `await` so
 * the adapter acks on real completion (the ack-timing contract).
 */
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

export const navHandlers = {
  'nav.open': ({ explorerRef }) => {
    explorerRef?.sendKeyToFocusedPane('Enter')
  },

  'nav.parent': ({ explorerRef }) => {
    explorerRef?.navigate({ pane: explorerRef.getFocusedPane(), to: { history: 'parent' }, source: 'user' })
  },

  'nav.back': ({ explorerRef }) => {
    explorerRef?.navigate({ pane: explorerRef.getFocusedPane(), to: { history: 'back' }, source: 'user' })
  },

  'nav.forward': ({ explorerRef }) => {
    explorerRef?.navigate({ pane: explorerRef.getFocusedPane(), to: { history: 'forward' }, source: 'user' })
  },

  'nav.home': ({ explorerRef }) => {
    explorerRef?.sendKeyToFocusedPane('Home')
  },

  'nav.end': ({ explorerRef }) => {
    explorerRef?.sendKeyToFocusedPane('End')
  },

  'nav.pageUp': ({ explorerRef }) => {
    explorerRef?.sendKeyToFocusedPane('PageUp')
  },

  'nav.pageDown': ({ explorerRef }) => {
    explorerRef?.sendKeyToFocusedPane('PageDown')
  },

  'nav.openUnderCursor': async ({ explorerRef }) => {
    // MCP `open_under_cursor` round-trip: AWAIT so the adapter's
    // `emit('mcp-response', { ok: true })` fires only after the open completes
    // (directory listed, or OS open-with-default dispatched). An exception
    // propagates to the adapter's try/catch, which replies `ok: false`.
    await explorerRef?.openItemUnderCursor()
  },

  'cursor.moveTo': async ({ explorerRef, dispatchArgs }) => {
    // MCP `move_cursor` round-trip: AWAIT for the same ack-timing reason. L1/L2
    // (focus re-anchor + `whenLoadSettles`) live inside `moveCursor` — untouched.
    const { pane, to } = dispatchArgs as CommandArgs['cursor.moveTo']
    await explorerRef?.moveCursor(pane, to)
  },

  'cursor.scrollTo': ({ explorerRef, dispatchArgs }) => {
    const { pane, index } = dispatchArgs as CommandArgs['cursor.scrollTo']
    explorerRef?.scrollTo(pane, index)
  },
} satisfies Partial<CommandHandlerRecord>
