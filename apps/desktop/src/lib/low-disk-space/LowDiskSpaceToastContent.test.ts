import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'

// Hoisted mocks: the component delegates to the settings writer + deep-link
// helper and the toast dismisser. We assert the exact wire calls without
// rendering the rest of the app.
const { setLowDiskSpaceNotificationsModeMock, openSettingsToLowDiskSpaceMock, dismissToastMock } = vi.hoisted(() => ({
  setLowDiskSpaceNotificationsModeMock: vi.fn(),
  openSettingsToLowDiskSpaceMock: vi.fn(() => Promise.resolve()),
  dismissToastMock: vi.fn(),
}))

vi.mock('./notifications-mode', () => ({
  setLowDiskSpaceNotificationsMode: setLowDiskSpaceNotificationsModeMock,
  openSettingsToLowDiskSpace: openSettingsToLowDiskSpaceMock,
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/settings/format-utils', () => ({
  formatFileSizeWithFormat: (bytes: number) => `${String(bytes)} B`,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

import LowDiskSpaceToastContent from './LowDiskSpaceToastContent.svelte'

type ToastProps = ComponentProps<typeof LowDiskSpaceToastContent>

function makeProps(overrides: Partial<ToastProps> = {}): ToastProps {
  return {
    toastId: 'low-disk-space:root',
    availableBytes: 42_000_000_000,
    freePercent: 4.2,
    ...overrides,
  }
}

describe('LowDiskSpaceToastContent', () => {
  beforeEach(() => {
    setLowDiskSpaceNotificationsModeMock.mockReset()
    openSettingsToLowDiskSpaceMock.mockReset().mockResolvedValue(undefined)
    dismissToastMock.mockReset()
  })

  it('renders the snapshotted free space and percent', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LowDiskSpaceToastContent, { target, props: makeProps() })
    await tick()

    expect(target.textContent).toContain('42000000000 B')
    expect(target.textContent).toContain('4.2%')
    expect(target.textContent).toContain('running low on space')
  })

  it('"Disable these notifications" flips the setting to off, dismisses, and deep-links to Settings', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LowDiskSpaceToastContent, { target, props: makeProps() })
    await tick()

    const disableButton = Array.from(target.querySelectorAll('button')).find((b) => /disable/i.test(b.textContent))
    if (!disableButton) throw new Error('Disable button not found')
    disableButton.click()
    await tick()

    expect(setLowDiskSpaceNotificationsModeMock).toHaveBeenCalledWith('off')
    expect(dismissToastMock).toHaveBeenCalledWith('low-disk-space:root')
    expect(openSettingsToLowDiskSpaceMock).toHaveBeenCalledTimes(1)
  })
})
