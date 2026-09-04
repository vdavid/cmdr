import type { ActionReturn } from 'svelte/action'

export interface InlineSizeParams {
  /** Runs with the element's content-box inline size in px, at mount and on every resize. */
  onResize: (inlineSizePx: number) => void
}

/**
 * Svelte action reporting an element's content-box inline size. This is the
 * house stand-in for a CSS container query.
 *
 * `@container` and `container-type` need Safari 16, and Cmdr's WebKit floor is
 * Safari 15 (`build.target` in `apps/desktop/vite.config.js`, guarded by the
 * `desktop-vite-build-target` check). Old WebKit drops an `@container` block
 * whole and in silence, so the styling inside it simply never applies and
 * nothing says so. `ResizeObserver` is Safari 13.1+, which is below every floor
 * Cmdr targets, so measuring here and branching in Svelte holds everywhere.
 *
 * It reports `contentRect.width`, the same content box a size query reads, so a
 * threshold ported straight from `@container (max-width: Npx)` keeps its
 * meaning. (`entry.contentBoxSize` says the same thing, but only from Safari
 * 15.4, which is above the floor this action exists for.)
 *
 * Usage:
 *     <div use:useInlineSize={{ onResize: (px) => (width = px) }} class:narrow={width > 0 && width <= 80}>
 *
 * A width of 0 means "not measured", not "narrower than everything": the
 * observer's first callback lands after layout. Callers gate on `> 0` so a
 * narrow-state style can't flash on during the first frame.
 */
export function useInlineSize(node: HTMLElement, params: InlineSizeParams): ActionReturn<InlineSizeParams> {
  let onResize = params.onResize

  const observer = new ResizeObserver((entries) => {
    // Only the last entry matters: the batch is this one element's sizes, oldest
    // first. `contentRect`, not `contentBoxSize`, which is Safari 15.4+ and so
    // above the floor this action exists for.
    if (entries.length === 0) return
    onResize(entries[entries.length - 1].contentRect.width)
  })
  observer.observe(node)

  // Seed from the element itself: the observer's own first callback only lands
  // after the next layout, and one frame of unmeasured styling is visible.
  onResize(contentInlineSize(node))

  return {
    update(next: InlineSizeParams) {
      onResize = next.onResize
      onResize(contentInlineSize(node))
    },
    destroy() {
      observer.disconnect()
    },
  }
}

/** `clientWidth` is the padding box, so the horizontal padding comes back off it. */
function contentInlineSize(node: HTMLElement): number {
  const style = getComputedStyle(node)
  const padding = pixels(style.paddingLeft) + pixels(style.paddingRight)
  return Math.max(0, node.clientWidth - padding)
}

/** A computed length in px. An empty or non-length value counts as 0. */
function pixels(value: string): number {
  const parsed = Number.parseFloat(value)
  return Number.isFinite(parsed) ? parsed : 0
}
