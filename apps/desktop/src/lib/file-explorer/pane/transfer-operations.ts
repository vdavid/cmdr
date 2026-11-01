import { getSelectionSnapshot, type Initiator } from '#lib/tauri-commands/index.js'
import { readPressedRows, type PressedPane } from './pressed-rows'
import type { TransferOperationType, VolumeInfo } from '../types'
import type { DuplicateFollowUp } from './duplicate-rename'

export interface TransferContext {
  showHiddenFiles: boolean
  sourcePath: string
  destPath: string
  sourceVolumeId: string
  destVolumeId: string
  /** The MCP round-trip this build serves, which a refusal fails so the agent hears why nothing started. */
  mcpRequestId?: string
  /** When the build stops waiting for the rows to settle (`readPressedRows`); defaults to its own budget. */
  deadline?: number
}

export interface TransferDialogPropsData {
  operationType: TransferOperationType
  sourcePaths: string[]
  destinationPath: string
  direction: 'left' | 'right'
  currentVolumeId: string
  fileCount: number
  folderCount: number
  sourceFolderPath: string
  sourceVolumeId: string
  destVolumeId: string
  /** When true, dialog auto-confirms without user interaction (MCP auto-confirm). */
  autoConfirm?: boolean
  /** Conflict resolution policy for auto-confirm (MCP). Maps to ConflictResolution. */
  autoConfirmOnConflict?: string
  /** MCP round-trip id, present only for an auto-confirmed MCP op. Threaded to the
   *  progress state, which replies `mcp-response` with the spawned operationId. */
  mcpRequestId?: string
  /** Who started this operation. Carried into the progress dialog's write config so
   *  the backend records provenance, and only `user` may skip the confirmation.
   *  Required, like `duplicateFollowUp`: each trigger states it. */
  initiator: Initiator
  /**
   * What happens when this transfer duplicates ONE item in the folder it already
   * lived in. Forwarded to the progress dialog's config on confirm; see
   * `dialog-props.ts` for why every trigger has to answer it.
   */
  duplicateFollowUp: DuplicateFollowUp
  /**
   * Rename mode: the move renames its one source to this name in place. Set only
   * when F2 hands a rename that copies too much to start unasked (a big S3
   * folder) to the Move dialog (`rename-as-move.ts`).
   */
  newName?: string
}

/**
 * Dialog props before the TRIGGER has had its say.
 *
 * The builders below describe the transfer (what, from where, to where); only
 * the gesture that asked for it knows who started it and whether a single-item
 * duplicate should end in the rename editor. Leaving those fields out here is what
 * makes each call site state them, rather than inheriting a default nobody chose.
 */
export type TransferDialogPropsDraft = Omit<TransferDialogPropsData, 'duplicateFollowUp' | 'initiator'>

/**
 * The transfer of what the press names: the pane's selection, or with
 * `cursorFallback` its cursor row, read through `readPressedRows` (revision
 * checked, waiting out a busy folder within `context.deadline`). `null` when
 * there's nothing to transfer, or after telling the user (and an MCP caller)
 * nothing started because the rows never settled.
 */
async function buildTransferPropsFromPane(
  operationType: TransferOperationType,
  paneRef: PressedPane,
  isLeft: boolean,
  context: TransferContext,
  cursorFallback: boolean,
): Promise<TransferDialogPropsDraft | null> {
  const pressed = await readPressedRows(
    paneRef,
    ({ listingId, backendRows, expectedSequence }) =>
      getSelectionSnapshot(listingId, context.showHiddenFiles, backendRows, expectedSequence),
    { cursorFallback, deadline: context.deadline, mcpRequestId: context.mcpRequestId },
  )
  // Each builder answers for its own kind of press: the cursor builder never
  // transfers a selection that appeared under it.
  if (pressed.kind !== 'read' || pressed.onSelection === cursorFallback) return null
  const snapshot = pressed.value
  if (snapshot.paths.length === 0) return null

  return {
    operationType,
    sourcePaths: snapshot.paths,
    destinationPath: context.destPath,
    direction: isLeft ? 'right' : 'left',
    currentVolumeId: context.destVolumeId,
    fileCount: snapshot.fileCount,
    folderCount: snapshot.folderCount,
    sourceFolderPath: context.sourcePath,
    sourceVolumeId: context.sourceVolumeId,
    destVolumeId: context.destVolumeId,
  }
}

/** The transfer of the pane's selection. A selection that vanishes while the rows settle is refused. */
export function buildTransferPropsFromSelection(
  operationType: TransferOperationType,
  paneRef: PressedPane,
  isLeft: boolean,
  context: TransferContext,
): Promise<TransferDialogPropsDraft | null> {
  return buildTransferPropsFromPane(operationType, paneRef, isLeft, context, false)
}

/** The transfer of the pane's cursor row, with nothing selected. */
export function buildTransferPropsFromCursor(
  operationType: TransferOperationType,
  paneRef: PressedPane,
  isLeft: boolean,
  context: TransferContext,
): Promise<TransferDialogPropsDraft | null> {
  return buildTransferPropsFromPane(operationType, paneRef, isLeft, context, true)
}

/**
 * Builds transfer dialog props from a search-results snapshot's selection.
 * The snapshot pane has no backend listing, so the listing-id-keyed builders
 * don't apply: each entry already carries an absolute `path`. We compute the
 * common parent for display ("From …"), count files vs. folders so the
 * confirmation dialog shows accurate totals, and route everything else through
 * the same `TransferDialogPropsDraft` shape used by normal panes. See plan §3.7
 * (`isSourceOK: true`) and `search/CLAUDE.md` § "Snapshot store".
 *
 * `sourceVolumeId` is resolved by the caller (`snapshot-source-volume.ts`) and
 * passed in, never assumed: a search covers one volume and any volume with a
 * persisted index is searchable, so the rows can live on an SMB share or an MTP
 * storage. The transfer pipeline reads this to pick the dispatch path.
 */
