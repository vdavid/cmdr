/**
 * The short "{file} is opening in {app}…" toast Enter and double-click raise when a
 * file goes to its default app.
 *
 * The launch itself is fire-and-forget (`open`), so without this a file on a slow
 * share looks like a dead keypress for seconds. The app name comes from LaunchServices
 * by EXTENSION (`default_app_name`), which never touches the file, so the toast lands
 * as fast on a slow share as on a local disk. A bundle (`.app`) skips the lookup: it
 * IS the app.
 *
 * Never throws and never delays the launch: the caller starts both side by side.
 */

import { defaultAppName } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { FileEntry } from '../types'

/** How long the toast stays. Long enough to read, short enough to never pile up. */
export const OPENING_TOAST_MS = 2000

/**
 * One id, so a burst of Enter presses replaces the toast instead of stacking it, and
 * an open that didn't happen replaces it with the reason (`default-app-open.ts`).
 */
export const OPENING_TOAST_ID = 'opening-file'

export async function announceOpening(entry: FileEntry): Promise<void> {
  // `defaultAppName` answers `null` rather than throwing (off macOS, or no app).
  const appName = entry.isDirectory ? null : await defaultAppName(entry.path)
  const message = appName
    ? tString('fileExplorer.openingToast.withApp', { fileName: entry.name, appName })
    : tString('fileExplorer.openingToast.noApp', { fileName: entry.name })
  addToast(message, { id: OPENING_TOAST_ID, dismissal: 'transient', timeoutMs: OPENING_TOAST_MS })
}
