import type { ActionReturn } from 'svelte/action'

type TooltipParam =
  | string
  | { text?: string; html?: string; shortcut?: string; overflowOnly?: boolean; contentEl?: HTMLElement }
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

/**
 * When a `contentEl` param adopts a caller-owned element into the shared tooltip, we record the
 * element and the host it came from so we can put it back. The tooltip element is a single app-wide
 * singleton, so any other trigger showing (or a live `update()`) must return this element first,
 * otherwise the host would keep a detached child and Svelte would update a dead subtree.
 */
let adoptedContentEl: HTMLElement | null = null
let adoptedContentHost: ParentNode | null = null

function ensureTooltipContainer(): HTMLDivElement {
  // Self-heal if the cached container got detached from the document (so the next show actually
  // renders). The container normally lives for the whole app lifetime, but staying defensive keeps a
  // stray `body` mutation from silently breaking every tooltip.
  if (tooltipContainer) {
    if (!tooltipContainer.isConnected) document.body.appendChild(tooltipContainer)
    return tooltipContainer
  }
  tooltipContainer = document.createElement('div')
  tooltipContainer.setAttribute('role', 'region')
  tooltipContainer.setAttribute('aria-label', 'Tooltips')
  tooltipContainer.className = 'sr-only-container'
  document.body.appendChild(tooltipContainer)
  return tooltipContainer
}

function ensureTooltipElement(): HTMLDivElement {
  if (tooltipEl) {
    // Re-attach via the container if a stray `body` mutation detached our subtree, so the next show
    // still renders. `ensureTooltipContainer` re-appends itself; the cached `tooltipEl` rides along.
    if (!tooltipEl.isConnected) ensureTooltipContainer()
    return tooltipEl
  }

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
    return !param.text && !param.html && !param.shortcut && !param.contentEl
  }
  return false
}

/**
 * Return any adopted `contentEl` to its host before the tooltip's content is replaced or the tooltip
 * hides. The host may have unmounted while the tooltip was showing (the owning component was destroyed
 * mid-show), so we only re-append when it's still connected; otherwise we just detach the element so
 * it doesn't dangle inside the shared tooltip.
 */
function returnAdoptedContentEl(): void {
  if (!adoptedContentEl) return

  if (adoptedContentHost && (adoptedContentHost as Node).isConnected) {
    adoptedContentHost.appendChild(adoptedContentEl)
  } else {
    adoptedContentEl.remove()
  }

  adoptedContentEl = null
  adoptedContentHost = null
}

function setTooltipContent(el: HTMLDivElement, param: TooltipParam): void {
  const nextContentEl = typeof param === 'object' && param !== null ? param.contentEl : undefined

  // Runs on every content write (show, the live `update()` path, and any future call site). If the
  // tooltip currently holds an adopted element and we're about to render something else into it (a
  // different `contentEl`, or a plain text/html param), return the old element to its host first.
  // Without this, the `innerHTML`/`textContent`/`appendChild` below would orphan the adopted node:
  // the owner's host would lose its child silently and Svelte would keep updating a detached subtree.
  if (adoptedContentEl && nextContentEl !== adoptedContentEl) {
    returnAdoptedContentEl()
  }

  if (nextContentEl) {
    // Adopt the caller's live element: record its current parent (the hidden host) so we can return it
    // on hide/destroy or when the param swaps, then move it into the tooltip. When the same element is
    // already adopted (the live `update()` path re-rendering unchanged rich content), it's still in the
    // tooltip and its host is already recorded, so there's nothing to do and Svelte keeps updating it.
    if (nextContentEl !== adoptedContentEl) {
      adoptedContentHost = nextContentEl.parentNode
      el.replaceChildren(nextContentEl)
      adoptedContentEl = nextContentEl
    }
    return
  }

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
  // Return any adopted rich content to its host so Svelte keeps updating it in place for the next show.
  returnAdoptedContentEl()
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
