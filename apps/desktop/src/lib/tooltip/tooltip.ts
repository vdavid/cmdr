import type { ActionReturn } from 'svelte/action'

type TooltipParam =
  | string
  | { text?: string; html?: string; shortcut?: string; overflowOnly?: boolean }
  | null
  | undefined

const SHOW_DELAY_MS = 400
const OFFSET_BELOW = 6
const VIEWPORT_MARGIN = 8

let tooltipEl: HTMLDivElement | null = null
let tooltipIdCounter = 0
let activeElement: HTMLElement | null = null
let showTimer: ReturnType<typeof setTimeout> | null = null
/** The element a pending show-timer belongs to, so we can cancel it if that element is torn down. */
let timerNode: HTMLElement | null = null

/** Shared container for tooltips (keeps them inside a landmark to satisfy axe's `region` rule). */
let tooltipContainer: HTMLDivElement | null = null

function ensureTooltipContainer(): HTMLDivElement {
  if (tooltipContainer) return tooltipContainer
  tooltipContainer = document.createElement('div')
  tooltipContainer.setAttribute('role', 'region')
  tooltipContainer.setAttribute('aria-label', 'Tooltips')
  tooltipContainer.className = 'sr-only-container'
  document.body.appendChild(tooltipContainer)
  return tooltipContainer
}

function ensureTooltipElement(): HTMLDivElement {
  if (tooltipEl) return tooltipEl

  tooltipEl = document.createElement('div')
  tooltipEl.className = 'cmdr-tooltip'
  tooltipEl.setAttribute('role', 'tooltip')
  tooltipEl.id = 'cmdr-tooltip-' + String(tooltipIdCounter++)

  ensureTooltipContainer().appendChild(tooltipEl)
  return tooltipEl
}

function isEmptyParam(param: TooltipParam): boolean {
  if (param === null || param === undefined || param === '') return true
  if (typeof param === 'object') {
    return !param.text && !param.html && !param.shortcut
  }
  return false
}

function setTooltipContent(el: HTMLDivElement, param: TooltipParam): void {
  if (typeof param === 'string') {
    el.textContent = param
    return
  }

  if (param && param.html) {
    el.innerHTML = param.html
    return
  }

  if (param && (param.text || param.shortcut)) {
    const parts: string[] = []
    if (param.text) {
      const escaped = escapeHtml(param.text)
      parts.push(`<span class="cmdr-tooltip-text">${escaped}</span>`)
    }
    if (param.shortcut) {
      const escaped = escapeHtml(param.shortcut)
      parts.push(`<kbd class="cmdr-tooltip-kbd">${escaped}</kbd>`)
    }
    el.innerHTML = parts.join('')
    return
  }

  el.textContent = ''
}

export function escapeHtml(text: string): string {
  const div = document.createElement('div')
  div.textContent = text
  return div.innerHTML
}

/**
 * A trigger removed from the DOM reports an all-zero `getBoundingClientRect()`, so positioning against
 * it would dump the tooltip in the top-left corner. `isConnected` is the precise signal for that.
 */
function isTriggerDetached(el: HTMLElement): boolean {
  return !el.isConnected
}

function positionTooltip(triggerEl: HTMLElement): void {
  // Guards the live-update path (content changes while shown) against a trigger that vanished meanwhile.
  if (isTriggerDetached(triggerEl)) {
    hideTooltip()
    return
  }

  const tip = ensureTooltipElement()
  const triggerRect = triggerEl.getBoundingClientRect()
  const tipRect = tip.getBoundingClientRect()

  let left = triggerRect.left + (triggerRect.width - tipRect.width) / 2
  let top = triggerRect.bottom + OFFSET_BELOW

  // Flip above if tooltip would overflow viewport bottom
  if (top + tipRect.height > window.innerHeight - VIEWPORT_MARGIN) {
    top = triggerRect.top - tipRect.height - OFFSET_BELOW
  }

  // Clamp horizontal to viewport edges
  left = Math.max(VIEWPORT_MARGIN, Math.min(left, window.innerWidth - tipRect.width - VIEWPORT_MARGIN))

  tip.style.left = String(left) + 'px'
  tip.style.top = String(top) + 'px'
}

