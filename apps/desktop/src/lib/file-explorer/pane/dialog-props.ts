/**
 * The prop shapes every dialog the pane can put on screen is opened with, plus
 * the dependency surface the dialog factories are built from.
 *
 * Types only, so the four dialog modules (`dialog-state.svelte.ts`,
 * `adopted-operation.svelte.ts`, `archive-password-flow.svelte.ts`,
 * `transfer-pane-effects.ts`) can name each other's data without importing each
 * other's behavior.
 */

import type { Initiator } from '$lib/tauri-commands'
import type { SoftDialogId } from '$lib/ui/dialog-registry'
import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
import type { TransferOperationType, SortColumn, SortOrder, ConflictResolution, WriteOperationError } from '../types'
import type { DuplicateFollowUp } from './duplicate-rename'
import type { FilePaneAPI } from './types'
import type { PaneRevealAPI } from '../navigation/navigate-and-select'

/**
 * `TransferDialog`'s confirm payload: everything the user (or an MCP
 * auto-confirm) decided about one transfer. Shared by `TransferDialog`'s
 * `onConfirm`, `DialogManager`'s `onTransferConfirm`, and
 * `dialog-state.svelte.ts`'s `handleTransferConfirm`.
 */
export interface TransferConfirmPayload {
  destination: string
  volumeId: string
  previewId: string | null
  conflictResolution: ConflictResolution
  operationType: TransferOperationType
  /** Source filenames known to conflict at dest, for the BE to bulk-skip
   *  under `Skip all`. Empty when no conflicts were found or the pre-flight
   *  scan failed. */
  preKnownConflicts: string[]
}

/**
 * What a transfer operation reports when it finishes: `TransferProgressDialog`'s
 * `onComplete`, shared by both the started and adopted arms
 * (`onTransferComplete` / `onAdoptedComplete`).
 */
export interface TransferCompletePayload {
  filesProcessed: number
  filesSkipped: number
  bytesProcessed: number
}

/**
 * BIRTH CONTEXT: what this window started, and therefore what it may do to its
 * panes afterwards. Also the input the archive-password submit re-dispatches
 * from, which is why it lives in a slot of its own. `DETAILS.md` § "Birth
 * context".
 */
export interface TransferProgressPropsData {
  operationType: TransferOperationType
  sourcePaths: string[]
  sourceFolderPath: string
  sourcePaneSide: 'left' | 'right'
  /** Not applicable for delete/trash */
  destinationPath?: string
  /** Not applicable for delete/trash */
  direction?: 'left' | 'right'
  sortColumn: SortColumn
  sortOrder: SortOrder
  previewId: string | null
  sourceVolumeId: string
  /** Not applicable for delete/trash */
  destVolumeId?: string
  /** Not applicable for delete/trash */
  conflictResolution?: ConflictResolution
  /** Per-item sizes for trash progress (from scan or drive index) */
  itemSizes?: number[]
  /** Source filenames known to conflict at dest (from pre-flight scan).
   *  Forwarded to the BE so it can bulk-skip them upfront under `Skip all`. */
  preKnownConflicts?: string[]
  /** Top-level files the operation will transfer (for the completion toast's per-type
   *  split). Supplied by F5/F6 (real selection counts), drag-and-drop, and clipboard
   *  paste (each from a top-level kind probe). Absent only when the split is unknown
   *  (a kind probe came back partial), where the composer falls back to file counts. */
  fileCount?: number
  /** Top-level folders the operation will transfer (for the completion toast's per-type split). */
  folderCount?: number
  /** MCP round-trip id, present only for an auto-confirmed MCP op. Forwarded to
   *  the progress state so it replies `mcp-response` with the spawned operationId. */
  mcpRequestId?: string
  /** Who triggered this operation (`aiClient` for MCP-originated writes). */
  initiator?: Initiator
  /**
   * What happens when this operation duplicates ONE item in the folder it
   * already lived in: `openRenameEditor` (paste and F5) or `nothing`.
   *
   * Required on purpose. Every gesture that duplicates dispatches this same
   * operation, so a trigger that says nothing would inherit whatever the last
   * one wanted; here it can't compile without answering. `duplicate-rename.ts`,
   * and `file-operations/transfer/DETAILS.md` § "One transfer entry seam" for
   * why the answer differs per gesture.
   */
  duplicateFollowUp: DuplicateFollowUp
}

