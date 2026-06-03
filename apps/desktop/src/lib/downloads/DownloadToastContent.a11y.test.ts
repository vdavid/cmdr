import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./go-to-latest', () => ({
  goToDownload: vi.fn(() => Promise.resolve()),
}))

vi.mock('./notifications-mode', () => ({
  setDownloadsNotificationsMode: vi.fn(),
  openSettingsToDownloadsNotifications: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

import DownloadToastContent from './DownloadToastContent.svelte'

describe('DownloadToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DownloadToastContent, {
      target,
      props: {
        toastId: 'downloads:a11y',
        explorer: undefined,
        event: {
          path: '/Users/me/Downloads/report.pdf',
          parentDir: '/Users/me/Downloads',
          fileName: 'report.pdf',
          observedAtMs: 1_700_000_000_000,
          inSubdir: false,
          sizeBytes: 1024,
        },
        shortcutHint: '⌘J',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
