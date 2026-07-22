import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'

// Hoisted mocks: the component delegates to the settings writer + deep-link
// helper and the toast dismisser. We assert the exact wire calls without
// rendering the rest of the app.
type VolumeSpacePayload = { volumeId: string; totalBytes: number; availableBytes: number }
type VolumeSpaceListener = (payload: VolumeSpacePayload) => void

const {
  setLowDiskSpaceNotificationsModeMock,
  openSettingsToLowDiskSpaceMock,
  dismissToastMock,
  onVolumeSpaceChangedMock,
} = vi.hoisted(() => ({
  setLowDiskSpaceNotificationsModeMock: vi.fn(),
  openSettingsToLowDiskSpaceMock: vi.fn(() => Promise.resolve()),
  dismissToastMock: vi.fn(),
  onVolumeSpaceChangedMock: vi.fn(),
}))

vi.mock('./notifications-mode', () => ({
  setLowDiskSpaceNotificationsMode: setLowDiskSpaceNotificationsModeMock,
  openSettingsToLowDiskSpace: openSettingsToLowDiskSpaceMock,
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/tauri-commands', () => ({
  onVolumeSpaceChanged: onVolumeSpaceChangedMock,
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
    volumeId: 'root',
    availableBytes: 42_000_000_000,
    totalBytes: 1_000_000_000_000,
    ...overrides,
  }
}

/** Capture the `volume-space-changed` listener the component registers so tests can pump it. */
function captureVolumeSpaceListener(): () => VolumeSpaceListener {
  let captured: VolumeSpaceListener | null = null
  onVolumeSpaceChangedMock.mockImplementation((handler: VolumeSpaceListener) => {
    captured = handler
    return Promise.resolve(() => {})
  })
  return () => {
    if (!captured) throw new Error('Component did not register a volume-space listener')
    return captured
  }
}

describe('LowDiskSpaceToastContent', () => {
  beforeEach(() => {
    setLowDiskSpaceNotificationsModeMock.mockReset()
    openSettingsToLowDiskSpaceMock.mockReset().mockResolvedValue(undefined)
    dismissToastMock.mockReset()
    onVolumeSpaceChangedMock.mockReset().mockReturnValue(Promise.resolve(() => {}))
  })

  it('renders the seeded free space and percent', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LowDiskSpaceToastContent, { target, props: makeProps() })
    await tick()

    expect(target.textContent).toContain('42000000000 B')
    expect(target.textContent).toContain('4.2%')
    expect(target.textContent).toContain('running low on space')
  })

  it('live-follows the boot volume: a volume-space-changed event updates the readout', async () => {
    const getListener = captureVolumeSpaceListener()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LowDiskSpaceToastContent, { target, props: makeProps() })
    await tick()

    // A matching-volume update recomputes both the free bytes and the percent.
    getListener()({ volumeId: 'root', totalBytes: 1_000_000_000_000, availableBytes: 21_000_000_000 })
    await tick()
    expect(target.textContent).toContain('21000000000 B')
    expect(target.textContent).toContain('2.1%')

    // An update for a different volume is ignored.
    getListener()({ volumeId: 'other', totalBytes: 500, availableBytes: 10 })
    await tick()
    expect(target.textContent).toContain('21000000000 B')
    expect(target.textContent).toContain('2.1%')
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
