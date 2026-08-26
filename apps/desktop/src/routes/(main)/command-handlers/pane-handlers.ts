/**
 * Pane handlers: switch / swap the active pane, toggle each pane's volume
 * chooser, copy a path between panes, and refresh the focused pane (⌘R and the
 * MCP `refresh` tool).
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
} satisfies Partial<CommandHandlerRecord>
