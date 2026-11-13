import {
  createDirectory,
  createFile,
  getSelectionEntries,
  trashRoutingForPaths,
  type Initiator,
} from '#lib/tauri-commands/index.js'
import { pluralize } from '#lib/utils/pluralize.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getSnapshot, resolveSnapshotEntries, snapshotIdFromPanePath } from '#lib/search/snapshot-store.svelte.js'
import { resolveSnapshotSourceVolume } from './snapshot-source-volume'
import { openFileViewer } from '#lib/file-viewer/open-viewer.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { getInitialFolderName } from '#lib/file-operations/mkdir/new-folder-operations.js'
import { getInitialFileName } from '#lib/file-operations/mkfile/new-file-operations.js'
import type { CloudOnlineOnlyExtent, DeleteSourceItem } from '#lib/file-operations/delete/delete-dialog-utils.js'
import type { TrashRouting } from '#lib/ipc/bindings.js'
import { getCommonParentPath, getDestinationVolumeInfo } from './transfer-operations'
import { createTransferDialogOpener } from './transfer-dialog-opener'
import { capabilitiesFor, capabilitiesForPane, rowIsServedByGitPortal } from './volume-capabilities'
import { pathCrossesArchiveBoundary } from './archive-paths'
import { paneOffersTrash } from './trash-availability'
import { checkTransferDestinationGuard } from './transfer-entry'
import { answeringDialogOpen, operationStartIsBlocked, refuseOpening } from './operation-start-gate'
import { readCursorRow, readPressedRows, type PressedRows } from './pressed-rows'
import { duplicateInPlace } from './duplicate-command'
import { passesHomeFolderGuard } from './home-folder-guard'
import { openRenameAsMove } from './rename-as-move'
import type { RenameAsMoveRequest } from './rename-flow.svelte'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type {
  FilePaneAPI,
  OpenDeleteDialogArgs,
  OpenNamingDialogArgs,
  OpenTransferDialogArgs,
  StartRenameOptions,
} from './types'
import type { TransferOperationType } from '../types'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'

const log = getAppLogger('fileExplorer')

/**
 * Whether these items hold online-only content in a cloud-storage folder, which
 * makes a trash the wrong thing to run: macOS would download every evicted file
 * just to put it in a folder the person is about to empty.
 *
 * The recognition itself is the backend's (`crates/cmdr-operations/src/delete/cloud_trash.rs`):
 * it's all-or-nothing across the selection, it resolves symlinks first, and it's
 * deliberately narrow. ❌ Don't re-derive any of it here.
 *
 * `folderMayHoldOnlineOnly` says the answer isn't final: a selected FOLDER can't
 * carry the flag itself, so only the confirmation dialog's own scan walk can find
 * one inside. The dialog waits on that walk before it confirms.
 *
 * A question that can't be answered (the IPC threw, the backend timed out on a
 * hung mount) keeps the OS trash and its typed refusal.
 */
async function cloudOnlineOnlyRouting(sourcePaths: string[]): Promise<{
  onlineOnly: CloudOnlineOnlyExtent | null
  folderMayHoldOnlineOnly: boolean
}> {
  try {
    const answer = await trashRoutingForPaths(sourcePaths)
    return {
      onlineOnly: ONLINE_ONLY_EXTENT_BY_ROUTING[answer.routing],
      folderMayHoldOnlineOnly: answer.folderMayHoldOnlineOnly,
    }
  } catch (error) {
    log.warn('trashRoutingForPaths threw, keeping the trash. error={error}', {
      error,
    })
    return NO_ONLINE_ONLY
  }
}

/** Exhaustive by construction: a new routing variant is a compile error here
 *  rather than a banner that silently stops appearing. */
const ONLINE_ONLY_EXTENT_BY_ROUTING: Record<TrashRouting, CloudOnlineOnlyExtent | null> = {
  trash: null,
  permanentDeleteAllOnlineOnly: 'all',
  permanentDeleteMixedOnlineOnly: 'mixed',
}

