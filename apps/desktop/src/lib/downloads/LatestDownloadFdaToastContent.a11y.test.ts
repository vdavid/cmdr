import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  openPrivacySettings: vi.fn(() => Promise.resolve()),
}))

import LatestDownloadFdaToastContent from './LatestDownloadFdaToastContent.svelte'

describe('LatestDownloadFdaToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LatestDownloadFdaToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
