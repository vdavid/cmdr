/**
 * Selection handlers: toggle / toggle-and-down / select-all / deselect-all, the
 * MCP range-select, and the two selection-dialog openers (Select files… /
 * Deselect files…). `selection.selectAll` carries its own `activeElement` input
 * branch (a focused `<input>` selects its own text), distinct from the core's
 * pre-dispatch text-region intercept.
 */
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerRecord } from './types'

export const selectionHandlers = {
  'selection.toggle': ({ explorerRef }) => {
    explorerRef?.handleSelectionAction('toggleAtCursor')
  },

  'selection.toggleAndDown': ({ explorerRef }) => {
    explorerRef?.handleSelectionAction('toggleAtCursorAndMoveDown')
  },

  'selection.selectAll': ({ explorerRef }) => {
    // ⌘A is a native menu accelerator (so it shows in the Edit menu), which means
    // macOS intercepts it before the webview. When a text input is focused, route
    // to the input's select-all instead of file selection.
    const active = document.activeElement
    if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) {
      active.select()
      return
    }
    explorerRef?.handleSelectionAction('selectAll')
  },

  'selection.deselectAll': ({ explorerRef }) => {
    explorerRef?.handleSelectionAction('deselectAll')
  },

  'selection.mcpSelect': ({ explorerRef, dispatchArgs }) => {
    // MCP `select` tool: range/all selection on a SPECIFIC pane with a typed
    // mode (`replace`/`add`/`subtract`).
    const { pane, start, count, mode } = dispatchArgs as CommandArgs['selection.mcpSelect']
    explorerRef?.handleMcpSelect(pane, start, count, mode)
  },

  'selection.selectFiles': ({ ctx }) => {
    ctx.dialogs.showSelectionDialog('add')
  },

  'selection.deselectFiles': ({ ctx }) => {
    ctx.dialogs.showSelectionDialog('remove')
  },
} satisfies Partial<CommandHandlerRecord>
