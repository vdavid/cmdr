import { describe, it, expect, vi, beforeEach } from 'vitest'
import { openExternalUrl, openSystemSettingsUrl } from '$lib/tauri-commands'
import { handleMarkdownLinkClick } from './markdown-link-click'

vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

/**
 * Builds a container that delegates clicks the way a markdown-rendering block does,
 * then clicks the element matching `selector` inside it.
 */
function clickInside(html: string, selector: string): MouseEvent {
  const container = document.createElement('div')
  container.innerHTML = html
  container.addEventListener('click', handleMarkdownLinkClick)
  document.body.append(container)
  const target = container.querySelector(selector)
  if (!target) throw new Error(`no element matches ${selector}`)
  const event = new MouseEvent('click', { bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  container.remove()
  return event
}

describe('handleMarkdownLinkClick', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('opens an anchor externally instead of navigating the webview', () => {
    const event = clickInside('<a href="https://getcmdr.com/changelog/">changelog</a>', 'a')

    expect(openExternalUrl).toHaveBeenCalledExactlyOnceWith('https://getcmdr.com/changelog/')
    expect(event.defaultPrevented).toBe(true)
  })

  it('routes a click on markup INSIDE the anchor, not just the anchor itself', () => {
    // A changelog entry can wrap a `code` span in a link, and the click target is then
    // the inner element.
    clickInside('<a href="https://getcmdr.com/"><code>cmdr</code></a>', 'code')

    expect(openExternalUrl).toHaveBeenCalledExactlyOnceWith('https://getcmdr.com/')
  })

  it('sends an x-apple.systempreferences URL through the settings opener', () => {
    // Tauri's opener plugin allows http/https/mailto/tel only, and swallows this scheme.
    clickInside('<a href="x-apple.systempreferences:com.apple.preference.security">Privacy</a>', 'a')

    expect(openSystemSettingsUrl).toHaveBeenCalledExactlyOnceWith(
      'x-apple.systempreferences:com.apple.preference.security',
    )
    expect(openExternalUrl).not.toHaveBeenCalled()
  })

  it('leaves a click that hits no anchor alone', () => {
    const event = clickInside('<p>Plain <strong>text</strong></p>', 'strong')

    expect(openExternalUrl).not.toHaveBeenCalled()
    expect(openSystemSettingsUrl).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(false)
  })

  it('leaves an anchor with no href alone', () => {
    const event = clickInside('<a>no destination</a>', 'a')

    expect(openExternalUrl).not.toHaveBeenCalled()
    expect(event.defaultPrevented).toBe(false)
  })
})
