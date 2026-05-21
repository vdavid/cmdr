/**
 * Pure geometry helpers for child-window positioning. No Tauri imports here
 * so the file is unit-testable in plain vitest. The Tauri-touching wrappers
 * live in `window-positioning.ts`.
 */

import type { ChildWindowRect } from '$lib/ipc/bindings'

/** Logical-pixel rectangle. Same shape as the Rust-side `ChildWindowRect`. */
export type Rect = ChildWindowRect

/** Logical-pixel monitor bounds. */
export type MonitorRect = { x: number; y: number; width: number; height: number }

/** A child window's intrinsic size; position will be derived. */
export type ChildSize = { width: number; height: number }

/**
 * True if every corner of `rect` lies inside a single monitor. Rects that
 * straddle two monitors return false: the user reads "split-across-displays"
 * as broken even when geometrically valid.
 */
export function isFullyOnScreen(rect: Rect, monitors: readonly MonitorRect[]): boolean {
  return monitors.some((m) => {
    const insideX = rect.x >= m.x && rect.x + rect.width <= m.x + m.width
    const insideY = rect.y >= m.y && rect.y + rect.height <= m.y + m.height
    return insideX && insideY
  })
}

/**
 * Pick the monitor whose center is closest to the rect's center. Used as
 * the clamp target when a saved rect no longer fits on any single display
 * (monitor disconnected, resolution changed, etc).
 */
export function nearestMonitor(rect: Rect, monitors: readonly MonitorRect[]): MonitorRect | null {
  if (monitors.length === 0) return null
  const rectCx = rect.x + rect.width / 2
  const rectCy = rect.y + rect.height / 2
  let best = monitors[0]
  let bestDist = Number.POSITIVE_INFINITY
  for (const m of monitors) {
    const mCx = m.x + m.width / 2
    const mCy = m.y + m.height / 2
    const dx = mCx - rectCx
    const dy = mCy - rectCy
    const dist = dx * dx + dy * dy
    if (dist < bestDist) {
      bestDist = dist
      best = m
    }
  }
  return best
}

/**
 * Shift `rect` (preserving size) so it fits inside `monitor`. If the rect
 * is wider/taller than the monitor, anchors to the monitor's top-left
 * instead of overflowing the other side.
 */
export function clampToMonitor(rect: Rect, monitor: MonitorRect): Rect {
  const width = Math.min(rect.width, monitor.width)
  const height = Math.min(rect.height, monitor.height)
  const maxX = monitor.x + monitor.width - width
  const maxY = monitor.y + monitor.height - height
  const x = Math.max(monitor.x, Math.min(rect.x, maxX))
  const y = Math.max(monitor.y, Math.min(rect.y, maxY))
  return { x, y, width, height }
}

/** Position `size` so its center matches `main`'s center. */
export function centerOnMain(main: Rect, size: ChildSize): Rect {
  return {
    x: main.x + (main.width - size.width) / 2,
    y: main.y + (main.height - size.height) / 2,
    width: size.width,
    height: size.height,
  }
}

/**
 * Cascade offset for the Nth viewer opened in this session, in logical px.
 * Wraps at `wrap` so a long session doesn't march viewers off the screen.
 */
export function cascadeOffset(index: number, step = 24, wrap = 8): number {
  return (index % wrap) * step
}

/** Position a viewer at `main`'s top-left + cascade offset. */
export function cascadeFromMain(main: Rect, size: ChildSize, index: number): Rect {
  const offset = cascadeOffset(index)
  return { x: main.x + offset, y: main.y + offset, width: size.width, height: size.height }
}

/**
 * Decide where a Settings/Debug-style child window should open.
 *
 * - With a saved rect that fits on one monitor: use it as-is.
 * - With a saved rect that's stale (no monitor contains it fully): clamp
 *   to the nearest monitor's bounds, preserving size where possible.
 * - With no saved rect: center on main, then clamp.
 *
 * Returns a fully-clamped rect that's safe to pass straight to Tauri.
 */
export function resolveChildPosition(opts: {
  size: ChildSize
  main: Rect
  monitors: readonly MonitorRect[]
  saved: Rect | null
}): Rect {
  const { size, main, monitors, saved } = opts
  if (saved && isFullyOnScreen(saved, monitors)) {
    return saved
  }
  const candidate = saved ?? centerOnMain(main, size)
  const monitor = nearestMonitor(candidate, monitors)
  if (!monitor) {
    return candidate
  }
  return clampToMonitor(candidate, monitor)
}
