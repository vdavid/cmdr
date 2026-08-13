/**
 * Birth: turning a confirmed transfer into a named backend operation.
 *
 * One call, one operation. Everything about WHICH command a copy, move,
 * compress, delete, or trash routes to lives here, and nothing about what
 * happens afterwards does: the moment the backend answers with an `operationId`,
 * the operation is something a session watches and a view renders
 * (`../operation-session/CLAUDE.md`).
 *
 * It reads three settings once, at dispatch, rather than reactively: an
 * operation is configured when it starts, and a slider moved mid-copy must not
 * change what the running transfer is doing.
 */

import {
  compressFiles,
  copyBetweenVolumes,
  deleteFiles,
  moveBetweenVolumes,
  moveFiles,
  trashFiles,
  DEFAULT_VOLUME_ID,
  type Initiator,
} from '$lib/tauri-commands'
import type { ConflictResolution, SortColumn, SortOrder, TransferOperationType } from '$lib/file-explorer/types'
import { getSetting } from '$lib/settings'
import { pathInsideArchive } from '$lib/file-explorer/pane/volume-capabilities'

/** Everything the backend needs to start this operation. Captured at the moment
 *  the user confirmed, and never re-read afterwards. */
export interface TransferDispatchConfig {
  operationType: TransferOperationType
  sourcePaths: string[]
  /** Destination path (not applicable for delete/trash). */
  destinationPath?: string
  /** Current sort column on the source pane (files processed in this order). */
  sortColumn: SortColumn
  /** Current sort order on the source pane. */
  sortOrder: SortOrder
  /** Preview scan ID from `TransferDialog` (for reusing scan results), or null. */
  previewId: string | null
  /** Source volume ID (like "root", "mtp-336592896:65537"). */
  sourceVolumeId: string
  /** Destination volume ID (not applicable for delete/trash). */
  destVolumeId?: string
  /** Conflict resolution policy from `TransferDialog` (not applicable for delete/trash). */
  conflictResolution?: ConflictResolution
  /** Source filenames known to conflict at dest (forwarded so the BE bulk-skips them under `Skip all`). */
  preKnownConflicts?: string[]
  /** Per-item sizes for trash progress (from scan or drive index). */
  itemSizes?: number[]
  /** Who triggered this operation. `undefined`/`user` for direct UI actions;
   *  `aiClient` when an MCP tool initiated it (drives the operation-log provenance). */
  initiator?: Initiator
}

/**
 * A move whose source OR destination is inside a zip must NOT take the local
 * `moveFiles` fast-path: an archive-inner path isn't a real folder, and the
 * backend fast-path rejects it. Route it through `moveBetweenVolumes`, which
 * resolves the archive boundary and runs the managed archive-edit flow (move
 * into = `{ add }`, move out = extract + `{ delete }`). Source and dest can share
 * the parent drive's `volumeId` (a zip lives on the same drive), so the volume-id
 * comparison alone misses this — the path check is what catches it.
 */
export function isVolumeMove(config: TransferDispatchConfig): boolean {
  if (config.operationType !== 'move') return false
  const touchesArchive =
    pathInsideArchive(config.destinationPath ?? '') || config.sourcePaths.some((p) => pathInsideArchive(p))
  return (
    config.sourceVolumeId !== DEFAULT_VOLUME_ID ||
    (config.destVolumeId ?? DEFAULT_VOLUME_ID) !== DEFAULT_VOLUME_ID ||
    touchesArchive
  )
}

/** Starts the operation and resolves with the id the backend gave it. Rejects
 *  with the backend's structured `WriteOperationError` on a validation failure. */
export function dispatchTransferOperation(config: TransferDispatchConfig): Promise<{ operationId: string }> {
  const progressIntervalMs = getSetting('fileOperations.progressUpdateInterval')
  const maxConflictsToShow = getSetting('fileOperations.maxConflictsToShow')
  // Threaded into every zip-writing path (Compress, and copy/move INTO an
  // archive); the backend clamps it to 1..=9 and ignores it for non-archive
  // copies.
  const compressionLevel = getSetting('behavior.archiveCompressionLevel')

  if (config.operationType === 'trash') {
    return trashFiles(
      config.sourcePaths,
      config.itemSizes,
      { progressIntervalMs, previewId: config.previewId },
      config.initiator,
    )
  }
  if (config.operationType === 'delete') {
    return deleteFiles(
      config.sourcePaths,
      { progressIntervalMs, sortColumn: config.sortColumn, sortOrder: config.sortOrder, previewId: config.previewId },
      config.sourceVolumeId,
      config.initiator,
    )
  }
  if (config.operationType === 'move') {
    // Volume move (MTP or other non-local); backend handles same-volume, cross-volume, etc.
    if (isVolumeMove(config)) {
      return moveBetweenVolumes(
        config.sourceVolumeId,
        config.sourcePaths,
        config.destVolumeId ?? DEFAULT_VOLUME_ID,
        config.destinationPath ?? '',
        {
          conflictResolution: config.conflictResolution ?? 'stop',
          progressIntervalMs,
          maxConflictsToShow,
          previewId: config.previewId,
          preKnownConflicts: config.preKnownConflicts ?? [],
          compressionLevel,
        },
        config.initiator,
      )
    }
    // Local-to-local move
    return moveFiles(
      config.sourcePaths,
      config.destinationPath ?? '',
      {
        conflictResolution: config.conflictResolution,
        progressIntervalMs,
        maxConflictsToShow,
        sortColumn: config.sortColumn,
        sortOrder: config.sortOrder,
        previewId: config.previewId,
        preKnownConflicts: config.preKnownConflicts ?? [],
      },
      config.initiator,
    )
  }
  if (config.operationType === 'compress') {
    return dispatchCompress(config, progressIntervalMs, maxConflictsToShow, compressionLevel)
  }
  return dispatchCopy(config, progressIntervalMs, maxConflictsToShow, compressionLevel)
}

/** Copy: always via `copyBetweenVolumes`; the backend optimizes local-to-local. */
function dispatchCopy(
  config: TransferDispatchConfig,
  progressIntervalMs: number,
  maxConflictsToShow: number,
  compressionLevel: number,
): Promise<{ operationId: string }> {
  return copyBetweenVolumes(
    config.sourceVolumeId,
    config.sourcePaths,
    config.destVolumeId ?? DEFAULT_VOLUME_ID,
    config.destinationPath ?? '',
    {
      conflictResolution: config.conflictResolution ?? 'stop',
      progressIntervalMs,
      maxConflictsToShow,
      previewId: config.previewId,
      preKnownConflicts: config.preKnownConflicts ?? [],
      compressionLevel,
    },
    config.initiator,
  )
}

/**
 * Compress: pack the sources into a NEW zip at the target. One command handles
 * local and (later) remote sources; the backend seeds a valid empty zip then packs.
 * The inner-conflict policy is moot (a fresh zip has no entries and two sources
 * in one folder can't share a name), so `overwrite` is a safe constant; an
 * existing target FILE was already resolved in the dialog.
 */
function dispatchCompress(
  config: TransferDispatchConfig,
  progressIntervalMs: number,
  maxConflictsToShow: number,
  compressionLevel: number,
): Promise<{ operationId: string }> {
  return compressFiles(
    config.sourceVolumeId,
    config.sourcePaths,
    config.destVolumeId ?? DEFAULT_VOLUME_ID,
    config.destinationPath ?? '',
    {
      conflictResolution: 'overwrite',
      progressIntervalMs,
      maxConflictsToShow,
      previewId: config.previewId,
      compressionLevel,
    },
    config.initiator,
  )
}
