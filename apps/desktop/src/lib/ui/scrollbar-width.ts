/**
 * Keeps `--spacing-scrollbar-width` on `:root` at the width an unstyled vertical
 * scrollbar takes from its scroller: 0 under macOS overlay scrollbars, 17 px under
 * classic ones (macOS 27 WKWebView). A scroller that reserves its gutter
 * (`scrollbar-gutter: stable`) spends it to keep its content's right edge where it
 * sits with overlay scrollbars (`DETAILS.md` § "Scrollbar gutters").
 *
 * Live: macOS flips the scrollbar style at runtime (a mouse connected or removed, or
 * "Show scroll bars" changed), WebKit re-lays-out every scroller when it does, and
 * the probe's content box changes with it, so a `ResizeObserver` on that box is the
 * whole subscription. No IPC and no polling.
 *
 * Measures a probe with NO `::-webkit-scrollbar` styling: styling any part makes a
 * classic bar of its own width, so a scroller that styles its bar (the file lists)
 * measures and overrides the token on itself instead.
 *
 * @returns teardown that removes the probe and stops observing
 */
export function trackScrollbarWidth(): () => void {
  if (typeof ResizeObserver === 'undefined') return () => {}

  const probe = document.createElement('div')
  probe.setAttribute('aria-hidden', 'true')
  probe.dataset.scrollbarProbe = ''
  // Off-screen and invisible, but laid out: `visibility: hidden` keeps the box (and its
  // scrollbars) where `display: none` would drop both. `contain: strict` keeps its
  // layout from touching anything else.
  probe.style.cssText =
    'position:fixed;top:-200px;left:-200px;width:100px;height:100px;overflow:scroll;visibility:hidden;pointer-events:none;contain:strict'
  // The probe has to really overflow. WebKit re-styles only the scrollers that can
  // scroll when the style flips: an empty `overflow: scroll` box followed a switch to
  // overlay but not the one back to classic, reading 0 while a fresh scroller read 17
  // (verified on macOS 27 WKWebView, `AppleShowScrollBars` flipped live via its
  // distributed notification, 2026-11-04).
  const filler = document.createElement('div')
  filler.style.cssText = 'width:200px;height:200px'
  probe.appendChild(filler)
  document.body.appendChild(probe)

  const root = document.documentElement
  let written = -1
  const measure = () => {
    const width = Math.max(0, probe.offsetWidth - probe.clientWidth)
    if (width === written) return
    written = width
    root.style.setProperty('--spacing-scrollbar-width', `${String(width)}px`)
  }

  const observer = new ResizeObserver(measure)
  observer.observe(probe, { box: 'content-box' })
  measure()

  return () => {
    observer.disconnect()
    probe.remove()
    root.style.removeProperty('--spacing-scrollbar-width')
  }
}
