/**
 * Tab handlers: open / close / reopen, cycle, pin, close others, and the per-pane
 * MCP tab action. `tab.close` awaits the close (and, on the last-tab branch,
 * awaits the window close after a lazy `@tauri-apps/api/window` import).
 */
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

export const tabHandlers = {
  'tab.new': ({ explorerRef }) => {
    const success = explorerRef?.newTab()
    if (success === false) {
      addToast(tString('commands.handler.tabLimitReached'), { level: 'warn' })
    }
  },

  'tab.close': async ({ explorerRef }) => {
    const result = await explorerRef?.closeActiveTabWithConfirmation()
    if (result === 'last-tab') {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().close()
    }
  },

  'tab.reopen': ({ explorerRef }) => {
    const result = explorerRef?.reopenLastClosedTab()
    if (result === 'empty') {
      addToast(tString('commands.handler.noRecentlyClosedTabs'), { level: 'warn' })
    } else if (result === 'cap') {
      addToast(tString('commands.handler.tabLimitReached'), { level: 'warn' })
    }
  },

  'tab.next': ({ explorerRef }) => {
    explorerRef?.cycleTab('next')
  },

  'tab.prev': ({ explorerRef }) => {
    explorerRef?.cycleTab('prev')
  },

  'tab.togglePin': ({ explorerRef }) => {
    explorerRef?.togglePinActiveTab()
  },

  'tab.closeOthers': ({ explorerRef }) => {
    explorerRef?.closeOtherTabs()
  },

  'tab.mcpAction': ({ explorerRef, dispatchArgs }) => {
    // MCP `tab` tool: a per-pane tab action targeting a SPECIFIC pane and tab
    // (the focused-pane `tab.new`/`tab.close`/etc. can't). Routes to the
    // component's `handleMcpTabAction`, which owns the tab-mutation primitives.
    const { pane, action, tabId, pinned } = dispatchArgs as CommandArgs['tab.mcpAction']
    explorerRef?.handleMcpTabAction(pane, action, tabId, pinned)
  },
} satisfies Partial<CommandHandlerRecord>
