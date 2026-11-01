import { getSnapshot, resolveSnapshotEntries, snapshotIdFromPanePath } from '#lib/search/snapshot-store.svelte.js'
import {
  type TransferContext,
  type TransferDialogPropsDraft,
  buildTransferPropsFromSelection,
  buildTransferPropsFromCursor,
  buildTransferPropsFromSnapshot,
} from './transfer-operations'
import { capabilitiesFor } from './volume-capabilities'
import { announceRowsStillSettling } from './operation-start-gate'
import { ROW_SETTLE_BUDGET_MS } from './pressed-rows'
import type { DuplicateFollowUp } from './duplicate-rename'
import type { FilePaneAPI, OpenTransferDialogArgs } from './types'
import type { TransferOperationType } from '../types'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'

type DialogState = ReturnType<typeof createDialogState>

/**
 * The last step of every transfer entry from the keyboard, the menu, and MCP: read what the
 * source pane offers (its selection, else the cursor row; a search-results snapshot, else a
 * backend listing) and open the transfer dialog on it. The guards before it (the start gate,
 * the destination guard) live in `file-operation-commands.ts`.
 */
export function createTransferDialogOpener(access: PaneAccess, dialogs: Pick<DialogState, 'showTransfer'>) {
  /** Builds a TransferContext from pane state. */
  function buildTransferContext(pane: 'left' | 'right', mcpRequestId?: string): TransferContext {
    const other = access.otherPane(pane)
    return {
      mcpRequestId,
      showHiddenFiles: access.getShowHiddenFiles(),
      sourcePath: access.getPanePath(pane),
      destPath: access.getPanePath(other),
      sourceVolumeId: access.getPaneVolumeId(pane),
      destVolumeId: access.getPaneVolumeId(other),
    }
  }

  /**
   * Builds transfer dialog props for a search-results source pane.
   * The snapshot view has no backend listing, so the listing-id-driven
   * builders don't apply; we read the snapshot directly and feed
   * absolute paths into `buildTransferPropsFromSnapshot`. Returns `null`
   * when there's no snapshot or nothing under the cursor / selection —
   * `resolveSnapshotEntries` answers both with an empty list.
   *
   * `canBeSource: true` per the `search-results` capability row: source-side
   * operations always run against the real underlying files. The volume they run
   * against is the one the search covered, which the snapshot carries
   * (`SearchSnapshot.volumeId`), so a phone unplugged under the pane still names
   * the phone rather than falling back to the boot drive. After a move completes,
   * `dialog-state::handleTransferComplete` already purges moved paths from every
   * snapshot via `removeEntryFromAllSnapshots`.
   */
  function buildSnapshotTransferProps(
    operationType: TransferOperationType,
    sourcePaneRef: FilePaneAPI | undefined,
    pane: 'left' | 'right',
  ) {
    const snapshotId = snapshotIdFromPanePath(sourcePaneRef?.getCurrentPath() ?? '')
    if (snapshotId === null) return null

    const selectedIndices = sourcePaneRef?.getSelectedIndices() ?? []
    const cursorIndex = sourcePaneRef?.getCursorIndex() ?? 0
    const entries = resolveSnapshotEntries(snapshotId, selectedIndices, cursorIndex)
    if (entries.length === 0) return null
    const snapshot = getSnapshot(snapshotId)
    if (!snapshot) return null

    const sourcePaths = entries.map((entry) => entry.path)
    const isDirectoryFlags = entries.map((entry) => entry.isDirectory)

    const other = access.otherPane(pane)
    return buildTransferPropsFromSnapshot(
      operationType,
      sourcePaths,
      isDirectoryFlags,
      pane === 'left',
      access.getPanePath(other),
      access.getPaneVolumeId(other),
      snapshot.volumeId,
    )
  }

  function transferSourceIsCurrent(source: {
    ref: FilePaneAPI
    pane: 'left' | 'right'
    listingId: string
    generation: number
  }): boolean {
    return (
      access.getPaneRef(source.pane) === source.ref &&
      source.ref.getListingId() === source.listingId &&
      source.ref.getViewGeneration() === source.generation
    )
  }

  /** Stamps who asked, and how, onto the dialog props, whichever source built them. */
  function showWithTrigger(props: TransferDialogPropsDraft, trigger: OpenTransferDialogArgs): void {
    if (trigger.autoConfirm) {
      props.autoConfirm = true
      props.autoConfirmOnConflict = trigger.onConflict
      props.mcpRequestId = trigger.mcpRequestId
    }
    // F5/F6 is a person at the keyboard directing this transfer, so a single-item
    // duplicate ends in the rename editor. An auto-confirmed one is MCP: an agent's
    // copy has no business pulling focus into a text field in front of whoever is
    // watching, so it takes the same route every other trigger does.
    const duplicateFollowUp: DuplicateFollowUp = trigger.autoConfirm ? 'nothing' : 'openRenameEditor'
    dialogs.showTransfer({ ...props, initiator: trigger.initiator, duplicateFollowUp })
  }

  /**
   * Waits, when the press was on a selection, for the source rows to settle. Answers `false`
   * after telling the user why nothing starts: the budget ran out, the selection vanished, or
   * the pane moved to another listing. A press on a selection never quietly turns into a press
   * on the cursor row, and never carries over to a different listing: both would act on items
   * the user didn't pick.
   */
  async function rowsSettled(
    ref: FilePaneAPI,
    press: { onSelection: boolean; listingId: string; deadline: number; mcpRequestId?: string },
  ): Promise<boolean> {
    const hasSelection = ref.getSelectedIndices().length > 0
    const stillTheSamePress = press.onSelection === hasSelection && ref.getListingId() === press.listingId
    const settled =
      stillTheSamePress &&
      (!hasSelection || ref.isRowStateReady() || (await ref.whenRowStateReady(press.deadline - Date.now())))
    if (!settled) announceRowsStillSettling(press.mcpRequestId)
    return settled
  }

  /** Builds the dialog props from the pane's backend listing: the selection, else the cursor row. */
  async function buildListingProps(
    operationType: TransferOperationType,
    ref: FilePaneAPI,
    pane: 'left' | 'right',
    press: { deadline: number; mcpRequestId?: string },
  ) {
    const listingId = ref.getListingId()
    if (!listingId) return null
    const selectedIndices = ref.getSelectedIndices()
    const context = { ...buildTransferContext(pane, press.mcpRequestId), deadline: press.deadline }
    const isLeft = pane === 'left'
    const generation = ref.getViewGeneration()
    const props =
      selectedIndices.length > 0
        ? await buildTransferPropsFromSelection(operationType, ref, isLeft, context)
        : await buildTransferPropsFromCursor(operationType, ref, isLeft, context)
    return props ? { props, listingId, generation } : null
  }

  /** Opens the unified transfer dialog for all volume types (local, MTP, search-results, etc.). */
  async function openUnifiedTransferDialog(
    operationType: TransferOperationType,
    sourcePaneRef: FilePaneAPI | undefined,
    pane: 'left' | 'right',
    trigger: OpenTransferDialogArgs,
  ) {
    const { mcpRequestId } = trigger

    // Snapshot source pane: no backend listing exists, so the listing-id-driven
    // builders don't apply — build from the snapshot's selected (or cursor)
    // entries. Routed off the kind's `hasBackendListing` capability, not a
    // `volumeId === 'search-results'` string compare. Among kinds that reach
    // this transfer opener, search-results is the only `!hasBackendListing` one
    // (a network pane can't be a transfer source — `canBeSource: false`).
    if (!capabilitiesFor(access.getPaneVolumeId(pane)).hasBackendListing) {
      const snapshotProps = buildSnapshotTransferProps(operationType, sourcePaneRef, pane)
      if (snapshotProps) showWithTrigger(snapshotProps, trigger)
      return
    }
    if (!sourcePaneRef) return

    // Selected row numbers only mean something against settled rows, and the rows can move
    // under the build (a listing reload, a resort, a watcher diff). Either way the press
    // used to vanish: F5 did nothing. So wait for the rows (`whenRowStateReady` is
    // event-driven) and rebuild if they moved meanwhile, within one budget. Past it, the
    // user hears why nothing started, never silence.
    const press = {
      onSelection: sourcePaneRef.getSelectedIndices().length > 0,
      listingId: sourcePaneRef.getListingId(),
      deadline: Date.now() + ROW_SETTLE_BUDGET_MS,
      mcpRequestId,
    }
    for (;;) {
      if (!(await rowsSettled(sourcePaneRef, press))) return
      const built = await buildListingProps(operationType, sourcePaneRef, pane, press)
      if (!built) return
      const { props, listingId, generation } = built
      if (transferSourceIsCurrent({ ref: sourcePaneRef, pane, listingId, generation })) {
        showWithTrigger(props, trigger)
        return
      }
      // The pane itself was swapped out (a tab switch), so the press belonged to rows that
      // are gone: dropping it is right. Otherwise the same rows moved; build again.
      if (access.getPaneRef(pane) !== sourcePaneRef) return
      if (Date.now() >= press.deadline) {
        announceRowsStillSettling(mcpRequestId)
        return
      }
    }
  }

  return { openUnifiedTransferDialog }
}
