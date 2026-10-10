/**
 * A single-item Move inside its own folder, handed to the SOURCE pane's rename flow
 * instead of a transfer (`transfer-target.ts::isRenameInPlace` flags it on the
 * confirm). Why the rename engine runs it: `file-operations/transfer/DETAILS.md`
 * § "Single-item destinations".
 */

import type { TransferDialogPropsData } from './transfer-operations'
import type { FilePaneAPI } from './types'

/** The two panes, by side. */
export interface PaneRefs {
  getLeftPaneRef: () => FilePaneAPI | undefined
  getRightPaneRef: () => FilePaneAPI | undefined
}

/**
 * Starts the rename and returns `true`, or `false` to send the move down the
 * transfer instead: an agent's MCP call waits for an operation id, and a pane that
 * has since left the folder can't rename what it no longer shows.
 */
export function renameInSourcePane(props: TransferDialogPropsData, newName: string, panes: PaneRefs): boolean {
  if (props.mcpRequestId !== undefined || props.sourcePaths.length !== 1) return false
  // `direction` points at the destination pane, so the source is the other one.
  const sourcePane = props.direction === 'right' ? panes.getLeftPaneRef() : panes.getRightPaneRef()
  const trim = (path: string) => path.replace(/\/+$/, '') || '/'
  if (!sourcePane || trim(sourcePane.getCurrentPath()) !== trim(props.sourceFolderPath)) return false
  const isDirectory = props.folderCount === 1 && props.fileCount === 0
  sourcePane.startRename({ initialName: newName, commitTarget: { path: props.sourcePaths[0], isDirectory } })
  return true
}
