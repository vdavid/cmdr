/**
 * The prop shapes every dialog the pane can put on screen is opened with, plus
 * the dependency surface the dialog factories are built from.
 *
 * Types only, so the four dialog modules (`dialog-state.svelte.ts`,
 * `adopted-operation.svelte.ts`, `unlock-flow.svelte.ts`,
 * `transfer-pane-effects.ts`) can name each other's data without importing each
 * other's behavior.
 */

import type { Initiator, MacOnlyFiles, ProgressAtStop } from '#lib/tauri-commands/index.js'
import type {
  AlreadyGone,
  AppearedDuringMove,
  MetadataLeftBehind,
  DurabilityUnconfirmed,
  LockedKept,
  TopLevelSkipped,
  TrashRefusedItems,
  OpKind,
  SpaceShortfall,
  LockedContainer,
} from '#lib/ipc/bindings.js'
import type { SoftDialogId } from '#lib/ui/dialog-registry.js'
import type { CloudOnlineOnlyExtent, DeleteSourceItem } from '#lib/file-operations/delete/delete-dialog-utils.js'
import type { TransferOperationType, ConflictResolution, WriteOperationError } from '../types'
import type { DuplicateFollowUp } from './duplicate-rename'
import type { FilePaneAPI } from './types'
import type { PaneRevealAPI } from '../navigation/navigate-and-select'

/**
 * `TransferDialog`'s confirm payload: everything the user (or an MCP
 * `dialog_confirm`) decided about one transfer. Shared by `TransferDialog`'s
 * `onConfirm`, `DialogManager`'s `onTransferConfirm`, and
 * `dialog-state.svelte.ts`'s `handleTransferConfirm`.
 */
export interface TransferConfirmPayload {
  destination: string
  destinationName?: string
  volumeId: string
  previewId: string | null
  conflictResolution: ConflictResolution
  operationType: TransferOperationType
  /** Source filenames known to conflict at dest, for the BE to bulk-skip
   *  under `Skip all`. Empty when no conflicts were found or the pre-flight
   *  scan failed. */
  preKnownConflicts: string[]
  /** Whether to leave Mac-only hidden files out, as the dialog's toggle said. Absent from a
   *  confirm the dialog never showed: the setting's default then decides. */
  macOnlyFiles?: MacOnlyFiles
  /** Rename mode only: the leaf of the edited path, which the source moves under
   *  into `destination` (then the folder part alone). */
  newName?: string
  /** A single-item Move inside the item's own folder: the source pane renames it
   *  (`transfer-target.ts::isRenameInPlace`) and no transfer starts. */
  renameInPlace?: boolean
  /** F2 or the Background button: start the operation with no progress dialog
   *  (`background-operations.svelte.ts`). Everything else is what Enter sends. */
  startInBackground?: boolean
  /** Pressed by an MCP `dialog_confirm`: its round-trip id, which the start
   *  answers with the operation's id. */
  mcpRequestId?: string
}

/** Who waits on a confirm's start: an MCP `dialog_confirm`, by its round-trip id. */
export interface ConfirmOrigin {
  mcpRequestId?: string
}

/** How a programmatic confirm presses the button: Confirm, or (with
 *  `startInBackground`) the Background button beside it. */
export interface ConfirmOptions extends ConfirmOrigin {
  startInBackground?: boolean
  /** The name to commit, for the rename editor and the naming dialogs. Omitted, what the field holds. */
  name?: string
}

/**
 * How a programmatic commit of a name ended: the rename or create is `done`, a slow
 * volume is still on it, or the name was `refused` with the sentence the field would
 * show a person. `needsMoveDialog`: a rename that moved on to the Move dialog.
 */
export type CreatePress =
  | { kind: 'done' }
  | { kind: 'stillRunning' }
  | { kind: 'refused'; reason: string }
  | { kind: 'needsMoveDialog' }

/** A naming dialog's own OK, as something a caller outside it can press, under `name` when given. */
export type CreateConfirmer = (name?: string) => Promise<CreatePress>

/**
 * What a programmatic press did. `pressed`: it reached the start, which answers
 * an MCP round-trip itself. `notStarted`: a guard the dialog holds for a person
 * too stopped it, and the dialog stays up. `refusedPermanentDelete`: a
 * background press on a delete dialog that would delete permanently.
 */
export type ConfirmPress = 'pressed' | 'notStarted' | 'refusedPermanentDelete'

/**
 * The transfer dialog's own confirm, as something a caller outside it can press
 * (the MCP `dialog_confirm`): the same function its button runs, under the conflict
 * policy the caller names, or with none named the one it shows. `startInBackground` presses the Background
 * button instead, with the same guards F2 has. Resolves once the press reached
 * the start or a guard.
 */
export type TransferConfirmer = (
  conflictResolution: ConflictResolution | undefined,
  options?: ConfirmOptions,
) => Promise<ConfirmPress>

