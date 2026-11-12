/**
 * `view.toggleFullScreen`: the main window in or out of full screen, through the window's own API.
 * On macOS that's the native full screen AppKit's View menu item enters (tao's `toggleFullScreen:`),
 * the same one `escape-key.ts` leaves; on Windows and Linux it's the window manager's. `window_state`
 * asks the window whether it's full screen each time it saves, so it records this road too.
 */
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('fullScreen')

export async function toggleFullScreen(): Promise<void> {
  try {
    const appWindow = getCurrentWindow()
    await appWindow.setFullscreen(!(await appWindow.isFullscreen()))
  } catch (error) {
    log.warn("Couldn't toggle full screen: {error}", { error })
  }
}
