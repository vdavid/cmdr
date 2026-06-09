export type DragAutoScrollAxis = 'vertical' | 'horizontal'

export interface DragAutoScrollPointer {
  x: number
  y: number
}

export interface DragAutoScrollRect {
  top: number
  right: number
  bottom: number
  left: number
}

export interface DragAutoScrollFrameResult {
  active: boolean
  scrolled: boolean
}

export interface DragAutoScrollStep extends DragAutoScrollFrameResult {
  delta: number
  nextScrollOffset: number
}

export interface DragAutoScrollConfig {
  axis: DragAutoScrollAxis
  pointer: DragAutoScrollPointer
  rect: DragAutoScrollRect
  scrollOffset: number
  maxScrollOffset: number
  elapsedMs: number
  edgeSize?: number
  maxSpeedPxPerSec?: number
}

export const DEFAULT_DRAG_AUTO_SCROLL_EDGE_PX = 56
export const DEFAULT_DRAG_AUTO_SCROLL_MAX_SPEED_PX_PER_SEC = 900

const clamp = (value: number, min: number, max: number): number => Math.max(min, Math.min(max, value))

function resolveEdgeIntensity(coord: number, start: number, end: number, edgeSize: number): number {
  const startDistance = coord - start
  const endDistance = end - coord
  const startIntensity = startDistance >= 0 && startDistance < edgeSize ? -(edgeSize - startDistance) / edgeSize : 0
  const endIntensity = endDistance >= 0 && endDistance < edgeSize ? (edgeSize - endDistance) / edgeSize : 0

  return Math.abs(startIntensity) > Math.abs(endIntensity) ? startIntensity : endIntensity
}

export function computeDragAutoScrollStep(config: DragAutoScrollConfig): DragAutoScrollStep {
  const {
    axis,
    pointer,
    rect,
    scrollOffset,
    maxScrollOffset,
    elapsedMs,
    edgeSize = DEFAULT_DRAG_AUTO_SCROLL_EDGE_PX,
    maxSpeedPxPerSec = DEFAULT_DRAG_AUTO_SCROLL_MAX_SPEED_PX_PER_SEC,
  } = config

  if (maxScrollOffset <= 0 || edgeSize <= 0 || elapsedMs <= 0) {
    return { active: false, scrolled: false, delta: 0, nextScrollOffset: scrollOffset }
  }

  const crossAxisCoord = axis === 'vertical' ? pointer.x : pointer.y
  const crossAxisStart = axis === 'vertical' ? rect.left : rect.top
  const crossAxisEnd = axis === 'vertical' ? rect.right : rect.bottom
  if (crossAxisCoord < crossAxisStart || crossAxisCoord > crossAxisEnd) {
    return { active: false, scrolled: false, delta: 0, nextScrollOffset: scrollOffset }
  }

  const coord = axis === 'vertical' ? pointer.y : pointer.x
  const start = axis === 'vertical' ? rect.top : rect.left
  const end = axis === 'vertical' ? rect.bottom : rect.right
  const intensity = resolveEdgeIntensity(coord, start, end, edgeSize)

  if (intensity === 0) {
    return { active: false, scrolled: false, delta: 0, nextScrollOffset: scrollOffset }
  }

  const direction = Math.sign(intensity)
  const speed = maxSpeedPxPerSec * Math.abs(intensity) * Math.abs(intensity)
  const delta = direction * speed * (elapsedMs / 1000)
  const nextScrollOffset = clamp(scrollOffset + delta, 0, maxScrollOffset)
  const scrolled = nextScrollOffset !== scrollOffset

  return { active: scrolled, scrolled, delta, nextScrollOffset }
}