/**
 * The delete dialog's own confirm, as something a caller outside it can press
 * (the MCP `dialog_confirm`): the same function its button runs.
 * `startInBackground` presses the Background button instead, which exists only
 * for a TRASH: on a permanent delete it does nothing, as F2 does, and says so.
 */
export type DeleteConfirmer = (options?: ConfirmOptions) => Promise<ConfirmPress>

/**
 * How the progress dialog is taking a completion: `keptOpen` when it stays up on
 * its done state ("Keep the progress window open when a file operation
 * finishes"), so the parent runs its pane tail now and unmounts nothing until the
 * dialog's `onDoneClosed`.
 */
export interface CompletionView {
  keptOpen: boolean
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
  /** What a cross-filesystem move left in the source because it never carried it there. `null`
   *  on every other ending, which is the ordinary case. */
  appearedDuringMove: AppearedDuringMove | null
  /** Which of the user's TOP-LEVEL selected items landed nothing, split by kind. `null` from an
   *  engine that doesn't track it. Needed because `filesSkipped` counts leaves, so it can't say
   *  what happened to the selection. */
  topLevelSkipped: TopLevelSkipped | null
  /** What a batch trash had to leave where it was, and why. `null` on every other ending and on
   *  a trash the OS took in full, which is the ordinary case. A refusal here is NOT a skip: the
   *  items are still in the pane, so the completion can't read as a clean success. */
  refused: TrashRefusedItems | null
  /** What the destination couldn't store of the sources' metadata. `null` when nothing worth
   *  keeping stayed behind, which is the ordinary case. */
  metadataLeftBehind: MetadataLeftBehind | null
  /** The Locked items a move left where they were. `null` when it left none, which is the
   *  ordinary case. */
  lockedKept: LockedKept | null
  /** A copy whose closing flush couldn't confirm its files are on the disk. `null` when the flush
   *  confirmed everything, which is the ordinary case. */
  durabilityUnconfirmed: DurabilityUnconfirmed | null
  /** What a delete or trash found already gone and counted as done. `null` when every item was
   *  there, which is the ordinary case. */
  alreadyGone: AlreadyGone | null
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
  /** Explicit leaf name for a single local copy. */
  destinationName?: string
  /** Not applicable for delete/trash */
  direction?: 'left' | 'right'
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
  /** Copy, move, compress: the dialog's "Skip Mac-only hidden files" answer, if it showed. */
  macOnlyFiles?: MacOnlyFiles
  /** Copy only: `proceed` when the person chose "Copy anyway" after a space shortfall. */
  spaceShortfall?: SpaceShortfall
  /** Top-level files the operation will transfer (for the completion toast's per-type
   *  split). Supplied by F5/F6 (real selection counts), drag-and-drop, and clipboard
   *  paste (each from a top-level kind probe). Absent only when the split is unknown
   *  (a kind probe came back partial), where the composer falls back to file counts. */
  fileCount?: number
  /** Top-level folders the operation will transfer (for the completion toast's per-type split). */
  folderCount?: number
  /** MCP round-trip id, present only for an op a `dialog_confirm` pressed. Forwarded
   *  to the start, which replies `mcp-response` with the spawned operationId. */
  mcpRequestId?: string
  /** Who started this operation, for the backend's provenance record. */
  initiator: Initiator
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
  /** Rename mode: a move of the ONE source into `destinationPath` under this name
   *  (`rename-as-move.ts`). Kept on retry, which renames the same way. */
  newName?: string
  /** The operation runs in the BACKGROUND, with no progress dialog: started
   *  there (F2 in a setup dialog) or sent there (Queue). A retry and an
   *  archive-password re-dispatch start it the same way again
   *  (`background-operations.svelte.ts`). */
  startInBackground?: boolean
}

/**
 * An operation this window did NOT start, shown in the progress dialog because
 * the user pressed Show on its queue row.
 *
 * Everything live comes from the operation's session; these fields are the
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
  /** Set when this operation IS the reversal of a finished one, to the kind of
   *  the operation it reverses, straight off the registry row. The dialog titles
   *  itself by what the reversal will DO rather than by `operationType`, which
   *  for a reversal names the syscall (undoing a copy runs as a delete).
   *  `#lib/file-operations/reversal-wording.ts`. */
  reverses: OpKind | null
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
  /** The pane it creates in, which an MCP caller names and focus doesn't follow. */
  pane: 'left' | 'right'
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
  /** Who started this create. */
  initiator: Initiator
  /** It opened by itself, so it must arm before a keystroke can touch it (`ModalDialog`). */
  guard?: boolean
}

export interface NewFileDialogPropsData {
  /** The pane it creates in, which an MCP caller names and focus doesn't follow. */
  pane: 'left' | 'right'
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
  /** Who started this create. */
  initiator: Initiator
  /** It opened by itself, so it must arm before a keystroke can touch it (`ModalDialog`). */
  guard?: boolean
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
  /** How far the operation got when it stopped, from the `write-error` event.
   *  Null for a failure adopted from a snapshot, which carries only the error. */
  progressAtStop: ProgressAtStop | null
  /** What the dialog's Retry starts: the failed operation's birth context, ready
   *  to dispatch again (`dialog-state.svelte.ts::retryPropsFrom`). Null when this
   *  window didn't start it (an adopted operation), and then there's no Retry. */
  retry: TransferProgressPropsData | null
}

