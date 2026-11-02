/**
 * Getting the eye to a place on the Settings page: which DOM element a section path means, where
 * to scroll so it's readable, and the flash that says "here". `DETAILS.md` § "Jumping to a section".
 */

/**
 * The content scroller's top fade reaches this far down once it's scrolled at least this much. A
 * target parked above this line sits partly or fully under the fade, so jumps land just below it.
 */
export const CONTENT_FADE_BAND_PX = 70

const FLASH_CLASS = 'settings-flash'

/** The `data-section-id` of a section path, the ONE definition the page and every jump share. */
export function sectionDomId(path: string[]): string {
  return path
    .join('-')
    .toLowerCase()
    .replaceAll('&', 'and')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
}

/**
 * The rendered sections a path means, in page order: the section itself, or for a parent like
 * `['AI']` (which has no element of its own), its rendered subsections.
 */
export function findSectionElements(container: HTMLElement, path: string[]): HTMLElement[] {
  const id = sectionDomId(path)
  return Array.from(container.querySelectorAll<HTMLElement>('[data-section-id]')).filter((el) => {
    const elementId = el.dataset.sectionId ?? ''
    return elementId === id || elementId.startsWith(`${id}-`)
  })
}

/**
 * The `scrollTop` that lands `target` just below the top fade. Measured from live rects, because
 * `offsetTop` counts from the nearest positioned ancestor, which differs per target.
 */
export function contentScrollTopFor(scroller: HTMLElement, target: HTMLElement): number {
  const distance = target.getBoundingClientRect().top - scroller.getBoundingClientRect().top
  return Math.max(scroller.scrollTop + distance - CONTENT_FADE_BAND_PX, 0)
}

/** A descendant's own animation ending bubbles up here too, so only the element's own counts. */
function endFlash(event: Event): void {
  if (event.target !== event.currentTarget || !(event.target instanceof HTMLElement)) return
  event.target.classList.remove(FLASH_CLASS)
  event.target.removeEventListener('animationend', endFlash)
}

/** Flash each element once, restarting a flash already playing. `app-utilities.css` draws it. */
export function flashElements(elements: HTMLElement[]): void {
  for (const element of elements) {
    element.classList.remove(FLASH_CLASS)
    // Forces a style flush: without it, removing and re-adding in one go doesn't restart the animation.
    element.getBoundingClientRect()
    element.classList.add(FLASH_CLASS)
    // The same function twice registers once, so replays don't pile up listeners.
    element.addEventListener('animationend', endFlash)
  }
}