export function buildTransferPropsFromSnapshot(
  operationType: TransferOperationType,
  sourcePaths: string[],
  isDirectoryFlags: boolean[],
  isLeft: boolean,
  destPath: string,
  destVolumeId: string,
  sourceVolumeId: string,
): TransferDialogPropsDraft | null {
  if (sourcePaths.length === 0) return null
  if (sourcePaths.length !== isDirectoryFlags.length) {
    // Defensive: a length mismatch means the caller resolved paths and flags
    // independently and one of them is stale. Bail rather than reporting wrong
    // counts on the confirmation dialog.
    return null
  }

  let fileCount = 0
  let folderCount = 0
  for (const isDir of isDirectoryFlags) {
    if (isDir) folderCount += 1
    else fileCount += 1
  }

  return {
    operationType,
    sourcePaths,
    destinationPath: destPath,
    direction: isLeft ? 'right' : 'left',
    currentVolumeId: destVolumeId,
    fileCount,
    folderCount,
    sourceFolderPath: getCommonParentPath(sourcePaths),
    sourceVolumeId,
    destVolumeId,
  }
}

/** Derives the common parent directory from a list of absolute paths. */
export function getCommonParentPath(paths: string[]): string {
  if (paths.length === 0) return '/'
  if (paths.length === 1) {
    const lastSlash = paths[0].lastIndexOf('/')
    return lastSlash > 0 ? paths[0].substring(0, lastSlash) : '/'
  }

  // Split each path and find the longest common prefix
  const segments = paths.map((p) => p.split('/'))
  const firstSegments = segments[0]
  let commonLength = 0
  for (let i = 0; i < firstSegments.length; i++) {
    if (segments.every((s) => s[i] === firstSegments[i])) {
      commonLength = i + 1
    } else {
      break
    }
  }

  const commonPath = firstSegments.slice(0, commonLength).join('/')
  return commonPath || '/'
}

/**
 * Builds transfer dialog props from externally dropped file paths.
 * Unlike the listing-based builders, this works with absolute paths directly
 * (no listing ID or pane ref needed).
 *
 * `sourceVolumeId` is RESOLVED by the caller (drag-drop-controller) via
 * `resolveSourceVolumeId` — the real volume the dropped paths live on, never a
 * placeholder. It's independent of `destVolumeId`: a cross-volume drop
 * (MTP→local, local→MTP) carries a different source and dest. Passing the dest
 * id here was a bug — it made the scan preview stat the source paths as the
 * wrong shape and report 0 bytes / 0 files (a wrong source volume id makes the preview report zeros).
 *
 * `isDirectoryFlags` (optional, index-aligned with `droppedPaths`) carries each
 * path's top-level kind from `statPathsKinds`: `true` = folder, `false` = file,
 * `null` = unknown. When every flag is known (no `null`) and the length matches,
 * the file/folder split flows to BOTH the confirmation dialog and the
 * completion toast. If ANY flag is unknown (a virtual MTP/SMB path on the
 * pasteboard, a vanished entry, a stat timeout) — or the flags are absent /
 * length-mismatched — the whole batch falls back to the legacy approximate
 * shape (`fileCount = count`, `folderCount = 0`). Honest beats half-right: a
 * partial split would misreport, so we degrade the whole batch at once.
 */
export function buildTransferPropsFromDroppedPaths(
  operationType: TransferOperationType,
  droppedPaths: string[],
  destPath: string,
  direction: 'left' | 'right',
  destVolumeId: string,
  sourceVolumeId: string,
  isDirectoryFlags?: (boolean | null)[],
): TransferDialogPropsDraft {
  const sourceFolderPath = getCommonParentPath(droppedPaths)

  const split = computeDroppedSplit(droppedPaths.length, isDirectoryFlags)

  return {
    operationType,
    sourcePaths: droppedPaths,
    destinationPath: destPath,
    direction,
    currentVolumeId: destVolumeId,
    fileCount: split.fileCount,
    folderCount: split.folderCount,
    sourceFolderPath,
    sourceVolumeId,
    destVolumeId,
  }
}

/**
 * Resolves the file/folder split for a dropped batch. Returns the real per-type
 * counts only when every flag is known and the array lines up with the path
 * count; otherwise the legacy approximate shape (all files, zero folders), which
 * makes the composer fall back to flattened wording. See the all-or-nothing
 * rationale on `buildTransferPropsFromDroppedPaths`.
 */
function computeDroppedSplit(
  pathCount: number,
  isDirectoryFlags?: (boolean | null)[],
): { fileCount: number; folderCount: number } {
  const allKnown =
    isDirectoryFlags !== undefined &&
    isDirectoryFlags.length === pathCount &&
    isDirectoryFlags.every((flag) => flag !== null)

  if (!allKnown) {
    return { fileCount: pathCount, folderCount: 0 }
  }

  let fileCount = 0
  let folderCount = 0
  for (const isDir of isDirectoryFlags) {
    if (isDir) folderCount += 1
    else fileCount += 1
  }
  return { fileCount, folderCount }
}

export function getDestinationVolumeInfo(
  volumeId: string,
  volumes: VolumeInfo[],
): { name: string; mountIsReadOnly: boolean } | undefined {
  const volume = volumes.find((v) => v.id === volumeId)
  if (volume) {
    return { name: volume.name, mountIsReadOnly: volume.mountIsReadOnly ?? false }
  }
  return undefined
}
