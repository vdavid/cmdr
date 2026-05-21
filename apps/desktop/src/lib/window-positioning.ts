/**
 * Tauri-touching positioning helpers for child windows (Settings, Debug,
 * file viewers).
 *
 * Tauri's `center: true` centers a new window on the *current monitor*, not
 * on the main window. We want children to feel attached to main: they should
 * open centered over main (Settings, Debug) or cascade from main's top-left
 * (viewers). The math lives in `window-positioning-utils.ts`; the runtime
 * wrappers here read main's geometry, the monitor list, and the in-session
 * rect cache (which `child_window_state.rs` exposes via Tauri commands).
 */

import { availableMonitors, getCurrentWindow } from '@tauri-apps/api/window'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { commands } from '$lib/ipc/bindings'
import type { MonitorRect, Rect } from './window-positioning-utils'

export type { ChildSize, MonitorRect, Rect } from './window-positioning-utils'
export {
  cascadeFromMain,
  cascadeOffset,
  centerOnMain,
  clampToMonitor,
  isFullyOnScreen,
  nearestMonitor,
  resolveChildPosition,
} from './window-positioning-utils'

/** Read main window's logical-pixel rect, or null if main isn't open. */
export async function readMainRect(): Promise<Rect | null> {
  const main = await WebviewWindow.getByLabel('main')
  if (!main) return null
  const [pos, size, scale] = await Promise.all([main.outerPosition(), main.outerSize(), main.scaleFactor()])
  return {
    x: pos.x / scale,
    y: pos.y / scale,
    width: size.width / scale,
    height: size.height / scale,
  }
}

/** Read all monitors as logical-pixel rects. */
export async function readMonitors(): Promise<MonitorRect[]> {
  const monitors = await availableMonitors()
  return monitors.map((m) => ({
    x: m.position.x / m.scaleFactor,
    y: m.position.y / m.scaleFactor,
    width: m.size.width / m.scaleFactor,
    height: m.size.height / m.scaleFactor,
  }))
}

/** Fetch the in-session saved rect for a child window label, or null. */
export async function readSavedRect(label: string): Promise<Rect | null> {
  return commands.getChildWindowRect(label)
}

/** Persist the current rect of a child window (called from move/resize listeners). */
export async function writeSavedRect(label: string, rect: Rect): Promise<void> {
  await commands.setChildWindowRect(label, rect)
}

/**
 * Attach move + resize listeners on the *current* window so its position
 * persists to the in-session cache. Returns an unlisten function.
 *
 * Use from inside a child window's `+page.svelte` `onMount` — Settings and
 * Debug each call this with their own label.
 */
export async function trackOwnRect(label: string): Promise<() => void> {
  const win = getCurrentWindow()
  const scale = await win.scaleFactor()

  const save = async (): Promise<void> => {
    try {
      const [pos, size] = await Promise.all([win.outerPosition(), win.outerSize()])
      await writeSavedRect(label, {
        x: pos.x / scale,
        y: pos.y / scale,
        width: size.width / scale,
        height: size.height / scale,
      })
    } catch {
      // Listener firing during teardown can throw; swallow silently.
    }
  }

  const unlistenMoved = await win.onMoved(() => void save())
  const unlistenResized = await win.onResized(() => void save())
  return () => {
    unlistenMoved()
    unlistenResized()
  }
}
