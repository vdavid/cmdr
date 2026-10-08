/**
 * Drag a tab to reorder it, or to move it to the other pane's tab bar.
 *
 * One controller serves both bars, because each `TabBar` sees only its own pane and a
 * drag has to be read from above them. `DualPaneExplorer` owns it, hands each bar its
 * `forPane()` face, and mounts `TabDragOverlay.svelte`, which draws `view`.
 *
 * Pointer events on `window`, ❌ never HTML5 drag and drop: Tauri's native file-drop
 * handler owns that channel (`../pane/drag-drop-controller.svelte.ts`, which is for FILE
 * drags and stays free of tab logic).
 *
 * The controller decides nothing about what a move means. It turns a pointer into a slot
 * (`tab-drop-slot.ts`) and hands the drop on; the rules live in `moveTab`. It does read
 * the tab lists, but only to draw an honest preview (no line on a slot that changes
 * nothing, a refusal over a bar that would refuse).
 */

import type { TabId, TabState } from './tab-types'
import { dropSlotAt, resolveDrop, slotLineX, type DropResolution, type TabSpan } from './tab-drop-slot'

type PaneId = 'left' | 'right'

/** How far the pointer travels, in px, before a press on a tab is a drag and no longer a click. */
const DRAG_THRESHOLD_PX = 5

/** A tab released on a bar. `toIndex` is the index it holds once moved. */
export interface TabDrop {
  fromPane: PaneId
  tabId: TabId
  toPane: PaneId
  toIndex: number
}

export interface TabDragDeps {
  getTabs: (pane: PaneId) => readonly TabState[]
  maxTabs: number
  /**
   * A release on a bar, unless it would leave the tab where it is. Fires for a drop the
   * bar refuses too (a full pane), so the refusal can be explained.
   */
  onDrop: (drop: TabDrop) => void
}

/** What the overlay draws for the drag in progress. All geometry is in viewport px. */
export interface TabDragView {
  fromPane: PaneId
  tabId: TabId
  label: string
  /** The copy of the tab that follows the pointer: locked to a bar's row while over one. */
  ghost: { left: number; top: number; width: number; height: number }
  /** The bar under the pointer. `null` means a release here cancels. */
  overPane: PaneId | null
  /** Where the tab would land. `null` when nothing would change, or the bar refuses. */
  line: { left: number; top: number; height: number } | null
  /** The bar under the pointer won't take this tab. */
  refused: boolean
}

/** One bar's face of the controller. */
export interface TabBarDrag {
  /** Svelte action for the bar's root element: the drop zone, and where its tabs are measured. */
  attach: (element: HTMLElement) => { destroy: () => void }
  /** A `pointerdown` on one of the bar's tabs. Becomes a drag once the pointer travels. */
  press: (press: { tabId: TabId; label: string; event: PointerEvent }) => void
  /** The tab being dragged, if it's one of this bar's. */
  readonly draggedTabId: TabId | null
  /** A drag is under way, from either bar. */
  readonly isDragging: boolean
}

export interface TabDragController {
  readonly view: TabDragView | null
  forPane: (pane: PaneId) => TabBarDrag
  destroy: () => void
}

/** A pressed tab: not a drag yet, and everything a drag needs to know if it becomes one. */
interface Press {
  pane: PaneId
  tabId: TabId
  label: string
  pointerId: number
  startX: number
  startY: number
  /** Where inside the tab the pointer took hold, so the ghost doesn't jump to its corner. */
  grabX: number
  grabY: number
  width: number
  height: number
}

interface Target {
  pane: PaneId
  slot: number
  resolution: DropResolution
  line: TabDragView['line']
  barTop: number
}

