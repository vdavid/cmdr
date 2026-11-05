/**
 * Keeps the top toast stack clear of the status corner. Both sit in the window's top-right corner, and the corner's
 * box grows with its members, the app zoom, and the macOS text size, so no fixed offset clears it: at 125% the
 * "Still running in the background" toast covered the copy's progress chip and ate its click. The corner publishes
 * its real bottom edge (viewport px) as `--titlebar-status-corner-bottom` on `:root`, and `ToastContainer.svelte` starts the
 * stack below it.
 */

export const STATUS_CORNER_BOTTOM_VAR = '--titlebar-status-corner-bottom'

/** Publishes `corner`'s bottom edge while it lives, following its size. Returns the stop, which clears the variable. */
export function publishCornerBottom(corner: HTMLElement): () => void {
  const root = corner.ownerDocument.documentElement
  const publish = () => {
    root.style.setProperty(STATUS_CORNER_BOTTOM_VAR, `${String(corner.getBoundingClientRect().bottom)}px`)
  }
  publish()
  // A member appearing, a zoom, or a text-size change resizes the corner; that's every way its bottom moves.
  const observer = typeof ResizeObserver === 'function' ? new ResizeObserver(publish) : undefined
  observer?.observe(corner)
  return () => {
    observer?.disconnect()
    root.style.removeProperty(STATUS_CORNER_BOTTOM_VAR)
  }
}
