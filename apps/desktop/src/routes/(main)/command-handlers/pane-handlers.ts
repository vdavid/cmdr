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

  'pane.refresh': ({ explorerRef }) => {
    // MCP `refresh` tool: re-list the focused pane.
    explorerRef?.refreshPane()
  },
} satisfies Partial<CommandHandlerRecord>
