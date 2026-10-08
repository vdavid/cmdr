/**
 * Where a dragged tab lands, as pure arithmetic over measured tab edges.
 *
 * A bar with `n` tabs has `n + 1` slots: slot `k` is the gap before tab `k`, and slot `n`
 * is the end. The pointer picks a slot (`dropSlotAt`), the slot gets a line drawn on it
 * (`slotLineX`), and the slot becomes an index the state manager's `moveTab` understands
 * (`resolveDrop`). `tab-drag-controller.svelte.ts` feeds these from the DOM.
 */

/** One tab's horizontal extent, in viewport px. */
export interface TabSpan {
  left: number
  right: number
}

/**
 * The slot under `pointerX`: a tab's left half lands before it, its right half after it.
 * Everything past the last tab's middle (the empty strip, the "+" button) is the append slot.
 */
export function dropSlotAt(pointerX: number, tabs: readonly TabSpan[]): number {
  let slot = 0
  for (const tab of tabs) {
    if (pointerX <= (tab.left + tab.right) / 2) break
    slot++
  }
  return slot
}

/** The x of the line marking `slot`: a tab edge at either end, the middle of the gap otherwise. */
export function slotLineX(slot: number, tabs: readonly TabSpan[], barLeft: number): number {
  if (tabs.length === 0) return barLeft
  if (slot <= 0) return tabs[0].left
  if (slot >= tabs.length) return tabs[tabs.length - 1].right
  return (tabs[slot - 1].right + tabs[slot].left) / 2
}

export interface DropQuery {
  /** Slot in the TARGET bar, counted with the dragged tab still in place for a same-pane drag. */
  slot: number
  /** The dragged tab's index in its own pane. */
  sourceIndex: number
  targetCount: number
  samePane: boolean
  maxTabs: number
}

export type DropResolution = { kind: 'move'; toIndex: number } | { kind: 'unchanged' } | { kind: 'refused' }

/**
 * What releasing on `slot` does. `toIndex` is the index the tab holds AFTER the move,
 * which is what `moveTab` takes. A full pane is the only refusal: a pane's only tab never
 * gets as far as a drag.
 */
export function resolveDrop(query: DropQuery): DropResolution {
  const { slot, sourceIndex, targetCount, samePane, maxTabs } = query
  if (samePane) {
    // Taking the tab out shifts every later slot down by one, so the two slots that
    // touch it (its own and the one right after) both mean "stay".
    const toIndex = slot > sourceIndex ? slot - 1 : slot
    return toIndex === sourceIndex ? { kind: 'unchanged' } : { kind: 'move', toIndex }
  }
  if (targetCount >= maxTabs) return { kind: 'refused' }
  return { kind: 'move', toIndex: slot }
}
