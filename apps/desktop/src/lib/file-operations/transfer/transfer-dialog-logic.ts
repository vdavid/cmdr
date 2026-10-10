/**
 * Pure derivation helpers for `TransferDialog.svelte` with no reactivity and no
 * Tauri/IPC coupling. Extracted from the dialog so each branch is unit-testable
 * without mounting the component or stubbing IPC. The reactive orchestration
 * (scan preview, conflict check) lives in the colocated `*.svelte.ts` factories;
 * this file is the "math" half.
 */

import type { TransferOperationType } from '$lib/file-explorer/types'
import type { SpaceInfo } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'

/**
 * Checks whether the destination path is invalid relative to the source paths.
 *
 * A complete single-item destination equal to the source cannot confirm for
 * either Copy or Move. A destination inside a source subtree is also rejected.
 * Folder-targeted batches retain the Move-only same-parent check; copying a
 * batch into its source folder uses the backend's duplicate naming instead.
 *
 * Trailing slashes are normalized off both sides before comparison. Returns the
 * user-facing error string, or `null` when the path is acceptable. The verb
 * ("copy" / "move") comes from the active operation so the message matches what
 * the user is doing.
 */
export function getPathValidationError(
  sources: string[],
  destination: string,
  operationType: TransferOperationType,
  includesName = false,
): string | null {
  const normDest = destination.replace(/\/+$/, '')

  // Compress creates ONE new `.zip` file, so the copy/move checks below (moving a
  // folder into itself, "already in this location") don't apply. The only rule is
  // that the target names a zip archive; the leaf is a new file, and the dialog's
  // dest-exists check surfaces an overwrite of an existing archive separately.
  if (operationType === 'compress') {
    const leaf = normDest.split('/').pop() ?? ''
    const lower = leaf.toLowerCase()
    if (!lower.endsWith('.zip') || lower === '.zip') {
      return tString('fileOperations.transferDialog.pathErrorNotZip')
    }
    return null
  }

  const verb = operationType === 'copy' ? 'copy' : 'move'

  for (const source of sources) {
    const refusal = sourceDestinationError(source, normDest, verb, includesName)
    if (refusal) return refusal
  }

  if (operationType === 'move' && !includesName) {
    for (const source of sources) {
      const normSource = source.replace(/\/+$/, '')
      const sourceParent = normSource.substring(0, normSource.lastIndexOf('/'))
      if (normDest === sourceParent) {
        const fileName = normSource.split('/').pop() ?? normSource
        return tString('fileOperations.transferDialog.pathErrorAlreadyThere', { name: fileName })
      }
    }
  }

  return null
}

function sourceDestinationError(
  source: string,
  destination: string,
  verb: 'copy' | 'move',
  includesName: boolean,
): string | null {
  const normalized = source.replace(/\/+$/, '')
  const name = normalized.split('/').pop() ?? normalized
  if (includesName && destination === normalized) {
    return tString('fileOperations.transferDialog.pathErrorAlreadyThere', { name })
  }
  if (destination === normalized || destination.startsWith(normalized + '/')) {
    return tString('fileOperations.transferDialog.pathErrorSubfolder', { verb, name })
  }
  return null
}

/**
 * Formats the free-space line ("12 GB free of 500 GB") for the volume selector,
 * or "64 MB used, no size limit" for storage with no ceiling, where there is no
 * free figure to state and the copy is certain to fit.
 * Intentionally uncolored upstream: red GB would falsely signal "low space".
 * Returns an empty string when no space info is available (the line is hidden).
 *
 * The byte formatter is injected so this stays pure and testable; the dialog
 * passes the user's configured size format via `formatFileSizeWithFormat`.
 */
export function formatSpaceInfo(space: SpaceInfo | null, formatSize: (bytes: number) => string): string {
  if (!space) return ''
  if (space.kind !== 'bounded') {
    return tString('fileOperations.transferDialog.spaceInfoUnbounded', { used: formatSize(space.usedBytes) })
  }
  const free = formatSize(space.availableBytes)
  const total = formatSize(space.totalBytes)
  return tString('fileOperations.transferDialog.spaceInfo', { free, total })
}
