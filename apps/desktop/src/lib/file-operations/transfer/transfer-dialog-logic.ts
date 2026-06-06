/**
 * Pure derivation helpers for `TransferDialog.svelte` with no reactivity and no
 * Tauri/IPC coupling. Extracted from the dialog so each branch is unit-testable
 * without mounting the component or stubbing IPC. The reactive orchestration
 * (scan preview, conflict check) lives in the colocated `*.svelte.ts` factories;
 * this file is the "math" half.
 */

import type { TransferOperationType } from '$lib/file-explorer/types'
import type { VolumeSpaceInfo } from '$lib/tauri-commands'

/**
 * Checks whether the destination path is invalid relative to the source paths.
 *
 * Two rejection cases, in order:
 *  - the destination IS a source or sits inside one (can't move a folder into
 *    its own subfolder), and
 *  - the destination is the source's own parent (the item is already there).
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
): string | null {
  const normDest = destination.replace(/\/+$/, '')
  const verb = operationType === 'copy' ? 'copy' : 'move'

  for (const source of sources) {
    const normSource = source.replace(/\/+$/, '')
    if (normDest === normSource || normDest.startsWith(normSource + '/')) {
      const folderName = normSource.split('/').pop() ?? normSource
      return `Can't ${verb} "${folderName}" into its own subfolder`
    }
  }

  for (const source of sources) {
    const normSource = source.replace(/\/+$/, '')
    const sourceParent = normSource.substring(0, normSource.lastIndexOf('/'))
    if (normDest === sourceParent) {
      const fileName = normSource.split('/').pop() ?? normSource
      return `"${fileName}" is already in this location`
    }
  }

  return null
}

/**
 * Formats the free-space line ("12 GB free of 500 GB") for the volume selector.
 * Intentionally uncolored upstream: red GB would falsely signal "low space".
 * Returns an empty string when no space info is available (the line is hidden).
 *
 * The byte formatter is injected so this stays pure and testable; the dialog
 * passes the user's configured size format via `formatFileSizeWithFormat`.
 */
export function formatSpaceInfo(space: VolumeSpaceInfo | null, formatBytes: (bytes: number) => string): string {
  if (!space) return ''
  const free = formatBytes(space.availableBytes)
  const total = formatBytes(space.totalBytes)
  return `${free} free of ${total}`
}
