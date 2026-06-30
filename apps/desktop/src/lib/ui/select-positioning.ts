/**
 * Pure geometry for the macOS pop-up-button overlap: where to translate the open menu so the
 * currently-selected row lands on the trigger, clamped to the viewport. Kept DOM-free (plain rects
 * in, a shift out) so `Select.svelte` stays a thin measure-and-apply wrapper and the math is unit
 * tested. See `Select.svelte` and `lib/ui/DETAILS.md` § Select for how it's wired.
 */

/** The subset of `DOMRect` the overlap math reads. */
export interface ShiftRect {
    left: number
    right: number
    top: number
    bottom: number
    height: number
}

export interface OverlapShiftInput {
    /** The trigger's value text (`.select-value`). */
    trigger: ShiftRect
    /** The checked row's label cell (`.select-item-text`). */
    item: ShiftRect
    /** The menu content (`.select-content`). */
    content: ShiftRect
    /** The shift currently applied to the content (rects already reflect it). */
    shiftX: number
    shiftY: number
    viewportWidth: number
    viewportHeight: number
    /** Minimum gap to keep between the content and the viewport edges. */
    pad: number
}

function clamp(n: number, min: number, max: number): number {
    // A content taller / wider than the viewport gives min > max; pin to min (top / left edge).
    if (min > max) return min
    return Math.max(min, Math.min(max, n))
}

/**
 * The shift (in px) to apply to the menu content so the checked row's label aligns horizontally with
 * the trigger value and vertically centers on it, clamped so the content stays within the viewport.
 *
 * Self-correcting: the input rects already include the currently-applied shift, so the residual gap
 * is folded into the current shift. That makes it safe to call repeatedly (open, scroll, resize)
 * without drift.
 */
export function computeOverlapShift(input: OverlapShiftInput): { x: number; y: number } {
    const { trigger, item, content, shiftX, shiftY, viewportWidth, viewportHeight, pad } = input

    let dx = shiftX + (trigger.left - item.left)
    let dy = shiftY + (trigger.top + trigger.height / 2 - (item.top + item.height / 2))

    // base* = the content's position with no shift applied, so the clamp reasons about the final spot.
    const baseLeft = content.left - shiftX
    const baseRight = content.right - shiftX
    const baseTop = content.top - shiftY
    const baseBottom = content.bottom - shiftY
    dx = clamp(dx, pad - baseLeft, viewportWidth - pad - baseRight)
    dy = clamp(dy, pad - baseTop, viewportHeight - pad - baseBottom)

    return { x: Math.round(dx), y: Math.round(dy) }
}