/**
 * What a password prompt unlocks: the `container` the backend's
 * `NeedsPassword` named (`LockedContainer` in `#lib/ipc/bindings`).
 * - `'archive'`: an encrypted archive. The password is stored unchecked and
 *   proves itself on the retry.
 * - `'resticRepo'`: a restic backup repo. The password is checked before the
 *   prompt closes, and may be remembered in the Keychain.
 */
export type UnlockContainer = LockedContainer

/** A listing that needs a password before it can list, bubbled from a pane to
 *  the browse-time prompt. */
export interface BrowseUnlockRequest {
  volumeId: string
  /** The path that failed to list. */
  path: string
  /** What's locked, from the listing error's typed reason. */
  container: UnlockContainer
  /** True when a stored password was just rejected. */
  wrongAttempt: boolean
  /** Re-lists the same directory once the password is in. */
  retry: () => void
}

export interface UnlockPromptPropsData {
  /** What's locked, as the backend's `NeedsPassword` said. */
  container: UnlockContainer
  /** Display name: the archive file ("photos.zip") or the repo folder ("m3-restic"). */
  name: string
  /** True when a password was rejected: re-prompt with distinct copy. */
  wrongAttempt: boolean
  /** Volume the container lives on (a routed pane keeps its parent drive's volume id). */
  parentVolumeId: string
  /** The container's path, or any path inside it, to unlock against. */
  path: string
  /**
   * Which flow raised the prompt:
   * - `'transfer'`: a copy/move out of a locked container; on unlock it
   *   re-dispatches the operation the birth slot holds.
   * - `'browse'`: a listing that needs the password (a header-encrypted
   *   archive, a repo's snapshots); on unlock it re-lists via `retry`.
   */
  mode: 'transfer' | 'browse'
  /** Browse mode only: re-load the same directory after the unlock. */
  retry?: () => void
  /** A repo is checking the password right now. */
  checking: boolean
  /** The last check couldn't reach the repo (gone, or too slow), so it said nothing about the password. */
  unreachable: boolean
  /** How many passwords the repo refused while this prompt was up. */
  rejections: number
}

export interface DeleteDialogPropsData {
  /** The pane these items are in, which an MCP caller names and focus doesn't follow. */
  sourcePaneSide: 'left' | 'right'
  sourceItems: DeleteSourceItem[]
  sourcePaths: string[]
  sourceFolderPath: string
  isPermanent: boolean
  supportsTrash: boolean
  isFromCursor: boolean
  sourceVolumeId: string
  /**
   * Source is INSIDE a zip. Deleting an archive entry is permanent (there's no
   * Trash inside a zip), so the dialog forces permanent mode and shows an
   * archive-specific warning instead of the generic no-trash banner.
   */
  isArchive?: boolean
  /**
   * A selected item in a cloud-storage folder (`~/Library/CloudStorage/<provider>/…`)
   * is online-only, so trashing it would download it first. The dialog forces
   * permanent mode and explains that the service keeps its own copy, in wording
   * that depends on whether the WHOLE selection is online-only. `null` when none
   * of it is. Decided in Rust: `crates/cmdr-operations/src/delete/cloud_trash.rs`.
   */
  cloudOnlineOnly?: CloudOnlineOnlyExtent | null
  /**
   * A selected FOLDER sits in such a drive and the top-level check found nothing:
   * only the dialog's own scan walk can say whether something inside is
   * online-only. The dialog flips itself when the walk reports one, and holds a
   * confirm until it knows.
   */
  cloudFolderMayHoldOnlineOnly?: boolean
  /** Who started this delete. Only `user` may skip the confirmation. */
  initiator: Initiator
  /** It opened by itself, so it must arm before a keystroke can touch it (`ModalDialog`). */
  guard?: boolean
}

export interface DialogStateDeps {
  getLeftPaneRef: () => FilePaneAPI | undefined
  getRightPaneRef: () => FilePaneAPI | undefined
  getFocusedPaneRef: () => FilePaneAPI | undefined
  getShowHiddenFiles: () => boolean
  /**
   * The pane-picking slice of the explorer, for a dialog outcome whose follow-up
   * action navigates (the trash toast's "Go to trash"). Narrow on purpose: a
   * dialog has no business driving the whole coordinator, and this is the subset
   * `#lib/file-explorer/navigation/navigate-and-select` needs.
   */
  getExplorer: () => PaneRevealAPI | undefined
  /** The "Skip confirmation" setting: copy, move, and trash start without their dialog. */
  skipsConfirmations: () => boolean
  onRefocus: () => void
  onOpenInEditor: (path: string) => void
}