export function createTabDragController(deps: TabDragDeps): TabDragController {
  const bars: Partial<Record<PaneId, HTMLElement>> = {}

  let press: Press | null = null
  let view = $state<TabDragView | null>(null)
  /**
   * A drag was cancelled with the button still down. The release that follows lands on
   * whatever is under the pointer, often the tab itself, and would click it.
   */
  let awaitingRelease = false
  let listening = false
  let removeClickSwallower: (() => void) | null = null

  function targetAt(current: Press, x: number, y: number): Target | null {
    for (const pane of ['left', 'right'] as const) {
      const bar = bars[pane]
      if (!bar) continue
      const rect = bar.getBoundingClientRect()
      if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue

      const spans: TabSpan[] = Array.from(bar.querySelectorAll('[role="tab"]'), (tab) => {
        const tabRect = tab.getBoundingClientRect()
        return { left: tabRect.left, right: tabRect.right }
      })
      const slot = dropSlotAt(x, spans)
      const sourceTabs = deps.getTabs(current.pane)
      const resolution = resolveDrop({
        slot,
        sourceIndex: sourceTabs.findIndex((tab) => tab.id === current.tabId),
        targetCount: deps.getTabs(pane).length,
        samePane: pane === current.pane,
        maxTabs: deps.maxTabs,
      })
      const line =
        resolution.kind === 'move'
          ? { left: slotLineX(slot, spans, rect.left), top: rect.top, height: rect.height }
          : null
      return { pane, slot, resolution, line, barTop: rect.top }
    }
    return null
  }

  function track(current: Press, x: number, y: number): Target | null {
    const target = targetAt(current, x, y)
    view = {
      fromPane: current.pane,
      tabId: current.tabId,
      label: current.label,
      ghost: {
        left: x - current.grabX,
        top: target ? target.barTop : y - current.grabY,
        width: current.width,
        height: current.height,
      },
      overPane: target?.pane ?? null,
      line: target?.line ?? null,
      refused: target?.resolution.kind === 'refused',
    }
    return target
  }

  function handlePointerMove(event: PointerEvent): void {
    if (!press || event.pointerId !== press.pointerId) return
    // No button down means the release happened where the window couldn't see it.
    if (event.buttons === 0) {
      stop()
      return
    }
    if (view === null) {
      const travelled = Math.hypot(event.clientX - press.startX, event.clientY - press.startY)
      if (travelled < DRAG_THRESHOLD_PX) return
    }
    track(press, event.clientX, event.clientY)
  }

  function handlePointerUp(event: PointerEvent): void {
    if (awaitingRelease) {
      swallowNextClick()
      stop()
      return
    }
    if (!press || event.pointerId !== press.pointerId) return
    const current = press
    const wasDragging = view !== null
    // Re-read the target at the release point: a fast drag can end between two moves.
    const target = wasDragging ? track(current, event.clientX, event.clientY) : null
    stop()
    if (!wasDragging) return

    swallowNextClick()
    if (!target || target.resolution.kind === 'unchanged') return
    deps.onDrop({
      fromPane: current.pane,
      tabId: current.tabId,
      toPane: target.pane,
      toIndex: target.resolution.kind === 'move' ? target.resolution.toIndex : target.slot,
    })
  }

  /** Ends the drag with nothing changed. The button is still down, so the release is still owed. */
  function cancelKeepingRelease(): void {
    const wasDragging = view !== null
    press = null
    view = null
    if (wasDragging) awaitingRelease = true
    else stop()
  }

  /**
   * A new press while a release is still owed means that release went elsewhere (the
   * user switched apps mid-drag and let go there). Without this, the stale wait would
   * swallow the next real click.
   */
  function handlePointerDown(): void {
    if (awaitingRelease) stop()
  }

  function handleKeyDown(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || view === null) return
    // The drag owns this Escape: nothing else in the app should also act on it.
    event.preventDefault()
    event.stopImmediatePropagation()
    cancelKeepingRelease()
  }

  /**
   * The `click` a browser fires after the release would switch to the dragged tab and
   * focus its pane. One capture-phase listener eats it; the timer takes the listener
   * back down when no click comes (a release outside the bars produces none).
   */
  function swallowNextClick(): void {
    removeClickSwallower?.()
    const swallow = (event: MouseEvent): void => {
      event.stopImmediatePropagation()
      event.preventDefault()
      remove()
    }
    const timer = setTimeout(() => {
      remove()
    }, 0)
    const remove = (): void => {
      clearTimeout(timer)
      window.removeEventListener('click', swallow, true)
      removeClickSwallower = null
    }
    window.addEventListener('click', swallow, true)
    removeClickSwallower = remove
  }

  function listen(): void {
    if (listening) return
    listening = true
    window.addEventListener('pointerdown', handlePointerDown, true)
    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', handlePointerUp)
    window.addEventListener('pointercancel', stop)
    window.addEventListener('blur', cancelKeepingRelease)
    // Capture, so the drag hears Escape before any handler that would stop it on the way down.
    window.addEventListener('keydown', handleKeyDown, true)
  }

  /** Forgets the press and the drag, and stops listening. */
  function stop(): void {
    press = null
    view = null
    awaitingRelease = false
    if (!listening) return
    listening = false
    window.removeEventListener('pointerdown', handlePointerDown, true)
    window.removeEventListener('pointermove', handlePointerMove)
    window.removeEventListener('pointerup', handlePointerUp)
    window.removeEventListener('pointercancel', stop)
    window.removeEventListener('blur', cancelKeepingRelease)
    window.removeEventListener('keydown', handleKeyDown, true)
  }

  function startPress(pane: PaneId, tabId: TabId, label: string, event: PointerEvent): void {
    if (event.button !== 0 || !event.isPrimary) return
    // The close button is its own click target: a press there never drags the tab.
    if (event.target instanceof Element && event.target.closest('.close-btn')) return
    const tabs = deps.getTabs(pane)
    const tab = tabs.find((candidate) => candidate.id === tabId)
    // A pinned tab stays put, and a pane's only tab has nowhere to go: it can't leave its
    // pane and has nothing to reorder against. Neither press ever becomes a drag.
    if (!tab || tab.pinned || tabs.length <= 1) return
    if (!(event.currentTarget instanceof HTMLElement)) return

    stop()
    const rect = event.currentTarget.getBoundingClientRect()
    press = {
      pane,
      tabId,
      label,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      grabX: event.clientX - rect.left,
      grabY: event.clientY - rect.top,
      width: rect.width,
      height: rect.height,
    }
    listen()
  }

  function faceFor(pane: PaneId): TabBarDrag {
    return {
      attach: (element) => {
        bars[pane] = element
        return {
          destroy: () => {
            if (bars[pane] === element) bars[pane] = undefined
          },
        }
      },
      press: ({ tabId, label, event }) => {
        startPress(pane, tabId, label, event)
      },
      get draggedTabId() {
        return view?.fromPane === pane ? view.tabId : null
      },
      get isDragging() {
        return view !== null
      },
    }
  }

  // One face per pane for the controller's whole life, so a bar's prop never changes identity.
  const faces: Record<PaneId, TabBarDrag> = { left: faceFor('left'), right: faceFor('right') }

  return {
    get view() {
      return view
    },
    forPane: (pane) => faces[pane],
    destroy: () => {
      stop()
      removeClickSwallower?.()
    },
  }
}