/** What every "we couldn't ask" path answers: today's behavior, the OS trash. */
const NO_ONLINE_ONLY = { onlineOnly: null, folderMayHoldOnlineOnly: false } as const

type DialogState = ReturnType<typeof createDialogState>

/**
 * Rename, new-folder / new-file, viewer, transfer (copy / move / duplicate), and
 * delete command bodies for the focused pane. Lifted out of `DualPaneExplorer` so the
 * read-only-volume, search-results-destination, and snapshot-pane guard chains
 * are headless-testable. Reads pane state through `PaneAccess`; opens dialogs
 * through the explorer's dialog state.
 */
export function createFileOperationCommands(access: PaneAccess, dialogs: DialogState) {
  const { openUnifiedTransferDialog } = createTransferDialogOpener(access, dialogs)

  /**
   * The read-only refusal alert for a write action on the focused pane, or `null`
   * when the pane accepts writes. A zip archive is WRITABLE (the pane's `volumeId`
   * is the parent drive and `capabilitiesForPane` gives the writable `archive`
   * row), so an archive pane falls through here and runs the real managed
   * archive-edit flow. A tar / 7z pane and a virtual `.git` portal pane each get
   * their own worded refusal. What still refuses beyond those is a read-only
   * `VolumeInfo` (a write-protected USB stick, a read-only disk image) —
   * including a zip that lives on such a volume, which can't be rewritten in
   * place. Surfacing this up front beats letting the user type a name and then
   * hit a backend rejection.
   */
  function readOnlyRefusal(
    action: 'rename' | 'mkdir' | 'mkfile' | 'delete',
    pane: 'left' | 'right' = access.getFocusedPane(),
  ): { title: string; message: string } | null {
    const volId = access.getPaneVolumeId(pane)

    const volumeInfo = getDestinationVolumeInfo(volId, access.getVolumes())

    // The three ROUTED kinds refuse up front rather than letting the user type a
    // name and hit the backend's typed rejection. Kind-from-path: the pane's
    // `volumeId` is the writable parent drive, so the PATH decides, and the
    // routed branches of `capabilitiesForPane` ignore fsType/category, so passing
    // only the id + path is enough.
    // - A read-only archive (tar / 7z) is browse + extract only. A writable zip
    //   has `canWrite` on and falls through to the managed archive-edit flow.
    // - A git-portal pane is a snapshot of history with no directory behind it,
    //   so it's read-only whatever the drive under it says. A restic pane is a
    //   backup's snapshot, read-only for the same reason.
    const paneCaps = capabilitiesForPane(volId, access.getPanePath(pane))
    if (paneCaps.kind === 'archive' && !paneCaps.canWrite) {
      return {
        title: tString('fileExplorer.readOnly.archiveTitle'),
        message: tString('fileExplorer.readOnly.archiveMessage'),
      }
    }
    if (paneCaps.kind === 'git-portal') {
      return {
        title: tString('fileExplorer.readOnly.gitPortalTitle'),
        message: tString('fileExplorer.readOnly.gitPortalMessage'),
      }
    }
    if (paneCaps.kind === 'restic') {
      return {
        title: tString('fileExplorer.readOnly.resticTitle'),
        message: tString('fileExplorer.readOnly.resticMessage'),
      }
    }

    if (volumeInfo?.mountIsReadOnly) {
      const messageKey: Record<typeof action, MessageKey> = {
        rename: 'fileExplorer.readOnly.renameMessage',
        mkdir: 'fileExplorer.readOnly.mkdirMessage',
        mkfile: 'fileExplorer.readOnly.mkfileMessage',
        delete: 'fileExplorer.readOnly.deleteMessage',
      }
      return { title: tString('fileExplorer.readOnly.volumeTitle'), message: tString(messageKey[action]) }
    }

    return null
  }

  /**
   * Activates inline rename on the focused pane's cursor item. Beyond the pane's
   * own refusal, a ROW the `.git` portal serves refuses too: a repo's `.git/`
   * pane is writable, but its `worktrees` row is git's real folder at the same
   * path. The rename flow refuses it again for every other activation.
   */
  function startRename(options?: StartRenameOptions) {
    const pane = options?.pane ?? access.getFocusedPane()
    const mcpRequestId = options?.mcpRequestId
    const paneRef = access.getPaneRef(pane)
    const rowPath = paneRef?.getPathUnderCursor()
    const refusal =
      readOnlyRefusal('rename', pane) ??
      (rowPath !== undefined && rowIsServedByGitPortal(rowPath)
        ? {
            title: tString('fileExplorer.readOnly.gitPortalTitle'),
            message: tString('fileExplorer.readOnly.gitPortalMessage'),
          }
        : null)
    if (refusal) {
      refuseOpening(mcpRequestId, { kind: 'readOnly', message: refusal.message }, () => {
        dialogs.showAlert(refusal.title, refusal.message)
      })
      return
    }

    void answeringDialogOpen(mcpRequestId, () => {
      if (!paneRef) return false
      paneRef.startRename(options)
      return true
    })
  }

  /**
   * Shows the refusal alert and returns `true` when the focused pane can't take a
   * Multi-rename (⌃M), so the sheet never opens on a folder Start would refuse.
   * Stricter than F2: the batch runs on a drive's own volume, so ANY archive
   * refuses, a writable zip included (F2's archive-edit flow covers one file at a
   * time there). The backend's `ReadOnly` answer at Start stays the safety net.
   */
  function refuseMultiRename(): boolean {
    const pane = access.getFocusedPane()
    const paneCaps = capabilitiesForPane(access.getPaneVolumeId(pane), access.getPanePath(pane))
    const refusal =
      paneCaps.kind === 'archive' && paneCaps.canWrite
        ? { title: tString('multiRename.archiveTitle'), message: tString('multiRename.archiveMessage') }
        : readOnlyRefusal('rename', pane)
    if (!refusal) return false
    dialogs.showAlert(refusal.title, refusal.message)
    return true
  }

  /** Cancels any active inline rename on either pane. */
  function cancelRename() {
    for (const side of ['left', 'right'] as const) {
      access.getPaneRef(side)?.cancelRename()
    }
  }

  /** Returns whether inline rename is active on either pane. */
  function isRenaming(): boolean {
    return (['left', 'right'] as const).some((side) => {
      return access.getPaneRef(side)?.isRenaming()
    })
  }

  /**
   * Opens the new folder dialog, pre-filled with the entry name under the cursor.
   * `pane` defaults to the focused pane; the MCP `mkdir` tool passes it explicitly
   * so the dialog targets the right pane regardless of focus timing. `initiator`
   * carries provenance.
   */
  async function openNewFolderDialog(args: OpenNamingDialogArgs) {
    await answeringDialogOpen(args.mcpRequestId, () => showNewFolderDialog(args))
  }

  async function showNewFolderDialog({
    pane = access.getFocusedPane(),
    mcpRequestId,
    initiator,
    guard,
  }: OpenNamingDialogArgs): Promise<boolean> {
    if (operationStartIsBlocked(mcpRequestId)) return false

    const paneRef = access.getPaneRef(pane)
    const path = access.getPanePath(pane)
    const volumeIdForPane = access.getPaneVolumeId(pane)

    // Read-only destinations (a write-protected volume, or inside an archive) can't
    // accept new folders. Surface that as an alert up front rather than letting the
    // user type a name and then hit a backend rejection. Mirrors `startRename`.
    const refusal = readOnlyRefusal('mkdir', pane)
    if (refusal) {
      refuseOpening(mcpRequestId, { kind: 'readOnly', message: refusal.message }, () => {
        dialogs.showAlert(refusal.title, refusal.message)
      })
      return false
    }

    const paneListingId = paneRef?.getListingId()
    if (!paneListingId) {
      log.warn('openNewFolderDialog: no listingId, bailing')
      return false
    }

    const initialName = await getInitialFolderName(paneRef)

    dialogs.showNewFolder({
      pane,
      currentPath: path,
      listingId: paneListingId,
      showHiddenFiles: access.getShowHiddenFiles(),
      initialName,
      volumeId: volumeIdForPane,
      initiator,
      guard,
    })
    return true
  }

  /**
   * Opens the new file dialog, pre-filled with the filename under the cursor. `pane`
   * defaults to the focused pane; the MCP `mkfile` tool passes it explicitly.
   * `initiator` carries provenance.
   */
  async function openNewFileDialog(args: OpenNamingDialogArgs) {
    await answeringDialogOpen(args.mcpRequestId, () => showNewFileDialog(args))
  }

  async function showNewFileDialog({
    pane = access.getFocusedPane(),
    mcpRequestId,
    initiator,
    guard,
  }: OpenNamingDialogArgs): Promise<boolean> {
    if (operationStartIsBlocked(mcpRequestId)) return false

    const paneRef = access.getPaneRef(pane)
    const path = access.getPanePath(pane)
    const volumeIdForPane = access.getPaneVolumeId(pane)

    const refusal = readOnlyRefusal('mkfile', pane)
    if (refusal) {
      refuseOpening(mcpRequestId, { kind: 'readOnly', message: refusal.message }, () => {
        dialogs.showAlert(refusal.title, refusal.message)
      })
      return false
    }

    const paneListingId = paneRef?.getListingId()
    if (!paneListingId) {
      log.warn('openNewFileDialog: no listingId, bailing')
      return false
    }

    const initialName = await getInitialFileName(paneRef)

    dialogs.showNewFile({
      pane,
      currentPath: path,
      listingId: paneListingId,
      showHiddenFiles: access.getShowHiddenFiles(),
      initialName,
      volumeId: volumeIdForPane,
      initiator,
      guard,
    })
    return true
  }

  /**
   * Creates a folder directly on the target pane (the MCP `mkdir` tool given a
   * name), using the pane's LIVE path + volumeId so it can't land in a stale
   * directory. `pane` defaults to the focused pane; the tool passes it explicitly
   * so a `nav_to_path` right before the create can't land in the wrong pane on a
   * focus-timing race. Throws the read-only refusal or the backend conflict error,
   * which the mcp-mkdir listener relays as the tool's failure.
   */
  async function createFolderDirect(
    name: string,
    pane: 'left' | 'right' = access.getFocusedPane(),
    initiator: Initiator,
  ): Promise<void> {
    const refusal = readOnlyRefusal('mkdir', pane)
    if (refusal) throw new Error(refusal.message)
    const path = access.getPanePath(pane)
    const volumeId = access.getPaneVolumeId(pane)
    await createDirectory(path, name, volumeId, initiator)
  }

  /** Creates an empty file directly on the target pane (the MCP `mkfile` tool given a name). */
  async function createFileDirect(
    name: string,
    pane: 'left' | 'right' = access.getFocusedPane(),
    initiator: Initiator,
  ): Promise<void> {
    const refusal = readOnlyRefusal('mkfile', pane)
    if (refusal) throw new Error(refusal.message)
    const path = access.getPanePath(pane)
    const volumeId = access.getPaneVolumeId(pane)
    await createFile(path, name, volumeId, initiator)
  }

  /** Closes any confirmation dialog (new folder, new file, or transfer) if open (for MCP). */
  function closeConfirmationDialog() {
    dialogs.closeConfirmationDialog()
  }

  /** Returns whether any confirmation dialog is currently open. */
  function isConfirmationDialogOpen(): boolean {
    return dialogs.isConfirmationDialogOpen()
  }

  /**
   * Opens the file viewer for the file under the cursor, read through the pane's
   * checked cursor read (`refreshCursorEntry`), so a watcher insert can't open the neighbor.
   */
  async function openViewerForCursor() {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    if (!paneRef?.getListingId()) return

    const file = await paneRef.refreshCursorEntry()
    if (!file || file.isDirectory || file.name === '..') return

    // Pass the pane's DRIVE volume id (an archive pane keeps its parent drive's
    // id) so a file inside a remote-hosted zip previews through that volume.
    void openFileViewer(file.path, paneRef.getVolumeId())
  }

  /** Opens the transfer dialog with the current selection info. */
  async function openTransferDialog(operationType: TransferOperationType, args: OpenTransferDialogArgs) {
    await answeringDialogOpen(args.mcpRequestId, () => showTransferDialog(operationType, args))
  }

  async function showTransferDialog(
    operationType: TransferOperationType,
    args: OpenTransferDialogArgs,
  ): Promise<boolean> {
    // Nothing new starts behind a dialog: the confirmation would stack over what
    // the user is reading, and this command reaches us from the native menu, whose
    // items stay clickable whatever is on screen. `operation-start-gate.ts`.
    if (operationStartIsBlocked(args.mcpRequestId)) return false

    const sourcePane = args.pane ?? access.getFocusedPane()
    const sourcePaneRef = access.getPaneRef(sourcePane)
    const destPane = access.otherPane(sourcePane)
    const destVolId = access.getPaneVolumeId(destPane)
    const destPath = access.getPanePath(destPane)

    // Shared destination guard chain (search-results refusal + archive/read-only
    // alert). Every transfer entry path — F5/F6, drag-and-drop, clipboard paste —
    // runs the same `checkTransferDestinationGuard`, so the refusal copy and
    // ordering can't drift between paths. `destPath` drives the archive
    // kind-from-path check. The F-key bar already disables F5/F6 when the OPPOSITE
    // pane is a snapshot, so the search-results branch here is a belt-and-braces
    // guard for the shortcut path.
    const guard = checkTransferDestinationGuard(destVolId, access.getVolumes(), destPath)
    if (!guard.ok) {
      const message = guard.toast ? guard.toast.message : guard.alert.message
      refuseOpening(args.mcpRequestId, { kind: 'destination', message }, () => {
        if (guard.toast) addToast(guard.toast.message, { level: guard.toast.level })
        else dialogs.showAlert(guard.alert.title, guard.alert.message)
      })
      return false
    }

    return openUnifiedTransferDialog(operationType, sourcePaneRef, sourcePane, args)
  }

  /** Opens the copy dialog (convenience wrapper for MCP/key binding). */
  async function openCopyDialog(args: OpenTransferDialogArgs) {
    await openTransferDialog('copy', args)
  }

  /** Opens the move dialog (convenience wrapper for MCP/key binding). */
  async function openMoveDialog(args: OpenTransferDialogArgs) {
    await openTransferDialog('move', args)
  }

  /** Opens the compress dialog (convenience wrapper for the ⌥F5 command/MCP). */
  async function openCompressDialog(args: OpenTransferDialogArgs) {
    await openTransferDialog('compress', args)
  }

  /**
   * Search-results pane delete path. The focused pane is on the
   * `search-results://<id>` virtual volume, so there's no backend listing to
   * fetch entries from; we read the snapshot directly.
   *
   * The snapshot pane shares `FilePane.selection` with normal panes, so F8 acts
   * on the selection when there is one and falls back to the cursor row when
   * there isn't — the same rule the clipboard and transfer openers follow, and
   * decided in the one place all three call (`resolveSnapshotEntries`).
   *
   * `sourceFolderPath` is the common parent of the resolved paths: a result set
   * is gathered from anywhere, and the dialog's "from" line plus the trash
   * toast's volume lookup both need a real directory. The volume id is the one the
   * search covered (`SearchSnapshot.volumeId`), and `resolveSnapshotSourceVolume`
   * reads its trash affordance off the live volume list: a search covers one volume
   * and it need not be the boot drive, so neither can be assumed.
   */
  /**
   * The snapshot rows a delete acts on: the selection, else the cursor row, or the cursor row alone (whatever is
   * selected) when the press names one item (`cursorPath`), and only while it's that item. `null` when there's nothing.
   */
  function snapshotDeleteEntries(
    snapshotId: string,
    sourcePaneRef: FilePaneAPI | undefined,
    cursorPath: string | undefined,
  ): { entries: ReturnType<typeof resolveSnapshotEntries>; hasSelection: boolean } | null {
    const selectedIndices = cursorPath === undefined ? (sourcePaneRef?.getSelectedIndices() ?? []) : []
    const hasSelection = selectedIndices.length > 0
    const cursorIndex = sourcePaneRef?.getCursorIndex() ?? 0
    // Both the cursor and a selected index can point past the end (clamping is
    // best-effort in the search-results keyboard path, and a cross-snapshot
    // delete shortens `entries` under a live selection), so the resolver drops
    // what it can't find and we bail on an empty result.
    const entries = resolveSnapshotEntries(snapshotId, selectedIndices, cursorIndex)
    if (cursorPath !== undefined && (entries.length !== 1 || entries[0].path !== cursorPath)) {
      log.warn('openDeleteFromSearchResults: the cursor row is no longer the item the press named, bailing')
      return null
    }
    if (entries.length === 0) {
      log.warn(
        'openDeleteFromSearchResults: nothing to delete (hasSelection={hasSelection}, cursorIndex={idx}), bailing',
        { hasSelection, idx: cursorIndex },
      )
      return null
    }
    return { entries, hasSelection }
  }

  async function openDeleteFromSearchResults(
    sourcePane: 'left' | 'right',
    { permanent, initiator, guard, mcpRequestId, cursorPath }: OpenDeleteDialogArgs,
  ): Promise<boolean> {
    const sourcePaneRef = access.getPaneRef(sourcePane)
    const snapshotId = snapshotIdFromPanePath(sourcePaneRef?.getCurrentPath() ?? '')
    if (snapshotId === null) {
      log.warn('openDeleteFromSearchResults: source pane volume is search-results but path is not. Bailing.')
      return false
    }
    const snapshot = getSnapshot(snapshotId)
    if (!snapshot) {
      log.warn('openDeleteFromSearchResults: snapshot {id} not found, bailing', { id: snapshotId })
      return false
    }
    const pressed = snapshotDeleteEntries(snapshotId, sourcePaneRef, cursorPath)
    if (!pressed) return false
    const { entries, hasSelection } = pressed

    const sourceItems: DeleteSourceItem[] = entries.map((entry) => ({
      name: entry.name,
      size: entry.size ?? undefined,
      isDirectory: entry.isDirectory,
      isSymlink: false,
      recursiveSize: undefined,
      recursiveFileCount: undefined,
    }))
    const sourcePaths = entries.map((entry) => entry.path)

    const sourceVolume = resolveSnapshotSourceVolume(snapshot.volumeId, access.getVolumes())
    // Before any confirm UI: the person's own home folder is refused in its place.
    const homeGuard = { sources: sourcePaths, volumeId: sourceVolume.volumeId, action: 'delete' as const, mcpRequestId }
    if (!(await passesHomeFolderGuard(dialogs, homeGuard))) return false
    // A search reaches into cloud-storage folders like anywhere else, and a hit
    // there can be online-only just the same.
    const cloud = await cloudOnlineOnlyRouting(sourcePaths)

    dialogs.showDeleteConfirmation({
      sourcePaneSide: sourcePane,
      sourceItems,
      sourcePaths,
      sourceFolderPath: getCommonParentPath(sourcePaths),
      isPermanent: permanent || cloud.onlineOnly !== null,
      supportsTrash: cloud.onlineOnly === null ? sourceVolume.supportsTrash : false,
      cloudOnlineOnly: cloud.onlineOnly,
      cloudFolderMayHoldOnlineOnly: cloud.folderMayHoldOnlineOnly,
      isFromCursor: !hasSelection,
      sourceVolumeId: sourceVolume.volumeId,
      initiator,
      guard,
    })
    return true
  }

  /** Opens the delete confirmation dialog for the current selection or cursor item. */
  async function openDeleteDialog(args: OpenDeleteDialogArgs) {
    await answeringDialogOpen(args.mcpRequestId, () => showDeleteDialog(args))
  }

  /**
   * The rows a delete acts on, or `null` when there are none. Full entries in one batch IPC,
   * only while the rows are still the ones the pane applied: a watcher insert above the
   * selection renumbers them, and the old row numbers would offer to delete the neighbor.
   * `readPressedRows` waits out a busy folder and re-reads the press from the pane each attempt.
   */
  async function readDeleteRows(
    sourcePaneRef: FilePaneAPI,
    listingId: string,
    { mcpRequestId, cursorPath }: Pick<OpenDeleteDialogArgs, 'mcpRequestId' | 'cursorPath'>,
  ) {
    const read = ({ backendRows, expectedSequence }: PressedRows) =>
      getSelectionEntries(listingId, access.getShowHiddenFiles(), backendRows, expectedSequence)
    let pressed
    try {
      // A press about one item (the "Tell me about this" card's Move to Trash) reads the cursor row alone, whatever is
      // selected, so a selection elsewhere in the pane is never what goes.
      pressed =
        cursorPath === undefined
          ? await readPressedRows(sourcePaneRef, read, { cursorFallback: true, mcpRequestId })
          : await readCursorRow(sourcePaneRef, read, { mcpRequestId })
    } catch (error) {
      log.warn('openDeleteDialog: getSelectionEntries threw, bailing. error={error}', {
        error,
      })
      return null
    }
    if (pressed.kind !== 'read') {
      if (pressed.kind === 'none') log.debug('openDeleteDialog: nothing selected or under the cursor, bailing')
      return null
    }
    const entries = pressed.value
    if (cursorPath !== undefined && (entries.length !== 1 || entries[0].path !== cursorPath)) {
      // The row under the cursor isn't the item the press was about any more (a watcher moved it): nothing goes.
      log.warn('openDeleteDialog: the cursor row is no longer the item the press named, bailing')
      return null
    }
    const validEntries = entries.filter((e) => e.name !== '..')
    if (validEntries.length === 0) {
      log.warn(
        'openDeleteDialog: no validEntries after getSelectionEntries (got {count} {entriesNoun}, all filtered as ".." parent), bailing',
        {
          count: entries.length,
          entriesNoun: pluralize(entries.length, 'entry', 'entries'),
        },
      )
      return null
    }
    return { validEntries, hasSelection: pressed.onSelection }
  }

  async function showDeleteDialog(args: OpenDeleteDialogArgs): Promise<boolean> {
    const { permanent, mcpRequestId, initiator, guard } = args
    if (operationStartIsBlocked(mcpRequestId)) return false

    const sourcePane = args.pane ?? access.getFocusedPane()
    const sourcePaneRef = access.getPaneRef(sourcePane)
    const sourceVolId = access.getPaneVolumeId(sourcePane)

    // Snapshot pane: no backend listing exists, so the listingId-driven path
    // can't fetch entries. Build the dialog directly from the snapshot's cursor
    // entry. Source-side delete is allowed (`canBeSource: true`): the underlying
    // file IS real, the confirmation dialog shows the real path, and on success
    // the entry is also removed from every other snapshot that contained it (see
    // `dialog-state::handleTransferComplete`). Routed off `hasBackendListing`,
    // not a `volumeId === 'search-results'` string compare; search-results
    // is the only source-capable `!hasBackendListing` kind to reach here.
    if (!capabilitiesFor(sourceVolId).hasBackendListing) {
      return openDeleteFromSearchResults(sourcePane, args)
    }

    const listingId = sourcePaneRef?.getListingId()
    if (!listingId) {
      log.warn('openDeleteDialog: no listingId, bailing')
      return false
    }

    // Read-only sources (a write-protected volume, or inside an archive) can't
    // accept deletes. Surface as an alert before any dialog opens. Mirrors
    // `startRename` and `openNewFolderDialog`.
    const refusal = readOnlyRefusal('delete', sourcePane)
    if (refusal) {
      refuseOpening(mcpRequestId, { kind: 'readOnly', message: refusal.message }, () => {
        dialogs.showAlert(refusal.title, refusal.message)
      })
      return false
    }

    if (!sourcePaneRef) return false
    const rows = await readDeleteRows(sourcePaneRef, listingId, args)
    if (!rows) return false
    const { validEntries, hasSelection } = rows
    log.debug('openDeleteDialog: opening delete confirmation. {count} {entriesNoun}, sourceVolId={volId}', {
      count: validEntries.length,
      entriesNoun: pluralize(validEntries.length, 'valid entry', 'valid entries'),
      volId: sourceVolId,
    })

    const sourceItems: DeleteSourceItem[] = validEntries.map((e) => ({
      name: e.name,
      size: e.size,
      isDirectory: e.isDirectory,
      isSymlink: e.isSymlink,
      recursiveSize: e.recursiveSize,
      recursiveSizeComplete: e.recursiveSizeComplete,
      recursiveFileCount: e.recursiveFileCount,
    }))
    const sourcePaths = validEntries.map((e) => e.path)

    // Before any confirm UI: the person's own home folder, or a folder holding it,
    // is refused in its place, so no Delete button ever shows for it.
    const homeGuard = { sources: sourcePaths, volumeId: sourceVolId, action: 'delete' as const, mcpRequestId }
    if (!(await passesHomeFolderGuard(dialogs, homeGuard))) return false

    const sourceFolderPath = access.getPanePath(sourcePane)
    // Deleting an entry INSIDE a zip is permanent: there's no Trash inside an
    // archive (the backend rejects trashing an archive-inner path), so force
    // permanent + the archive warning regardless of the parent drive's trash
    // support or the F8/Shift+F8 preselect.
    const sourceIsArchive = pathCrossesArchiveBoundary(sourceFolderPath)
    // Asked for both F8 and Shift+F8: the dialog's own switch could send a
    // Shift+F8 back to the trash, into the download this routing exists to
    // avoid. An archive already has no trash, so it never needs asking.
    const cloud = sourceIsArchive ? NO_ONLINE_ONLY : await cloudOnlineOnlyRouting(sourcePaths)
    // `paneOffersTrash` answers CAN it (volume + archive); online-only is this
    // site's own reason to force a permanent delete, laid on top.
    const supportsTrash =
      cloud.onlineOnly === null && paneOffersTrash(sourceVolId, sourceFolderPath, access.getVolumes())

    dialogs.showDeleteConfirmation({
      sourcePaneSide: sourcePane,
      sourceItems,
      sourcePaths,
      sourceFolderPath,
      isPermanent: permanent || sourceIsArchive || cloud.onlineOnly !== null,
      supportsTrash,
      isArchive: sourceIsArchive,
      cloudOnlineOnly: cloud.onlineOnly,
      cloudFolderMayHoldOnlineOnly: cloud.folderMayHoldOnlineOnly,
      isFromCursor: !hasSelection,
      sourceVolumeId: sourceVolId,
      initiator,
      guard,
    })
    return true
  }

  return {
    startRename,
    refuseMultiRename,
    cancelRename,
    isRenaming,
    openNewFolderDialog,
    openNewFileDialog,
    createFolderDirect,
    createFileDirect,
    closeConfirmationDialog,
    isConfirmationDialogOpen,
    openViewerForCursor,
    openTransferDialog,
    openCopyDialog,
    duplicateInPlace: () => duplicateInPlace(access, dialogs),
    /** The home-folder guard a folder's rename editor waits for (`home-folder-guard.ts`). */
    passesRenameHomeFolderGuard: ({ path, volumeId }: { path: string; volumeId: string }) =>
      passesHomeFolderGuard(dialogs, { sources: [path], volumeId, action: 'rename' }),
    confirmRenameAsMove: (pane: 'left' | 'right', request: RenameAsMoveRequest) => {
      openRenameAsMove(access, dialogs, pane, request)
    },
    openMoveDialog,
    openCompressDialog,
    openDeleteDialog,
  }
}
