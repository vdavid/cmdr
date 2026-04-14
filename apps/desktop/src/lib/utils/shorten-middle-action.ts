import type { ActionReturn } from 'svelte/action'
import { shortenMiddle, createPretextMeasure } from './shorten-middle'

export interface ShortenMiddleParams {
  text: string
  preferBreakAt?: string
  startRatio?: number
}

// Shared across all action instances so the dynamic import runs at most once.
let pretextPromise: Promise<typeof import('@chenglou/pretext')> | null = null

function loadPretext(): Promise<typeof import('@chenglou/pretext')> {
  if (!pretextPromise) {
    pretextPromise = import('@chenglou/pretext')
  }
  return pretextPromise
}

/**
 * Reads the CSS font string from a DOM element. Falls back to constructing
 * it from fontSize + fontFamily when `getComputedStyle().font` is empty.
 */
function readFont(node: HTMLElement): string {
  const style = getComputedStyle(node)
  if (style.font) return style.font
  return `${style.fontSize} ${style.fontFamily}`
}

/**
 * Svelte action that truncates text in the middle to fit its container width.
 * Uses `@chenglou/pretext` for pixel-accurate measurement (loaded async).
 * Before pretext loads, the full text is shown with CSS `text-overflow: ellipsis`.
 */
export function useShortenMiddle(node: HTMLElement, params: ShortenMiddleParams): ActionReturn<ShortenMiddleParams> {
  let measureWidth: ((text: string) => number) | null = null
  let currentText = params.text

  // CSS fallback while pretext is loading
  node.style.overflow = 'hidden'
  node.style.textOverflow = 'ellipsis'
  node.style.whiteSpace = 'nowrap'
  node.textContent = params.text
  node.title = params.text

  function truncate() {
    if (!measureWidth) return
    const width = node.clientWidth
    if (width <= 0) return
    const result = shortenMiddle(currentText, width, measureWidth, {
      preferBreakAt: params.preferBreakAt,
      startRatio: params.startRatio,
    })
    node.textContent = result
  }

  // Load pretext, then switch from CSS fallback to pixel-accurate truncation
  loadPretext()
    .then((pretext) => {
      const font = readFont(node)
      measureWidth = createPretextMeasure(font, pretext)
      truncate()
    })
    .catch(() => {
      // Pretext unavailable — CSS text-overflow: ellipsis remains as fallback
    })

  const observer = new ResizeObserver(() => {
    truncate()
  })
  observer.observe(node)

  return {
    update(newParams: ShortenMiddleParams) {
      const textChanged = newParams.text !== currentText
      params = newParams
      if (!textChanged) return
      currentText = newParams.text
      node.title = currentText
      if (measureWidth) {
        truncate()
      } else {
        node.textContent = currentText
      }
    },
    destroy() {
      observer.disconnect()
    },
  }
}
