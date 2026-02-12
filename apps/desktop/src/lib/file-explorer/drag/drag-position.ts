/**
 * Corrects Tauri DragDropEvent coordinates when the Web Inspector is docked.
 *
 * Dev-only: in production, DevTools is never open, so toViewportPosition()
 * is a no-op passthrough with zero overhead.
 *
 * When the Web Inspector is docked, the viewport shrinks but Tauri reports
 * positions relative to the full window frame. We detect the offset via
 * getCurrentWindow().outerSize() (full window, stable) vs window.innerHeight
 * (viewport, shrinks with DevTools).
 */

import { getCurrentWindow } from '@tauri-apps/api/window'

let offsetX = 0
let offsetY = 0

/**
 * Recomputes the offset between the window frame and the visible viewport.
 * Only runs in dev mode. Call on mount and on window resize.
 */
export async function recalculateWebviewOffset(): Promise<void> {
    if (!import.meta.env.DEV) return
    try {
        const dpr = window.devicePixelRatio || 1
        const outerSize = await getCurrentWindow().outerSize()
        offsetX = outerSize.width / dpr - window.innerWidth
        offsetY = outerSize.height / dpr - window.innerHeight
    } catch {
        // Tauri API unavailable (tests, SSR). Offsets stay at zero.
    }
}

/**
 * Adjusts Tauri DragDropEvent coordinates to viewport coordinates.
 * In production, returns the position unchanged (offset is always zero).
 */
export function toViewportPosition(position: { x: number; y: number }): { x: number; y: number } {
    if (offsetX === 0 && offsetY === 0) return position
    return {
        x: position.x + offsetX,
        y: position.y + offsetY,
    }
}
