/**
 * A touch or pen press held still on a row starts what a mouse press starts: the drag tracking in
 * `drag-drop.ts`, then the drag once the finger moves past the threshold.
 *
 * A mouse arms a drag on the press, but a finger can't: a press and a swipe start the same way, and
 * a swipe scrolls the list. So a touch arms only after it's held still for `TOUCH_HOLD_MS`; moving
 * sooner is a scroll and cancels it. Once held, the touch never scrolls (the page would pan under
 * the drag), until that finger lifts. Holding still past the OS's own long press brings the context
 * menu, which ends the drag tracking (`drag-drop.ts`): hold and move drags, hold still opens the
 * menu, as iPadOS's Files does.
 *
 * Why one non-passive `touchmove` listener on the window, from the first view on: a browser decides
 * whether a touch may scroll before a held press exists, and a `preventDefault` from a listener
 * added mid-gesture is ignored (iOS Safari; dnd-kit's `TouchSensor` registers the same no-op
 * listener for it). It blocks nothing until a hold is active.
 */

/** How long a touch stays still before it arms a drag; shorter than every OS's long press. */
export const TOUCH_HOLD_MS = 300

/** How far a touch may wander and still count as held (CSS px), as a tap's slop. */
export const TOUCH_HOLD_SLOP_PX = 10

/** True for a press by a finger or a pen, which a `pointerdown` names and a `mousedown` doesn't. */
export function isTouchPress(event: MouseEvent): event is PointerEvent {
  return 'pointerType' in event && typeof event.pointerType === 'string' && event.pointerType !== 'mouse'
}

/** The pointers holding a drag right now; the window's `touchmove` blocker reads it. */
const heldPointers = new Set<number>()
let scrollBlockerInstalled = false

function installScrollBlocker(): void {
  if (scrollBlockerInstalled || typeof window === 'undefined') return
  scrollBlockerInstalled = true
  window.addEventListener(
    'touchmove',
    (event) => {
      if (heldPointers.size > 0 && event.cancelable) event.preventDefault()
    },
    { passive: false },
  )
}

export interface TouchHold {
  /** Feed a row's `pointerdown`; `onHold` runs once the touch has stayed still long enough. */
  press: (event: PointerEvent, onHold: () => void) => void
}

export interface TouchHoldTimers {
  setTimeout: (run: () => void, ms: number) => unknown
  clearTimeout: (handle: unknown) => void
}

const realTimers: TouchHoldTimers = {
  setTimeout: (run, ms) => setTimeout(run, ms),
  clearTimeout: (handle) => {
    clearTimeout(handle as ReturnType<typeof setTimeout>)
  },
}

export function createTouchHold(timers: TouchHoldTimers = realTimers): TouchHold {
  installScrollBlocker()

  return {
    press(event, onHold) {
      if (!isTouchPress(event) || !event.isPrimary || event.button !== 0) return
      const { pointerId, clientX: startX, clientY: startY } = event
      let held = false

      const end = () => {
        timers.clearTimeout(timer)
        heldPointers.delete(pointerId)
        document.removeEventListener('pointermove', onMove)
        document.removeEventListener('pointerup', onEnd)
        document.removeEventListener('pointercancel', onEnd)
        document.removeEventListener('contextmenu', onMenu)
      }
      const onMove = (move: PointerEvent) => {
        if (held || move.pointerId !== pointerId) return
        // Moved before the hold: a scroll, which the browser is already running.
        if (Math.hypot(move.clientX - startX, move.clientY - startY) > TOUCH_HOLD_SLOP_PX) end()
      }
      const onEnd = (up: PointerEvent) => {
        if (up.pointerId === pointerId) end()
      }
      // The OS's long press came first: the menu takes the press.
      const onMenu = () => {
        if (!held) end()
      }
      const timer = timers.setTimeout(() => {
        held = true
        heldPointers.add(pointerId)
        onHold()
      }, TOUCH_HOLD_MS)

      document.addEventListener('pointermove', onMove)
      document.addEventListener('pointerup', onEnd)
      document.addEventListener('pointercancel', onEnd)
      document.addEventListener('contextmenu', onMenu)
    },
  }
}
