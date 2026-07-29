/**
 * Behavior of the two link shapes and the jump button in
 * `AcknowledgementsDialog.svelte`.
 *
 * Every link in this dialog is an `<a href>` that must NOT navigate: Tauri blocks
 * webview navigation, so the click has to be intercepted and handed to the opener
 * plugin. That contract is easy to break silently (the href alone looks right), so
 * it's pinned here alongside the "jump to npm" scroll.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import AcknowledgementsDialog from './AcknowledgementsDialog.svelte'
import { openExternalUrl } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

vi.mock('./third-party-packages.gen.json', () => ({
  default: {
    rust: [
      { name: 'serde', version: '1.0.228', license: 'MIT OR Apache-2.0', url: 'https://github.com/serde-rs/serde' },
      { name: 'mystery', version: '1.0.0', license: 'MIT', url: '' },
    ],
    npm: [{ name: '@ark-ui/svelte', version: '5.22.1', license: 'MIT', url: 'https://ark-ui.com' }],
  },
}))

const NOTICES_URL = 'https://github.com/vdavid/cmdr/blob/main/THIRD-PARTY-NOTICES.md'

/**
 * Mounts the dialog and waits for the dynamic package-list import to resolve.
 * A fixed number of `tick()`s isn't enough: the list arrives from an `import()`
 * that settles over an unknown number of macrotasks, so poll for the first
 * rendered row instead.
 */
async function mountLoaded(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AcknowledgementsDialog, { target, props: { onClose: () => {} } })
  for (let attempt = 0; attempt < 100; attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 5))
    await tick()
    if (target.querySelector('.package-list li')) return target
  }
  throw new Error("The package list never rendered; the dialog's dynamic import didn't resolve")
}

/** The one element whose text matches exactly, or a failure if there isn't exactly one. */
function byText(target: HTMLElement, selector: string, text: string): HTMLElement {
  const matches = [...target.querySelectorAll<HTMLElement>(selector)].filter((el) => el.textContent.trim() === text)
  expect(matches).toHaveLength(1)
  return matches[0]
}

describe('AcknowledgementsDialog', () => {
  beforeEach(() => {
    vi.mocked(openExternalUrl).mockClear()
    document.body.innerHTML = ''
  })

  it('opens a package link in the system browser instead of navigating', async () => {
    const target = await mountLoaded()
    const link = byText(target, 'a.link-button', 'serde')

    const event = new MouseEvent('click', { bubbles: true, cancelable: true })
    link.dispatchEvent(event)

    expect(openExternalUrl).toHaveBeenCalledWith('https://github.com/serde-rs/serde')
    expect(event.defaultPrevented).toBe(true)
  })

  it('renders a package with no URL as plain text', async () => {
    const target = await mountLoaded()
    expect(byText(target, 'span.package-name', 'mystery').tagName).toBe('SPAN')
  })

  it('opens the notices file on GitHub from the footnote link', async () => {
    const target = await mountLoaded()
    const link = byText(target, 'a.link-button', 'THIRD-PARTY-NOTICES.md')

    link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))

    expect(openExternalUrl).toHaveBeenCalledWith(NOTICES_URL)
  })

  it('scrolls the npm heading into view from the jump button', async () => {
    const target = await mountLoaded()
    const npmHeading = [...target.querySelectorAll('h3')].find((h) => h.textContent.includes('npm'))
    if (!npmHeading) throw new Error('The npm section heading never rendered')
    const scrollIntoView = vi.fn()
    npmHeading.scrollIntoView = scrollIntoView

    byText(target, 'button.btn', 'Jump to npm packages').click()

    expect(scrollIntoView).toHaveBeenCalledTimes(1)
    expect(scrollIntoView.mock.calls[0]?.[0]).toMatchObject({ block: 'start' })
  })
})
