import { getFileAt, getListingStats, getPathsAtIndices } from '$lib/tauri-commands'
import { toBackendIndices, toBackendCursorIndex } from '$lib/file-operations/transfer/transfer-dialog-utils'
import type { SortColumn, SortOrder, TransferOperationType, VolumeInfo } from '../types'
import type { FilePaneAPI } from './types'

export interface TransferContext {
  showHiddenFiles: boolean
  sourcePath: string
  destPath: string
  sourceVolumeId: string
  destVolumeId: string
  sortColumn: SortColumn
  sortOrder: SortOrder
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
  sortColumn: SortColumn
  sortOrder: SortOrder
  sourceVolumeId: string
  destVolumeId: string
  /** When true, dialog auto-confirms without user interaction (MCP auto-confirm). */
  autoConfirm?: boolean
  /** Conflict resolution policy for auto-confirm (MCP). Maps to ConflictResolution. */
  autoConfirmOnConflict?: string
}

export async function getSelectedFilePaths(
  listingId: string,
  selectedIndices: number[],
  showHiddenFiles: boolean,
  hasParent: boolean,
): Promise<string[]> {
  return getPathsAtIndices(listingId, selectedIndices, showHiddenFiles, hasParent)
}

export async function buildTransferPropsFromSelection(
  operationType: TransferOperationType,
  listingId: string,
  selectedIndices: number[],
  hasParent: boolean,
  isLeft: boolean,
  context: TransferContext,
): Promise<TransferDialogPropsData | null> {
  const backendIndices = toBackendIndices(selectedIndices, hasParent)
  if (backendIndices.length === 0) return null

  const stats = await getListingStats(listingId, context.showHiddenFiles, backendIndices)
  const sourcePaths = await getSelectedFilePaths(listingId, selectedIndices, context.showHiddenFiles, hasParent)
  if (sourcePaths.length === 0) return null

  return {
    operationType,
    sourcePaths,
    destinationPath: context.destPath,
    direction: isLeft ? 'right' : 'left',
    currentVolumeId: context.destVolumeId,
    fileCount: stats.selectedFiles ?? 0,
    folderCount: stats.selectedDirs ?? 0,
    sourceFolderPath: context.sourcePath,
    sortColumn: context.sortColumn,
    sortOrder: context.sortOrder,
    sourceVolumeId: context.sourceVolumeId,
    destVolumeId: context.destVolumeId,
  }
}

export async function buildTransferPropsFromCursor(
  operationType: TransferOperationType,
  listingId: string,
  paneRef: FilePaneAPI | undefined,
  hasParent: boolean,
  isLeft: boolean,
  context: TransferContext,
): Promise<TransferDialogPropsData | null> {
  const cursorIndex = paneRef?.getCursorIndex()
  const backendIndex = toBackendCursorIndex(cursorIndex ?? -1, hasParent)
  if (backendIndex === null) return null

  const file = await getFileAt(listingId, backendIndex, context.showHiddenFiles)
  if (!file || file.name === '..') return null

  return {
    operationType,
    sourcePaths: [file.path],
    destinationPath: context.destPath,
    direction: isLeft ? 'right' : 'left',
    currentVolumeId: context.destVolumeId,
    fileCount: file.isDirectory ? 0 : 1,
    folderCount: file.isDirectory ? 1 : 0,
    sourceFolderPath: context.sourcePath,
    sortColumn: context.sortColumn,
    sortOrder: context.sortOrder,
    sourceVolumeId: context.sourceVolumeId,
    destVolumeId: context.destVolumeId,
  }
}

/**
 * Builds transfer dialog props from a search-results snapshot's selection.
 * The snapshot pane has no backend listing, so the listing-id-keyed builders
 * don't apply: each entry already carries an absolute `path`. We compute the
 * common parent for display ("From …"), count files vs. folders so the
 * confirmation dialog shows accurate totals, and route everything else through
 * the same `TransferDialogPropsData` shape used by normal panes. See plan §3.7
 * (`isSourceOK: true`) and `search/CLAUDE.md` § "Snapshot store".
 *
 * `sourceVolumeId` is `'root'` because snapshot entries are always real local
 * files (the indexer doesn't index remote volumes today). The transfer pipeline
 * uses this to choose the local-filesystem path; if we ever index SMB / MTP,
 * the per-entry volume needs to be resolved here.
 */
export function buildTransferPropsFromSnapshot(
  operationType: TransferOperationType,
  sourcePaths: string[],
  isDirectoryFlags: boolean[],
  isLeft: boolean,
  destPath: string,
  destVolumeId: string,
  sortColumn: SortColumn,
  sortOrder: SortOrder,
): TransferDialogPropsData | null {
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
    sortColumn,
    sortOrder,
    sourceVolumeId: 'root',
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
  sortColumn: SortColumn,
  sortOrder: SortOrder,
  isDirectoryFlags?: (boolean | null)[],
): TransferDialogPropsData {
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
    sortColumn,
    sortOrder,
    sourceVolumeId: destVolumeId,
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
): { name: string; isReadOnly: boolean } | undefined {
  const volume = volumes.find((v) => v.id === volumeId)
  if (volume) {
    return { name: volume.name, isReadOnly: volume.isReadOnly ?? false }
  }
  return undefined
}
