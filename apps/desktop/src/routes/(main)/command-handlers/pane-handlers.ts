/**
 * Pane handlers: switch / swap the active pane, move focus to its controls, clone the focused pane into the
 * other, toggle each pane's volume chooser, copy a path between panes, refresh the focused pane (⌘R and the
 * MCP `refresh` tool), and move or center the divider between the panes.
 */
import type { CommandArgs } from '#lib/commands/index.js'
import type { CommandHandlerRecord } from './types'

/** Only an ABSENT payload reads as a person: every other road names who started it. */
function paneMoveBy(args: unknown) {
  const typed = args as CommandArgs['pane.switch'] | undefined
  return typed?.by ?? 'person'
}

export const paneHandlers = {
  'pane.switch': ({ explorerRef, dispatchArgs }) => {
    explorerRef?.switchPane(paneMoveBy(dispatchArgs))
  },

  'pane.quickFilter': ({ explorerRef }) => {
    explorerRef?.cycleQuickFilter()
  },

  'pane.focusControls': ({ explorerRef }) => {
    explorerRef?.focusPaneControls()
  },

  'pane.swap': ({ explorerRef, dispatchArgs }) => {
    explorerRef?.swapPanes(paneMoveBy(dispatchArgs))
  },

  'pane.clone': ({ explorerRef }) => {
    if (!explorerRef) return
    const source = explorerRef.getFocusedPane()
    const target = source === 'left' ? 'right' : 'left'
    explorerRef.copyPathBetweenPanes({ source, target, followCursor: false })
  },

  'pane.leftVolumeChooser': ({ explorerRef }) => {
    explorerRef?.toggleVolumeChooser('left')
  },

  'pane.rightVolumeChooser': ({ explorerRef }) => {
    explorerRef?.toggleVolumeChooser('right')
  },

  'pane.copyPathLeftToRight': ({ explorerRef }) => {
    explorerRef?.copyPathBetweenPanes({ source: 'left', target: 'right' })
  },

  'pane.copyPathRightToLeft': ({ explorerRef }) => {
    explorerRef?.copyPathBetweenPanes({ source: 'right', target: 'left' })
  },

  'pane.refresh': async ({ explorerRef }) => {
    // A round-trip for the MCP `refresh` tool: AWAIT so the adapter acks on a real
    // backend re-read, and a re-read still running past its wait reaches its
    // try/catch. The ⌘R path absorbs that rejection in `dispatchFromUi`, after the
    // toast `refreshPane` already raised.
    await explorerRef?.refreshPane()
  },

  'pane.moveDividerLeft': ({ explorerRef }) => {
    explorerRef?.moveDivider('left')
  },

  'pane.moveDividerRight': ({ explorerRef }) => {
    explorerRef?.moveDivider('right')
  },

  'pane.resetDivider': ({ explorerRef }) => {
    explorerRef?.resetDivider()
  },
} satisfies Partial<CommandHandlerRecord>
