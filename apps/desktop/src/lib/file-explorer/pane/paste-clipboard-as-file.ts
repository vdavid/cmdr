/**
 * The no-file-URLs fallback for ⌘V: when the clipboard holds pasteable content
 * (text, image, PDF) instead of file URLs, this creates a file from it, lands the
 * cursor, toasts, and optionally starts a suppressed inline rename — gated by the
 * `fileOperations.pasteClipboardAsFile` setting.
 *
 * Lives beside `clipboard-operations.ts` (which calls it from `pasteFromClipboard`)
 * so the gating, dispatch, and toast composition are headless-testable.
 */
import { findFileIndex, onDirectoryDiff, pasteClipboardAsFile, type PastedClipboardFile } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { addToastForPane, dismissToast, type ToastOriginPane } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { asMutationError } from '$lib/file-operations/mutation-error'
import { renderMutationError } from '$lib/file-operations/mutation-error-messages'
import { moveCursorToNewFolder } from '$lib/file-operations/mkdir/new-folder-operations'
import PasteClipboardToastContent from '../PasteClipboardToastContent.svelte'
import type { FilePaneAPI } from './types'

/** Transient info toast on a successful paste-as-file (matches the transfer-complete precedent). */
const PASTE_TOAST_TIMEOUT_MS = 7000

const log = getAppLogger('clipboard')

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
  /** The focused pane side, so the paste-as-file toast is tagged to it. */
  originPane: ToastOriginPane
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

  // A slow volume: say so, and once it lands only REPORT it, since by then the
  // person may be anywhere (no cursor move, no auto-rename).
  const stillPastingToastId = `paste-still-running-${deps.originPane}`
  // An object, not a `let`: the flag flips inside a callback TypeScript can't see run.
  const progress = { slow: false }
  let created: PastedClipboardFile | null
  try {
    created = await pasteClipboardAsFile(deps.volumeId, deps.directory, {
      onStillRunning: () => {
        progress.slow = true
        addToastForPane(deps.originPane, tString('fileExplorer.clipboard.stillPasting'), {
          level: 'info',
          dismissal: 'persistent',
          id: stillPastingToastId,
        })
      },
    })
  } catch (e) {
    if (progress.slow) dismissToast(stillPastingToastId)
    const failure = asMutationError(e)
    if (!failure) log.warn('Paste-as-file threw an untyped value: {error}', { error: String(e) })
    const reason = renderMutationError(failure ?? { type: 'unexpected', detail: '' }, 'file')
    // Persistent: a late refusal may arrive after the person moved on.
    addToastForPane(deps.originPane, tString('fileExplorer.clipboard.notPasted', { reason }), {
      level: 'error',
      dismissal: 'persistent',
    })
    return
  }
  if (progress.slow) dismissToast(stillPastingToastId)
  if (!created) {
    // Nothing pasteable on the clipboard: replicate today's no-file feedback.
    deps.onNothingCreated()
    return
  }

  // Land the cursor on the new file (reuses the mkfile/mkdir pending-cursor-name
  // plumbing that survives the trailing directory-diff).
  if (!progress.slow) {
    await moveCursorToNewFolder(
      deps.listingId,
      created.name,
      deps.paneRef,
      deps.hasParent,
      deps.showHiddenFiles,
      onDirectoryDiff,
      (args) => findFileIndex(args.listingId, args.filename, args.showHiddenFiles),
    )
  }

  addToastForPane(deps.originPane, PasteClipboardToastContent, {
    level: 'info',
    timeoutMs: PASTE_TOAST_TIMEOUT_MS,
    props: { filename: created.name, kind: created.kind },
  })

  if (mode === 'createFileAndRename' && !progress.slow) {
    // Suppress the extension-change warning for THIS auto-started rename only, and
    // pass `expectedName` so the rename activates ONLY once the new file is under
    // the cursor — never latching a different row while the synthetic diff lands.
    deps.paneRef?.startRename({ suppressExtensionWarning: true, expectedName: created.name })
  }
}
