/**
 * Handing a file to the text editor the user chose, and saying anything the launch
 * brings back other than a plain success.
 *
 * The pane guard (`file-explorer/pane/editor-open.ts`) has already decided the row
 * is a real file on this Mac, so this module never touches a pane. Rust decides
 * which app launches and whether it's still there; this side owns the stored
 * choice and every word shown.
 */

import { asOpenInEditorError, openInEditor } from '$lib/tauri-commands'
import { addToast, dismissToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { SYSTEM_DEFAULT_EDITOR_CHOICE } from './text-editor-choice'
import { getTextEditorChoice, setTextEditorChoice } from './text-editor-setting'
import TextEditorToastContent from './TextEditorToastContent.svelte'

/**
 * One id for everything this feature says, so a second word replaces the first
 * rather than stacking: they're all about the same press and the same setting.
 *
 * ❗ An id, ❌ never `toastGroup` + `maxInGroup: 1`: a group full of PERSISTENT
 * toasts drops the INCOMING one instead of evicting anything
 * (`open-terminal/DETAILS.md` § The toasts). The pane guard's "not on this Mac"
 * refusal keeps its own transient toast.
 */
const TOAST_ID = 'text-editor'

/** Wider than the 360 default, so a sentence carrying a long app name doesn't run to five lines. */
const TOAST_WIDTH_PX = 400

/**
 * Says one thing, retiring whatever this feature said before.
 *
 * The dismiss is what makes the replacement total: `addToast`'s same-id path keeps
 * the first toast's `dismissal` and width, which is wrong for a different message.
 */
function replaceToast(content: Parameters<typeof addToast>[0], options: Parameters<typeof addToast>[1]): void {
  dismissToast(TOAST_ID)
  addToast(content, { ...options, id: TOAST_ID })
}

/**
 * Opens `path` in the chosen text editor. Resolves `true` when an app was asked to
 * open the file (a fallback to the system default included), `false` when the
 * launch never started.
 *
 * Never throws: every outcome the user should hear about becomes a toast, and the
 * callers are fire-and-forget key and command handlers.
 */
export async function openFileInEditor(path: string): Promise<boolean> {
  // Read synchronously, before any await, so the launch starts in the same tick
  // as the key press that asked for it.
  const appChoice = getTextEditorChoice()

  let report: Awaited<ReturnType<typeof openInEditor>>
  try {
    report = await openInEditor(path, appChoice, false)
  } catch (error) {
    const refusal = asOpenInEditorError(error)
    replaceToast(
      tString(refusal?.type === 'timedOut' ? 'fileExplorer.edit.timedOut' : 'fileExplorer.edit.launchRefused'),
      { level: 'error' },
    )
    return false
  }

  if (report.outcome === 'chosen_app_missing_opened_default_instead') {
    reportMissingApp(report.openedInName)
  }
  return true
}

/**
 * Puts the setting back to the system default, so the next press opens the same
 * app without saying this again, and names the app the file DID open in.
 *
 * Worded from the report, never from the setting: once a bundle is gone there's
 * nothing left to read its name from, and the setting has just been reset anyway.
 */
function reportMissingApp(openedInName: string | null): void {
  setTextEditorChoice(SYSTEM_DEFAULT_EDITOR_CHOICE)
  const message =
    openedInName === null
      ? tString('fileExplorer.edit.appMissingUnnamed')
      : tString('fileExplorer.edit.appMissing', { app: openedInName })
  replaceToast(TextEditorToastContent, {
    level: 'info',
    dismissal: 'persistent',
    props: { message },
    widthPx: TOAST_WIDTH_PX,
  })
}
