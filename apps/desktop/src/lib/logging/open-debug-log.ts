import { getDebugLogPath } from '$lib/tauri-commands'
import { openFileViewer } from '$lib/file-viewer/open-viewer'
import { tString } from '$lib/intl/messages.svelte'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from './logger'

/** Both the Help menu and the palette open the same local, read-only log viewer, tailed so it's live. */
export async function openDebugLog(): Promise<void> {
  try {
    const path = await getDebugLogPath()
    if (path == null) {
      addToast(tString('commands.handler.viewDebugLog.disabled'), { level: 'info' })
      return
    }
    await openFileViewer(path, 'root', { tail: true })
  } catch (error) {
    getAppLogger('logging').error('Could not open debug log: {error}', { error: String(error) })
    addToast(tString('commands.handler.viewDebugLog.unavailable'), { level: 'error' })
  }
}
