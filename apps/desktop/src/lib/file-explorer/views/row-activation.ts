/**
 * Double presses in a file list: opening a row (Brief and Full) and going up from the empty background
 * (`pane/pane-pointer.ts`) share one core, `createDoublePress`.
 *
 * A mouse acts on `dblclick`, so the OS's double-click speed and slop decide, as everywhere else on the system.
 * A touch or pen can't lean on `dblclick`: Chrome's DevTools device mode and other touch browsers turn a double
 * tap into two plain clicks and never send one (github.com/Leaflet/Leaflet/issues/6777). So for those the second
 * of two quick clicks on the same target acts, and a `dblclick` that does follow is dropped, else it acts twice.
 * The kind of pointer comes from the last `pointerdown`: every engine we ship to sends it as a `PointerEvent`,
 * which a `click` isn't on older WebKit.
 */

/** Chrome's own double-tap window. */
export const DOUBLE_TAP_MS = 300

export interface DoublePress {
  onPointerDown: (event: PointerEvent) => void
  /**
   * Feed every click on a target; true on the second quick touch or pen click on the same `target`.
   * The key carries the target's whole identity (for a row, the listing too: a retained row and its
   * replacement never pair up).
   */
  isDoubleTap: (target: string) => boolean
  /** True when a `dblclick` should act: false when a touch or pen produced it (the taps already did). */
  isMouseDoubleClick: () => boolean
}

export function createDoublePress(now: () => number = () => Date.now()): DoublePress {
  // Undefined until a press: a `dblclick` with no pointer seen is a mouse's.
  let lastPointerType: string | undefined
  let lastTap: { target: string; time: number } | undefined

  const fromTouch = () => lastPointerType !== undefined && lastPointerType !== 'mouse'

  return {
    onPointerDown(event) {
      lastPointerType = event.pointerType
    },
    isDoubleTap(target) {
      if (!fromTouch()) {
        lastTap = undefined
        return false
      }
      const time = now()
      if (lastTap?.target === target && time - lastTap.time < DOUBLE_TAP_MS) {
        // Forget it, so a third quick tap starts a new pair rather than acting again.
        lastTap = undefined
        return true
      }
      lastTap = { target, time }
      return false
    },
    isMouseDoubleClick: () => !fromTouch(),
  }
}

export interface RowActivationDeps {
  /** The listing the rows belong to, part of a row's identity. */
  getListingId: () => string
  /** Opens the row at `index`. */
  activate: (index: number) => void
  now?: () => number
}

export interface RowActivation {
  onPointerDown: (event: PointerEvent) => void
  onClick: (index: number) => void
  onDoubleClick: (index: number) => void
}

export function createRowActivation(deps: RowActivationDeps): RowActivation {
  const press = createDoublePress(deps.now)
  return {
    onPointerDown: press.onPointerDown,
    onClick(index) {
      if (press.isDoubleTap(JSON.stringify([deps.getListingId(), index]))) deps.activate(index)
    },
    onDoubleClick(index) {
      if (press.isMouseDoubleClick()) deps.activate(index)
    },
  }
}
