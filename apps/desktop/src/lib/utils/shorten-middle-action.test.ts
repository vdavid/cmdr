import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useShortenMiddle } from './shorten-middle-action'

/**
 * The action truncates asynchronously (pretext loads on demand), so these tests cover
 * the part that matters for hover: WHICH tooltip carries the full text. It has to be the
 * house tooltip, never the native `title` — a `title` bubble has the OS delay and the OS
 * chrome, so a pane row and a dialog row hovering differently is the visible symptom.
 */
describe('useShortenMiddle tooltips', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  function makeNode(): HTMLElement {
    const el = document.createElement('span')
    document.body.appendChild(el)
    vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({
      left: 100,
      top: 100,
      right: 150,
      bottom: 120,
      width: 50,
      height: 20,
      x: 100,
      y: 100,
      toJSON: () => ({}),
    })
    return el
  }

  function hover(el: HTMLElement): HTMLElement | null {
    el.dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)
    return document.querySelector('.cmdr-tooltip.visible')
  }

  const LONG_PATH = '/Volumes/testshare/test_photos/Meet Recordings/2025-08-19 a very long name indeed.mp4'

  it('shows the full text in the house tooltip, not a native title', () => {
    const el = makeNode()
    const action = useShortenMiddle(el, { text: LONG_PATH, preferBreakAt: '/' })

    expect(el.hasAttribute('title')).toBe(false)
    expect(hover(el)?.textContent).toBe(LONG_PATH)

    action.destroy?.()
  })

  it('stays silent under `tooltipWhenTruncated` while the text still fits', () => {
    const el = makeNode()
    const action = useShortenMiddle(el, { text: 'short.txt', tooltipWhenTruncated: true })

    expect(hover(el)).toBeNull()

    action.destroy?.()
  })

  it('follows the text through an update', () => {
    const el = makeNode()
    const action = useShortenMiddle(el, { text: LONG_PATH, preferBreakAt: '/' })
    const nextPath = '/Volumes/testshare/documents/medical/2026 blood work.pdf'

    action.update?.({ text: nextPath, preferBreakAt: '/' })

    expect(hover(el)?.textContent).toBe(nextPath)

    action.destroy?.()
  })

  it('hovers a fuller text than it shows, fitting or not, and follows it through an update', () => {
    const el = makeNode()
    const action = useShortenMiddle(el, {
      text: 'IMG_0001.jpg',
      tooltip: 'Photos/2024/IMG_0001.jpg',
      tooltipWhenTruncated: true,
    })

    expect(el.textContent).toBe('IMG_0001.jpg')
    expect(hover(el)?.textContent).toBe('Photos/2024/IMG_0001.jpg')

    el.dispatchEvent(new MouseEvent('mouseleave'))
    action.update?.({ text: 'IMG_0001.jpg', tooltip: 'Photos/2025/IMG_0001.jpg', tooltipWhenTruncated: true })
    expect(hover(el)?.textContent).toBe('Photos/2025/IMG_0001.jpg')

    action.destroy?.()
  })

  // #706: the copy dialog's From field showed the S3 access key id through here.
  it('shows and hovers a path the way a person reads it, an S3 key id taken out', () => {
    const el = makeNode()
    const action = useShortenMiddle(el, { text: 's3://AKIAEXAMPLEKEY@127.0.0.1:14480/family-photos/2025' })
    const shown = 's3://127.0.0.1:14480/family-photos/2025'

    expect(el.textContent).toBe(shown)
    expect(hover(el)?.textContent).toBe(shown)

    el.dispatchEvent(new MouseEvent('mouseleave'))
    action.update?.({ text: 's3://AKIAEXAMPLEKEY@127.0.0.1:14480/family-photos/2024' })
    expect(el.textContent).toBe('s3://127.0.0.1:14480/family-photos/2024')

    action.destroy?.()
  })

  it('drops its tooltip when the node goes away', () => {
    const el = makeNode()
    const action = useShortenMiddle(el, { text: LONG_PATH })

    el.dispatchEvent(new MouseEvent('mouseenter'))
    el.remove()
    action.destroy?.()
    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
  })
})
