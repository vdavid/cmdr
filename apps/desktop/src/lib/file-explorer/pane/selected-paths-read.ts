/**
 * The focused pane's SELECTION as paths, for the copy-path / copy-filename
 * commands (Finder's ⌥⌘C "Copy as Pathname" and Total Commander's "Copy names
 * with full path" act on every selected item, so ours do too).
 *
 * ❗ The selected indices are read synchronously, at dispatch time, and resolved
 * through the revision-checked `getSelectionSnapshot` against the pane's last
 * seen sequence: a watcher insert renumbers rows, and copying the paths of
 * whatever now sits at the old indices would put the wrong files on the
 * clipboard. A moved-on listing is refused (`changed`), never reinterpreted,
 * the same answer the F5/F6 dialog builder gives (`transfer-operations.ts`).
 *
 * The rows' own `path` is what lands on the clipboard, exactly what the
 * single-entry `getPathToCopyUnderCursor` reads off the cursor row, so archive,
 * S3, SFTP, and phone panes copy the same kind of path for N rows as for one.
 */
import { getSelectionSnapshot } from '$lib/tauri-commands'
import { resolveSnapshotPaths, snapshotIdFromPanePath } from '$lib/search/snapshot-store.svelte'
import { toBackendIndices } from '$lib/file-operations/transfer/transfer-dialog-utils'
import { capabilitiesFor } from './volume-capabilities'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'

/**
 * - `noSelection`: nothing selected (or nothing a selection could mean), so the
 *   caller falls back to its cursor-row behavior.
 * - `paths`: every selected row's path, in pane display order.
 * - `changed`: the listing moved on since the pane last saw it; copy nothing.
 */
export type SelectedPathsRead = { kind: 'noSelection' } | { kind: 'paths'; paths: string[] } | { kind: 'changed' }

const NO_SELECTION: SelectedPathsRead = { kind: 'noSelection' }
const CHANGED: SelectedPathsRead = { kind: 'changed' }

/** True for the listing refusals that mean "your indices are stale", not a bug. */
function isStaleListingRefusal(error: unknown): boolean {
  const refusal = error as { type?: string; failure?: { type?: string } } | null
  const type = refusal?.failure?.type ?? refusal?.type
  return type === 'changed' || type === 'gone'
}

/**
 * A search-results pane has no backend listing: its rows ARE the frozen
 * snapshot, so indices into it can't shift under us. A pane with neither (the
 * Servers hub) has no file selection to copy.
 */
function readListinglessSelection(paneRef: FilePaneAPI, selectedIndices: number[]): SelectedPathsRead {
  const snapshotId = snapshotIdFromPanePath(paneRef.getCurrentPath())
  if (snapshotId === null) return NO_SELECTION
  // Cursor `-1`: the selection is non-empty here, so the cursor fallback never applies.
  const paths = resolveSnapshotPaths(snapshotId, selectedIndices, -1)
  return paths.length > 0 ? { kind: 'paths', paths } : NO_SELECTION
}

export async function readSelectedPathsForCopy(access: PaneAccess): Promise<SelectedPathsRead> {
  const focusedPane = access.getFocusedPane()
  const paneRef = access.getPaneRef(focusedPane)
  if (!paneRef) return NO_SELECTION
  const selectedIndices = paneRef.getSelectedIndices()
  if (selectedIndices.length === 0) return NO_SELECTION

  if (!capabilitiesFor(access.getPaneVolumeId(focusedPane)).hasBackendListing) {
    return readListinglessSelection(paneRef, selectedIndices)
  }

  if (!paneRef.isRowStateReady()) return CHANGED
  const listingId = paneRef.getListingId()
  const backendIndices = toBackendIndices(selectedIndices, paneRef.hasParentEntry())
  if (!listingId || backendIndices.length === 0) return NO_SELECTION
  const expectedSequence = paneRef.getLastSequence()

  try {
    const snapshot = await getSelectionSnapshot(
      listingId,
      access.getShowHiddenFiles(),
      backendIndices,
      expectedSequence,
    )
    return snapshot.paths.length > 0 ? { kind: 'paths', paths: snapshot.paths } : NO_SELECTION
  } catch (error) {
    if (isStaleListingRefusal(error)) return CHANGED
    throw error
  }
}
