/**
 * Pure geometry for a fixed-position surface hung off an anchor: below it when it fits, above
 * when only that fits, else the roomier side with a `maxHeight` so it scrolls inside itself;
 * always clamped inside the viewport. DOM-free (plain rects in, a placement out) so `Menu.svelte`
 * and `Popover.svelte` stay measure-and-apply wrappers and share one rule. See `lib/ui/DETAILS.md`
 * § Menu.
 */

/** The subset of `DOMRect` the placement reads. */
export interface PlacementRect {
  left: number
  top: number
  right: number
  bottom: number
}

interface Size {
  width: number
  height: number
}

export interface AnchorPlacementInput {
  /** The anchor's viewport rect; a zero-size rect for a point anchor. */
  anchor: PlacementRect
  /** The surface's natural size, before any height cap (`scrollHeight`, not `offsetHeight`). */
  size: Size
  viewport: Size
  /** Space between the anchor and the surface, on whichever side it opens. */
  gap: number
  /** Space the surface keeps off every viewport edge. */
  margin: number
  /** A floor above the viewport's own bottom edge (the volume switcher stops above its pane's footer). */
  bottomLimit?: number
}

export interface AnchorPlacement {
  left: number
  top: number
  /** The room on the chosen side; the surface scrolls past it. */
  maxHeight: number
  side: 'below' | 'above'
}

function clamp(n: number, min: number, max: number): number {
  // A surface wider / taller than the viewport gives min > max; pin to min (left / top edge).
  if (min > max) return min
  return Math.max(min, Math.min(max, n))
}

export function placeOffAnchor(input: AnchorPlacementInput): AnchorPlacement {
  const { anchor, size, viewport, gap, margin, bottomLimit } = input
  const left = clamp(anchor.left, margin, viewport.width - size.width - margin)

  const floor =
    bottomLimit === undefined ? viewport.height - margin : Math.min(viewport.height - margin, bottomLimit - gap)
  const belowTop = Math.max(margin, anchor.bottom + gap)
  const roomBelow = floor - belowTop
  const aboveBottom = anchor.top - gap
  const roomAbove = aboveBottom - margin

  const below = { left, top: belowTop, maxHeight: roomBelow, side: 'below' } as const
  if (size.height <= roomBelow) return below
  if (size.height <= roomAbove) {
    return { left, top: aboveBottom - size.height, maxHeight: roomAbove, side: 'above' }
  }
  // Fits neither: the roomier side, capped. Ties go below, the direction a menu reads.
  if (roomBelow >= roomAbove) return below
  return { left, top: margin, maxHeight: roomAbove, side: 'above' }
}

export interface RowPlacementInput {
  /** The parent row's viewport rect. */
  row: PlacementRect
  /** The submenu's natural size, before any height cap. */
  size: Size
  viewport: Size
  /** How far above the row's top the submenu starts, so its first row lines up with the parent row. */
  gap: number
  /** How far it overlaps the parent row horizontally, the way macOS hands a submenu off. */
  overlap: number
  margin: number
}

/**
 * A submenu beside its parent row: to the right when it fits, flipped to the left when only that
 * fits, else clamped on the roomier side. Vertically it slides up to keep its bottom on screen.
 */
export function placeBesideRow(input: RowPlacementInput): { left: number; top: number; maxHeight: number } {
  const { row, size, viewport, gap, overlap, margin } = input
  const rightLeft = row.right - overlap
  const leftLeft = row.left + overlap - size.width
  const maxLeft = viewport.width - size.width - margin

  let left: number
  if (rightLeft <= maxLeft) left = rightLeft
  else if (leftLeft >= margin) left = leftLeft
  else left = viewport.width - rightLeft >= row.left + overlap ? maxLeft : margin
  left = clamp(left, margin, maxLeft)

  const top = clamp(row.top - gap, margin, viewport.height - size.height - margin)
  return { left, top, maxHeight: viewport.height - 2 * margin }
}
