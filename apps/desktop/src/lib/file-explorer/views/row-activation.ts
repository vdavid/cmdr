/**
 * Opening a row with a double press, shared by Brief and Full.
 *
 * A mouse opens on `dblclick`, so the OS's double-click speed and slop decide, as everywhere else on the system.
 * A touch or pen can't lean on `dblclick`: Chrome's DevTools device mode and other touch browsers turn a double
 * tap into two plain clicks and never send one (github.com/Leaflet/Leaflet/issues/6777). So for those the second
 * of two quick clicks on the same row opens it, and a `dblclick` that does follow is dropped, else the row opens
 * twice. The kind of pointer comes from the row's last `pointerdown`: every engine we ship to sends it as a
 * `PointerEvent`, which a `click` isn't on older WebKit.
 */

import { isDoubleClick, type BriefListClick } from './brief-list-utils'

/** Chrome's own double-tap window. */
export const DOUBLE_TAP_MS = 300

export interface RowActivationDeps {
  /** The listing the rows belong to, so taps on a retained row and its replacement never pair up. */
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
  const now = deps.now ?? (() => Date.now())
  // Undefined until a press: a `dblclick` with no pointer seen is a mouse's.
  let lastPointerType: string | undefined
  let lastTap: BriefListClick | undefined

  const fromTouch = () => lastPointerType !== undefined && lastPointerType !== 'mouse'

  return {
    onPointerDown(event) {
      lastPointerType = event.pointerType
    },
    onClick(index) {
      if (!fromTouch()) {
        lastTap = undefined
        return
      }
      const current = { listingId: deps.getListingId(), index, time: now() }
      if (isDoubleClick({ previous: lastTap, current, doubleClickMs: DOUBLE_TAP_MS })) {
        // Forget it, so a third quick tap starts a new pair rather than opening again.
        lastTap = undefined
        deps.activate(index)
        return
      }
      lastTap = current
    },
    onDoubleClick(index) {
      if (fromTouch()) return
      deps.activate(index)
    },
  }
}
