/**
 * Makes the inline rename editor an observable state: the soft dialog `rename`, in this window's
 * `open-dialogs` set and the backend's `SoftDialogTracker`.
 *
 * It isn't a dialog, but it waits on a person like one. Without this an agent couldn't see it or
 * confirm it, and `rename a` then `copy` silently ended the first editor (the copy dialog takes
 * focus, the blur cancels) while the agent believed it was still waiting. As a blocking dialog it
 * refuses that start out loud (`#lib/file-explorer/pane/DETAILS.md` § "The operation-start gate").
 *
 * Synchronous on purpose, unlike an effect: `closeEditor` opens a waiting Move dialog in the same
 * tick, and that open passes the same gate.
 *
 * Counted across the panes, since each has its own editor state and the id is one.
 */

import { notifyDialogClosed, notifyDialogOpened } from '#lib/tauri-commands/index.js'
import { registerDialogClose, unregisterDialogClose } from '#lib/ui/dialog-close-registry.js'
import { markDialogClosed, markDialogOpen } from '#lib/ui/open-dialogs.svelte.js'

const RENAME_DIALOG_ID = 'rename'

let openEditors = 0

/**
 * One pane's editor. `opened` and `closed` are safe to repeat: a chained step reopens with the editor
 * already up, and a pane's teardown closes whatever may be left.
 *
 * @param dismiss How `dialog close rename` ends this editor.
 */
export function createRenameEditorPresence(dismiss: () => void) {
  let open = false
  return {
    opened(): void {
      if (open) return
      open = true
      registerDialogClose(RENAME_DIALOG_ID, dismiss)
      if (openEditors++ === 0) {
        markDialogOpen(RENAME_DIALOG_ID)
        void notifyDialogOpened(RENAME_DIALOG_ID)
      }
    },
    closed(): void {
      if (!open) return
      open = false
      unregisterDialogClose(RENAME_DIALOG_ID, dismiss)
      if (--openEditors === 0) {
        markDialogClosed(RENAME_DIALOG_ID)
        void notifyDialogClosed(RENAME_DIALOG_ID)
      }
    },
  }
}
