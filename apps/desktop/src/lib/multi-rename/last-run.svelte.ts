/**
 * The last Multi-Rename run of this session, which the sheet's Undo rename (⌘⌥Z) rolls
 * back. Outlives the sheet, so the flow is: rename, look at the pane, ⌃M, ⌘⌥Z.
 * Not persisted: after a restart, the operation log is the place to undo.
 */

export interface MultiRenameRun {
  operationId: string
  renaming: number
}

let lastRun = $state<MultiRenameRun | null>(null)

export function getLastMultiRenameRun(): MultiRenameRun | null {
  return lastRun
}

export function setLastMultiRenameRun(run: MultiRenameRun | null): void {
  lastRun = run
}