/**
 * An operation this window did NOT start, shown in the progress dialog because
 * the user pressed Show on its queue row.
 *
 * Everything live comes from the operation's session; these four fields are the
 * dialog's chrome, and they are exactly what the registry snapshot carries.
 * There is deliberately nothing else here: no `sourcePaths`, no pane side, no
 * counts: `DETAILS.md` § "Birth context" argues why an adopted view must not
 * invent them.
 */
export interface AdoptedOperationData {
  operationId: string
  operationType: TransferOperationType
  /** The operation's source, from its registry row. Display only. */
  sourcePath: string | null
  /** The operation's destination, from its registry row. Display only. */
  destinationPath: string | null
}

/** What came of a request to show a running operation in the progress dialog.
 *  `busy` is a refusal the caller has to surface; `alreadyShowing` is a
 *  successful no-op (the user pressed Show on the operation already up). */
export type ForegroundOperationVerdict = 'adopted' | 'alreadyShowing' | 'busy'

/**
 * What came of a command that would START a file operation.
 *
 * The refusal names the dialog in the way as a TYPED id, never only in prose: an
 * MCP agent acts on it to decide what to close, so it's a contract, and the
 * repo's `no-error-string-match` rule applies to a message an agent parses just
 * as it does to one our own code would.
 */
export type OperationStartVerdict = 'started' | { blockedBy: SoftDialogId }

export interface NewFolderDialogPropsData {
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
  /** Who triggered this create (`aiClient` for the MCP `mkdir` tool). */
  initiator?: Initiator
}

export interface NewFileDialogPropsData {
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
  /** Who triggered this create (`aiClient` for the MCP `mkfile` tool). */
  initiator?: Initiator
}

export interface AlertDialogPropsData {
  title: string
  message: string
  /** A path the alert is about, shown as a copyable block instead of inside `message`. */
  path?: string
}

export interface TransferErrorPropsData {
  operationType: TransferOperationType
  error: WriteOperationError
}

export interface ArchivePasswordPropsData {
  /** Display name of the archive being unlocked (e.g. "photos.zip"). */
  archiveName: string
  /** True when the stored password was rejected: re-prompt with distinct copy. */
  wrongAttempt: boolean
  /** Volume the archive lives on (the archive pane's parent-drive volume id). */
  parentVolumeId: string
  /** The archive path (or an inner path) to store the password against. */
  archivePath: string
  /**
   * Which flow raised the prompt:
   * - `'transfer'`: a copy/move out of an encrypted archive; on unlock it
   *   re-dispatches the operation the birth slot holds.
   * - `'browse'`: a directory listing of a header-encrypted archive; on unlock it
   *   re-lists the same directory via `retry`.
   */
  mode: 'transfer' | 'browse'
  /** Browse mode only: re-load the same directory after the password is stored. */
  retry?: () => void
}

export interface DeleteDialogPropsData {
  sourceItems: DeleteSourceItem[]
  sourcePaths: string[]
  sourceFolderPath: string
  isPermanent: boolean
  supportsTrash: boolean
  isFromCursor: boolean
  sortColumn: SortColumn
  sortOrder: SortOrder
  sourceVolumeId: string
  /**
   * Source is INSIDE a zip. Deleting an archive entry is permanent (there's no
   * Trash inside a zip), so the dialog forces permanent mode and shows an
   * archive-specific warning instead of the generic no-trash banner.
   */
  isArchive?: boolean
  /** When true, dialog auto-confirms without user interaction (MCP auto-confirm). */
  autoConfirm?: boolean
  /** MCP round-trip id, present only for an auto-confirmed MCP delete/trash.
   *  Forwarded to the progress state so it replies with the spawned operationId. */
  mcpRequestId?: string
  /** Who triggered this delete (`aiClient` for the MCP `delete` tool). */
  initiator?: Initiator
}

export interface DialogStateDeps {
  getLeftPaneRef: () => FilePaneAPI | undefined
  getRightPaneRef: () => FilePaneAPI | undefined
  getFocusedPaneRef: () => FilePaneAPI | undefined
  getFocusedPaneSide: () => 'left' | 'right'
  getShowHiddenFiles: () => boolean
  /**
   * The pane-picking slice of the explorer, for a dialog outcome whose follow-up
   * action navigates (the trash toast's "Go to trash"). Narrow on purpose: a
   * dialog has no business driving the whole coordinator, and this is the subset
   * `$lib/file-explorer/navigation/navigate-and-select` needs.
   */
  getExplorer: () => PaneRevealAPI | undefined
  onRefocus: () => void
  onOpenInEditor: (path: string) => void
}
