/**
 * Grow the MAIN window when the Ask Cmdr rail opens, and shrink it back when it
 * closes, so the two panes keep their size instead of being squeezed to make
 * room for the rail. The rail sits on the right, so the window grows rightward
 * (the panes stay put), sliding left only when its right edge would leave the
 * screen, and capping at the monitor width (past that the panes do give up space
 * — there's nowhere else to take it from).
 *
 * The geometry math is the pure `growRectForRail` / `shrinkRectForRail` in
 * `window-positioning-utils.ts`; this file is the Tauri-touching wrapper (read
 * the window's rect + monitors, apply `setSize` / `setPosition`). We run on the
 * main window, which holds the size/position perms in `capabilities/default.json`.
 *
 * Fullscreen and maximized are left alone: the window already fills the screen,
 * so it can't grow and the flex layout shrinks the panes instead (the capped
 * case). E2E runs are skipped entirely: they deliberately keep the window
 * ordered to the back (`show_main_window`), and resizing it re-fronts the window
 * over the developer's work — the whole point of the E2E backgrounding. See
 * `lib/ask-cmdr/DETAILS.md` § Window growth.
 */

import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { getAppMode } from '$lib/app-mode'
import { getAppLogger } from '$lib/logging/logger'
import { readMonitors } from '$lib/window-positioning'
import { growRectForRail, nearestMonitor, shrinkRectForRail, type Rect } from '$lib/window-positioning-utils'

const log = getAppLogger('askCmdr')

/** Mirrors `tauri.conf.json` `minWidth`; the OS enforces it too, so this is a floor for the math. */
const MIN_WINDOW_WIDTH = 950

/** How the last open grew and slid the window, so the matching close can reverse exactly that. */
let lastGrowth: { grewBy: number; shiftedLeftBy: number } | null = null

/** Read the current (main) window's rect in logical pixels, matching `readMonitors`' units. */
async function readCurrentRect(win: ReturnType<typeof getCurrentWindow>): Promise<Rect> {
  const [pos, size, scale] = await Promise.all([win.outerPosition(), win.outerSize(), win.scaleFactor()])
  return { x: pos.x / scale, y: pos.y / scale, width: size.width / scale, height: size.height / scale }
}

/** True while the window can't be resized to fit the rail (it already fills the screen). */
async function fillsScreen(win: ReturnType<typeof getCurrentWindow>): Promise<boolean> {
  const [fullscreen, maximized] = await Promise.all([win.isFullscreen(), win.isMaximized()])
  return fullscreen || maximized
}

/**
 * Grow the main window to fit a rail of `railWidth` px, keeping the panes their
 * current size. No-op (and forgets any prior growth) when the window already
 * fills the screen — the panes then shrink via flex, which is the intended
 * fallback. Records the applied growth for {@link shrinkMainWindowForRail}.
 */
export async function growMainWindowForRail(railWidth: number): Promise<void> {
  if (getAppMode() === 'e2e') return
  try {
    const win = getCurrentWindow()
    if (await fillsScreen(win)) {
      lastGrowth = null
      return
    }
    const [rect, monitors] = await Promise.all([readCurrentRect(win), readMonitors()])
    const monitor = nearestMonitor(rect, monitors)
    if (!monitor) {
      lastGrowth = null
      return
    }
    const plan = growRectForRail(rect, railWidth, monitor)
    lastGrowth = { grewBy: plan.grewBy, shiftedLeftBy: plan.shiftedLeftBy }
    // Slide left first (fits at the old, smaller width), then widen — so the window
    // never overflows the screen edge mid-move and gets clamped by the OS.
    await win.setPosition(new LogicalPosition(plan.rect.x, plan.rect.y))
    await win.setSize(new LogicalSize(plan.rect.width, plan.rect.height))
  } catch (e) {
    log.warn('growing the window for the Ask Cmdr rail failed: {error}', { error: String(e) })
  }
}

/**
 * Shrink the main window back after the rail closes. Reverses the growth recorded
 * by {@link growMainWindowForRail}; when there's no record (the rail was open at
 * startup, so it was never grown this session), falls back to removing one rail's
 * width so a persisted-open window still shrinks on close.
 */
export async function shrinkMainWindowForRail(railWidth: number): Promise<void> {
  if (getAppMode() === 'e2e') return
  const growth = lastGrowth ?? { grewBy: railWidth, shiftedLeftBy: 0 }
  lastGrowth = null
  if (growth.grewBy <= 0 && growth.shiftedLeftBy === 0) return
  try {
    const win = getCurrentWindow()
    if (await fillsScreen(win)) return
    const [rect, monitors] = await Promise.all([readCurrentRect(win), readMonitors()])
    const monitor = nearestMonitor(rect, monitors)
    if (!monitor) return
    const target = shrinkRectForRail(rect, growth.grewBy, growth.shiftedLeftBy, monitor, MIN_WINDOW_WIDTH)
    // Narrow first (fits at the old x), then slide right into place.
    await win.setSize(new LogicalSize(target.width, target.height))
    await win.setPosition(new LogicalPosition(target.x, target.y))
  } catch (e) {
    log.warn('shrinking the window after the Ask Cmdr rail closed failed: {error}', { error: String(e) })
  }
}
