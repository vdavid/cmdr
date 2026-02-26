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

function ensureTooltipElement(): HTMLDivElement {
    if (tooltipEl) return tooltipEl

    tooltipEl = document.createElement('div')
    tooltipEl.className = 'cmdr-tooltip'
    tooltipEl.setAttribute('role', 'tooltip')
    tooltipEl.id = 'cmdr-tooltip-' + String(tooltipIdCounter++)

    document.body.appendChild(tooltipEl)
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

function escapeHtml(text: string): string {
    const div = document.createElement('div')
    div.textContent = text
    return div.innerHTML
}

function positionTooltip(triggerEl: HTMLElement): void {
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
}

function startShowTimer(triggerEl: HTMLElement, param: TooltipParam): void {
    cancelTimer()
    showTimer = setTimeout(() => {
        showTimer = null
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

            // If this element's tooltip is showing, hide it
            if (activeElement === node) {
                hideTooltip()
            }
        },
    }
}
