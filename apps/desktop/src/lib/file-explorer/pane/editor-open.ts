/**
 * F4: hand a file to the text editor, or say plainly why not.
 *
 * Every editor launch takes a path the Mac's own filesystem knows. A row on a phone
 * (MTP, ADB), on a server (SFTP, WebDAV, S3), inside an archive, or in the virtual `.git`
 * portal has none, and handing its `adb://` or archive-inner path over does nothing
 * at all: no editor, no message. So every F4 goes through here, and a row with no
 * real file behind it gets a toast pointing at F3, which can read it.
 * ❌ Editing those files in place (pull, edit, write back) isn't built.
 *
 * Which app opens the file, and everything said around that launch, belongs to
 * `#lib/text-editor`. The guard stays here because it's a pane question: is there
 * a real file behind this row. The setting never widens what it accepts.
 *
 * The gate is `rowHasFileOnThisMac`, the same one Enter's default-app open uses
 * (`default-app-open.ts`): the path scheme first, then the row's capability, which
 * knows what a path can't show (an archive's insides and the `.git` portal look like
 * plain paths).
 */

import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { openFileInEditor } from '#lib/text-editor/open-file-in-editor.js'
import { rowHasFileOnThisMac } from './volume-capabilities'

/**
 * What an F4 press did: asked an app to open the file, refused the row on screen,
 * or tried and couldn't start the launch (which `#lib/text-editor` has already
 * said). Named for the PRESS, apart from the wire type `EditorOpenOutcome`, which
 * says what Rust's launch did.
 */
export type EditorPressOutcome = 'opened' | 'refusedNotOnThisMac' | 'launchFailed'

/** Whether `rowPath`, living on `volumeId`, is a file the OS text editor can open. */
export function canOpenInEditor(volumeId: string, rowPath: string): boolean {
  return rowHasFileOnThisMac(volumeId, rowPath)
}

/**
 * Opens the file in the chosen text editor, or tells the user why it can't, and
 * reports which. `volumeId` is the volume the ROW lives on: a pane's own, or the one
 * a search-results snapshot covered.
 */
export async function openInEditorOrExplain(volumeId: string, rowPath: string): Promise<EditorPressOutcome> {
  if (!canOpenInEditor(volumeId, rowPath)) {
    addToast(tString('fileExplorer.edit.notOnThisMac'), { level: 'info' })
    return 'refusedNotOnThisMac'
  }
  return (await openFileInEditor(rowPath)) ? 'opened' : 'launchFailed'
}
