/**
 * Enter, double-click, and the context menu's Open on a file: hand it to its default
 * app, or say plainly why that didn't happen.
 *
 * Two gates, then the launch:
 *
 * 1. `rowHasFileOnThisMac`, instant and frontend-only: a row on a phone, a server, or
 *    in the `.git` portal or a backup snapshot has no path another app can open.
 * 2. The backend's own check, inside `openFile`: a direct SMB share whose macOS mount
 *    went away still lists `/Volumes/…` rows, and only its volume knows they lead
 *    nowhere (`src-tauri/src/file_system/default_app.rs`).
 *
 * Either refusal gets a toast pointing at the viewer, which reads the file over Cmdr's
 * own connection. The launch itself now answers too: when macOS's opener says no, the
 * toast says so, where the open used to vanish into a silent catch.
 * ❌ Downloading to a temp file and opening that isn't built.
 */

import { asOpenPathError, openFile } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
import type { FileEntry } from '../types'
import { announceOpening, OPENING_TOAST_ID } from './opening-toast'
import { rowHasFileOnThisMac } from './volume-capabilities'

const log = getAppLogger('fileExplorer')

/** Long enough to read a two-sentence notice that says what to do next. */
const NOTICE_TIMEOUT_MS = 8000

/**
 * What an open did: asked macOS to open the file, refused the row without launching,
 * or launched and heard macOS say no (or nothing in time).
 */
export type DefaultAppOpenOutcome = 'opened' | 'refusedNotOnThisMac' | 'notOpened'

/**
 * The key bound to View right now, for a toast to name. ❗ Read live, ❌ never a
 * literal "F3": it's rebindable in Settings > Shortcuts.
 */
export function viewShortcut(): string {
  return shortcutAsText('file.view', 'toast') ?? 'F3'
}

/**
 * Opens `entry` in its default app, or tells the user why it can't. `volumeId` is the
 * volume the ROW lives on: a pane's own, or the search-results snapshot's (which the
 * backend has no volume for, so its rows go straight to the opener).
 *
 * Never throws: every outcome the user should hear about becomes a toast, and the
 * callers are fire-and-forget key and menu handlers.
 */
export async function openWithDefaultAppOrExplain(volumeId: string, entry: FileEntry): Promise<DefaultAppOpenOutcome> {
  if (!rowHasFileOnThisMac(volumeId, entry.path)) {
    log.info('Not opening {path}: no file another app can open on volume {volumeId}', { path: entry.path, volumeId })
    explainNotOnThisMac()
    return 'refusedNotOnThisMac'
  }
  // Side by side with the launch, never in front of it: the toast is the feedback a
  // slow share otherwise lacks.
  const announcing = announceOpening(entry)
  try {
    await openFile(entry.path, volumeId)
    return 'opened'
  } catch (error) {
    // Let the "Opening…" toast land first, so the reason replaces it and never the
    // other way round: its app-name lookup can answer after a fast refusal.
    await announcing
    return explainRefusal(entry, error)
  }
}

function explainRefusal(entry: FileEntry, error: unknown): DefaultAppOpenOutcome {
  const failure = asOpenPathError(error)
  log.info('{fileName} didn’t open: {reason}', { fileName: entry.name, reason: failure?.type ?? error })
  if (failure?.type === 'notOnThisMac') {
    explainNotOnThisMac()
    return 'refusedNotOnThisMac'
  }
  // The opener stopped answering, which on a wedged mount is no answer at all. The app
  // may still open the file, so saying it didn't could be wrong; the log has it.
  if (failure?.type === 'timedOut') return 'notOpened'
  addToast(tString('fileExplorer.open.didNotOpen', { fileName: entry.name }), {
    id: OPENING_TOAST_ID,
    level: 'warn',
    timeoutMs: NOTICE_TIMEOUT_MS,
  })
  return 'notOpened'
}

function explainNotOnThisMac(): void {
  addToast(tString('fileExplorer.open.notOnThisMac', { viewKey: viewShortcut() }), {
    id: OPENING_TOAST_ID,
    level: 'info',
    timeoutMs: NOTICE_TIMEOUT_MS,
  })
}
