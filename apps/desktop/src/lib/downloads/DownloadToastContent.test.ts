import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'

// Hoisted mocks: the component delegates to the go-to-download-by-path helper and the
// "Stop showing these" deep-link + settings writer. We assert the exact wire
// calls without rendering the rest of the app.
const {
  goToDownloadMock,
  setDownloadsNotificationsModeMock,
  openSettingsToDownloadsNotificationsMock,
  dismissToastMock,
} = vi.hoisted(() => ({
  goToDownloadMock: vi.fn(() => Promise.resolve()),
  setDownloadsNotificationsModeMock: vi.fn(),
  openSettingsToDownloadsNotificationsMock: vi.fn(() => Promise.resolve()),
  dismissToastMock: vi.fn(),
}))

vi.mock('./go-to-latest', () => ({
  goToDownload: goToDownloadMock,
}))

vi.mock('./notifications-mode', () => ({
  setDownloadsNotificationsMode: setDownloadsNotificationsModeMock,
  openSettingsToDownloadsNotifications: openSettingsToDownloadsNotificationsMock,
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: dismissToastMock,
}))

import DownloadToastContent from './DownloadToastContent.svelte'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

type DownloadToastProps = ComponentProps<typeof DownloadToastContent>

function makeProps(overrides: Partial<DownloadToastProps> = {}): DownloadToastProps {
  return {
    toastId: 'downloads:test-id',
    explorer: undefined as ExplorerAPI | undefined,
    event: {
      parentDir: '/Users/me/Downloads',
      fileName: 'report.pdf',
      inSubdir: false,
      sizeBytes: 1024,
    },
    shortcutHint: '⌘J',
    globalBinding: '⌃⌥⌘J',
    ...overrides,
  }
}

describe('DownloadToastContent', () => {
  beforeEach(() => {
    goToDownloadMock.mockReset().mockResolvedValue(undefined)
    setDownloadsNotificationsModeMock.mockReset()
    openSettingsToDownloadsNotificationsMock.mockReset().mockResolvedValue(undefined)
    dismissToastMock.mockReset()
  })

  it('renders the filename in monospace and the shortcut hint snapshotted at creation', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps() })
    await tick()

    // Filename appears verbatim in the title row.
    expect(target.textContent).toContain('report.pdf')
    // Snapshotted shortcut hint: the prop value, not whatever the live binding is now.
    expect(target.textContent).toContain('⌘J')
    expect(target.textContent).toContain('jump')
  })

  it('teaches both shortcuts: the in-app chip and the global from-any-app chip plus animation', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps() })
    await tick()

    // In-app chip and the global from-any-app chip both render.
    expect(target.textContent).toContain('⌘J')
    expect(target.textContent).toContain('⌃⌥⌘J')
    expect(target.textContent.toLowerCase()).toContain('from any app')
    // The default global binding gets the keyboard animation.
    expect(target.querySelector('.shortcut-animation svg')).not.toBeNull()
  })

  it('omits the global hint entirely when globalBinding is empty (hotkey off or unbound)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps({ globalBinding: '' }) })
    await tick()

    // In-app chip still shows; nothing about "any app" and no animation.
    expect(target.textContent).toContain('⌘J')
    expect(target.textContent.toLowerCase()).not.toContain('from any app')
    expect(target.querySelector('.shortcut-animation')).toBeNull()
  })

  it('keeps the global chip but drops the animation when the global binding is remapped', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps({ globalBinding: '⌃⌥⌘K' }) })
    await tick()

    // The remapped combo is taught via the text chip...
    expect(target.textContent).toContain('⌃⌥⌘K')
    // ...but the animation (which lights up the literal default keys) is gone.
    expect(target.querySelector('.shortcut-animation')).toBeNull()
  })

  it('omits the in-app hint when its shortcut is unbound, still teaching the global one', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps({ shortcutHint: '' }) })
    await tick()

    expect(target.textContent.toLowerCase()).not.toContain('to jump here')
    expect(target.textContent).toContain('⌃⌥⌘J')
  })

  it('renders the relative-subdir hint when inSubdir is true', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, {
      target,
      props: makeProps({
        event: {
          parentDir: '/Users/me/Downloads/Chrome',
          fileName: 'setup.dmg',
          inSubdir: true,
          sizeBytes: null,
        },
      }),
    })
    await tick()

    expect(target.textContent.toLowerCase()).toContain('chrome')
  })

  it('clicking the "Jump to file" button goes to the specific file by path', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps() })
    await tick()

    const jumpButton = Array.from(target.querySelectorAll('button')).find((b) => /jump/i.test(b.textContent))
    if (!jumpButton) throw new Error('Jump button not found')
    jumpButton.click()
    await tick()

    expect(goToDownloadMock).toHaveBeenCalledTimes(1)
    expect(goToDownloadMock).toHaveBeenCalledWith(undefined, '/Users/me/Downloads', 'report.pdf')
    // Pressing the explicit button also dismisses the toast.
    expect(dismissToastMock).toHaveBeenCalledWith('downloads:test-id')
  })

  it('clicking the toast body (outside the buttons) also triggers go-to-by-path', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps() })
    await tick()

    // The outer container is the clickable surface; click an inner non-button
    // element (the title span) and let the event bubble up.
    const title = target.querySelector('.title')
    if (!title) throw new Error('Title element not found')
    ;(title as HTMLElement).click()
    await tick()

    expect(goToDownloadMock).toHaveBeenCalledTimes(1)
    expect(goToDownloadMock).toHaveBeenCalledWith(undefined, '/Users/me/Downloads', 'report.pdf')
  })

  it('clicking a button does NOT also trigger the body-click handler (stopPropagation)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps() })
    await tick()

    const stopButton = Array.from(target.querySelectorAll('button')).find((b) => /stop showing/i.test(b.textContent))
    if (!stopButton) throw new Error('Stop button not found')
    stopButton.click()
    await tick()

    // The Stop button's own action fires...
    expect(setDownloadsNotificationsModeMock).toHaveBeenCalledTimes(1)
    expect(setDownloadsNotificationsModeMock).toHaveBeenCalledWith('neither')
    expect(openSettingsToDownloadsNotificationsMock).toHaveBeenCalledTimes(1)
    // ...but the body-click jump MUST NOT fire (stopPropagation).
    expect(goToDownloadMock).not.toHaveBeenCalled()
  })

  it('the clickable body is not focusable (mouse-only convenience; buttons own keyboard focus)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, { target, props: makeProps() })
    await tick()

    const root = target.querySelector('.toast-body')
    if (!root) throw new Error('Toast body root not found')
    // No `tabindex` makes the div skipped by Tab; the two buttons inside take
    // the keyboard-activation path independently.
    expect(root.hasAttribute('tabindex')).toBe(false)
    expect(root.getAttribute('role')).not.toBe('button')
  })
})
