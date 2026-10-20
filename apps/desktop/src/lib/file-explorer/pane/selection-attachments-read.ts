/**
 * "Ask about selection" (the Ask Cmdr composer's paperclip): the focused pane's
 * selection, or its cursor row with nothing selected, as attachment refs, path +
 * kind only.
 *
 * ❗ The rows resolve against the BACKEND listing (`getSelectionEntries`), which
 * holds every row: the pane only loads the window on screen, and resolving
 * against that dropped every selected row scrolled out of it, silently. The read
 * goes through `readPressedRows` like every other press, so a watcher insert
 * can't hand it the neighbor and a busy folder is waited out.
 *
 * A selection past `SELECTION_ATTACHMENT_CAP` keeps its first rows in pane order
 * and says how many it left out (`leftOut`): every ref rides into the model's
 * context on the next send, so ten thousand of them would cost the user's tokens
 * and bury the question. The cap is also what bounds the read itself.
 */
import { getSelectionEntries, type AttachmentRef } from '#lib/tauri-commands/index.js'
import { resolveSnapshotEntries, snapshotIdFromPanePath } from '#lib/search/snapshot-store.svelte.js'
import { announceRowsStillSettling } from './operation-start-gate'
import { readPressedRows } from './pressed-rows'
import { capabilitiesFor } from './volume-capabilities'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'

/** The most rows one "Ask about selection" attaches. */
export const SELECTION_ATTACHMENT_CAP = 100

/**
 * - `refs`: the rows' refs, in pane order (empty when there's nothing to attach),
 *   and how many selected rows the cap left out.
 * - `changed`: the rows never settled, so nothing attached. Already announced.
 */
export type SelectionAttachmentsRead = { kind: 'refs'; refs: AttachmentRef[]; leftOut: number } | { kind: 'changed' }

const NOTHING: SelectionAttachmentsRead = { kind: 'refs', refs: [], leftOut: 0 }

function toRef(entry: { path: string; isDirectory: boolean }): AttachmentRef {
  return { path: entry.path, kind: entry.isDirectory ? 'folder' : 'file' }
}

/** The first `SELECTION_ATTACHMENT_CAP` rows in pane order, and how many it dropped. */
function capRows(rows: number[]): { kept: number[]; leftOut: number } {
  const sorted = [...rows].sort((a, b) => a - b)
  return {
    kept: sorted.slice(0, SELECTION_ATTACHMENT_CAP),
    leftOut: Math.max(0, sorted.length - SELECTION_ATTACHMENT_CAP),
  }
}

/** A search-results pane's rows ARE its frozen snapshot, so they can't shift under the read. */
function readSnapshotSelection(paneRef: FilePaneAPI): SelectionAttachmentsRead {
  const snapshotId = snapshotIdFromPanePath(paneRef.getCurrentPath())
  if (snapshotId === null) return NOTHING
  const { kept, leftOut } = capRows(paneRef.getSelectedIndices())
  const entries = resolveSnapshotEntries(snapshotId, kept, paneRef.getCursorIndex())
  return { kind: 'refs', refs: entries.map(toRef), leftOut }
}

export async function readSelectionAttachments(access: PaneAccess): Promise<SelectionAttachmentsRead> {
  const focusedPane = access.getFocusedPane()
  const paneRef = access.getPaneRef(focusedPane)
  if (!paneRef) return NOTHING
  if (!capabilitiesFor(access.getPaneVolumeId(focusedPane)).hasBackendListing) return readSnapshotSelection(paneRef)
  if (!paneRef.isRowStateReady()) {
    announceRowsStillSettling(undefined)
    return { kind: 'changed' }
  }

  const pressed = await readPressedRows(
    paneRef,
    async ({ listingId, backendRows, expectedSequence }) => {
      const { kept, leftOut } = capRows(backendRows)
      const entries = await getSelectionEntries(listingId, access.getShowHiddenFiles(), kept, expectedSequence)
      return { refs: entries.map(toRef), leftOut }
    },
    { cursorFallback: true },
  )
  if (pressed.kind === 'refused') return { kind: 'changed' }
  return pressed.kind === 'read' ? { kind: 'refs', ...pressed.value } : NOTHING
}
