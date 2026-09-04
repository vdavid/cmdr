/**
 * Click counting for the viewer's pointer gestures.
 *
 * The viewer counts consecutive presses itself instead of reading a mouse event's
 * `detail`, so word and line selection depend only on the `pointerdown` stream the drag
 * controller already owns. Pure and time-free: the caller passes each press's position
 * and timestamp, which makes the whole cycle testable without a clock or a layout engine.
 */

/**
 * How long after a press the next one still belongs to the same gesture.
 *
 * macOS keeps the real interval in `NSEvent.doubleClickInterval` (the "Double-Click
 * Speed" slider, 0.5 s by default), but nothing on the Tauri side hands it to the
 * frontend, and the viewer window is a restricted one: reaching it would mean a new IPC
 * command and a capability entry. 500 ms matches the platform default, so only a person
 * who moved that slider notices, and they'd only notice a triple-click that has to be
 * quicker than they set it. If that ever bites, the fix is a command returning
 * `NSEvent.doubleClickInterval`, not a bigger constant.
 */
export const MULTI_CLICK_INTERVAL_MS = 500

/**
 * How far the pointer may drift between two presses and still count as one gesture, in
 * CSS pixels. A fast double-click drags the hand a pixel or two; a press this far away is
 * aimed somewhere else.
 */
export const MULTI_CLICK_SLOP_PX = 4

/** One press: where it landed and when. */
export interface MultiClickPress {
  x: number
  y: number
  /** The event's timestamp, in milliseconds on any monotonic timeline. */
  time: number
}

/** A press plus its place in the click cycle. */
export interface MultiClickState extends MultiClickPress {
  /** 1 = plain click, 2 = word, 3 = line. */
  count: 1 | 2 | 3
}

/**
 * Places `press` in the click cycle, given the press before it.
 *
 * The cycle runs 1 → 2 → 3 and starts over, so a fourth quick press is a plain click
 * again (what editors do) rather than leaving the whole line selected. Any press that
 * comes too late, lands too far away, or arrives before its predecessor starts a fresh
 * cycle. Each press is compared against the one right before it, so a gesture that creeps
 * a couple of pixels per press stays one gesture.
 */
export function advanceMultiClick(prev: MultiClickState | null, press: MultiClickPress): MultiClickState {
  return { count: continuesGesture(prev, press) ? nextCount(prev.count) : 1, ...press }
}

function continuesGesture(prev: MultiClickState | null, press: MultiClickPress): prev is MultiClickState {
  if (prev === null) return false
  const elapsed = press.time - prev.time
  if (elapsed < 0 || elapsed > MULTI_CLICK_INTERVAL_MS) return false
  return Math.abs(press.x - prev.x) <= MULTI_CLICK_SLOP_PX && Math.abs(press.y - prev.y) <= MULTI_CLICK_SLOP_PX
}

function nextCount(count: 1 | 2 | 3): 1 | 2 | 3 {
  if (count === 1) return 2
  if (count === 2) return 3
  return 1
}
