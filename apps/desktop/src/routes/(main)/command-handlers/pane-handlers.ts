/**
 * Pane handlers: switch / swap the active pane, toggle each pane's volume
 * chooser, copy a path between panes, and the MCP `refresh` (re-list the focused
 * pane).
 */
import type { CommandHandlerRecord } from './types'

export const paneHandlers = {
  'pane.switch': ({ explorerRef }) => {
    explorerRef?.switchPane()
  },

  'pane.swap': ({ explorerRef }) => {
    explorerRef?.swapPanes()
  },

  'pane.leftVolumeChooser': ({ explorerRef }) => {
    explorerRef?.toggleVolumeChooser('left')
  },

  'pane.rightVolumeChooser': ({ explorerRef }) => {
    explorerRef?.toggleVolumeChooser('right')
  },

  'pane.copyPathLeftToRight': ({ explorerRef }) => {
    explorerRef?.copyPathBetweenPanes('left', 'right')
  },

  'pane.copyPathRightToLeft': ({ explorerRef }) => {
    explorerRef?.copyPathBetweenPanes('right', 'left')
  },

  'pane.refresh': async ({ explorerRef }) => {
    // MCP `refresh` tool: a round-trip — AWAIT so the adapter acks on a real
    // backend re-read, and a failure reaches its try/catch.
    await explorerRef?.refreshPane()
  },
} satisfies Partial<CommandHandlerRecord>
