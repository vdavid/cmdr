/**
 * Focus a viewer mode takes for itself, without the keyboard-focus ring.
 *
 * Each mode focuses its scroll region once it shows, so the arrows and Page keys scroll it.
 * WebKit matches `:focus-visible` for a programmatic `focus()` that follows a key press (the
 * `0`–`3` that switched modes), which painted a window-sized accent ring (#761). A view's ring
 * rule reads `:focus-visible:not([data-quiet-focus])`; the mark lives until the next blur, so a
 * later Tab focus rings as usual. `focusVisible: false` alone isn't enough: older macOS WebKits
 * (the minimum is 10.15) ignore it.
 */

export const QUIET_FOCUS_ATTRIBUTE = 'data-quiet-focus'

function clearQuietFocus(this: HTMLElement): void {
  this.removeAttribute(QUIET_FOCUS_ATTRIBUTE)
}

export function focusQuietly(el: HTMLElement, options: FocusOptions = {}): void {
  el.setAttribute(QUIET_FOCUS_ATTRIBUTE, '')
  el.addEventListener('blur', clearQuietFocus, { once: true })
  el.focus({ ...options, focusVisible: false })
  // No focus means no blur to clear the mark, and a stale one would hide a later Tab ring.
  if (document.activeElement !== el) {
    el.removeEventListener('blur', clearQuietFocus)
    clearQuietFocus.call(el)
  }
}