function showTooltip(triggerEl: HTMLElement, param: TooltipParam): void {
  // The trigger may have been removed from the DOM during the show delay (e.g. a virtual-scroll row
  // recycled while hovered). Never show against a detached element: its rect is all-zero, which would
  // place the tooltip in the top-left corner.
  if (isTriggerDetached(triggerEl)) return

  const tip = ensureTooltipElement()
  setTooltipContent(tip, param)
  triggerEl.setAttribute('aria-describedby', tip.id)

  // Position offscreen first to measure, then reposition
  tip.style.left = '0px'
  tip.style.top = '0px'
  tip.classList.add('visible')
  positionTooltip(triggerEl)

  activeElement = triggerEl
}

function hideTooltip(): void {
  cancelTimer()
  if (tooltipEl) {
    tooltipEl.classList.remove('visible')
  }
  if (activeElement) {
    activeElement.removeAttribute('aria-describedby')
    activeElement = null
  }
}

function cancelTimer(): void {
  if (showTimer !== null) {
    clearTimeout(showTimer)
    showTimer = null
  }
  timerNode = null
}

function startShowTimer(triggerEl: HTMLElement, param: TooltipParam): void {
  cancelTimer()
  timerNode = triggerEl
  showTimer = setTimeout(() => {
    showTimer = null
    timerNode = null
    showTooltip(triggerEl, param)
  }, SHOW_DELAY_MS)
}

function isOverflowing(el: HTMLElement): boolean {
  return el.scrollWidth > el.clientWidth
}

function shouldShow(el: HTMLElement, param: TooltipParam): boolean {
  if (typeof param === 'object' && param !== null && param.overflowOnly) {
    return isOverflowing(el)
  }
  return true
}

export function tooltip(node: HTMLElement, param: TooltipParam): ActionReturn<TooltipParam> {
  let currentParam = param

  // Remove native title to prevent double-tooltip
  node.removeAttribute('title')

  const handleMouseEnter = (): void => {
    if (!isEmptyParam(currentParam) && shouldShow(node, currentParam)) {
      startShowTimer(node, currentParam)
    }
  }

  const handleMouseLeave = (): void => {
    hideTooltip()
  }

  const handleFocus = (): void => {
    if (!isEmptyParam(currentParam) && shouldShow(node, currentParam)) {
      startShowTimer(node, currentParam)
    }
  }

  const handleBlur = (): void => {
    hideTooltip()
  }

  const handleKeyDown = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      hideTooltip()
    }
  }

  node.addEventListener('mouseenter', handleMouseEnter)
  node.addEventListener('mouseleave', handleMouseLeave)
  node.addEventListener('focus', handleFocus)
  node.addEventListener('blur', handleBlur)
  node.addEventListener('keydown', handleKeyDown)

  return {
    update(newParam: TooltipParam) {
      currentParam = newParam
      node.removeAttribute('title')

      // If tooltip is currently visible for this element, update content live
      if (activeElement === node && tooltipEl) {
        if (isEmptyParam(currentParam)) {
          hideTooltip()
        } else {
          setTooltipContent(tooltipEl, currentParam)
          positionTooltip(node)
        }
      }
    },
    destroy() {
      node.removeEventListener('mouseenter', handleMouseEnter)
      node.removeEventListener('mouseleave', handleMouseLeave)
      node.removeEventListener('focus', handleFocus)
      node.removeEventListener('blur', handleBlur)
      node.removeEventListener('keydown', handleKeyDown)

      // Cancel a pending show-timer owned by this node. Svelte removes a virtual-scroll row's DOM node
      // without firing `mouseleave`, so without this the timer would fire later against a detached node
      // and the tooltip would land in the top-left corner. `activeElement` is still null during the
      // delay window, so the `activeElement === node` branch below wouldn't catch it.
      if (timerNode === node) {
        cancelTimer()
      }

      // If this element's tooltip is showing, hide it
      if (activeElement === node) {
        hideTooltip()
      }
    },
  }
}
