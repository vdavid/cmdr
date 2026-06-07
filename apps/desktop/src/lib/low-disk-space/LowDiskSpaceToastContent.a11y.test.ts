import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./notifications-mode', () => ({
  setLowDiskSpaceNotificationsMode: vi.fn(),
  openSettingsToLowDiskSpace: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings/format-utils', () => ({
  formatFileSizeWithFormat: (bytes: number) => `${String(bytes)} B`,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

import LowDiskSpaceToastContent from './LowDiskSpaceToastContent.svelte'

describe('LowDiskSpaceToastContent a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LowDiskSpaceToastContent, {
      target,
      props: {
        toastId: 'low-disk-space:root',
        availableBytes: 42_000_000_000,
        freePercent: 4.2,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
