/**
 * The no-file-URLs fallback for ⌘V: when the clipboard holds pasteable content
 * (text, image, PDF) instead of file URLs, this creates a file from it, lands the
 * cursor, toasts, and optionally starts a suppressed inline rename — gated by the
 * `fileOperations.pasteClipboardAsFile` setting.
 *
 * Lives beside `clipboard-operations.ts` (which calls it from `pasteFromClipboard`)
 * so the gating, dispatch, and toast composition are headless-testable.
 */
import { findFileIndex, onDirectoryDiff, pasteClipboardAsFile } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { addToast } from '$lib/ui/toast'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import PasteClipboardToastContent from '../PasteClipboardToastContent.svelte'
import type { FilePaneAPI } from './types'

/** Transient info toast on a successful paste-as-file (matches the transfer-complete precedent). */
const PASTE_TOAST_TIMEOUT_MS = 7000

/** Everything the fallback needs from the focused pane + destination. */
export interface PasteClipboardAsFileDeps {
  /** Destination volume id (e.g. the default local volume). */
  volumeId: string
  /** Destination directory (the focused pane's current path). */
  directory: string
  /** Destination listing id, for landing the cursor on the new file. */
  listingId: string
  /** Whether the destination listing shows a synthetic `..` parent row. */
  hasParent: boolean
  showHiddenFiles: boolean
  /** The focused pane, for cursor-land + optional rename. */
  paneRef: FilePaneAPI | undefined
  /**
   * Replicates today's no-file paste feedback (the "No files on the clipboard"
   * warn toast). Called when no file is created: setting = `doNothing`, or the
   * command reports nothing pasteable.
   */
  onNothingCreated: () => void
}

/**
 * Runs the no-file-URLs fallback. Gated by `fileOperations.pasteClipboardAsFile`:
 * `doNothing` short-circuits to `onNothingCreated`; otherwise it calls the backend
 * command, and on a created file lands the cursor + shows the info toast (and, for
 * `createFileAndRename`, starts a rename with the extension-change warning
 * suppressed). A `null` command result (nothing pasteable) routes to
 * `onNothingCreated`.
 */
export async function pasteClipboardContentAsFile(deps: PasteClipboardAsFileDeps): Promise<void> {
  const mode = getSetting('fileOperations.pasteClipboardAsFile')
  if (mode === 'doNothing') {
    deps.onNothingCreated()
    return
  }

  const created = await pasteClipboardAsFile(deps.volumeId, deps.directory)
  if (!created) {
    // Nothing pasteable on the clipboard: replicate today's no-file feedback.
    deps.onNothingCreated()
    return
  }

  // Land the cursor on the new file (reuses the mkfile/mkdir pending-cursor-name
  // plumbing that survives the trailing directory-diff).
  await moveCursorToNewFolder(
    deps.listingId,
    created.name,
    deps.paneRef,
    deps.hasParent,
    deps.showHiddenFiles,
    onDirectoryDiff,
    findFileIndex,
  )

  addToast(PasteClipboardToastContent, {
    level: 'info',
    timeoutMs: PASTE_TOAST_TIMEOUT_MS,
    props: { filename: created.name, kind: created.kind },
  })

  if (mode === 'createFileAndRename') {
    // Suppress the extension-change warning for THIS auto-started rename only, and
    // pass `expectedName` so the rename activates ONLY once the new file is under
    // the cursor — never latching a different row while the synthetic diff lands.
    deps.paneRef?.startRename({ suppressExtensionWarning: true, expectedName: created.name })
  }
}
